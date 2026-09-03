from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .config import PipelinePaths
from .database import build_database
from .downloaders import CHGISManualImporter, download_dataset
from .export import export_parquet
from .logging import log_event
from .parsers import iter_classical_modern, stage_cbdb, stage_ctext, write_jsonl
from .phase_reports import write_phase_reports
from .query_service import HistoryQueryService
from .real_build import build_from_staging
from .review import ensure_review_table
from .sample import sample_records
from .semantic_layer import build_semantic_layer, write_link_qa_report, write_semantic_report, write_story_samples
from .stats import write_reports
from .validation import validate_database


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(prog="history-data", description="中国历史离线数据仓库管线")
    root.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2], help="history-data-pipeline 目录")
    commands = root.add_subparsers(dest="command", required=True)
    download = commands.add_parser("download")
    download.add_argument("dataset", choices=("cbdb", "ctext", "niutrans", "wikipedia", "wikisource"))
    importer = commands.add_parser("import")
    importer.add_argument("dataset", choices=("chgis",))
    importer.add_argument("--input", type=Path, required=True)
    importer.add_argument("--version", required=True)
    importer.add_argument("--license", default="CHGIS 官方用户协议；仅非商业学术研究，已由用户确认许可")
    for name in ("parse", "normalize", "link", "validate", "stats", "export", "build"):
        command = commands.add_parser(name)
        if name == "build":
            command.add_argument("--sample", action="store_true", help="构建最小验证 Dataset")
            command.add_argument("--from-staging", action="store_true", help="使用真实 staging 构建正式数据库")
    query = commands.add_parser("query", help="只读查询正式 History DuckDB")
    query.add_argument("kind", choices=("person", "work", "text", "source", "stats", "periods", "regimes", "stories", "story", "event"))
    query.add_argument("query", nargs="?", help="人物或作品名称")
    query.add_argument("--relations", action="store_true", help="人物查询同时返回人物关系")
    query.add_argument("--places", action="store_true", help="人物查询同时返回人物地点关系")
    query.add_argument("--work", help="按作品名称筛选 HistoricalText")
    query.add_argument("--entity-type", help="Source provenance 的实体类型")
    query.add_argument("--entity-id", help="Source provenance 的实体 ID")
    query.add_argument("--limit", type=int, default=20)
    query.add_argument("--json", action="store_true", dest="as_json", help="输出 JSON")
    query.add_argument("--events", action="store_true", help="Story 查询返回有序事件")
    query.add_argument("--people", action="store_true", help="Event 查询返回人物")
    query.add_argument("--texts", action="store_true", help="Event 查询返回 HistoricalText")
    return root


def _print_query_result(result, as_json: bool) -> None:
    if as_json:
        print(json.dumps(result, ensure_ascii=False, indent=2, default=str))
        return
    if isinstance(result, dict):
        if "counts" in result:
            print("History DuckDB 统计")
            for name, count in result["counts"].items():
                print(f"- {name}: {count:,}")
            print(f"- 完整古文记录: {result['historical_texts_complete']:,}")
            print(f"- 译文等于简体原文: {result['translation_equals_simplified']:,}")
            print(f"- 待复核队列: {result['review_queue_pending']:,}")
            return
        print(f"{result.get('canonical_name_zh_cn', result.get('title', '结果'))} [{result.get('id', '')}]")
        for key in ("birth_year", "death_year", "quality_status", "created_from_source", "aliases", "source_mappings", "relations", "places"):
            if key in result:
                value = result[key]
                if isinstance(value, list):
                    print(f"- {key}: {len(value)} 条")
                    for row in value[:5]:
                        print(f"  {row}")
                else:
                    print(f"- {key}: {value}")
        return
    if isinstance(result, list):
        print(f"共 {len(result)} 条")
        for row in result[:20]:
            if "canonical_name_zh_cn" in row:
                print(f"- {row['canonical_name_zh_cn']} [{row['id']}] {row.get('birth_year')}–{row.get('death_year')}")
            elif "original_text" in row:
                print(f"- {row.get('work_title') or row.get('title_zh_cn')} / {row.get('chapter')}: {str(row['original_text'])[:60]}")
            elif "dataset" in row:
                print(f"- {row.get('dataset')} [{row.get('id')}] {row.get('dataset_version')}")
            else:
                print(f"- {row}")


