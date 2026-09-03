from __future__ import annotations

import json
from pathlib import Path
from typing import Any

SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS sources (
  id VARCHAR PRIMARY KEY, dataset VARCHAR NOT NULL, original_id VARCHAR,
  original_url VARCHAR, snapshot_version VARCHAR, snapshot_date VARCHAR,
  dataset_version VARCHAR, source_type VARCHAR, license VARCHAR, retrieved_at VARCHAR,
  raw_file VARCHAR, raw_path VARCHAR, staging_path VARCHAR, quality VARCHAR, quality_status VARCHAR,
  commercial_use BOOLEAN, redistribution VARCHAR, attribution VARCHAR, notes VARCHAR
);
CREATE TABLE IF NOT EXISTS periods (id VARCHAR PRIMARY KEY, name_zh_cn VARCHAR NOT NULL, name_raw VARCHAR, start_year INTEGER, end_year INTEGER, date_precision VARCHAR, description_zh_cn VARCHAR, quality_status VARCHAR, source_type VARCHAR, source_reference VARCHAR, source_ids VARCHAR);
CREATE TABLE IF NOT EXISTS dynasties (id VARCHAR PRIMARY KEY, name_zh_cn VARCHAR NOT NULL, name_raw VARCHAR, start_year INTEGER, end_year INTEGER, parent_id VARCHAR, description_zh_cn VARCHAR, date_precision VARCHAR, confidence DOUBLE, source_ids VARCHAR);
CREATE TABLE IF NOT EXISTS regimes (id VARCHAR PRIMARY KEY, name_zh_cn VARCHAR NOT NULL, name_raw VARCHAR, start_year INTEGER, end_year INTEGER, date_precision VARCHAR, period_id VARCHAR, parent_regime_id VARCHAR, capital_place_id VARCHAR, parent_dynasty_id VARCHAR, description_zh_cn VARCHAR, quality_status VARCHAR, source_type VARCHAR, source_reference VARCHAR, source_ids VARCHAR);
CREATE TABLE IF NOT EXISTS people (id VARCHAR PRIMARY KEY, canonical_name_zh_cn VARCHAR NOT NULL, name_raw VARCHAR, traditional_name VARCHAR, birth_year INTEGER, death_year INTEGER, birth_precision VARCHAR, death_precision VARCHAR, gender VARCHAR, period_ids VARCHAR, regime_ids VARCHAR, dynasty_ids VARCHAR, intro_zh_cn VARCHAR, quality_status VARCHAR, created_from_source VARCHAR, search_name VARCHAR, search_aliases VARCHAR, search_text VARCHAR, pinyin VARCHAR, initials VARCHAR);
CREATE TABLE IF NOT EXISTS person_aliases (person_id VARCHAR, alias VARCHAR, alias_zh_cn VARCHAR, alias_type VARCHAR, source VARCHAR, source_id VARCHAR, external_id VARCHAR, PRIMARY KEY(person_id, alias, alias_type, source_id));
CREATE TABLE IF NOT EXISTS places (id VARCHAR PRIMARY KEY, canonical_name_zh_cn VARCHAR NOT NULL, historical_name VARCHAR, modern_name VARCHAR, longitude DOUBLE, latitude DOUBLE, geometry VARCHAR, place_type VARCHAR, valid_from INTEGER, valid_to INTEGER, parent_place_id VARCHAR, source_ids VARCHAR, source_id VARCHAR, external_id VARCHAR, quality_status VARCHAR);
CREATE TABLE IF NOT EXISTS events (id VARCHAR PRIMARY KEY, name_zh_cn VARCHAR NOT NULL, name_raw VARCHAR, event_type VARCHAR, start_year INTEGER, start_month INTEGER, start_day INTEGER, end_year INTEGER, end_month INTEGER, end_day INTEGER, date_precision VARCHAR, period_id VARCHAR, period_ids VARCHAR, dynasty_ids VARCHAR, regime_id VARCHAR, regime_ids VARCHAR, summary_zh_cn VARCHAR, background_zh_cn VARCHAR, result_zh_cn VARCHAR, importance VARCHAR, quality_status VARCHAR, source_type VARCHAR, source_reference VARCHAR, source_ids VARCHAR, search_name VARCHAR, search_text VARCHAR);
CREATE TABLE IF NOT EXISTS stories (id VARCHAR PRIMARY KEY, title_zh_cn VARCHAR NOT NULL, title_raw VARCHAR, start_year INTEGER, end_year INTEGER, summary_zh_cn VARCHAR, background_zh_cn VARCHAR, result_zh_cn VARCHAR, story_type VARCHAR, importance VARCHAR, period_id VARCHAR, period_ids VARCHAR, quality_status VARCHAR, source_type VARCHAR, source_reference VARCHAR, source_ids VARCHAR, usable BOOLEAN);
CREATE TABLE IF NOT EXISTS story_events (story_id VARCHAR, event_id VARCHAR, sequence INTEGER, role VARCHAR, importance VARCHAR, transition_text_zh_cn VARCHAR, quality_status VARCHAR, PRIMARY KEY(story_id, event_id));
CREATE TABLE IF NOT EXISTS event_relations (source_event_id VARCHAR, target_event_id VARCHAR, relation_type VARCHAR, confidence DOUBLE, description_zh_cn VARCHAR, source_type VARCHAR, source_id VARCHAR, source_ids VARCHAR, quality_status VARCHAR, PRIMARY KEY(source_event_id, target_event_id, relation_type));
CREATE TABLE IF NOT EXISTS person_relations (person_a_id VARCHAR, person_b_id VARCHAR, relation_type VARCHAR, start_year INTEGER, end_year INTEGER, description VARCHAR, source_ids VARCHAR, confidence DOUBLE, PRIMARY KEY(person_a_id, person_b_id, relation_type));
CREATE TABLE IF NOT EXISTS event_person (event_id VARCHAR, person_id VARCHAR, role VARCHAR, role_zh_cn VARCHAR, side VARCHAR, importance VARCHAR, description VARCHAR, source_type VARCHAR, source_id VARCHAR, quality_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE, link_reason VARCHAR, PRIMARY KEY(event_id, person_id, role));
CREATE TABLE IF NOT EXISTS event_place (id VARCHAR PRIMARY KEY, event_id VARCHAR NOT NULL, place_id VARCHAR, place_name_raw VARCHAR, role VARCHAR, sequence INTEGER, description VARCHAR, description_zh_cn VARCHAR, source_type VARCHAR, source_id VARCHAR, quality_status VARCHAR, link_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE, link_reason VARCHAR);
CREATE TABLE IF NOT EXISTS event_text (event_id VARCHAR, historical_text_id VARCHAR, role VARCHAR, sequence INTEGER, description_zh_cn VARCHAR, source_type VARCHAR, source_id VARCHAR, quality_status VARCHAR, source_quality_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE, link_reason VARCHAR, temporal_score DOUBLE, person_score DOUBLE, place_score DOUBLE, keyword_score DOUBLE, work_score DOUBLE, context_score DOUBLE, chapter_score DOUBLE, PRIMARY KEY(event_id, historical_text_id));
CREATE TABLE IF NOT EXISTS event_text_candidates (event_id VARCHAR, historical_text_id VARCHAR, work_title VARCHAR, chapter VARCHAR, source_quality_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE, link_reason VARCHAR, temporal_score DOUBLE, person_score DOUBLE, place_score DOUBLE, keyword_score DOUBLE, work_score DOUBLE, context_score DOUBLE, chapter_score DOUBLE, PRIMARY KEY(event_id, historical_text_id));
CREATE TABLE IF NOT EXISTS story_person (story_id VARCHAR, person_id VARCHAR, role VARCHAR, importance VARCHAR, source_type VARCHAR, source_id VARCHAR, quality_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE, link_reason VARCHAR, PRIMARY KEY(story_id, person_id));
CREATE TABLE IF NOT EXISTS story_place (id VARCHAR PRIMARY KEY, story_id VARCHAR NOT NULL, place_id VARCHAR, place_name_raw VARCHAR, role VARCHAR, importance VARCHAR, source_type VARCHAR, source_id VARCHAR, quality_status VARCHAR, link_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE, link_reason VARCHAR);
CREATE TABLE IF NOT EXISTS relation_type_dictionary (source_dataset VARCHAR, source_relation_code VARCHAR, relation_category VARCHAR, relation_name_raw VARCHAR, relation_name_zh_cn VARCHAR, inverse_relation_name_zh_cn VARCHAR, directional BOOLEAN, description_zh_cn VARCHAR, source_reference VARCHAR, quality_status VARCHAR, PRIMARY KEY(source_dataset, source_relation_code));
CREATE TABLE IF NOT EXISTS works (id VARCHAR PRIMARY KEY, title VARCHAR NOT NULL, title_raw VARCHAR, title_zh_cn VARCHAR, author_ids VARCHAR, period VARCHAR, book_type VARCHAR, description VARCHAR, source_ids VARCHAR, source_id VARCHAR, quality_status VARCHAR);
CREATE TABLE IF NOT EXISTS historical_texts (id VARCHAR PRIMARY KEY, title_zh_cn VARCHAR, book_id VARCHAR, chapter VARCHAR, section VARCHAR, original_text VARCHAR, original_simplified VARCHAR, translation_zh_cn VARCHAR, notes_zh_cn VARCHAR, intro_zh_cn VARCHAR, translation_type VARCHAR, translation_source VARCHAR, quality_status VARCHAR, source_id VARCHAR, alignment_quality VARCHAR);
CREATE TABLE IF NOT EXISTS entity_source_mapping (entity_type VARCHAR, entity_id VARCHAR, source_id VARCHAR, external_id VARCHAR, match_type VARCHAR, confidence DOUBLE, PRIMARY KEY(entity_type, entity_id, source_id, external_id));
CREATE TABLE IF NOT EXISTS fact_assertions (id VARCHAR PRIMARY KEY, subject VARCHAR, predicate VARCHAR, value VARCHAR, source_id VARCHAR, confidence DOUBLE, preferred BOOLEAN);
CREATE TABLE IF NOT EXISTS text_alignments (text_id VARCHAR, line_number INTEGER, original_text VARCHAR, translation VARCHAR, alignment_quality VARCHAR, source_id VARCHAR, PRIMARY KEY(text_id, line_number));
CREATE TABLE IF NOT EXISTS person_place (person_id VARCHAR, place_id VARCHAR, relation_type VARCHAR, start_year INTEGER, end_year INTEGER, source_id VARCHAR, external_id VARCHAR, quality_status VARCHAR, PRIMARY KEY(person_id, place_id, relation_type, source_id));
CREATE TABLE IF NOT EXISTS data_review (
  id VARCHAR PRIMARY KEY, entity_type VARCHAR NOT NULL, entity_id VARCHAR NOT NULL,
  field_name VARCHAR NOT NULL, issue_type VARCHAR NOT NULL, current_value VARCHAR,
  source_value VARCHAR, review_status VARCHAR NOT NULL, review_note VARCHAR,
  reviewed_by VARCHAR, created_at VARCHAR NOT NULL, reviewed_at VARCHAR
);
CREATE INDEX IF NOT EXISTS idx_people_name ON people(canonical_name_zh_cn);
CREATE INDEX IF NOT EXISTS idx_events_time ON events(start_year, end_year);
CREATE INDEX IF NOT EXISTS idx_event_person_person ON event_person(person_id);
CREATE INDEX IF NOT EXISTS idx_event_relations_source ON event_relations(source_event_id);
CREATE INDEX IF NOT EXISTS idx_story_events_story ON story_events(story_id, sequence);
"""

TABLES = ("sources", "periods", "dynasties", "regimes", "people", "person_aliases", "places", "events", "stories", "story_events", "event_relations", "person_relations", "event_person", "event_place", "event_text", "event_text_candidates", "story_person", "story_place", "relation_type_dictionary", "person_place", "works", "historical_texts", "entity_source_mapping", "fact_assertions", "text_alignments", "data_review")


def _value(value: Any) -> Any:
    if isinstance(value, (list, dict)):
        return json.dumps(value, ensure_ascii=False)
    return value


def build_database(path: Path, records: dict[str, list[dict[str, Any]]]) -> None:
    try:
        import duckdb
    except ImportError as exc:
        raise RuntimeError("缺少 DuckDB，请先安装 history-data-pipeline/requirements.txt") from exc
    path.parent.mkdir(parents=True, exist_ok=True)
    with duckdb.connect(str(path)) as connection:
        connection.execute(SCHEMA_SQL)
        for table in TABLES:
            rows = records.get(table, [])
            if not rows:
                continue
            columns = [column[0] for column in connection.execute(f"DESCRIBE {table}").fetchall()]
            for row in rows:
                values = [_value(row.get(column)) for column in columns]
                placeholders = ", ".join("?" for _ in columns)
                connection.execute(f"INSERT OR REPLACE INTO {table} ({', '.join(columns)}) VALUES ({placeholders})", values)


def load_records(path: Path) -> dict[str, list[dict[str, Any]]]:
    try:
        import duckdb
    except ImportError as exc:
        raise RuntimeError("缺少 DuckDB，请先安装 history-data-pipeline/requirements.txt") from exc
    with duckdb.connect(str(path), read_only=True) as connection:
        output: dict[str, list[dict[str, Any]]] = {}
        for table in TABLES:
            columns = [row[0] for row in connection.execute(f"DESCRIBE {table}").fetchall()]
            output[table] = [dict(zip(columns, row)) for row in connection.execute(f"SELECT * FROM {table}").fetchall()]
        return output
