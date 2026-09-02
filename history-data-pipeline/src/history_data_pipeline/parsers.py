from __future__ import annotations

import json
import re
import sqlite3
import zipfile
from pathlib import Path
from collections.abc import Iterable, Iterator


def _read_lines(path: Path) -> list[str]:
    return [line.rstrip("\n\r") for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines()]


def iter_classical_modern(snapshot_dir: Path) -> Iterator[dict]:
    """读取 NiuTrans 的 source/target/bitext，不把启发式句对标成人工校验。"""
    repository = snapshot_dir / "repository"
    for source in repository.rglob("source.txt") if repository.exists() else []:
        target = source.with_name("target.txt")
        if not target.exists():
            continue
        source_lines, target_lines = _read_lines(source), _read_lines(target)
        for index, (original, translation) in enumerate(zip(source_lines, target_lines), 1):
            if not original.strip() or not translation.strip():
                continue
            yield {"source_path": str(source.relative_to(snapshot_dir)), "line_number": index, "original_text": original, "translation_zh_cn": translation, "alignment_quality": "heuristic_unverified"}


def parse_classical_modern(snapshot_dir: Path) -> list[dict]:
    """兼容小型调用方；生产写入请使用流式迭代器。"""
    return list(iter_classical_modern(snapshot_dir))


def write_jsonl(records: Iterable[dict], target: Path) -> int:
    target.parent.mkdir(parents=True, exist_ok=True)
    count = 0
    with target.open("w", encoding="utf-8") as stream:
        for record in records:
            stream.write(json.dumps(record, ensure_ascii=False) + "\n")
            count += 1
    return count


def _int(value):
    try:
        return int(value) if value not in (None, "", "NULL") else None
    except (TypeError, ValueError):
        return None


def _person_year(value):
    """CBDB 人物年份字段以 0 表示未知，不把它解释成公元 0 年。"""
    parsed = _int(value)
    return None if parsed == 0 else parsed


def _float(value):
    try:
        return float(value) if value not in (None, "", "NULL") else None
    except (TypeError, ValueError):
        return None


def _text(value):
    return str(value).strip() if value not in (None, "") else None


def cbdb_database(snapshot_dir: Path) -> Path:
    candidates = [path for path in snapshot_dir.iterdir() if path.is_file() and path.name not in {"metadata.json", "checksum.sha256"} and not path.name.endswith(".zip")]
    if not candidates:
        raise FileNotFoundError(f"CBDB Snapshot 未找到解包后的 SQLite: {snapshot_dir}")
    return candidates[0]


def iter_cbdb_people(snapshot_dir: Path) -> Iterator[dict]:
    path = cbdb_database(snapshot_dir)
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        query = "SELECT c_personid, c_name_chn, c_name, c_birthyear, c_deathyear, c_by_range, c_dy_nh_year, c_female, c_dy FROM BIOG_MAIN"
        for person_id, name_zh, name_raw, birth, death, birth_range, death_nh_year, female, dynasty in connection.execute(query):
            canonical = _text(name_zh) or _text(name_raw) or f"CBDB人物{person_id}"
            yield {"id": f"cbdb-person-{person_id}", "canonical_name_zh_cn": canonical, "name_raw": _text(name_raw), "birth_year": _person_year(birth), "death_year": _person_year(death), "birth_precision": "range" if _text(birth_range) else "year", "death_precision": "year" if _text(death_nh_year) else "unknown", "gender": "female" if female in (1, "1", "Y") else "male" if female in (0, "0", "N") else "unknown", "period_ids": [], "regime_ids": [], "dynasty_ids": [f"cbdb-dynasty-{dynasty}"] if dynasty not in (None, "") else [], "quality_status": "source_backed", "search_name": canonical, "search_aliases": "", "search_text": canonical, "pinyin": "", "initials": ""}
    finally:
        connection.close()


def iter_cbdb_aliases(snapshot_dir: Path) -> Iterator[dict]:
    connection = sqlite3.connect(f"file:{cbdb_database(snapshot_dir)}?mode=ro", uri=True)
    try:
        query = "SELECT c_personid, c_alt_name_chn, c_alt_name, c_alt_name_type_code FROM ALTNAME_DATA"
        for person_id, alias_zh, alias_raw, alias_type in connection.execute(query):
            alias = _text(alias_zh) or _text(alias_raw)
            if alias:
                yield {"person_id": f"cbdb-person-{person_id}", "alias": alias, "alias_type": _text(alias_type) or "unknown", "source": "cbdb"}
    finally:
        connection.close()


def iter_cbdb_places(snapshot_dir: Path) -> Iterator[dict]:
    connection = sqlite3.connect(f"file:{cbdb_database(snapshot_dir)}?mode=ro", uri=True)
    try:
        query = "SELECT c_addr_id, c_name_chn, c_name, x_coord, y_coord, c_admin_type, c_firstyear, c_lastyear FROM ADDR_CODES"
        for place_id, name_zh, name_raw, longitude, latitude, place_type, valid_from, valid_to in connection.execute(query):
            name = _text(name_zh) or _text(name_raw) or f"CBDB地点{place_id}"
            yield {"id": f"cbdb-place-{place_id}", "canonical_name_zh_cn": name, "historical_name": _text(name_zh), "modern_name": None, "longitude": _float(longitude), "latitude": _float(latitude), "place_type": _text(place_type), "valid_from": _int(valid_from), "valid_to": _int(valid_to), "quality_status": "source_backed", "source_ids": ["source-cbdb"]}
    finally:
        connection.close()


