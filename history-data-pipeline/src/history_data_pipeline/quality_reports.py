from __future__ import annotations

import json
from pathlib import Path

from .database import TABLES


def _rows(connection, sql: str, parameters=None):
    result = connection.execute(sql, parameters or [])
    columns = [item[0] for item in result.description]
    return [dict(zip(columns, row)) for row in result.fetchall()]


def _count(connection, table: str) -> int:
    return connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]


def _coverage(connection, table: str, columns: list[str]) -> dict:
    total = _count(connection, table)
    output = {"total": total}
    for column in columns:
        present = connection.execute(f"SELECT COUNT(*) FROM {table} WHERE {column} IS NOT NULL AND CAST({column} AS VARCHAR) <> ''").fetchone()[0]
        output[column] = {"count": present, "rate": round(present / total, 4) if total else 0.0}
    return output


def write_real_reports(paths) -> dict:
    import duckdb

    paths.reports.mkdir(parents=True, exist_ok=True)
    paths.exports.mkdir(parents=True, exist_ok=True)
    with duckdb.connect(str(paths.database), read_only=True) as connection:
        counts = {table: _count(connection, table) for table in TABLES}
        triple_texts = connection.execute("SELECT COUNT(*) FROM historical_texts WHERE original_text IS NOT NULL AND original_simplified IS NOT NULL AND translation_zh_cn IS NOT NULL").fetchone()[0]
        equal_translation = connection.execute("SELECT COUNT(*) FROM historical_texts WHERE translation_zh_cn IS NOT NULL AND translation_zh_cn = original_simplified").fetchone()[0]
        invalid_person_dates = connection.execute("SELECT COUNT(*) FROM people WHERE birth_year IS NOT NULL AND death_year IS NOT NULL AND birth_year > death_year").fetchone()[0]
        ctext_cbdb = connection.execute("SELECT COUNT(*) FROM entity_source_mapping WHERE source_id = 'source-ctext' AND entity_type = 'person' AND entity_id LIKE 'cbdb-person-%'").fetchone()[0]
        coverage = {"people": _coverage(connection, "people", ["canonical_name_zh_cn", "birth_year", "death_year", "gender", "created_from_source"]), "places": _coverage(connection, "places", ["canonical_name_zh_cn", "longitude", "latitude", "valid_from", "valid_to"]), "historical_texts": _coverage(connection, "historical_texts", ["original_text", "original_simplified", "translation_zh_cn", "book_id", "alignment_quality"])}
        duplicate_queries = {
            "people_same_name": "SELECT canonical_name_zh_cn AS value, COUNT(*) AS count FROM people GROUP BY canonical_name_zh_cn HAVING COUNT(*) > 1 ORDER BY count DESC, value LIMIT 100",
            "duplicate_person_external_id": "SELECT external_id, COUNT(*) AS count FROM entity_source_mapping WHERE entity_type='person' GROUP BY source_id, external_id HAVING COUNT(*) > 1 ORDER BY count DESC LIMIT 100",
            "works_same_title": "SELECT title, COUNT(*) AS count FROM works GROUP BY title HAVING COUNT(*) > 1 ORDER BY count DESC, title LIMIT 100",
            "texts_same_original": "SELECT original_text, COUNT(*) AS count FROM historical_texts WHERE original_text <> '' GROUP BY original_text HAVING COUNT(*) > 1 ORDER BY count DESC LIMIT 100",
            "places_same_name": "SELECT canonical_name_zh_cn AS value, COUNT(*) AS count FROM places GROUP BY canonical_name_zh_cn HAVING COUNT(*) > 1 ORDER BY count DESC, value LIMIT 100",
        }
        duplicate_report = {name: _rows(connection, query) for name, query in duplicate_queries.items()}
        duplicate_totals = {
            "people_same_name_rows": connection.execute("SELECT COALESCE(SUM(n), 0) FROM (SELECT COUNT(*) AS n FROM people GROUP BY canonical_name_zh_cn HAVING COUNT(*) > 1)").fetchone()[0],
            "works_same_title_rows": connection.execute("SELECT COALESCE(SUM(n), 0) FROM (SELECT COUNT(*) AS n FROM works GROUP BY title HAVING COUNT(*) > 1)").fetchone()[0],
            "texts_same_original_rows": connection.execute("SELECT COALESCE(SUM(n), 0) FROM (SELECT COUNT(*) AS n FROM historical_texts WHERE original_text <> '' GROUP BY original_text HAVING COUNT(*) > 1)").fetchone()[0],
            "places_same_name_rows": connection.execute("SELECT COALESCE(SUM(n), 0) FROM (SELECT COUNT(*) AS n FROM places GROUP BY canonical_name_zh_cn HAVING COUNT(*) > 1)").fetchone()[0],
        }
        duplicate_lines = ["# DUPLICATE_REPORT", "", "重复只报告，不自动合并；列表最多展示前 100 个重复键。", "", "## 重复行总量", ""]
        duplicate_lines.extend(f"- {name}: {value:,}" for name, value in duplicate_totals.items())
        duplicate_lines.extend(["", "## 重复键样本", ""])
        duplicate_lines.extend(f"- {name}: {len(rows)} 个样本" for name, rows in duplicate_report.items())
        (paths.reports / "DUPLICATE_REPORT.md").write_text("\n".join(duplicate_lines) + "\n", encoding="utf-8")
        gaps = {"chgis": "未进入 DuckDB：无许可的 CHGIS 仍保持手动导入策略", "wikipedia": "已保存 Raw，尚未抽取人物/事件现代介绍", "wikisource": "已保存 Raw，尚未抽取古籍章节史料", "periods": "当前为空；CBDB dynasty 不强行转换为 Period", "regimes": "当前为空；没有可靠政权 staging", "events": "当前为空；现有 staging 没有可靠 Canonical Event 表", "stories": "当前为空；没有人工审核 Story seed", "person_place_broken": connection.execute("SELECT COUNT(*) FROM person_place pp LEFT JOIN people p ON p.id=pp.person_id LEFT JOIN places pl ON pl.id=pp.place_id WHERE p.id IS NULL OR pl.id IS NULL").fetchone()[0], "person_relation_broken": connection.execute("SELECT COUNT(*) FROM person_relations pr LEFT JOIN people a ON a.id=pr.person_a_id LEFT JOIN people b ON b.id=pr.person_b_id WHERE a.id IS NULL OR b.id IS NULL").fetchone()[0], "text_work_broken": connection.execute("SELECT COUNT(*) FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id WHERE w.id IS NULL").fetchone()[0]}
        (paths.reports / "RELATION_GAPS.md").write_text("# RELATION_GAPS\n\n当前缺口均保持为空或明确标记，不猜测补齐。\n\n" + "\n".join(f"- **{key}**: {value}" for key, value in gaps.items()) + "\n", encoding="utf-8")
        samples = {"people": _rows(connection, "SELECT id, canonical_name_zh_cn, name_raw, birth_year, death_year, gender, quality_status, created_from_source FROM people WHERE canonical_name_zh_cn IN ('曹操','刘备','李世民','苏轼') ORDER BY canonical_name_zh_cn"), "historical_texts": _rows(connection, "SELECT id, title_zh_cn, book_id, chapter, original_text, original_simplified, translation_zh_cn, translation_type, alignment_quality, source_id FROM historical_texts WHERE original_text IS NOT NULL AND original_simplified IS NOT NULL AND translation_zh_cn IS NOT NULL LIMIT 20"), "sources": _rows(connection, "SELECT id, dataset, snapshot_version, original_url, license, raw_path, staging_path, quality_status FROM sources ORDER BY dataset")}
        (paths.exports / "samples").mkdir(parents=True, exist_ok=True)
        (paths.exports / "samples" / "people_samples.json").write_text(json.dumps(samples["people"], ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        (paths.exports / "samples" / "historical_text_samples.json").write_text(json.dumps(samples["historical_texts"], ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        stats = {"counts": counts, "historical_texts_with_original_simplified_translation": triple_texts, "historical_texts_translation_equals_simplified": equal_translation, "people_birth_after_death": invalid_person_dates, "ctext_cbdb_direct_person_matches": ctext_cbdb, "coverage": coverage, "source_datasets": [row["dataset"] for row in _rows(connection, "SELECT dataset FROM sources ORDER BY dataset")], "quality_notes": ["CBDB/CText/NiuTrans 来自真实 staging", "NiuTrans alignment_quality 默认为 heuristic_unverified", "未生成任何 Event/Story Mock", "Wikipedia/Wikisource 当前仅保留 Raw 快照，未伪造结构化事实"]}
        (paths.reports / "data_stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        lines = ["# DUCKDB_BUILD_REPORT", "", "## Q1-Q10", "", f"- Q1 人物：{counts['people']:,}", f"- Q2 历史地点：{counts['places']:,}", f"- Q3 Work：{counts['works']:,}", f"- Q4 HistoricalText：{counts['historical_texts']:,}", f"- Q5 同时有原文、简体、译文：{triple_texts:,}", f"- Q6 人物关系：{counts['person_relations']:,}", f"- Q7 人物—地点关系：{counts['person_place']:,}", f"- Q8 CText 与 CBDB 直接匹配人物：{ctext_cbdb:,}", "- Q9 尚未进入 Canonical DuckDB：Wikipedia/Wikisource 的结构化内容、CHGIS、Period、Regime、Event、Story。Raw 已保留的来源仍记录在 sources。", f"- Q10 最大质量问题：Event/Story 没有可靠 staging；NiuTrans 中有 {equal_translation:,} 条译文与简体原文相同，另有 {invalid_person_dates:,} 条人物年代出现 birth_year > death_year，均保留原始事实并列入复核范围。", "", "## NULL Coverage", "", "```json", json.dumps(coverage, ensure_ascii=False, indent=2), "```", "", "## Counts", ""]
        lines.extend(f"- {table}: {count:,}" for table, count in counts.items())
        (paths.reports / "DUCKDB_BUILD_REPORT.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
        (paths.reports / "DATA_REPORT.md").write_text("# China History Dataset Report\n\n本文件对应当前正式 DuckDB；详细 Q1-Q10、覆盖率与缺口见 `DUCKDB_BUILD_REPORT.md`、`RELATION_GAPS.md`。\n\n" + "\n".join(f"- {table}: {count:,}" for table, count in counts.items()) + "\n", encoding="utf-8")
        return stats
