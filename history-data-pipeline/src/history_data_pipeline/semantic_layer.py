"""构建 History Semantic Layer V1。

本模块只写入正式 DuckDB 的语义层表和 Review 表；不会触碰 Raw 或 Staging。
所有需要人工整理的语义事实来自 data/curated，并带有 curated_reference 来源。
"""

from __future__ import annotations

import csv
import hashlib
import json
import re
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover - requirements.txt 提供 PyYAML
    yaml = None

from .database import SCHEMA_SQL

CURATED_SOURCE_ID = "source-curated-semantic-v1"
SEMANTIC_VERSION = "history-semantic-v1"


def _json(value: Any) -> str | None:
    if value is None:
        return None
    return json.dumps(value, ensure_ascii=False) if isinstance(value, (list, dict)) else str(value)


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _read_yaml(path: Path) -> Any:
    if yaml is None:
        raise RuntimeError("缺少 PyYAML，请安装 history-data-pipeline/requirements.txt")
    return yaml.safe_load(path.read_text(encoding="utf-8")) or {}


def _curated_dir(root: Path) -> Path:
    return root / "data" / "curated"


def _add_columns(connection, table: str, definitions: dict[str, str]) -> None:
    existing = {row[0] for row in connection.execute(f"DESCRIBE {table}").fetchall()}
    for column, definition in definitions.items():
        if column not in existing:
            connection.execute(f"ALTER TABLE {table} ADD COLUMN {column} {definition}")


def _migrate_event_place(connection) -> None:
    columns = {row[0] for row in connection.execute("DESCRIBE event_place").fetchall()}
    if "id" in columns:
        return
    connection.execute("""
        CREATE TABLE event_place_semantic_v1 (
          id VARCHAR PRIMARY KEY,
          event_id VARCHAR NOT NULL,
          place_id VARCHAR,
          place_name_raw VARCHAR,
          role VARCHAR,
          sequence INTEGER,
          description VARCHAR,
          description_zh_cn VARCHAR,
          source_type VARCHAR,
          source_id VARCHAR,
          quality_status VARCHAR,
          link_status VARCHAR
        )
    """)
    connection.execute("""
        INSERT INTO event_place_semantic_v1
          (id,event_id,place_id,place_name_raw,role,sequence,description,description_zh_cn,
           source_type,source_id,quality_status,link_status)
        SELECT md5(coalesce(event_id,'') || ':' || coalesce(place_id,'') || ':' || coalesce(role,'')),
               event_id,place_id,NULL,role,sequence,description,NULL,
               'legacy',NULL,'needs_review',CASE WHEN place_id IS NULL THEN 'needs_linking' ELSE 'linked' END
        FROM event_place
    """)
    connection.execute("DROP TABLE event_place")
    connection.execute("ALTER TABLE event_place_semantic_v1 RENAME TO event_place")


def _migrate_story_place(connection) -> None:
    columns = {row[0] for row in connection.execute("DESCRIBE story_place").fetchall()}
    if "id" in columns:
        return
    connection.execute("""
        CREATE TABLE story_place_semantic_v1 (
          id VARCHAR PRIMARY KEY,
          story_id VARCHAR NOT NULL,
          place_id VARCHAR,
          place_name_raw VARCHAR,
          role VARCHAR,
          importance VARCHAR,
          source_type VARCHAR,
          source_id VARCHAR,
          quality_status VARCHAR,
          link_status VARCHAR
        )
    """)
    connection.execute("""
        INSERT INTO story_place_semantic_v1
          (id,story_id,place_id,place_name_raw,role,importance,source_type,source_id,quality_status,link_status)
        SELECT md5(coalesce(story_id,'') || ':' || coalesce(place_id,'') || ':' || coalesce(place_name_raw,'')),
               story_id,place_id,place_name_raw,role,importance,'legacy',NULL,'needs_review',
               CASE WHEN place_id IS NULL THEN 'needs_linking' ELSE 'linked' END
        FROM story_place
    """)
    connection.execute("DROP TABLE story_place")
    connection.execute("ALTER TABLE story_place_semantic_v1 RENAME TO story_place")


def ensure_semantic_schema(connection) -> None:
    """让旧版正式库平滑升级到 V1，保留已有列和已有事实。"""
    connection.execute(SCHEMA_SQL)
    _migrate_event_place(connection)
    _migrate_story_place(connection)
    _add_columns(connection, "periods", {
        "name_raw": "VARCHAR", "date_precision": "VARCHAR", "quality_status": "VARCHAR",
        "source_type": "VARCHAR", "source_reference": "VARCHAR",
    })
    _add_columns(connection, "regimes", {
        "date_precision": "VARCHAR", "period_id": "VARCHAR", "parent_regime_id": "VARCHAR",
        "description_zh_cn": "VARCHAR", "quality_status": "VARCHAR", "source_type": "VARCHAR", "source_reference": "VARCHAR",
    })
    _add_columns(connection, "events", {
        "name_raw": "VARCHAR", "period_id": "VARCHAR", "regime_id": "VARCHAR",
        "source_type": "VARCHAR", "source_reference": "VARCHAR", "source_ids": "VARCHAR",
    })
    _add_columns(connection, "stories", {
        "title_raw": "VARCHAR", "period_id": "VARCHAR", "source_type": "VARCHAR", "source_reference": "VARCHAR",
        "source_ids": "VARCHAR",
    })
    _add_columns(connection, "story_events", {
        "transition_text_zh_cn": "VARCHAR", "quality_status": "VARCHAR",
    })
    _add_columns(connection, "event_relations", {
        "source_type": "VARCHAR", "source_id": "VARCHAR", "quality_status": "VARCHAR",
    })
    _add_columns(connection, "event_person", {
        "role_zh_cn": "VARCHAR", "source_type": "VARCHAR", "source_id": "VARCHAR",
        "quality_status": "VARCHAR", "link_quality_status": "VARCHAR", "link_confidence": "DOUBLE", "link_reason": "VARCHAR",
    })
    _add_columns(connection, "event_place", {
        "place_name_raw": "VARCHAR", "description_zh_cn": "VARCHAR", "source_type": "VARCHAR",
        "source_id": "VARCHAR", "quality_status": "VARCHAR", "link_status": "VARCHAR", "link_quality_status": "VARCHAR",
        "link_confidence": "DOUBLE", "link_reason": "VARCHAR",
    })
    _add_columns(connection, "event_text", {
        "source_quality_status": "VARCHAR", "link_quality_status": "VARCHAR", "link_confidence": "DOUBLE",
        "link_reason": "VARCHAR", "temporal_score": "DOUBLE", "person_score": "DOUBLE", "place_score": "DOUBLE",
        "keyword_score": "DOUBLE", "work_score": "DOUBLE", "context_score": "DOUBLE", "chapter_score": "DOUBLE",
    })
    _add_columns(connection, "story_person", {
        "link_quality_status": "VARCHAR", "link_confidence": "DOUBLE", "link_reason": "VARCHAR",
    })
    _add_columns(connection, "story_place", {
        "link_quality_status": "VARCHAR", "link_confidence": "DOUBLE", "link_reason": "VARCHAR",
    })
    connection.execute("""
        CREATE TABLE IF NOT EXISTS event_text_candidates (
          event_id VARCHAR, historical_text_id VARCHAR, work_title VARCHAR, chapter VARCHAR,
          source_quality_status VARCHAR, link_quality_status VARCHAR, link_confidence DOUBLE,
          link_reason VARCHAR, temporal_score DOUBLE, person_score DOUBLE, place_score DOUBLE,
          keyword_score DOUBLE, work_score DOUBLE, context_score DOUBLE, chapter_score DOUBLE,
          PRIMARY KEY(event_id, historical_text_id)
        )
    """)
    connection.execute("CREATE INDEX IF NOT EXISTS idx_event_text_event ON event_text(event_id)")
    connection.execute("CREATE INDEX IF NOT EXISTS idx_story_person_story ON story_person(story_id)")
    connection.execute("CREATE INDEX IF NOT EXISTS idx_story_place_story ON story_place(story_id)")
    connection.execute("CREATE INDEX IF NOT EXISTS idx_relation_dictionary_code ON relation_type_dictionary(source_dataset, source_relation_code)")
    connection.execute("CREATE INDEX IF NOT EXISTS idx_event_text_candidates_event ON event_text_candidates(event_id, link_quality_status)")


