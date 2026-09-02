from __future__ import annotations

from collections import Counter
from pathlib import Path


def validate_records(records: dict[str, list[dict]]) -> list[str]:
    errors: list[str] = []
    for table in ("people", "events", "stories", "places", "historical_texts"):
        ids = [row.get("id") for row in records.get(table, [])]
        if None in ids or len(ids) != len(set(ids)):
            errors.append(f"{table}: ID 缺失或重复")
    people = {row["id"] for row in records.get("people", [])}
    events = {row["id"] for row in records.get("events", [])}
    places = {row["id"] for row in records.get("places", [])}
    stories = {row["id"] for row in records.get("stories", [])}
    for row in records.get("people", []):
        if row.get("birth_year") is not None and row.get("death_year") is not None and row["birth_year"] > row["death_year"]:
            errors.append(f"people/{row['id']}: birth_year > death_year")
    for row in records.get("events", []):
        if row.get("start_year") is not None and row.get("end_year") is not None and row["start_year"] > row["end_year"]:
            errors.append(f"events/{row['id']}: start_year > end_year")
    for row in records.get("event_person", []):
        if row.get("event_id") not in events or row.get("person_id") not in people:
            errors.append(f"event_person: 存在孤儿引用 {row}")
    for row in records.get("event_place", []):
        if row.get("event_id") not in events or row.get("place_id") not in places:
            errors.append(f"event_place: 存在孤儿引用 {row}")
    for row in records.get("event_relations", []):
        if row.get("source_event_id") not in events or row.get("target_event_id") not in events:
            errors.append(f"event_relations: 存在孤儿引用 {row}")
    sequences: dict[str, list[int]] = {}
    for row in records.get("story_events", []):
        if row.get("story_id") not in stories or row.get("event_id") not in events:
            errors.append(f"story_events: 存在孤儿引用 {row}")
        sequences.setdefault(row.get("story_id"), []).append(row.get("sequence"))
    for story_id, values in sequences.items():
        if values != sorted(values) or len(values) != len(set(values)):
            errors.append(f"story_events/{story_id}: sequence 不唯一或未排序")
    for row in records.get("historical_texts", []):
        if row.get("original_text") and row.get("original_text") == row.get("original_simplified") and any(ord(c) > 0x3000 for c in row["original_text"]):
            # 繁简相同本身可能合法；这里只拒绝把 translation 伪装成 simplified。
            pass
        if row.get("translation_zh_cn") and row.get("translation_zh_cn") == row.get("original_simplified"):
            errors.append(f"historical_texts/{row['id']}: translation_zh_cn 不得等于 original_simplified")
        if row.get("original_text") is None:
            errors.append(f"historical_texts/{row['id']}: 缺少 original_text")
    return errors


def completeness(records: dict[str, list[dict]]) -> dict:
    people = records.get("people", [])
    events = records.get("events", [])
    texts = records.get("historical_texts", [])
    def ratio(count: int, total: int) -> float:
        return round(count / total, 4) if total else 0.0
    person_ids_with_place = {row["person_id"] for row in records.get("event_person", [])}
    return {"people": {"total": len(people), "with_birth_year": ratio(sum(row.get("birth_year") is not None for row in people), len(people)), "with_death_year": ratio(sum(row.get("death_year") is not None for row in people), len(people)), "with_intro": ratio(sum(bool(row.get("intro_zh_cn")) for row in people), len(people)), "with_event_relation": ratio(len(person_ids_with_place), len(people))}, "events": {"total": len(events), "with_exact_or_year": ratio(sum(row.get("date_precision") in {"exact", "year"} for row in events), len(events)), "with_place": ratio(len({row["event_id"] for row in records.get("event_place", [])}), len(events)), "with_person": ratio(len({row["event_id"] for row in records.get("event_person", [])}), len(events)), "with_source_text": ratio(len({row.get("source_id") for row in texts if row.get("source_id")}), len(events))}}


def relation_counts(records: dict[str, list[dict]]) -> Counter:
    return Counter({"person_relations": len(records.get("person_relations", [])), "event_relations": len(records.get("event_relations", [])), "event_person": len(records.get("event_person", [])), "event_place": len(records.get("event_place", []))})


def validate_database(database: Path) -> list[str]:
    """在 DuckDB 内直接检查正式库，避免将全部数据加载到 Python 内存。"""
    import duckdb

    errors: list[str] = []
    with duckdb.connect(str(database), read_only=True) as connection:
        checks = [
            ("people", "id IS NULL OR canonical_name_zh_cn IS NULL", "存在 NULL 主键或名称"),
            ("places", "id IS NULL OR canonical_name_zh_cn IS NULL", "存在 NULL 主键或名称"),
            ("works", "id IS NULL OR title IS NULL", "存在 NULL 主键或标题"),
            ("historical_texts", "id IS NULL OR original_text IS NULL", "存在 NULL 主键或原文"),
            ("person_relations", "person_a_id IS NULL OR person_b_id IS NULL", "存在 NULL 关系端点"),
            ("person_place", "person_id IS NULL OR place_id IS NULL", "存在 NULL 关系端点"),
        ]
        for table, predicate, message in checks:
            if connection.execute(f"SELECT COUNT(*) FROM {table} WHERE {predicate}").fetchone()[0]:
                errors.append(f"{table}: {message}")
        orphan_person_relations = connection.execute("SELECT COUNT(*) FROM person_relations pr LEFT JOIN people a ON a.id=pr.person_a_id LEFT JOIN people b ON b.id=pr.person_b_id WHERE a.id IS NULL OR b.id IS NULL").fetchone()[0]
        if orphan_person_relations:
            errors.append(f"person_relations: {orphan_person_relations} 条孤儿关系")
        orphan_person_place = connection.execute("SELECT COUNT(*) FROM person_place pp LEFT JOIN people p ON p.id=pp.person_id LEFT JOIN places pl ON pl.id=pp.place_id WHERE p.id IS NULL OR pl.id IS NULL").fetchone()[0]
        if orphan_person_place:
            errors.append(f"person_place: {orphan_person_place} 条孤儿关系")
        # birth_year > death_year 是可复核的数据质量异常，不是阻断 DuckDB 读取的结构错误。
        # CLI 会先写入 data_review；Raw 与当前 Canonical 值保持不变。
    return errors
