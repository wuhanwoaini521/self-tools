from __future__ import annotations

import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path

from .database import SCHEMA_SQL
from .normalization import simplify_text


def _sql_path(path: Path) -> str:
    return str(path.resolve()).replace("'", "''")


def _metadata_sources(paths) -> list[dict]:
    rows = []
    for dataset_dir in sorted(paths.raw.iterdir()):
        if not dataset_dir.is_dir():
            continue
        snapshots = sorted(path for path in dataset_dir.iterdir() if path.is_dir())
        for snapshot in snapshots:
            metadata_path = snapshot / "metadata.json"
            if not metadata_path.exists():
                continue
            metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
            dataset = metadata.get("dataset", dataset_dir.name)
            source_id = f"source-{dataset}"
            rows.append({"id": source_id, "dataset": dataset, "original_id": metadata.get("filename"), "original_url": metadata.get("source_url"), "snapshot_version": metadata.get("version"), "snapshot_date": metadata.get("version"), "dataset_version": metadata.get("version"), "source_type": "official_snapshot", "license": metadata.get("license"), "retrieved_at": metadata.get("downloaded_at"), "raw_file": metadata.get("filename"), "raw_path": str(snapshot.relative_to(paths.root)), "staging_path": str((paths.staging / dataset).relative_to(paths.root)), "quality": "source_backed", "quality_status": "source_backed", "commercial_use": False if dataset == "ctext" else None, "redistribution": "unknown", "attribution": "required", "notes": metadata.get("notes")})
    return rows


def _source_tuple(row: dict) -> tuple:
    return tuple(row.get(column) for column in ("id", "dataset", "original_id", "original_url", "snapshot_version", "snapshot_date", "dataset_version", "source_type", "license", "retrieved_at", "raw_file", "raw_path", "staging_path", "quality", "quality_status", "commercial_use", "redistribution", "attribution", "notes"))


def _insert_json(connection, table: str, path: Path, columns: list[str], expressions: list[str], where: str = "") -> None:
    if not path.exists() or path.stat().st_size == 0:
        return
    source = f"read_json_auto('{_sql_path(path)}', records=true)"
    select = ", ".join(expressions)
    connection.execute(f"INSERT OR REPLACE INTO {table} ({', '.join(columns)}) SELECT {select} FROM {source} {where}")


def _work_title(source_path: str) -> str:
    parts = Path(source_path).parts
    if "双语数据" in parts:
        index = parts.index("双语数据")
        if index + 1 < len(parts):
            return parts[index + 1]
    return "未命名作品"


def _iter_texts(path: Path):
    with path.open(encoding="utf-8") as stream:
        for line in stream:
            row = json.loads(line)
            original = row.get("original_text") or ""
            translation = row.get("translation_zh_cn") or ""
            source_path = row.get("source_path") or ""
            line_number = int(row.get("line_number") or 0)
            title = _work_title(source_path)
            work_id = "work-niutrans-" + hashlib.sha1(title.encode("utf-8")).hexdigest()[:16]
            text_id = "text-niutrans-" + hashlib.sha1(f"{source_path}:{line_number}".encode("utf-8")).hexdigest()[:20]
            yield {"id": text_id, "title_zh_cn": title, "book_id": work_id, "chapter": Path(source_path).parent.name, "section": None, "original_text": original, "original_simplified": simplify_text(original), "translation_zh_cn": translation, "translation_type": "dataset", "translation_source": "NiuTrans Classical-Modern", "quality_status": "unverified", "source_id": "source-classical-modern", "alignment_quality": row.get("alignment_quality") or "heuristic_unverified"}


def _write_normalized_texts(source: Path, target: Path) -> int:
    target.parent.mkdir(parents=True, exist_ok=True)
    count = 0
    with target.open("w", encoding="utf-8") as stream:
        for row in _iter_texts(source):
            stream.write(json.dumps(row, ensure_ascii=False) + "\n")
            count += 1
    return count