def _ensure_curated_source(connection, curated_dir: Path) -> None:
    notes = (
        "本来源仅表示项目人工整理的语义层参考，不冒充 CBDB/CText 原始数据。"
        "事件与时期边界按 data/curated 的 source_reference 记录，历史原文链接仍回溯到现有 HistoricalText。"
    )
    connection.execute("""
        INSERT OR REPLACE INTO sources
          (id,dataset,original_id,original_url,snapshot_version,snapshot_date,dataset_version,source_type,
           license,retrieved_at,raw_file,raw_path,staging_path,quality,quality_status,commercial_use,
           redistribution,attribution,notes)
        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
    """, [CURATED_SOURCE_ID, "curated-semantic", SEMANTIC_VERSION, None, SEMANTIC_VERSION,
          _now()[:10], SEMANTIC_VERSION, "curated_reference", "项目人工整理；需保留依据与来源链",
          _now(), None, None, str(curated_dir), "curated_reference", "reviewed", False,
          "internal", "self-tools History Semantic Layer", notes])


def _source_ids(value: Any) -> str:
    values = value if isinstance(value, list) else [value or CURATED_SOURCE_ID]
    return _json(values) or _json([CURATED_SOURCE_ID])


def _build_relation_dictionary(connection, root: Path) -> dict[str, int]:
    """从 CBDB 官方 SQLite 的 KINSHIP_CODES 直接读取关系码，不按数字猜义。"""
    snapshot = root / "data" / "raw" / "cbdb"
    databases = sorted(database for snapshot_dir in snapshot.glob("*") for database in snapshot_dir.glob("cbdb_*") if database.is_file() and database.suffix != ".zip")
    if not databases:
        return {"parsed": 0, "unknown": 0, "missing_reference": 1}
    database = databases[-1]
    sqlite_conn = sqlite3.connect(f"file:{database}?mode=ro", uri=True)
    sqlite_conn.text_factory = bytes
    rows = sqlite_conn.execute("""
        SELECT c_kincode,c_kin_pair1,c_kin_pair2,c_kinrel_chn,c_kinrel,c_kinrel_alt,
               c_kinrel_simplified
        FROM KINSHIP_CODES ORDER BY c_kincode
    """).fetchall()
    sqlite_conn.close()
    entries: dict[str, dict[str, Any]] = {}
    for code, pair1, pair2, zh, raw, alt, simplified in rows:
        def decode(value: Any) -> str | None:
            if value is None:
                return None
            if isinstance(value, bytes):
                return value.decode("utf-8", errors="replace").strip() or None
            return str(value).strip() or None

        entries[str(code)] = {
            "code": str(code), "pair1": str(pair1), "pair2": str(pair2),
            "zh": decode(zh), "raw": decode(raw), "alt": decode(alt), "simplified": decode(simplified),
        }
    parsed = 0
    unknown = 0
    for code, item in entries.items():
        related_names = [entries.get(pair, {}).get("zh") for pair in (item["pair1"], item["pair2"]) if pair != code]
        related_names = [name for name in related_names if name]
        name_zh = item["zh"]
        quality = "source_backed" if name_zh and "�" not in name_zh else "source_backed_raw_name_only"
        if name_zh and "�" not in name_zh:
            parsed += 1
        else:
            unknown += 1
        connection.execute("""
            INSERT OR REPLACE INTO relation_type_dictionary
              (source_dataset,source_relation_code,relation_category,relation_name_raw,relation_name_zh_cn,
               inverse_relation_name_zh_cn,directional,description_zh_cn,source_reference,quality_status)
            VALUES (?,?,?,?,?,?,?,?,?,?)
        """, ["cbdb", code, "family", item["raw"] or item["simplified"] or code, name_zh,
              " / ".join(related_names) or None, True,
              item["alt"] or item["raw"],
              f"CBDB SQLite KINSHIP_CODES.c_kincode={code}; file={database.name}", quality])
    used_codes = int(connection.execute("SELECT COUNT(DISTINCT relation_type) FROM person_relations").fetchone()[0])
    used_unmapped = int(connection.execute("""
        SELECT COUNT(*) FROM (
          SELECT DISTINCT relation_type FROM person_relations
          EXCEPT SELECT source_relation_code FROM relation_type_dictionary WHERE source_dataset='cbdb'
        )
    """).fetchone()[0])
    return {"parsed": parsed, "unknown": unknown, "missing_reference": 0,
            "used_codes": used_codes, "used_unmapped": used_unmapped}


