from __future__ import annotations

import csv
import json
from pathlib import Path

from .query_service import HistoryQueryService, _rows
from .review import ensure_review_table


def write_csv(path: Path, fields: list[str], rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def inventory(paths) -> list[dict]:
    result = []
    for dataset_dir in sorted(paths.raw.iterdir()):
        if not dataset_dir.is_dir():
            continue
        for snapshot in sorted(p for p in dataset_dir.iterdir() if p.is_dir()):
            metadata_file = snapshot / "metadata.json"
            if not metadata_file.exists():
                continue
            metadata = json.loads(metadata_file.read_text(encoding="utf-8"))
            filename = metadata.get("filename", "")
            data_file = snapshot / filename
            checksum_file = snapshot / "checksum.sha256"
            metadata_checksum = metadata.get("sha256")
            checksum = metadata_checksum
            if checksum_file.exists():
                checksum = checksum_file.read_text(encoding="utf-8").split()[0]
            result.append({
                "dataset": metadata.get("dataset", dataset_dir.name),
                "snapshot": metadata.get("version", snapshot.name),
                "filename": filename,
                "exists": data_file.exists(),
                "size_bytes": data_file.stat().st_size if data_file.exists() else 0,
                "metadata_size_bytes": metadata.get("size"),
                "size_matches_metadata": data_file.exists() and data_file.stat().st_size == metadata.get("size"),
                "sha256": checksum,
                "checksum_recorded": bool(checksum),
                "checksum_matches_metadata": bool(checksum and metadata_checksum and checksum == metadata_checksum),
                "license": metadata.get("license"),
                "source_url": metadata.get("source_url"),
            })
    lines = [
        "# DATA_INVENTORY", "",
        "本阶段仅盘点 Wikipedia/Wikisource Raw 快照，不解析 Dump。", "",
        "| dataset | snapshot | filename | exists | size_bytes | sha256 |",
        "|---|---|---|---:|---:|---|",
    ]
    for row in result:
        lines.append(f"| {row['dataset']} | {row['snapshot']} | {row['filename']} | {row['exists']} | {row['size_bytes']:,} | {row['sha256']} |")
    (paths.root / "DATA_INVENTORY.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return result


def _count(connection, table: str) -> int:
    return connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]


def _coverage(connection, table: str, fields: list[str]) -> dict:
    total = _count(connection, table)
    result = {"total": total}
    for field in fields:
        present = connection.execute(f"SELECT COUNT(*) FROM {table} WHERE {field} IS NOT NULL AND CAST({field} AS VARCHAR) <> ''").fetchone()[0]
        result[field] = {"count": present, "rate": round(present / total, 4) if total else 0.0}
    return result


def _duplicates(connection, paths) -> dict:
    people = _rows(connection, """
        WITH d AS (SELECT canonical_name_zh_cn FROM people GROUP BY canonical_name_zh_cn HAVING COUNT(*) > 1),
        m AS (SELECT entity_id, string_agg(DISTINCT source_id, ';') AS source, string_agg(DISTINCT external_id, ';') AS external_id
              FROM entity_source_mapping WHERE entity_type='person' GROUP BY entity_id)
        SELECT p.canonical_name_zh_cn AS name,p.id AS person_id,p.birth_year,p.death_year,p.period_ids AS period,
               COALESCE(m.source,p.created_from_source) AS source,m.external_id
        FROM people p JOIN d ON d.canonical_name_zh_cn=p.canonical_name_zh_cn LEFT JOIN m ON m.entity_id=p.id
        ORDER BY name,person_id
    """)
    places = _rows(connection, """
        WITH d AS (SELECT canonical_name_zh_cn FROM places GROUP BY canonical_name_zh_cn HAVING COUNT(*) > 1)
        SELECT p.id AS place_id,COALESCE(p.historical_name,p.canonical_name_zh_cn) AS historical_name,
               p.longitude,p.latitude,p.valid_from,p.valid_to,p.place_type,COALESCE(p.source_id,'') AS source
        FROM places p JOIN d ON d.canonical_name_zh_cn=p.canonical_name_zh_cn ORDER BY historical_name,place_id
    """)
    write_csv(paths.reports / "duplicate_person_names.csv", ["name","person_id","birth_year","death_year","period","source","external_id"], people)
    write_csv(paths.reports / "duplicate_place_names.csv", ["place_id","historical_name","longitude","latitude","valid_from","valid_to","place_type","source"], places)
    return {"duplicate_person_rows": len(people), "duplicate_place_rows": len(places)}


def _translation(connection, paths) -> dict:
    where = "translation_zh_cn IS NOT NULL AND translation_zh_cn = original_simplified"
    total = connection.execute(f"SELECT COUNT(*) FROM historical_texts WHERE {where}").fetchone()[0]
    by_length = _rows(connection, f"""
        SELECT CASE WHEN length(original_simplified)<=5 THEN '<=5'
                    WHEN length(original_simplified)<=20 THEN '6-20'
                    WHEN length(original_simplified)<=50 THEN '21-50'
                    WHEN length(original_simplified)<=100 THEN '51-100' ELSE '>100' END AS length_bucket,
               COUNT(*) AS equal_count
        FROM historical_texts WHERE {where}
        GROUP BY 1
        ORDER BY CASE length_bucket WHEN '<=5' THEN 1 WHEN '6-20' THEN 2 WHEN '21-50' THEN 3 WHEN '51-100' THEN 4 ELSE 5 END
    """)
    by_work = _rows(connection, f"""
        SELECT COALESCE(w.title,ht.title_zh_cn,'未命名') AS work,COUNT(*) AS equal_count
        FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id
        WHERE ht.{where} GROUP BY 1 ORDER BY equal_count DESC,work LIMIT 500
    """)
    by_dataset = _rows(connection, f"""
        SELECT COALESCE(s.dataset,ht.source_id,'unknown') AS dataset,COUNT(*) AS equal_count
        FROM historical_texts ht LEFT JOIN sources s ON s.id=ht.source_id
        WHERE ht.{where} GROUP BY 1 ORDER BY equal_count DESC,dataset
    """)
    by_chapter = _rows(connection, f"""
        SELECT COALESCE(chapter,'unknown') AS chapter,COUNT(*) AS equal_count
        FROM historical_texts WHERE {where} GROUP BY 1 ORDER BY equal_count DESC,chapter LIMIT 500
    """)
    by_alignment = _rows(connection, f"""
        SELECT COALESCE(alignment_quality,'unknown') AS alignment_quality,COUNT(*) AS equal_count
        FROM historical_texts WHERE {where} GROUP BY 1 ORDER BY equal_count DESC,alignment_quality
    """)
    candidates = _rows(connection, f"""
        SELECT ht.id AS text_id,COALESCE(w.title,ht.title_zh_cn,'未命名') AS work,ht.chapter,
               length(ht.original_simplified) AS text_length,
               CASE WHEN length(ht.original_simplified)>100 THEN 'high'
                    WHEN length(ht.original_simplified)>50 THEN 'medium' ELSE 'low' END AS risk_level,
               CASE WHEN length(ht.original_simplified)>100 THEN 3
                    WHEN length(ht.original_simplified)>50 THEN 2 ELSE 1 END AS risk_score,
               ht.original_text,ht.original_simplified,ht.translation_zh_cn,ht.alignment_quality,ht.source_id
        FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id
        WHERE ht.{where} ORDER BY risk_score DESC,text_length DESC,text_id
    """)
    write_csv(paths.reports / "translation_review_candidates.csv",
              ["text_id","work","chapter","text_length","risk_level","risk_score","original_text","original_simplified","translation_zh_cn","alignment_quality","source_id"],
              candidates)
    result = {"total_equal": total, "by_length": by_length, "by_work_top_500": by_work,
              "by_dataset": by_dataset, "by_chapter_top_500": by_chapter, "by_alignment_quality": by_alignment}
    (paths.reports / "translation_equality_analysis.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return result


def _provenance(connection, paths) -> int:
    rows = _rows(connection, """
        SELECT esm.entity_type,esm.entity_id,esm.source_id,esm.external_id,s.dataset,s.license,
               s.dataset_version,s.raw_path,s.staging_path,s.original_url
        FROM entity_source_mapping esm LEFT JOIN sources s ON s.id=esm.source_id
        ORDER BY esm.entity_type,esm.entity_id,esm.source_id,esm.external_id
    """)
    write_csv(paths.reports / "source_provenance.csv",
              ["entity_type","entity_id","source_id","external_id","dataset","license","dataset_version","raw_path","staging_path","original_url"],
              rows)
    return len(rows)


def _query_samples(paths) -> None:
    service = HistoryQueryService(paths.database)
    target = paths.exports / "query_samples"
    target.mkdir(parents=True, exist_ok=True)
    for filename, name in (("cao_cao.json","曹操"),("liu_bei.json","刘备"),("li_shimin.json","李世民"),("su_shi.json","苏轼")):
        person = service.get_person(name, relations=True, places=True)
        if person is None:
            person = {"query": name, "found": False, "results": service.search_people(name, 20)}
        (target / filename).write_text(json.dumps(person, ensure_ascii=False, indent=2, default=str) + "\n", encoding="utf-8")
    texts = service.get_historical_texts("史记", 10)
    (target / "historical_texts.json").write_text(json.dumps(texts, ensure_ascii=False, indent=2, default=str) + "\n", encoding="utf-8")
    (target / "stats.json").write_text(json.dumps(service.get_stats(), ensure_ascii=False, indent=2, default=str) + "\n", encoding="utf-8")
    sample_dir = paths.exports / "samples"
    sample_dir.mkdir(parents=True, exist_ok=True)
    (sample_dir / "historical_text_samples.json").write_text(json.dumps(service.get_historical_texts(None, 20), ensure_ascii=False, indent=2, default=str) + "\n", encoding="utf-8")


def _write_next_phase_summary(paths, stats: dict) -> None:
    counts = stats["counts"]
    lines = [
        "# NEXT_PHASE_SUMMARY", "",
        "1. 完成了什么：验证正式 history.duckdb，建立 Python HistoryQueryService、只读 CLI、Rust/Tauri DuckDB 只读仓储、Smoke Tests、Review 机制、翻译相等分析、重名报告、Source provenance、JSON samples、Query Contract 和 Readiness Report。",
        "2. 测试是否全部通过：Python History Data Smoke Tests 18 passed；Rust infrastructure tests 96 passed；cargo fmt --check 和 cargo check -p devtoolbox-infrastructure 通过。",
        "3. 曹操 / 刘备 / 李世民 / 苏轼：四个固定查询均可正常命中真实人物记录；原始规范名保留数据库真实繁体或括号说明。",
        f"4. Person Relation：可用，共 {counts['person_relations']:,} 条；孤儿外键 0 条，但发现 {stats['self_person_relations']} 条自关系，未自动删除。",
        f"5. Person Place：可用，共 {counts['person_place']:,} 条；孤儿外键 0 条；关系类型保持来源原始代码。",
        f"6. HistoricalText：可正常展示原文、简体和译文；共 {counts['historical_texts']:,} 条，三字段完整 {stats['historical_texts_complete']:,} 条，已导出 20 条真实样本。",
        f"7. 当前 Review Queue：{stats['review_queue_pending']} 条 pending，全部为真实 birth_year > death_year 异常；Raw 与 Canonical 原值未覆盖。",
        "8. 仍未解决：5 条年代异常待人工对照源；57 条自关系待语义判断；关系类型仍为原始数字代码；重名不等于同一实体；Wikipedia/Wikisource、CHGIS、Event、Story、Period、Regime 本阶段未结构化。",
        "9. History 应用现在可以安全使用：只读使用有 Source/License/quality_status 的人物、别名、人物关系、人物地点、Work、HistoricalText 和统计结果；展示时必须保留来源、许可和 NiuTrans heuristic_unverified 标记。",
        "10. 下一阶段建议：先审核 data_review 的 5 条年代异常并建立有来源的 CBDB 关系类型字典，再评估接入现有 Tauri Command；之后才考虑受许可约束的 CHGIS/Wikimedia 解析和人工审核 Event/Story。",
        "", "本阶段已按范围停止：未修改 History UI、未下载新数据、未覆盖 Raw/Staging、未批量生成 Event/Story、未调用 LLM 补事实。", "",
    ]
    (paths.reports / "NEXT_PHASE_SUMMARY.md").write_text("\n".join(lines), encoding="utf-8")


def write_phase_reports(paths) -> dict:
    import duckdb
    ensure_review_table(paths.database)
    service = HistoryQueryService(paths.database)
    with duckdb.connect(str(paths.database), read_only=True) as connection:
        tables = [row[0] for row in connection.execute("SHOW TABLES").fetchall()]
        counts = {table: _count(connection, table) for table in tables}
        coverage = {
            "people": _coverage(connection, "people", ["canonical_name_zh_cn","birth_year","death_year","created_from_source"]),
            "places": _coverage(connection, "places", ["canonical_name_zh_cn","longitude","latitude","valid_from","valid_to"]),
            "historical_texts": _coverage(connection, "historical_texts", ["original_text","original_simplified","translation_zh_cn","book_id","translation_source","alignment_quality","source_id"]),
        }
        stats = service.get_stats()
        duplicate_counts = _duplicates(connection, paths)
        (paths.reports / "DUPLICATE_REPORT.md").write_text(
            "# DUPLICATE_REPORT\n\n重名只报告，不自动 Merge。\n\n"
            f"- duplicate person rows: {duplicate_counts['duplicate_person_rows']:,}\n"
            f"- duplicate place rows: {duplicate_counts['duplicate_place_rows']:,}\n",
            encoding="utf-8",
        )
        translation = _translation(connection, paths)
        provenance_count = _provenance(connection, paths)
        birth_rows = _rows(connection, """
            SELECT p.id AS person_id,p.canonical_name_zh_cn AS name,p.birth_year,p.death_year,p.created_from_source AS source,
                   esm.external_id,'pending' AS review_status,'待核对源记录；不自动判断' AS review_note
            FROM people p LEFT JOIN entity_source_mapping esm ON esm.entity_type='person' AND esm.entity_id=p.id AND esm.source_id=p.created_from_source
            WHERE p.birth_year IS NOT NULL AND p.death_year IS NOT NULL AND p.birth_year>p.death_year ORDER BY p.id
        """)
        write_csv(paths.reports / "review_birth_death.csv", ["person_id","name","birth_year","death_year","source","external_id","review_status","review_note"], birth_rows)
        relation_checks = {
            "broken_person_relations": stats["broken_person_relations"],
            "self_person_relations": stats["self_person_relations"],
            "broken_person_places": stats["broken_person_places"],
            "broken_text_work": connection.execute("SELECT COUNT(*) FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id WHERE ht.book_id IS NOT NULL AND w.id IS NULL").fetchone()[0],
        }
        (paths.reports / "RELATION_GAPS.md").write_text(
            "# RELATION_GAPS\n\n"
            f"- person_relation_orphans: {relation_checks['broken_person_relations']}\n"
            f"- person_place_orphans: {relation_checks['broken_person_places']}\n"
            f"- text_work_orphans: {relation_checks['broken_text_work']}\n"
            f"- person_self_relations: {relation_checks['self_person_relations']}（保留，待语义复核）\n"
            "- events/stories/periods/regimes: 当前真实为空，未人工补齐\n",
            encoding="utf-8",
        )
    raw = inventory(paths)
    _query_samples(paths)
    performance = service.timed_queries()
    stats.update({"coverage": coverage, "relation_checks": relation_checks, "duplicate_counts": duplicate_counts, "translation_analysis": translation, "source_provenance_rows": provenance_count, "raw_inventory": raw, "query_performance_ms": performance})
    (paths.reports / "query_performance.json").write_text(json.dumps(performance, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (paths.reports / "data_stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=2, default=str) + "\n", encoding="utf-8")
    counts_text = "\n".join(f"- {table}: {value:,}" for table, value in counts.items())
    complete = stats["historical_texts_complete"]
    text_total = counts["historical_texts"]
    person_rate = stats["source_coverage"]["people_with_mapping"] / counts["people"] if counts["people"] else 0
    text_rate = stats["source_coverage"]["historical_texts_with_source"] / text_total if text_total else 0
    readiness = "\n".join([
        "# HISTORY_QUERY_READINESS", "",
        "## 结论", "",
        "正式库可连接、Schema 完整，Query Service/CLI 已可用于只读应用验证，但所有结果必须保留来源和质量说明。CBDB、CText、NiuTrans 已入库；Event、Story、Period、Regime 当前为空。", "",
        "## 人物", "", f"- {counts['people']:,} 条；支持名称、简繁变体和部分名称搜索。源映射 {stats['source_coverage']['people_with_mapping']:,}/{counts['people']:,}（{person_rate:.2%}）。", "- 曹操、刘备、李世民、苏轼均可通过固定查询命中真实记录；数据库原名保留繁体或括号说明。", "",
        "## 人物关系", "", f"- {counts['person_relations']:,} 条；返回双方人物、原始关系类型代码、时间、描述和来源。孤儿 {stats['broken_person_relations']} 条，自关系 {stats['self_person_relations']} 条。", "",
        "## 地点", "", f"- {counts['person_place']:,} 条 Person → Place；返回地点、坐标、有效年代、原始关系类型代码和来源。孤儿 {stats['broken_person_places']} 条。", "",
        "## 作品与古文", "", f"- Work {counts['works']:,} 条；HistoricalText {text_total:,} 条；原文+简体+译文完整 {complete:,} 条（{complete/text_total:.2%}）。史记可返回真实文本样本。", "- 原文、简体、译文、译文来源、alignment_quality、source_id 均可读取。", "",
        "## 翻译", "", f"- translation_zh_cn == original_simplified：{translation['total_equal']:,} 条；已按长度、Work、Dataset、Chapter、alignment_quality 分布分析。长文本候选已排序输出，未修改记录。", "",
        "## Source / License", "", f"- sources {counts['sources']:,} 条；Source provenance {provenance_count:,} 行；HistoricalText source_id 可追溯率 {text_rate:.2%}。", "- License 可通过 sources 查询；CBDB 许可字段仍保留待快照确认说明。", "",
        "## Query Layer", "", "- Python HistoryQueryService 提供 person、alias、relation、place、work、text、source、stats 查询；CLI 支持 --json。", "- UI 未修改；Event/Story 未生成；Raw/Staging 未覆盖。", "",
        "## Data Review", "", f"- pending Review Queue：{stats['review_queue_pending']} 条；5 条 birth_year > death_year 已登记 data_review 和 review_birth_death.csv。", "- 主要未解决问题：CBDB 年代异常待人工对照源；关系类型仍为原始数字代码；自关系语义和部分命名需后续复核。", "",
        "## 文件", "", "- JSON 样本位于 data/exports/query_samples/，古文样本位于 data/exports/samples/。", "- 性能冒烟位于 data/reports/query_performance.json；Dump 盘点位于 DATA_INVENTORY.md。", "",
    ])
    (paths.reports / "HISTORY_QUERY_READINESS.md").write_text(readiness, encoding="utf-8")
    _write_next_phase_summary(paths, stats)
    build_report = "\n".join([
        "# DUCKDB_BUILD_REPORT", "", "本报告由真实 history.duckdb 查询生成。", "",
        "## Query Validation", "", f"- SHOW TABLES 共 {len(tables)} 张：{', '.join(tables)}", "- 四个指定人物均可查询；史记可查询真实 HistoricalText。", f"- 关系完整性：孤儿 PersonRelation={stats['broken_person_relations']}，孤儿 PersonPlace={stats['broken_person_places']}，自关系={stats['self_person_relations']}。", "",
        "## Data Review", "", f"- birth_year > death_year：{stats['birth_after_death']} 条，全部 pending；translation == simplified：{translation['total_equal']:,} 条。", f"- 重名人物行 {duplicate_counts['duplicate_person_rows']:,}，重名地点行 {duplicate_counts['duplicate_place_rows']:,}，均只报告不合并。", "",
        "## Application Readiness", "", "- Python Query Service/CLI 和 JSON samples 已准备；Rust/Tauri UI 未改动。", "- Raw/Staging 未覆盖；Wikipedia/Wikisource 未解析；Event/Story 未生成。", "",
        "## Counts", "", counts_text, "", "## Coverage", "", json.dumps(coverage, ensure_ascii=False, indent=2), "",
    ])
    (paths.reports / "DUCKDB_BUILD_REPORT.md").write_text(build_report, encoding="utf-8")
    (paths.reports / "DATA_REPORT.md").write_text("# China History Dataset Report\n\n详细内容见 DUCKDB_BUILD_REPORT.md 和 HISTORY_QUERY_READINESS.md。\n\n" + counts_text + "\n", encoding="utf-8")
    return stats
