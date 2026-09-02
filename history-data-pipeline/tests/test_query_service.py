from pathlib import Path

import pytest

from history_data_pipeline.query_service import HistoryQueryService


DATABASE = Path(__file__).parents[1] / "data" / "normalized" / "history.duckdb"


@pytest.fixture(scope="module")
def service():
    if not DATABASE.exists():
        pytest.skip("正式 history.duckdb 不存在")
    return HistoryQueryService(DATABASE)


def test_duckdb_open(service):
    stats = service.get_stats()
    assert stats["counts"]["people"] > 0
    assert stats["counts"]["historical_texts"] > 0


def test_person_query(service):
    for name in ("曹操", "刘备", "李世民", "苏轼"):
        person = service.get_person(name)
        assert person is not None
        assert person["id"]
        assert person["canonical_name_zh_cn"]
        assert person["source_mappings"]


def test_alias_query(service):
    person = service.get_person("曹操")
    assert person is not None
    assert isinstance(service.get_person_aliases(person["id"]), list)


def test_person_relation_query(service):
    person = service.get_person("曹操")
    assert person is not None
    relations = service.get_person_relations(person["id"])
    assert relations
    assert all(row["person_a_id"] and row["person_b_id"] and row["relation_type"] for row in relations)


def test_person_place_query(service):
    person = service.get_person("曹操")
    assert person is not None
    places = service.get_person_places(person["id"])
    assert places
    assert all(row["place_id"] and row["place_name"] and row["source_id"] for row in places)


def test_work_query(service):
    works = service.get_work("史记")
    assert works
    assert any(row["title"] in ("史记", "史記") for row in works)


def test_historical_text_query(service):
    texts = service.get_historical_texts("史记", 10)
    assert len(texts) == 10
    assert all(row["original_text"] and row["original_simplified"] and row["translation_zh_cn"] for row in texts)


def test_source_query(service):
    sources = service.get_sources()
    assert {row["dataset"] for row in sources} >= {"cbdb", "ctext", "classical-modern"}


def test_license_query(service):
    sources = service.get_sources()
    assert all(row["license"] for row in sources)