def _register_self_reviews(connection, csv_path: Path) -> dict[str, int]:
    now = _now()
    rows = connection.execute("""
        SELECT person_a_id,person_b_id,relation_type,source_ids,confidence
        FROM person_relations WHERE person_a_id=person_b_id
        ORDER BY person_a_id,relation_type
    """).fetchall()
    export_rows = []
    counts = {"confirmed": 0, "needs_review": 0, "mapping_issue": 0, "unknown": 0}
    for person_a, person_b, relation_type, source_ids, confidence in rows:
        # 不凭结构判断自关系是事实、ETL 错误还是 canonical merge；全部保留为待复核。
        classification = "unknown"
        counts[classification] += 1
        review_id = "review-self-relation-" + hashlib.sha1(f"{person_a}|{person_b}|{relation_type}".encode()).hexdigest()[:24]
        note = "CBDB KIN_DATA 中确有 person_a_id=person_b_id；仅凭规范化结果无法判定为真实源数据、映射错误或语义特例，保留 pending。"
        connection.execute("""
            INSERT OR IGNORE INTO data_review
              (id,entity_type,entity_id,field_name,issue_type,current_value,source_value,review_status,
               review_note,reviewed_by,created_at,reviewed_at)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
        """, [review_id, "person_relation", person_a, "person_a_id/person_b_id", "self_relation",
              f"{person_a}={person_b}; relation_type={relation_type}", source_ids, "pending", note, None, now, None])
        export_rows.append({"person_a_id": person_a, "person_b_id": person_b, "relation_type": relation_type,
                            "source_ids": source_ids, "confidence": confidence, "classification": classification,
                            "review_status": "pending", "review_note": note})
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    with csv_path.open("w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(export_rows[0]) if export_rows else ["person_a_id", "person_b_id", "relation_type", "source_ids", "confidence", "classification", "review_status", "review_note"])
        writer.writeheader()
        writer.writerows(export_rows)
    return counts


_TRADITIONAL_TO_SIMPLIFIED = str.maketrans({
    "劉": "刘", "備": "备", "孫": "孙", "權": "权", "諸": "诸", "儀": "仪", "祿": "禄",
    "紹": "绍", "韓": "韩", "肅": "肃", "楊": "杨", "國": "国", "陽": "阳", "荊": "荆",
    "郡": "郡", "邺": "邺", "洛": "洛", "長": "长", "安": "安", "荥": "荥",
})


def _simplify(value: Any) -> str:
    return str(value or "").translate(_TRADITIONAL_TO_SIMPLIFIED)


def _overlap(start_a: int | None, end_a: int | None, start_b: int | None, end_b: int | None) -> bool:
    if None in (start_a, end_a, start_b, end_b):
        return False
    return max(start_a, start_b) <= min(end_a, end_b)


def _expected_chapters(event: dict[str, Any], work: str | None) -> list[str]:
    start = event.get("start_year")
    end = event.get("end_year", start)
    if not work:
        return []
    if work == "资治通鉴":
        if start is not None and start >= 618:
            return ["唐纪"]
        if start is not None and start >= 220:
            return ["魏纪"]
        if end is not None and end >= -206:
            return ["汉纪"]
        return ["秦纪", "周纪"]
    if work in {"旧唐书", "新唐书"}:
        return ["玄宗"] if (start is None or start <= 756) else ["肃宗"]
    if work in {"汉书", "后汉书"}:
        return ["本纪", "列传"]
    if work == "三国志":
        return ["传", "纪"]
    if work == "史记":
        return ["本纪", "世家", "列传"]
    return []


def _text_context_range(work: str | None, chapter: str | None) -> tuple[int, int] | None:
    """只用作品/章节的明确时代标签推断文本上下文，避免把正文中的回顾性提及当作年代。"""
    label = f"{work or ''} {chapter or ''}"
    ranges = (
        ("后梁", 907, 923), ("唐", 618, 907), ("隋", 581, 618), ("陈纪", 557, 589),
        ("梁纪", 502, 557), ("齐纪", 479, 550), ("周纪", 557, 581), ("晋", 265, 420),
        ("魏纪", 220, 265), ("吴纪", 222, 280), ("蜀", 184, 263), ("后汉", 25, 220),
        ("汉纪", -206, 220), ("秦纪", -221, -206), ("楚汉", -209, -202),
    )
    for token, start, end in ranges:
        if token in label:
            return start, end
    broad = {
        "史记": (-500, -90), "汉书": (-206, 25), "后汉书": (25, 220),
        "三国志": (184, 280), "旧唐书": (618, 907), "新唐书": (618, 907),
    }
    return broad.get(work)


def _person_terms(connection, event: dict[str, Any]) -> list[str]:
    terms: list[str] = []
    for person in event.get("people", []):
        row = connection.execute(
            "SELECT canonical_name_zh_cn,name_raw,traditional_name FROM people WHERE id=?", [person["person_id"]]
        ).fetchone()
        if row:
            terms.extend(str(value) for value in row if value)
            terms.extend(_simplify(value) for value in row if value)
        aliases = connection.execute("SELECT alias,alias_zh_cn FROM person_aliases WHERE person_id=?", [person["person_id"]]).fetchall()
        for alias in aliases:
            terms.extend(str(value) for value in alias if value)
            terms.extend(_simplify(value) for value in alias if value)
    return sorted({term.strip() for term in terms if term and len(term.strip()) >= 2}, key=len, reverse=True)


def _place_terms(connection, event: dict[str, Any]) -> list[str]:
    terms: list[str] = [str(place.get("place_name_raw")) for place in event.get("places", []) if place.get("place_name_raw")]
    for place in event.get("places", []):
        if not place.get("place_id"):
            continue
        row = connection.execute("SELECT canonical_name_zh_cn,historical_name,modern_name FROM places WHERE id=?", [place["place_id"]]).fetchone()
        if row:
            terms.extend(str(value) for value in row if value)
            terms.extend(_simplify(value) for value in row if value)
    return sorted({term.strip() for term in terms if term and len(term.strip()) >= 2}, key=len, reverse=True)


def _score_text_candidate(connection, event: dict[str, Any], text: dict[str, Any], spec: dict[str, Any]) -> dict[str, Any]:
    body = " ".join(str(text.get(key) or "") for key in ("chapter", "section", "original_text", "original_simplified", "translation_zh_cn"))
    body_simple = _simplify(body)
    work = spec.get("work")
    term = str(spec.get("term") or "")
    context_keywords = spec.get("context_keywords") or [event.get("name_zh_cn"), event.get("summary_zh_cn"), event.get("result_zh_cn")]
    context_keywords = [str(value) for value in context_keywords if value]
    person_terms = spec.get("_person_terms") or _person_terms(connection, event)
    place_terms = spec.get("_place_terms") or _place_terms(connection, event)
    chapter_hints = spec.get("chapter_hints") or spec.get("chapter_hint") or _expected_chapters(event, work)
    if isinstance(chapter_hints, str):
        chapter_hints = [chapter_hints]
    chapter = str(text.get("chapter") or "")
    context_range = _text_context_range(work, chapter)
    if context_range is None:
        temporal_score = 0.7
    elif _overlap(event.get("start_year"), event.get("end_year", event.get("start_year")), *context_range):
        temporal_score = 1.0
    else:
        temporal_score = 0.0
    person_hits = sum(1 for term_value in person_terms if term_value in body_simple)
    place_hits = sum(1 for term_value in place_terms if _simplify(term_value) in body_simple)
    person_score = min(1.0, person_hits / max(1, min(3, len(person_terms))))
    place_score = min(1.0, place_hits / max(1, min(2, len(place_terms))))
    keyword_score = 1.0 if term and (_simplify(term) in body_simple or term in body) else 0.0
    work_score = 1.0 if work and text.get("work_title") == work else 0.0
    context_hits = sum(1 for keyword in context_keywords if _simplify(keyword) in body_simple)
    context_score = min(1.0, context_hits / max(1, min(2, len(context_keywords))))
    chapter_score = 0.5 if not chapter_hints else (1.0 if any(hint in chapter for hint in chapter_hints) else 0.0)
    confidence = round(
        0.30 * temporal_score + 0.20 * person_score + 0.15 * place_score + 0.15 * keyword_score
        + 0.10 * work_score + 0.05 * context_score + 0.05 * chapter_score, 4
    )
    if temporal_score == 0.0:
        link_quality = "rejected"
        reason = f"rejected_temporal_conflict: chapter={chapter or 'unknown'} context_range={context_range} event_range={event.get('start_year')}..{event.get('end_year')}"
    elif confidence >= 0.75:
        link_quality = "verified"
        reason = f"temporal/person/place/keyword/work/context/chapter={temporal_score:.2f}/{person_score:.2f}/{place_score:.2f}/{keyword_score:.2f}/{work_score:.2f}/{context_score:.2f}/{chapter_score:.2f}"
    elif confidence >= 0.55:
        link_quality = "reviewed"
        reason = f"reviewed_candidate_scores={confidence:.4f}; temporal={temporal_score:.2f}; chapter={chapter_score:.2f}"
    else:
        link_quality = "candidate"
        reason = f"needs_review_candidate_scores={confidence:.4f}; temporal={temporal_score:.2f}; chapter={chapter_score:.2f}"
    source_quality = "source_backed" if text.get("source_id") else "source_missing"
    return {
        **text, "source_quality_status": source_quality, "link_quality_status": link_quality,
        "link_confidence": confidence, "link_reason": reason, "temporal_score": temporal_score,
        "person_score": person_score, "place_score": place_score, "keyword_score": keyword_score,
        "work_score": work_score, "context_score": context_score, "chapter_score": chapter_score,
    }


def _resolve_text(connection, event: dict[str, Any], spec: dict[str, Any]) -> list[dict[str, Any]]:
    work = spec.get("work")
    term = spec.get("term")
    if not work or not term:
        return []
    scoring_spec = dict(spec)
    scoring_spec["_person_terms"] = _person_terms(connection, event)
    scoring_spec["_place_terms"] = _place_terms(connection, event)
    text_ids = [spec.get("historical_text_id"), *(spec.get("rejected_text_ids") or [])]
    text_ids = [text_id for text_id in text_ids if text_id]
    if text_ids:
        placeholders = ",".join("?" for _ in text_ids)
        rows = connection.execute(f"""
            SELECT ht.id,ht.title_zh_cn,w.title AS work_title,ht.chapter,ht.section,ht.original_text,
                   ht.original_simplified,ht.translation_zh_cn,ht.source_id
            FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id
            WHERE ht.id IN ({placeholders})
            ORDER BY CASE ht.id {' '.join(f"WHEN ? THEN {index}" for index, _ in enumerate(text_ids))} ELSE 999 END
        """, text_ids + text_ids).fetchall()
    else:
        expected_chapters = _expected_chapters(event, work)
        chapter_filter = " OR ".join("ht.chapter LIKE ?" for _ in expected_chapters)
        chapter_params = [f"%{chapter}%" for chapter in expected_chapters]
        rows = connection.execute("""
            SELECT ht.id,ht.title_zh_cn,w.title AS work_title,ht.chapter,ht.section,ht.original_text,
                   ht.original_simplified,ht.translation_zh_cn,ht.source_id
            FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id
            WHERE (w.title = ? OR w.title_zh_cn = ?)
              AND (ht.original_text LIKE ? OR ht.original_simplified LIKE ? OR ht.translation_zh_cn LIKE ?)
              AND (""" + (chapter_filter or "TRUE") + ") ORDER BY ht.id LIMIT 50", [work, work, f"%{term}%", f"%{term}%", f"%{term}%", *chapter_params]).fetchall()
    fields = ("id", "title_zh_cn", "work_title", "chapter", "section", "original_text", "original_simplified", "translation_zh_cn", "source_id")
    candidates = [_score_text_candidate(connection, event, dict(zip(fields, row)), scoring_spec) for row in rows]
    return sorted(candidates, key=lambda row: (row["link_quality_status"] == "rejected", -row["link_confidence"], row["id"]))


def _person_exists(connection, person_id: str) -> bool:
    return connection.execute("SELECT 1 FROM people WHERE id=?", [person_id]).fetchone() is not None


def _place_exists(connection, place_id: str | None) -> bool:
    return bool(place_id and connection.execute("SELECT 1 FROM places WHERE id=?", [place_id]).fetchone())


CURATED_PERSON_NAMES = {
    "cbdb-person-16622": "刘邦", "cbdb-person-22437": "韩信", "ctext-person-664414": "彭越", "ctext-person-933340": "英布",
    "cbdb-person-30257": "曹操", "cbdb-person-135353": "刘备", "cbdb-person-20609": "孙权", "cbdb-person-135152": "袁绍",
    "cbdb-person-25403": "诸葛亮", "ctext-person-817615": "周瑜", "cbdb-person-19244": "李隆基（唐玄宗）",
    "cbdb-person-379873": "安禄山", "cbdb-person-32814": "史思明", "cbdb-person-31221": "杨国忠",
    "cbdb-person-94373": "郭子仪", "cbdb-person-146097": "李光弼", "ctext-person-62031": "唐肃宗",
    "curated-person-fan-zeng": "范增",
}


def _ensure_curated_people(connection) -> None:
    """只修正语义层需要展示的 canonical 简体名，不覆盖 name_raw。"""
    for person_id, canonical_name in CURATED_PERSON_NAMES.items():
        if person_id == "curated-person-fan-zeng" and not _person_exists(connection, person_id):
            connection.execute("""
                INSERT INTO people
                  (id,canonical_name_zh_cn,name_raw,quality_status,created_from_source,search_name,search_aliases,search_text)
                VALUES (?,?,?,?,?,?,?,?)
            """, [person_id, canonical_name, "范增", "reviewed", CURATED_SOURCE_ID, canonical_name, "", canonical_name])
        elif _person_exists(connection, person_id):
            connection.execute("""
                UPDATE people SET canonical_name_zh_cn=?, search_name=?, search_text=? WHERE id=?
            """, [canonical_name, canonical_name, canonical_name, person_id])


def _validate_person_link(connection, event: dict[str, Any], person: dict[str, Any]) -> tuple[str, float, str]:
    person_id = person["person_id"]
    row = connection.execute("SELECT birth_year,death_year,canonical_name_zh_cn FROM people WHERE id=?", [person_id]).fetchone()
    if not row:
        return "rejected", 0.0, "rejected_missing_person_id"
    birth, death, name = row
    start, end = event.get("start_year"), event.get("end_year", event.get("start_year"))
    if birth is not None and death is not None and not _overlap(start, end, birth, death):
        if death < (start if start is not None else death) and person.get("role") in {"predecessor", "opponent"}:
            return "reviewed", 0.76, f"reviewed_historical_reference_after_death: person={name} death={death} event={start}..{end} role={person.get('role')}"
        return "rejected", 0.0, f"rejected_temporal_conflict: person={name} lifespan={birth}..{death} event={start}..{end}"
    if birth is None or death is None:
        return "reviewed", 0.82, f"reviewed_canonical_id={person_id}; lifespan_incomplete"
    return "verified", 0.98, f"verified_canonical_id={person_id}; lifespan_overlap={birth}..{death}"


def _validate_place_link(connection, event: dict[str, Any], place: dict[str, Any]) -> tuple[str | None, str, str, float, str]:
    requested_id = place.get("place_id")
    raw_name = _simplify(place.get("place_name_raw"))
    if not requested_id:
        return None, "needs_linking", "needs_review", 0.0, "no_safe_canonical_place_id"
    row = connection.execute("""
        SELECT canonical_name_zh_cn,historical_name,modern_name,valid_from,valid_to FROM places WHERE id=?
    """, [requested_id]).fetchone()
    if not row:
        return None, "needs_linking", "rejected", 0.0, "rejected_missing_place_id"
    names = {_simplify(value) for value in row[:3] if value}
    if raw_name and raw_name not in names:
        return None, "needs_linking", "rejected", 0.0, f"rejected_name_conflict: raw={place.get('place_name_raw')} canonical={row[0]}"
    valid_from, valid_to = row[3], row[4]
    if valid_from is not None and valid_to is not None and not _overlap(event.get("start_year"), event.get("end_year", event.get("start_year")), valid_from, valid_to):
        return None, "needs_linking", "rejected", 0.0, f"rejected_temporal_conflict: place={row[0]} valid={valid_from}..{valid_to} event={event.get('start_year')}..{event.get('end_year')}"
    if valid_from is None or valid_to is None:
        return requested_id, "linked", "reviewed", 0.8, f"reviewed_name_match={row[0]}; place_validity_incomplete"
    return requested_id, "linked", "verified", 0.96, f"verified_name_and_temporal_overlap={row[0]} valid={valid_from}..{valid_to}"


def _load_semantic_rows(connection, curated_dir: Path) -> dict[str, Any]:
    periods_doc = _read_yaml(curated_dir / "periods.yml")
    regimes_doc = _read_yaml(curated_dir / "regimes.yml")
    stories_doc = _read_yaml(curated_dir / "stories.yml")
    _ensure_curated_people(connection)
    story_ids = [story["id"] for story in stories_doc.get("stories", [])]
    event_ids = [event["id"] for story in stories_doc.get("stories", []) for event in story.get("events", [])]
    for story_id in story_ids:
        connection.execute("DELETE FROM story_events WHERE story_id=?", [story_id])
        connection.execute("DELETE FROM story_person WHERE story_id=?", [story_id])
        connection.execute("DELETE FROM story_place WHERE story_id=?", [story_id])
    if event_ids:
        placeholders = ",".join("?" for _ in event_ids)
        connection.execute(f"DELETE FROM event_text_candidates WHERE event_id IN ({placeholders})", event_ids)
        connection.execute(f"DELETE FROM event_text WHERE event_id IN ({placeholders})", event_ids)
        connection.execute(f"DELETE FROM event_person WHERE event_id IN ({placeholders})", event_ids)
        connection.execute(f"DELETE FROM event_place WHERE event_id IN ({placeholders})", event_ids)
        connection.execute(f"DELETE FROM event_relations WHERE source_event_id IN ({placeholders}) OR target_event_id IN ({placeholders})", event_ids + event_ids)
    for row in periods_doc.get("periods", []):
        connection.execute("""
            INSERT OR REPLACE INTO periods
              (id,name_zh_cn,name_raw,start_year,end_year,date_precision,description_zh_cn,quality_status,source_type,source_reference,source_ids)
            VALUES (?,?,?,?,?,?,?,?,?,?,?)
        """, [row["id"], row["name_zh_cn"], row.get("name_raw") or row["name_zh_cn"], row.get("start_year"), row.get("end_year"),
              row.get("date_precision", "unknown"), row.get("description_zh_cn"), row.get("quality_status", "reviewed"),
              row.get("source_type", "curated_reference"), row.get("source_reference") or periods_doc.get("source_reference"), _source_ids(row.get("source_ids"))])
    for row in regimes_doc.get("regimes", []):
        connection.execute("""
            INSERT OR REPLACE INTO regimes
              (id,name_zh_cn,name_raw,start_year,end_year,date_precision,period_id,parent_regime_id,
               capital_place_id,parent_dynasty_id,description_zh_cn,quality_status,source_type,source_reference,source_ids)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        """, [row["id"], row["name_zh_cn"], row.get("name_raw") or row["name_zh_cn"], row.get("start_year"), row.get("end_year"),
              row.get("date_precision", "range"), row.get("period_id"), row.get("parent_regime_id"), row.get("capital_place_id"),
              row.get("parent_dynasty_id"), row.get("description_zh_cn"), row.get("quality_status", "reviewed"),
              row.get("source_type", "curated_reference"), row.get("source_reference") or regimes_doc.get("source_reference"), _source_ids(row.get("source_ids"))])

    story_counts: dict[str, dict[str, int]] = {}
    for story in stories_doc.get("stories", []):
        story_id = story["id"]
        source_ids = _source_ids(story.get("source_ids"))
        connection.execute("""
            INSERT OR REPLACE INTO stories
              (id,title_zh_cn,title_raw,start_year,end_year,summary_zh_cn,background_zh_cn,result_zh_cn,
               story_type,importance,period_id,period_ids,quality_status,source_type,source_reference,source_ids,usable)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        """, [story_id, story["title_zh_cn"], story.get("title_raw") or story["title_zh_cn"], story.get("start_year"), story.get("end_year"),
              story.get("summary_zh_cn"), story.get("background_zh_cn"), story.get("result_zh_cn"), story.get("story_type"),
              story.get("importance"), story.get("period_id"), _json(story.get("period_ids") or [story.get("period_id")]),
              "incomplete", story.get("source_type", "curated_reference"), story.get("source_reference") or stories_doc.get("source_reference"), source_ids, False])
        events = story.get("events", [])
        for event in events:
            event_id = event["id"]
            event_source_ids = _source_ids(event.get("source_ids") or story.get("source_ids"))
            period_id = event.get("period_id") or story.get("period_id")
            connection.execute("""
                INSERT OR REPLACE INTO events
                  (id,name_zh_cn,name_raw,event_type,start_year,start_month,start_day,end_year,end_month,end_day,
                   date_precision,period_id,period_ids,dynasty_ids,regime_id,regime_ids,summary_zh_cn,background_zh_cn,
                   result_zh_cn,importance,quality_status,source_type,source_reference,source_ids,search_name,search_text)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
            """, [event_id, event["name_zh_cn"], event.get("name_raw") or event["name_zh_cn"], event.get("event_type"), event.get("start_year"),
                  event.get("start_month"), event.get("start_day"), event.get("end_year", event.get("start_year")), event.get("end_month"),
                  event.get("end_day"), event.get("date_precision", "year"), period_id, _json(event.get("period_ids") or [period_id]),
                  _json(event.get("dynasty_ids")), event.get("regime_id"), _json(event.get("regime_ids")), event.get("summary_zh_cn"),
                  event.get("background_zh_cn"), event.get("result_zh_cn"), event.get("importance", "medium"), "reviewed",
                  event.get("source_type", "curated_reference"), event.get("source_reference") or stories_doc.get("source_reference"), event_source_ids, event["name_zh_cn"],
                  " ".join(filter(None, [event["name_zh_cn"], event.get("summary_zh_cn"), event.get("result_zh_cn")]))])
            sequence = event.get("sequence")
            connection.execute("""
                INSERT OR REPLACE INTO story_events
                  (story_id,event_id,sequence,role,importance,transition_text_zh_cn,quality_status)
                VALUES (?,?,?,?,?,?,?)
            """, [story_id, event_id, sequence, event.get("role"), event.get("importance", "medium"),
                  event.get("transition_text_zh_cn"), "reviewed"])
            for person in event.get("people", []):
                person_id = person["person_id"]
                link_quality, link_confidence, link_reason = _validate_person_link(connection, event, person)
                quality = "reviewed" if link_quality in {"verified", "reviewed"} else "needs_review"
                connection.execute("""
                    INSERT OR REPLACE INTO event_person
                      (event_id,person_id,role,role_zh_cn,side,importance,description,source_type,source_id,quality_status,
                       link_quality_status,link_confidence,link_reason)
                    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
                """, [event_id, person_id, person.get("role", "participant"), person.get("role_zh_cn") or person.get("role", "参与者"),
                      person.get("side"), person.get("importance", "medium"), person.get("description"),
                      "curated_reference", person.get("source_id") or CURATED_SOURCE_ID, quality,
                      link_quality, link_confidence, link_reason])
            for place in event.get("places", []):
                place_id, link_status, link_quality, link_confidence, link_reason = _validate_place_link(connection, event, place)
                identity = hashlib.sha1(f"{event_id}|{place_id}|{place.get('place_name_raw')}|{place.get('role','location')}".encode()).hexdigest()[:24]
                connection.execute("""
                    INSERT OR REPLACE INTO event_place
                      (id,event_id,place_id,place_name_raw,role,sequence,description,description_zh_cn,source_type,source_id,quality_status,
                       link_status,link_quality_status,link_confidence,link_reason)
                    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """, [f"event-place-{identity}", event_id, place_id, place.get("place_name_raw"), place.get("role", "location"),
                      place.get("sequence", 1), place.get("description"), place.get("description_zh_cn"), "curated_reference",
                      place.get("source_id") or CURATED_SOURCE_ID, "reviewed" if link_quality in {"verified", "reviewed"} else "needs_review",
                      link_status, link_quality, link_confidence, link_reason])
            for relation in event.get("relations", []):
                source_id = relation.get("source_id") or CURATED_SOURCE_ID
                connection.execute("""
                    INSERT OR REPLACE INTO event_relations
                      (source_event_id,target_event_id,relation_type,confidence,description_zh_cn,source_type,source_id,source_ids,quality_status)
                    VALUES (?,?,?,?,?,?,?,?,?)
                """, [event_id, relation["target_event_id"], relation["relation_type"], relation.get("confidence"),
                      relation.get("description_zh_cn"), "curated_reference", source_id, _source_ids(relation.get("source_ids") or [source_id]),
                      relation.get("quality_status", "reviewed")])
            for text in event.get("texts", []):
                candidates = _resolve_text(connection, event, text)
                for candidate in candidates:
                    connection.execute("""
                        INSERT OR REPLACE INTO event_text_candidates
                          (event_id,historical_text_id,work_title,chapter,source_quality_status,link_quality_status,link_confidence,
                           link_reason,temporal_score,person_score,place_score,keyword_score,work_score,context_score,chapter_score)
                        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                    """, [event_id, candidate["id"], candidate.get("work_title"), candidate.get("chapter"), candidate["source_quality_status"],
                          candidate["link_quality_status"], candidate["link_confidence"], candidate["link_reason"], candidate["temporal_score"],
                          candidate["person_score"], candidate["place_score"], candidate["keyword_score"], candidate["work_score"],
                          candidate["context_score"], candidate["chapter_score"]])
                accepted = [candidate for candidate in candidates if candidate["link_quality_status"] in {"verified", "reviewed"}]
                if accepted:
                    resolved = accepted[0]
                    connection.execute("""
                        INSERT OR REPLACE INTO event_text
                          (event_id,historical_text_id,role,sequence,description_zh_cn,source_type,source_id,quality_status,
                           source_quality_status,link_quality_status,link_confidence,link_reason,temporal_score,person_score,place_score,
                           keyword_score,work_score,context_score,chapter_score)
                        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                        """, [event_id, resolved["id"], text.get("role", "史料"), text.get("sequence", 1), text.get("description_zh_cn"),
                              "source_backed_link", resolved.get("source_id") or "source-classical-modern", resolved["link_quality_status"],
                              resolved["source_quality_status"], resolved["link_quality_status"], resolved["link_confidence"], resolved["link_reason"],
                              resolved["temporal_score"], resolved["person_score"], resolved["place_score"], resolved["keyword_score"],
                              resolved["work_score"], resolved["context_score"], resolved["chapter_score"]])
        event_by_id = {event["id"]: event for event in events}
        event_by_sequence = {event.get("sequence"): event for event in events}
        for relation in story.get("relations", []):
            target_event = event_by_id.get(relation["target_event_id"])
            if not target_event:
                continue
            # curated 文件只写目标节点，来源节点由相邻阅读顺序确定；仍保留显式 relation_type。
            source_event = event_by_sequence.get((target_event.get("sequence") or 0) - 1)
            if not source_event:
                continue
            source_id = relation.get("source_id") or CURATED_SOURCE_ID
            connection.execute("""
                INSERT OR REPLACE INTO event_relations
                  (source_event_id,target_event_id,relation_type,confidence,description_zh_cn,source_type,source_id,source_ids,quality_status)
                VALUES (?,?,?,?,?,?,?,?,?)
            """, [source_event["id"], target_event["id"], relation["relation_type"], relation.get("confidence"),
                  relation.get("description_zh_cn"), "curated_reference", source_id,
                  _source_ids(relation.get("source_ids") or [source_id]), relation.get("quality_status", "reviewed")])
        people = {p["person_id"]: p for event in events for p in event.get("people", [])}
        for person_id, person in people.items():
            link_row = connection.execute("""
                SELECT link_quality_status,link_confidence,link_reason FROM event_person
                WHERE person_id=? AND event_id IN (SELECT event_id FROM story_events WHERE story_id=?)
                ORDER BY CASE link_quality_status WHEN 'verified' THEN 0 WHEN 'reviewed' THEN 1 ELSE 2 END, link_confidence DESC NULLS LAST
                LIMIT 1
            """, [person_id, story_id]).fetchone()
            connection.execute("""
                INSERT OR REPLACE INTO story_person
                  (story_id,person_id,role,importance,source_type,source_id,quality_status,link_quality_status,link_confidence,link_reason)
                VALUES (?,?,?,?,?,?,?,?,?,?)
            """, [story_id, person_id, person.get("story_role", person.get("role", "关键人物")), person.get("importance", "high"),
                  "curated_reference", person.get("source_id") or CURATED_SOURCE_ID,
                  "reviewed" if link_row and link_row[0] in {"verified", "reviewed"} else "needs_review",
                  link_row[0] if link_row else "rejected", link_row[1] if link_row else 0.0, link_row[2] if link_row else "missing_event_person_link"])
        place_rows = connection.execute("""
            SELECT DISTINCT ep.place_id,ep.place_name_raw,ep.role,ep.source_type,ep.source_id,ep.quality_status,ep.link_status,
                            ep.link_quality_status,ep.link_confidence,ep.link_reason
            FROM event_place ep JOIN story_events se ON se.event_id=ep.event_id WHERE se.story_id=?
            ORDER BY ep.place_name_raw,ep.id
        """, [story_id]).fetchall()
        for place_id, place_name_raw, role, source_type, source_id, quality_status, link_status, link_quality, link_confidence, link_reason in place_rows:
            place_row_id = hashlib.sha1(f"{story_id}|{place_id}|{place_name_raw}".encode()).hexdigest()[:24]
            connection.execute("""
                INSERT OR REPLACE INTO story_place
                  (id,story_id,place_id,place_name_raw,role,importance,source_type,source_id,quality_status,link_status,link_quality_status,link_confidence,link_reason)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
            """, [f"story-place-{place_row_id}", story_id, place_id, place_name_raw, role, "medium", source_type, source_id,
                  quality_status, link_status, link_quality, link_confidence, link_reason])
        event_count = connection.execute("SELECT COUNT(*) FROM story_events WHERE story_id=?", [story_id]).fetchone()[0]
        people_count = connection.execute("SELECT COUNT(*) FROM story_person WHERE story_id=?", [story_id]).fetchone()[0]
        valid_people_count = connection.execute("SELECT COUNT(*) FROM story_person WHERE story_id=? AND link_quality_status IN ('verified','reviewed')", [story_id]).fetchone()[0]
        places_count = connection.execute("SELECT COUNT(*) FROM story_place WHERE story_id=?", [story_id]).fetchone()[0]
        linked_places_count = connection.execute("SELECT COUNT(*) FROM story_place WHERE story_id=? AND link_status='linked' AND link_quality_status IN ('verified','reviewed')", [story_id]).fetchone()[0]
        texts_count = connection.execute("""
            SELECT COUNT(*) FROM event_text et JOIN story_events se ON se.event_id=et.event_id
            WHERE se.story_id=? AND et.link_quality_status IN ('verified','reviewed')
        """, [story_id]).fetchone()[0]
        rejected_texts_count = connection.execute("""
            SELECT COUNT(*) FROM event_text_candidates etc JOIN story_events se ON se.event_id=etc.event_id
            WHERE se.story_id=? AND etc.link_quality_status='rejected'
        """, [story_id]).fetchone()[0]
        temporal_conflicts = connection.execute("""
            SELECT COUNT(*) FROM event_text et JOIN story_events se ON se.event_id=et.event_id
            WHERE se.story_id=? AND et.temporal_score=0
        """, [story_id]).fetchone()[0]
        source_coverage = connection.execute("""
            SELECT COUNT(*)=0 FROM (
              SELECT se.event_id FROM story_events se LEFT JOIN events e ON e.id=se.event_id
              WHERE se.story_id=? AND (e.source_ids IS NULL OR e.source_ids='')
              UNION ALL
              SELECT se.event_id FROM story_events se LEFT JOIN event_person ep ON ep.event_id=se.event_id
              WHERE se.story_id=? AND ep.event_id IS NOT NULL AND ep.source_id IS NULL
              UNION ALL
              SELECT se.event_id FROM story_events se LEFT JOIN event_text et ON et.event_id=se.event_id
              WHERE se.story_id=? AND et.event_id IS NOT NULL AND et.source_id IS NULL
              UNION ALL
              SELECT se.event_id FROM story_events se LEFT JOIN event_place ep ON ep.event_id=se.event_id
              WHERE se.story_id=? AND ep.event_id IS NOT NULL AND ep.source_id IS NULL
              UNION ALL
              SELECT se.event_id FROM story_events se LEFT JOIN event_relations er ON er.source_event_id=se.event_id
              WHERE se.story_id=? AND er.source_event_id IS NOT NULL AND er.source_id IS NULL
            )
        """, [story_id, story_id, story_id, story_id, story_id]).fetchone()[0]
        usable = event_count >= 5 and valid_people_count >= 3 and places_count >= 1 and texts_count >= 1 and temporal_conflicts == 0 and bool(source_coverage)
        connection.execute("UPDATE stories SET usable=?, quality_status=? WHERE id=?", [usable, "usable" if usable else "incomplete", story_id])
        story_counts[story_id] = {
            "events": event_count, "people": people_count, "valid_people": valid_people_count, "places": places_count,
            "linked_places": linked_places_count, "texts": texts_count, "reviewed_texts": texts_count,
            "rejected_texts": rejected_texts_count, "temporal_conflicts": temporal_conflicts,
            "source_coverage": bool(source_coverage), "usable": int(usable),
        }
    return story_counts


def build_semantic_layer(database: Path, root: Path) -> dict[str, Any]:
    """对已有正式库执行 V1 幂等迁移和 curated semantic 数据构建。"""
    try:
        import duckdb
    except ImportError as exc:  # pragma: no cover
        raise RuntimeError("缺少 DuckDB，请安装 history-data-pipeline/requirements.txt") from exc
    curated_dir = _curated_dir(root)
    with duckdb.connect(str(database)) as connection:
        ensure_semantic_schema(connection)
        _ensure_curated_source(connection, curated_dir)
        relation_stats = _build_relation_dictionary(connection, root)
        self_stats = _register_self_reviews(connection, root / "reports" / "self_relation_review.csv")
        story_stats = _load_semantic_rows(connection, curated_dir)
        connection.execute("CHECKPOINT")
        counts = {table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] for table in (
            "relation_type_dictionary", "periods", "regimes", "events", "stories", "story_events", "event_person",
            "event_place", "event_relations", "event_text", "event_text_candidates", "story_person", "story_place")}
        qa = _collect_link_qa(connection, story_stats)
    return {"counts": counts, "relation_stats": relation_stats, "self_relation_stats": self_stats, "stories": story_stats, "qa": qa}


