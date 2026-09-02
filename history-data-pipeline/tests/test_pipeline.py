from history_data_pipeline.normalization import normalize_historical_text
from history_data_pipeline.sample import sample_records
from history_data_pipeline.validation import validate_records


def test_unique_ids():
    records = sample_records()
    for table in ("people", "events", "stories", "places", "historical_texts"):
        ids = [row["id"] for row in records[table]]
        assert len(ids) == len(set(ids))


def test_event_dates_and_person_dates():
    records = sample_records()
    assert all(row["start_year"] <= row["end_year"] for row in records["events"])
    assert all(row["birth_year"] <= row["death_year"] for row in records["people"])


def test_relation_targets_exist():
    assert validate_records(sample_records()) == []


def test_story_event_sequence():
    values = [row["sequence"] for row in sample_records()["story_events"]]
    assert values == [1, 2, 3]


def test_source_mapping():
    records = sample_records()
    source_ids = {row["id"] for row in records["sources"]}
    assert all(row["source_id"] in source_ids for row in records["entity_source_mapping"])


def test_text_preserves_original():
    original = "項籍者，下相人也"
    normalized = normalize_historical_text({"original_text": original, "translation_zh_cn": "项籍是下相人。"})
    assert normalized["original_text"] == original
    assert normalized["original_simplified"] == "项籍者，下相人也"


def test_translation_not_equal_simplified():
    text = sample_records()["historical_texts"][0]
    assert text["translation_zh_cn"] != text["original_simplified"]


def test_bce_sorting():
    years = [row["birth_year"] for row in sample_records()["people"]]
    assert sorted(years)[0] < 0
    assert -221 < 1


def test_orphan_entities():
    records = sample_records()
    records["event_person"].append({"event_id": "missing", "person_id": "person-cao-cao", "role": "x"})
    assert any("孤儿引用" in error for error in validate_records(records))