def main(argv: list[str] | None = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    args = parser().parse_args(argv)
    paths = PipelinePaths(args.root)
    paths.ensure()
    if args.command == "download":
        target = download_dataset(paths, args.dataset)
        log_event(paths.logs, "download.log", "Downloaded", f"dataset={args.dataset} path={target}")
        print(target)
        return 0
    if args.command == "import":
        target = CHGISManualImporter(paths).import_directory(args.input, args.version, args.license)
        log_event(paths.logs, "download.log", "Imported", f"dataset=chgis path={target}")
        print(target)
        return 0
    if args.command == "parse":
        found = 0
        for snapshot in sorted((paths.raw / "classical-modern").glob("*")):
            if not snapshot.is_dir():
                continue
            target = paths.staging / "classical-modern" / f"{snapshot.name}.jsonl"
            count = write_jsonl(iter_classical_modern(snapshot), target)
            if count:
                found += count
        cbdb_counts = {}
        for snapshot in sorted((paths.raw / "cbdb").glob("*")):
            if snapshot.is_dir():
                cbdb_counts = stage_cbdb(snapshot, paths.staging / "cbdb" )
        ctext_count = 0
        for snapshot in sorted((paths.raw / "ctext").glob("*")):
            if snapshot.is_dir():
                ctext_count += stage_ctext(snapshot, paths.staging / "ctext")
        log_event(paths.logs, "normalize.log", "Parsed", f"classical-modern_records={found} cbdb={cbdb_counts} ctext_entities={ctext_count}")
        print(f"Parsed classical-modern={found}; cbdb={cbdb_counts}; ctext={ctext_count}")
        return 0
    if args.command == "build":
        if args.from_staging:
            target = build_from_staging(paths)
            semantic_result = build_semantic_layer(target, paths.root)
            write_story_samples(target, paths.root)
            write_link_qa_report(target, paths.root, semantic_result)
            write_semantic_report(target, paths.root, semantic_result)
            write_phase_reports(paths)
            print(target)
            return 0
        if not args.sample:
            raise SystemExit("请指定 --from-staging 或 --sample")
        records = sample_records()
        (paths.staging / "sample_records.json").write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding="utf-8")
        build_database(paths.database, records)
        log_event(paths.logs, "normalize.log", "Normalized", "sample_records")
        log_event(paths.logs, "link.log", "Linked", "sample_relations")
        write_reports(records, paths.reports)
        print(paths.database)
        return 0
    if not paths.database.exists():
        raise SystemExit(f"数据库不存在，请先运行 build --from-staging: {paths.database}")
    if args.command == "query":
        service = HistoryQueryService(paths.database)
        if args.kind == "person":
            if not args.query:
                raise SystemExit("query person 需要名称")
            result = service.get_person(args.query, relations=args.relations, places=args.places)
            if result is None:
                result = service.search_people(args.query, args.limit)
        elif args.kind == "work":
            if not args.query:
                raise SystemExit("query work 需要作品名称")
            result = service.get_work(args.query, args.limit)
        elif args.kind == "text":
            result = service.get_historical_texts(args.work, args.limit)
        elif args.kind == "source":
            if bool(args.entity_type) != bool(args.entity_id):
                raise SystemExit("query source 的 --entity-type 与 --entity-id 必须同时提供")
            result = service.get_sources(args.entity_type, args.entity_id)
        elif args.kind == "periods":
            result = service.list_periods(args.limit)
        elif args.kind == "regimes":
            if not args.query:
                raise SystemExit("query regimes 需要提供 Period 名称或 ID")
            result = service.list_regimes_by_period(args.query, args.limit)
        elif args.kind == "stories":
            result = service.list_stories(args.query, args.limit)
        elif args.kind == "story":
            if not args.query:
                raise SystemExit("query story 需要提供 Story 名称或 ID")
            result = service.get_story(args.query, include_events=True)
            if result is None:
                raise SystemExit(f"未找到 Story: {args.query}")
        elif args.kind == "event":
            if not args.query:
                raise SystemExit("query event 需要提供 Event 名称或 ID")
            result = service.get_event(args.query, include_details=True)
            if result is None:
                raise SystemExit(f"未找到 Event: {args.query}")
        else:
            result = service.get_stats()
        _print_query_result(result, args.as_json)
        return 0
    if args.command == "validate":
        pending = ensure_review_table(paths.database)
        errors = validate_database(paths.database)
        validation_lines = [*(errors or ["OK"]), f"review_queue_pending={pending}"]
        (paths.reports / "validation.log").write_text("\n".join(validation_lines) + "\n", encoding="utf-8")
        if errors:
            print("\n".join(errors), file=sys.stderr)
            return 1
        print("Validation OK")
        return 0
    if args.command == "stats":
        ensure_review_table(paths.database)
        print(json.dumps(write_phase_reports(paths), ensure_ascii=False, indent=2, default=str))
        return 0
    if args.command == "export":
        export_parquet(paths.database, paths.exports)
        print(paths.exports)
        return 0
    print(f"{args.command}: 已预留接口；V1 构建使用 sample records", file=sys.stderr)
    return 0