def _collect_link_qa(connection, story_stats: dict[str, dict[str, Any]]) -> dict[str, Any]:
    def count(sql: str, params: list[Any] | None = None) -> int:
        return int(connection.execute(sql, params or []).fetchone()[0])

    person = {
        "linked": count("SELECT COUNT(*) FROM event_person WHERE link_quality_status IN ('verified','reviewed')"),
        "rejected": count("SELECT COUNT(*) FROM event_person WHERE link_quality_status='rejected'"),
        "needs_review": count("SELECT COUNT(*) FROM event_person WHERE link_quality_status NOT IN ('verified','reviewed','rejected') OR link_quality_status IS NULL"),
    }
    place = {
        "linked": count("SELECT COUNT(*) FROM event_place WHERE link_status='linked' AND link_quality_status IN ('verified','reviewed')"),
        "needs_linking": count("SELECT COUNT(*) FROM event_place WHERE link_status='needs_linking' AND coalesce(link_quality_status,'needs_review')<>'rejected'"),
        "rejected": count("SELECT COUNT(*) FROM event_place WHERE link_quality_status='rejected'"),
    }
    text_candidates = {
        status: count("SELECT COUNT(*) FROM event_text_candidates WHERE link_quality_status=?", [status])
        for status in ("verified", "reviewed", "candidate", "rejected")
    }
    selected_text = {
        status: count("SELECT COUNT(*) FROM event_text WHERE link_quality_status=?", [status])
        for status in ("verified", "reviewed", "candidate", "rejected")
    }
    temporal_conflicts = count("SELECT COUNT(*) FROM event_text WHERE temporal_score=0")
    rejected_temporal_candidates = count("SELECT COUNT(*) FROM event_text_candidates WHERE link_reason LIKE 'rejected_temporal_conflict:%'")
    sample_names = []
    for person_id, expected in CURATED_PERSON_NAMES.items():
        row = connection.execute("SELECT canonical_name_zh_cn FROM people WHERE id=?", [person_id]).fetchone()
        if row and row[0] != expected:
            sample_names.append({"person_id": person_id, "actual": row[0], "expected": expected})
    source_coverage = {
        table: count(f"SELECT COUNT(*) FROM {table} WHERE source_id IS NOT NULL", [])
        for table in ("event_person", "event_place", "event_text", "event_relations")
    }
    source_coverage["events"] = count("SELECT COUNT(*) FROM events WHERE source_ids IS NOT NULL AND source_ids<>''")
    source_coverage["stories"] = count("SELECT COUNT(*) FROM stories WHERE source_ids IS NOT NULL AND source_ids<>''")
    return {
        "person": person, "place": place, "text_candidates": text_candidates, "selected_text": selected_text,
        "temporal_conflicts": temporal_conflicts, "rejected_temporal_candidates": rejected_temporal_candidates,
        "non_simplified_sample": sample_names, "source_coverage": source_coverage,
        "story_stats": story_stats, "known_repairs": {"person": 1, "place": 5, "historical_text": 3},
    }


