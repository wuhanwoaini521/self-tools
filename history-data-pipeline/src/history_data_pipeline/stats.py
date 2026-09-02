from __future__ import annotations

import json
from pathlib import Path

from .validation import completeness, relation_counts


PERIOD_NAMES = ("夏", "商", "西周", "春秋", "战国", "秦", "西汉", "东汉", "三国", "晋", "南北朝", "隋", "唐", "五代十国", "宋", "辽", "西夏", "金", "元", "明", "清", "近代")


def build_stats(records: dict[str, list[dict]]) -> dict:
    stats = {"counts": {table: len(rows) for table, rows in records.items()}, "relations": dict(relation_counts(records)), "completeness": completeness(records), "coverage_by_period": {}}
    for period in PERIOD_NAMES:
        stats["coverage_by_period"][period] = {"people": 0, "events": 0, "stories": 0, "places": 0, "texts": 0, "relations": 0}
    for table, period_key in (("people", "period_ids"), ("events", "period_ids"), ("stories", "period_ids")):
        for row in records.get(table, []):
            for period_id in row.get(period_key) or []:
                period_name = next((p["name_zh_cn"] for p in records.get("periods", []) if p["id"] == period_id), period_id)
                if period_name in stats["coverage_by_period"]:
                    stats["coverage_by_period"][period_name][table] += 1
    return stats


def write_reports(records: dict[str, list[dict]], reports_dir: Path) -> dict:
    reports_dir.mkdir(parents=True, exist_ok=True)
    stats = build_stats(records)
    (reports_dir / "data_stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    counts = stats["counts"]
    lines = ["# China History Dataset Report", "", "## Counts", "", *[f"- {key}: {value:,}" for key, value in counts.items()], "", "## Coverage By Period", "", "| Period | People | Events | Stories | Places | Texts | Relations |", "|---|---:|---:|---:|---:|---:|---:|"]
    for period, values in stats["coverage_by_period"].items():
        lines.append(f"| {period} | {values['people']} | {values['events']} | {values['stories']} | {values['places']} | {values['texts']} | {values['relations']} |")
    lines += ["", "## Completeness", "", "```json", json.dumps(stats["completeness"], ensure_ascii=False, indent=2), "```", ""]
    (reports_dir / "DATA_REPORT.md").write_text("\n".join(lines), encoding="utf-8")
    return stats
