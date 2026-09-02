from __future__ import annotations

import csv
from pathlib import Path

try:
    from opencc import OpenCC
    _OPENCC = OpenCC("t2s")
except ImportError:
    _OPENCC = None


FALLBACK_TRADITIONAL = str.maketrans({"項": "项", "漢": "汉", "書": "书", "學": "学", "國": "国", "後": "后", "體": "体", "關": "关", "戰": "战", "與": "与", "為": "为", "會": "会", "說": "说", "時": "时", "從": "从", "見": "见", "號": "号", "來": "来", "東": "东", "門": "门", "劉": "刘", "曹": "曹", "雲": "云"})


def simplify_text(original_text: str, overrides: dict[str, str] | None = None) -> str:
    """只产生派生字段，绝不修改 original_text。"""
    value = _OPENCC.convert(original_text) if _OPENCC else original_text.translate(FALLBACK_TRADITIONAL)
    for source, target in (overrides or {}).items():
        value = value.replace(source, target)
    return value


def load_overrides(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    with path.open(encoding="utf-8-sig", newline="") as stream:
        return {row["original"]: row["replacement"] for row in csv.DictReader(stream) if row.get("original")}


def normalize_historical_text(record: dict, overrides: dict[str, str] | None = None) -> dict:
    normalized = dict(record)
    original = record.get("original_text") or ""
    normalized["original_text"] = original
    normalized["original_simplified"] = simplify_text(original, overrides)
    if record.get("translation_zh_cn") is not None:
        normalized["translation_zh_cn"] = record["translation_zh_cn"]
    return normalized