def build_from_staging(paths) -> Path:
    import duckdb

    paths.ensure()
    target = paths.database
    building = target.with_name("history.building.duckdb")
    if building.exists():
        building.unlink()
    connection = duckdb.connect(str(building))
    try:
        connection.execute(SCHEMA_SQL)
        source_rows = _metadata_sources(paths)
        source_columns = ["id", "dataset", "original_id", "original_url", "snapshot_version", "snapshot_date", "dataset_version", "source_type", "license", "retrieved_at", "raw_file", "raw_path", "staging_path", "quality", "quality_status", "commercial_use", "redistribution", "attribution", "notes"]
        connection.executemany(f"INSERT OR REPLACE INTO sources ({', '.join(source_columns)}) VALUES ({', '.join('?' for _ in source_columns)})", [_source_tuple(row) for row in source_rows])
        cbdb = paths.staging / "cbdb"
        _insert_json(connection, "dynasties", cbdb / "dynasties.jsonl", ["id", "name_zh_cn", "name_raw", "start_year", "end_year", "date_precision", "confidence", "source_ids"], ["id", "name_zh_cn", "name_raw", "start_year", "end_year", "date_precision", "confidence", "CAST(source_ids AS VARCHAR)"])
        _insert_json(connection, "people", cbdb / "people.jsonl", ["id", "canonical_name_zh_cn", "name_raw", "birth_year", "death_year", "birth_precision", "death_precision", "gender", "period_ids", "regime_ids", "dynasty_ids", "quality_status", "created_from_source", "search_name", "search_aliases", "search_text", "pinyin", "initials"], ["id", "canonical_name_zh_cn", "name_raw", "birth_year", "death_year", "birth_precision", "death_precision", "gender", "CAST(period_ids AS VARCHAR)", "CAST(regime_ids AS VARCHAR)", "CAST(dynasty_ids AS VARCHAR)", "quality_status", "'source-cbdb'", "search_name", "search_aliases", "search_text", "pinyin", "initials"])
        _insert_json(connection, "entity_source_mapping", cbdb / "people.jsonl", ["entity_type", "entity_id", "source_id", "external_id", "match_type", "confidence"], ["'person'", "id", "'source-cbdb'", "regexp_extract(id, '[0-9]+$')", "'direct'", "1.0"])
        _insert_json(connection, "person_aliases", cbdb / "person_aliases.jsonl", ["person_id", "alias", "alias_zh_cn", "alias_type", "source", "source_id", "external_id"], ["person_id", "alias", "alias", "alias_type", "'CBDB'", "'source-cbdb'", "alias"])
        _insert_json(connection, "places", cbdb / "places.jsonl", ["id", "canonical_name_zh_cn", "historical_name", "modern_name", "longitude", "latitude", "place_type", "valid_from", "valid_to", "source_ids", "source_id", "external_id", "quality_status"], ["id", "canonical_name_zh_cn", "historical_name", "modern_name", "longitude", "latitude", "place_type", "valid_from", "valid_to", "CAST(source_ids AS VARCHAR)", "'source-cbdb'", "regexp_extract(id, '[0-9]+$')", "quality_status"])
        _insert_json(connection, "entity_source_mapping", cbdb / "places.jsonl", ["entity_type", "entity_id", "source_id", "external_id", "match_type", "confidence"], ["'place'", "id", "'source-cbdb'", "regexp_extract(id, '[0-9]+$')", "'direct'", "1.0"])
        _insert_json(connection, "person_place", cbdb / "person_place.jsonl", ["person_id", "place_id", "relation_type", "start_year", "end_year", "source_id", "external_id", "quality_status"], ["person_id", "place_id", "relation_type", "start_year", "end_year", "source_id", "external_id", "quality_status"])
        _insert_json(connection, "person_relations", cbdb / "person_relations.jsonl", ["person_a_id", "person_b_id", "relation_type", "description", "source_ids", "confidence"], ["person_a_id", "person_b_id", "relation_type", "description", "CAST(source_ids AS VARCHAR)", "confidence"], "WHERE person_a_id IN (SELECT id FROM people) AND person_b_id IN (SELECT id FROM people)")
        ctext = next(iter(sorted((paths.staging / "ctext").glob("entities.jsonl"))), None)
        if ctext:
            _insert_json(connection, "people", ctext, ["id", "canonical_name_zh_cn", "name_raw", "quality_status", "created_from_source", "search_name", "search_aliases", "search_text"], ["'ctext-person-' || external_id", "label", "label", "'source_backed'", "'source-ctext'", "label", "''", "label"], "WHERE entity_type='person' AND cbdb_external_id IS NULL")
            _insert_json(connection, "places", ctext, ["id", "canonical_name_zh_cn", "historical_name", "place_type", "source_id", "external_id", "quality_status"], ["'ctext-place-' || external_id", "label", "label", "'historical_place'", "'source-ctext'", "external_id", "'source_backed'"], "WHERE entity_type='place'")
            _insert_json(connection, "works", ctext, ["id", "title", "title_raw", "title_zh_cn", "source_ids", "source_id", "quality_status"], ["'ctext-work-' || external_id", "label", "label", "label", "'[\\\"source-ctext\\\"]'", "'source-ctext'", "'source_backed'"], "WHERE entity_type='work'")
            _insert_json(connection, "entity_source_mapping", ctext, ["entity_type", "entity_id", "source_id", "external_id", "match_type", "confidence"], ["entity_type", "CASE WHEN entity_type='person' AND cbdb_external_id IS NOT NULL THEN 'cbdb-person-' || cbdb_external_id ELSE 'ctext-' || entity_type || '-' || external_id END", "source_id", "external_id", "CASE WHEN cbdb_external_id IS NOT NULL THEN 'direct_external_id' ELSE 'source_native' END", "1.0"], "WHERE entity_type IN ('person','place','work')")
        niutrans = next(iter(sorted((paths.staging / "classical-modern").glob("*.jsonl"))), None)
        normalized_texts = paths.staging / "normalized.building" / "historical_texts.jsonl"
        text_count = _write_normalized_texts(niutrans, normalized_texts) if niutrans else 0
        if niutrans:
            _insert_json(connection, "historical_texts", normalized_texts, ["id", "title_zh_cn", "book_id", "chapter", "section", "original_text", "original_simplified", "translation_zh_cn", "translation_type", "translation_source", "quality_status", "source_id", "alignment_quality"], ["id", "title_zh_cn", "book_id", "chapter", "section", "original_text", "original_simplified", "translation_zh_cn", "translation_type", "translation_source", "quality_status", "source_id", "alignment_quality"])
            _insert_json(connection, "works", normalized_texts, ["id", "title", "title_raw", "title_zh_cn", "source_ids", "source_id", "quality_status"], ["book_id", "title_zh_cn", "title_zh_cn", "title_zh_cn", "'[\\\"source-classical-modern\\\"]'", "'source-classical-modern'", "'source_backed'"], "QUALIFY row_number() OVER (PARTITION BY book_id ORDER BY id) = 1")
        connection.execute("CHECKPOINT")
    finally:
        connection.close()
    if target.exists():
        previous = target.with_name("history.previous.duckdb")
        if previous.exists():
            stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
            archived = target.with_name(f"history.previous.{stamp}.duckdb")
            suffix = 1
            while archived.exists():
                archived = target.with_name(f"history.previous.{stamp}.{suffix}.duckdb")
                suffix += 1
            previous.replace(archived)
        target.replace(previous)
    os.replace(building, target)
    return target