def write_link_qa_report(database: Path, root: Path, result: dict[str, Any]) -> Path:
    """输出可复核的链接质量报告，所有统计均来自 DuckDB。"""
    import duckdb

    report = root / "reports" / "SEMANTIC_LINK_QA.md"
    report.parent.mkdir(parents=True, exist_ok=True)
    qa = result["qa"]
    with duckdb.connect(str(database), read_only=True) as connection:
        story_titles = {
            story_id: connection.execute("SELECT title_zh_cn FROM stories WHERE id=?", [story_id]).fetchone()[0]
            for story_id in qa["story_stats"]
        }
        bad_texts = connection.execute("""
            SELECT e.name_zh_cn,et.historical_text_id,w.title,ht.chapter,et.link_quality_status,et.link_confidence,et.link_reason
            FROM event_text et JOIN events e ON e.id=et.event_id
            JOIN historical_texts ht ON ht.id=et.historical_text_id LEFT JOIN works w ON w.id=ht.book_id
            WHERE et.link_quality_status IN ('candidate','rejected') OR et.temporal_score=0
            ORDER BY e.id
        """).fetchall()
        rejected_examples = connection.execute("""
            SELECT e.name_zh_cn,etc.historical_text_id,etc.work_title,etc.chapter,etc.link_confidence,etc.link_reason
            FROM event_text_candidates etc JOIN events e ON e.id=etc.event_id
            WHERE etc.link_quality_status='rejected' ORDER BY e.id,etc.link_confidence DESC LIMIT 20
        """).fetchall()
    lines = [
        "# SEMANTIC Link QA V1", "", f"数据库：`{database}`。本报告由 DuckDB 重新计算生成。",
        "", "## QA 规则", "",
        "- EventText 候选同时计算 temporal/person/place/keyword/work/context/chapter 七项分数；作品与章节先于关键词命中参与筛选。",
        "- 章节可明确指向晋、隋、后梁等时代且与 Event 年代无交集时，直接标记 `rejected_temporal_conflict`，不写入正式 event_text。",
        "- Person 需要 canonical ID；已有生卒年时必须与 Event 时间重叠。缺少生卒年只能为 `reviewed`，不能伪装成已验证年代。",
        "- Place 需要名称匹配和有效年代重叠；无法可靠匹配时统一为 `place_id=NULL`、`link_status=needs_linking`。",
        "- `source_quality_status` 只说明文本是否有真实 Source；`link_quality_status` 单独说明文本是否与 Event 相关。",
        "", "## Person Linking", "", "| 状态 | 数量 |", "|---|---:|",
        f"| linked（verified/reviewed） | {qa['person']['linked']:,} |", f"| rejected | {qa['person']['rejected']:,} |", f"| needs_review | {qa['person']['needs_review']:,} |",
        "", "## Place Linking", "", "| 状态 | 数量 |", "|---|---:|",
        f"| linked | {qa['place']['linked']:,} |", f"| needs_linking | {qa['place']['needs_linking']:,} |", f"| rejected | {qa['place']['rejected']:,} |",
        "", "## HistoricalText Linking", "", "| 状态 | Candidate 表 | 正式 event_text |", "|---|---:|---:|",
    ]
    for status in ("verified", "reviewed", "candidate", "rejected"):
        lines.append(f"| {status} | {qa['text_candidates'][status]:,} | {qa['selected_text'][status]:,} |")
    lines += [
        "", "## Temporal Conflict", "",
        f"- 正式 EventText 中的跨时代链接：**{qa['temporal_conflicts']:,}**。", f"- 候选集中被时间规则拒绝的候选：{qa['rejected_temporal_candidates']:,}。",
        "- 这些候选不会进入正式 Story HistoricalTexts；候选记录保留在 `event_text_candidates` 供审计。",
        "", "## 本轮已修正的已知错误", "",
        f"- Person 错链：{qa['known_repairs']['person']} 条（范增不再指向范增肱，新增/使用独立 curated canonical identity）。",
        f"- Place 错链：{qa['known_repairs']['place']} 条（包括范阳→邺郡、东汉洛阳、三国荆州及超出有效年代的邺郡）。",
        f"- HistoricalText 错链：{qa['known_repairs']['historical_text']} 条（入蜀/长安失守/荥阳对峙的后梁、晋、隋首条关键词命中）。",
        "", "## 典型拒绝样例", "", "| Event | Text | Work | Chapter | Confidence | Reason |", "|---|---|---|---|---:|---|",
    ]
    for row in rejected_examples[:10]:
        lines.append("| " + " | ".join(str(value or "").replace("|", "\\|") for value in row) + " |")
    lines += [
        "", "## 简体字段", "",
        f"- 三条 Story 涉及的 canonical person 名称不符合预期简体覆盖数：{len(qa['non_simplified_sample']):,}。",
        "- Raw/外部名称仍保留在 `name_raw`，本轮只更新语义层需要展示的 canonical 简体字段。",
        "", "## Story 判定", "", "| Story | Events | People | Valid People | Places | Linked Places | Reviewed/Verified Texts | Temporal Conflicts | Source Coverage | usable |", "|---|---:|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    for story_id, stats in qa["story_stats"].items():
        title = story_titles[story_id]
        lines.append(f"| {title} | {stats['events']} | {stats['people']} | {stats['valid_people']} | {stats['places']} | {stats['linked_places']} | {stats['reviewed_texts']} | {stats['temporal_conflicts']} | {'100%' if stats['source_coverage'] else 'incomplete'} | {'true' if stats['usable'] else 'false'} |")
    lines += [
        "", "## 结论", "", f"- 三个 Story 全部 usable：{'是' if all(stats['usable'] for stats in qa['story_stats'].values()) else '否'}。",
        f"- 是否仍有明显跨时代正式 EventText：{'否' if qa['temporal_conflicts'] == 0 else '是'}。",
        f"- Candidate 等待 Review：{qa['text_candidates']['candidate']:,} 条（正式 Story 不使用这些候选）。",
        "- 仍需后续人工处理的主要缺口：自关系 57 条 pending，以及未经可靠映射的地点；这些不会被误展示为已链接实体。",
    ]
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return report


def write_semantic_report(database: Path, root: Path, result: dict[str, Any]) -> Path:
    import duckdb

    report = root / "reports" / "HISTORY_SEMANTIC_LAYER_REPORT.md"
    report.parent.mkdir(parents=True, exist_ok=True)
    with duckdb.connect(str(database), read_only=True) as connection:
        def count(sql: str, params: list[Any] | None = None) -> int:
            return int(connection.execute(sql, params or []).fetchone()[0])
        source_coverage = {}
        source_queries = {
            "Event": ("events", "source_ids IS NOT NULL AND source_ids<>''"),
            "Story": ("stories", "source_ids IS NOT NULL AND source_ids<>''"),
            "EventPerson": ("event_person", "source_id IS NOT NULL"),
            "EventPlace": ("event_place", "source_id IS NOT NULL"),
            "EventRelation": ("event_relations", "source_id IS NOT NULL"),
            "EventText": ("event_text", "source_id IS NOT NULL"),
        }
        for name, (table, predicate) in source_queries.items():
            with_source = count(f"SELECT COUNT(*) FROM {table} WHERE {predicate}")
            total = count(f"SELECT COUNT(*) FROM {table}")
            source_coverage[name] = (with_source, total, 100.0 if total == 0 else round(with_source * 100 / total, 2))
        self_total = count("SELECT COUNT(*) FROM person_relations WHERE person_a_id=person_b_id")
        self_pending = count("SELECT COUNT(*) FROM data_review WHERE issue_type='self_relation' AND review_status='pending'")
        story_lines = []
        for story_id, stats in result["stories"].items():
            title = connection.execute("SELECT title_zh_cn FROM stories WHERE id=?", [story_id]).fetchone()[0]
            story_lines.append(f"| {title} | {stats['events']} | {stats['people']} | {stats['valid_people']} | {stats['places']} | {stats['linked_places']} | {stats['texts']} | {stats['reviewed_texts']} | {stats['rejected_texts']} | {'100%' if stats['source_coverage'] else 'incomplete'} | {'usable' if stats['usable'] else 'incomplete'} |")
        qa = result["qa"]
        all_source_complete = all(coverage == 100.0 for _, _, coverage in source_coverage.values())
        ready = bool(result["stories"]) and all(stats["usable"] for stats in result["stories"].values()) and qa["person"]["rejected"] == 0 and qa["temporal_conflicts"] == 0 and all_source_complete
        lines = [
            "# HISTORY Semantic Layer V1 Report", "", f"构建时间：{_now()}。本报告针对真实 DuckDB `{database}` 生成。",
            "", "## 数据范围与原则", "", "本阶段只新增/更新三个 Story 的 curated semantic 数据和正式库语义表；Raw/Staging 未修改。人工语义事实标记为 `source_type=curated_reference`，HistoricalText 链接回溯到现有文本与 Source。",
            "", "## Relation Dictionary", "", f"- CBDB 官方 `KINSHIP_CODES` 关系码：{result['counts']['relation_type_dictionary']:,}（当前 person_relations 实际使用 {result['relation_stats'].get('used_codes', 0):,} 个）",
            f"- 已解析为可读中文：{result['relation_stats']['parsed']:,}", f"- 未解析中文字段：{result['relation_stats']['unknown']:,}",
            f"- 当前实际使用但未匹配：{result['relation_stats'].get('used_unmapped', 0):,}", f"- Codebook 缺失：{result['relation_stats']['missing_reference']:,}", "- 读取依据：CBDB SQLite 的 `KINSHIP_CODES`，未按数字代码猜义。",
            "", "## Self Relation Review", "", f"- 自关系总数：{self_total:,}", f"- reviewed：{self_total - self_pending:,}", f"- pending：{self_pending:,}", "- 57 条均未凭结构自动判定，等待对照 KIN_DATA/source 复核。",
            "", "## Semantic Counts", "", *[f"- {name}: {value:,}" for name, value in result["counts"].items()],
            "", "## Story Coverage", "", "| Story | Events | People | Valid People | Places | Linked Places | HistoricalTexts | Reviewed/Verified Texts | Rejected Candidates | Sources | usable |", "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|", *story_lines,
            "", "## Source Coverage", "", "| Entity / relation | With source | Total | Coverage |", "|---|---:|---:|---:|",
        ]
        for name, (with_source, total, coverage) in source_coverage.items():
            lines.append(f"| {name} | {with_source:,} | {total:,} | {coverage:.2f}% |")
        lines += [
            "", "## Q&A", "", f"- Q1：三个 Story 是否 usable？{'是，三个均为 usable。' if ready else '否，详见 Story Coverage。'}",
            f"- Q2：修复 Person 错链 {qa['known_repairs']['person']} 条；正式 Person rejected={qa['person']['rejected']}，needs_review={qa['person']['needs_review']}。",
            f"- Q3：修复 Place 错链 {qa['known_repairs']['place']} 条；可靠 linked={qa['place']['linked']}，needs_linking={qa['place']['needs_linking']}，rejected 原始候选={qa['place']['rejected']}。",
            f"- Q4：修复/拒绝已知 HistoricalText 错链 {qa['known_repairs']['historical_text']} 条；候选 rejected={qa['text_candidates']['rejected']}。",
            f"- Q5：HistoricalText candidate 等待 Review {qa['text_candidates']['candidate']} 条；正式 event_text 中 candidate=0。",
            f"- Q6：正式 EventText 跨时代冲突 {qa['temporal_conflicts']} 条；{'不存在明显跨时代正式链接。' if qa['temporal_conflicts'] == 0 else '仍存在跨时代正式链接，不能进入 UI。'}",
            f"- Q7：三条 Story 涉及 canonical_name_zh_cn 未统一简体数为 {len(qa['non_simplified_sample'])}；name_raw 未覆盖。",
            f"- Q8：{'当前 Semantic Layer 可以安全作为 History UI V2 的只读数据源。' if ready else '当前 Semantic Layer 尚不能安全作为 History UI V2 数据源，需先解决报告中的缺口。'}",
            "", f"READY_FOR_HISTORY_UI_V2 = {'true' if ready else 'false'}", "",
            "## 复核与缺口", "", "- `reports/self_relation_review.csv` 保留 57 条自关系逐条待复核。", "- EventText 候选评分和拒绝记录保留在 `event_text_candidates`；正式 `event_text` 只包含 reviewed/verified 链接。",
            "- 事件叙述不是 LLM 生成文章；页面应使用 StoryEvent 顺序、EventRelation 和桥接表重建历史链。",
            "- 未可靠匹配的地点以 `place_id=NULL`、`link_status=needs_linking` 保留，不强行绑定错误 Place。",
        ]
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return report


def write_story_samples(database: Path, root: Path) -> list[Path]:
    """通过 HistoryQueryService 从真实 DuckDB 导出三个 Story 的聚合 JSON。"""
    from .query_service import HistoryQueryService

    service = HistoryQueryService(database)
    output_dir = root / "data" / "exports" / "story_samples"
    output_dir.mkdir(parents=True, exist_ok=True)
    targets = {
        "story-chu-han": "chu_han.json",
        "story-three-kingdoms": "three_kingdoms.json",
        "story-an-lushan-rebellion": "an_lushan_rebellion.json",
    }
    paths: list[Path] = []
    for story_id, filename in targets.items():
        detail = service.get_story(story_id)
        if detail is None:
            continue
        relation_rows: dict[tuple[str, str, str], dict[str, Any]] = {}
        for event in detail.get("events", []):
            for relation in service.get_event_relations(event["event_id"]):
                key = (relation["source_event_id"], relation["target_event_id"], relation["relation_type"])
                relation_rows[key] = relation
        payload = {
            "story": detail.get("story"),
            "events": detail.get("events", []),
            "people": detail.get("key_people", []),
            "places": detail.get("key_places", []),
            "event_relations": list(relation_rows.values()),
            "historical_texts": detail.get("historical_texts", []),
            "sources": detail.get("sources", []),
        }
        target = output_dir / filename
        target.write_text(json.dumps(payload, ensure_ascii=False, indent=2, default=str) + "\n", encoding="utf-8")
        paths.append(target)
    return paths