def iter_cbdb_person_places(snapshot_dir: Path) -> Iterator[dict]:
    connection = sqlite3.connect(f"file:{cbdb_database(snapshot_dir)}?mode=ro", uri=True)
    try:
        query = "SELECT c_personid, c_addr_id, c_addr_type, c_firstyear, c_lastyear FROM BIOG_ADDR_DATA WHERE COALESCE(c_delete, 0) = 0"
        for person_id, place_id, relation_type, start_year, end_year in connection.execute(query):
            if person_id not in (None, "") and place_id not in (None, ""):
                yield {"person_id": f"cbdb-person-{person_id}", "place_id": f"cbdb-place-{place_id}", "relation_type": _text(relation_type) or "associated", "start_year": _int(start_year), "end_year": _int(end_year), "source_id": "source-cbdb", "external_id": str(place_id), "quality_status": "source_backed"}
    finally:
        connection.close()


def iter_cbdb_person_relations(snapshot_dir: Path) -> Iterator[dict]:
    connection = sqlite3.connect(f"file:{cbdb_database(snapshot_dir)}?mode=ro", uri=True)
    try:
        query = "SELECT c_personid, c_kin_id, c_kin_code, c_source FROM KIN_DATA"
        for person_a, person_b, relation_type, source in connection.execute(query):
            if person_a not in (None, "") and person_b not in (None, ""):
                yield {"person_a_id": f"cbdb-person-{person_a}", "person_b_id": f"cbdb-person-{person_b}", "relation_type": _text(relation_type) or "kin", "description": None, "source_ids": ["source-cbdb"], "confidence": 1.0, "external_id": _text(source)}
    finally:
        connection.close()


def iter_cbdb_dynasties(snapshot_dir: Path) -> Iterator[dict]:
    connection = sqlite3.connect(f"file:{cbdb_database(snapshot_dir)}?mode=ro", uri=True)
    try:
        for dynasty_id, raw, name_zh, start, end, _sort in connection.execute("SELECT c_dy, c_dynasty, c_dynasty_chn, c_start, c_end, c_sort FROM DYNASTIES"):
            name = _text(name_zh) or _text(raw) or f"CBDB朝代{dynasty_id}"
            yield {"id": f"cbdb-dynasty-{dynasty_id}", "name_zh_cn": name, "name_raw": _text(raw), "start_year": _int(start), "end_year": _int(end), "date_precision": "range", "confidence": 1.0, "source_ids": ["source-cbdb"]}
    finally:
        connection.close()


def stage_cbdb(snapshot_dir: Path, staging_dir: Path) -> dict[str, int]:
    staging_dir.mkdir(parents=True, exist_ok=True)
    counts = {}
    for table, records in (("people", iter_cbdb_people(snapshot_dir)), ("person_aliases", iter_cbdb_aliases(snapshot_dir)), ("places", iter_cbdb_places(snapshot_dir)), ("dynasties", iter_cbdb_dynasties(snapshot_dir)), ("person_place", iter_cbdb_person_places(snapshot_dir)), ("person_relations", iter_cbdb_person_relations(snapshot_dir))):
        counts[table] = write_jsonl(records, staging_dir / f"{table}.jsonl")
    return counts


def iter_ctext_entities(snapshot_dir: Path) -> Iterator[dict]:
    archives = list(snapshot_dir.glob("*.zip"))
    if not archives:
        raise FileNotFoundError(f"CText Snapshot 未找到 ZIP: {snapshot_dir}")
    pattern_subject = re.compile(r"^ctext:(\d+)\s")
    pattern_type = re.compile(r'claim:type\s+"([^"]+)"')
    pattern_label = re.compile(r'rdfs:label\s+"((?:\\.|[^"\\])*)"')
    pattern_name = re.compile(r'claim:name\s+"((?:\\.|[^"\\])*)"')
    pattern_cbdb = re.compile(r'claim:authority-cbdb\s+"([^"]+)"')
    pattern_wikidata = re.compile(r'claim:authority-wikidata\s+"([^"]+)"')
    pattern_wikipedia = re.compile(r'claim:link-wikipedia_zh\s+"([^"]+)"')
    with zipfile.ZipFile(archives[0]) as archive, archive.open(archive.namelist()[0]) as stream:
        subject = None
        block: list[str] = []

        def emit(current_subject: str | None, current_block: list[str]):
            if not current_subject:
                return None
            text = " ".join(current_block)
            kind = pattern_type.search(text)
            label = pattern_label.search(text) or pattern_name.search(text)
            if not (kind and label):
                return None
            value = label.group(1).replace('\\"', '"').replace('\\\\', '\\')
            return {"entity_type": kind.group(1), "external_id": current_subject, "label": value, "cbdb_external_id": pattern_cbdb.search(text).group(1) if pattern_cbdb.search(text) else None, "wikidata_id": pattern_wikidata.search(text).group(1) if pattern_wikidata.search(text) else None, "wikipedia_url": pattern_wikipedia.search(text).group(1) if pattern_wikipedia.search(text) else None, "source_id": "source-ctext"}

        for raw_line in stream:
            line = raw_line.decode("utf-8", errors="replace").strip()
            match = pattern_subject.match(line)
            if match:
                new_subject = match.group(1)
                if subject is None:
                    subject, block = new_subject, [line]
                elif new_subject == subject:
                    block.append(line)
                else:
                    record = emit(subject, block)
                    if record:
                        yield record
                    subject, block = new_subject, [line]
            elif subject:
                block.append(line)
        record = emit(subject, block)
        if record:
            yield record


def stage_ctext(snapshot_dir: Path, staging_dir: Path) -> int:
    return write_jsonl(iter_ctext_entities(snapshot_dir), staging_dir / "entities.jsonl")
