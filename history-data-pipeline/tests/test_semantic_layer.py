from pathlib import Path

import pytest

from history_data_pipeline.query_service import HistoryQueryService


DATABASE = Path(__file__).parents[1] / "data" / "normalized" / "history.duckdb"


@pytest.fixture(scope="module")
def service():
    if not DATABASE.exists():
        pytest.skip("正式 history.duckdb 不存在")
    return HistoryQueryService(DATABASE)


def test_relation_dictionary(service):
    rows = service.get_relation_type_dictionary(limit=1000)
    assert len(rows) >= 438
    assert all(row["source_dataset"] == "cbdb" for row in rows)
    assert all(row["source_reference"] for row in rows)


def test_period_and_regime_dates(service):
    periods = service.list_periods()
    regimes = service.list_regimes_by_period("三国")
    assert len(periods) >= 20
    assert len(regimes) >= 3
    assert all(row["date_precision"] in {"exact", "year", "range", "approximate", "unknown"} for row in periods)
    assert all(row["start_year"] <= row["end_year"] for row in periods if row["start_year"] is not None and row["end_year"] is not None)
    assert all(row["start_year"] <= row["end_year"] for row in regimes if row["start_year"] is not None and row["end_year"] is not None)


def test_stories_have_ordered_events_and_traceability(service):
    stories = service.list_stories()
    assert {row["title_zh_cn"] for row in stories} == {"楚汉争霸", "三国格局形成", "安史之乱"}
    for story in stories:
        events = service.get_story_events(story["id"])
        assert len(events) >= 5
        assert [row["sequence"] for row in events] == sorted(row["sequence"] for row in events)
        assert len({row["sequence"] for row in events}) == len(events)
        assert all(row["event_quality_status"] for row in events)
        detail = service.get_story(story["id"])
        assert detail is not None
        assert len(detail["key_people"]) >= 3
        assert detail["key_places"]
        assert detail["historical_texts"]
        assert detail["sources"]


def test_event_targets_and_relations(service):
    for event_name in ("鸿门宴", "赤壁之战", "安禄山起兵"):
        event = service.get_event(event_name)
        assert event is not None
        assert event["people"]
        assert event["places"]
        assert event["historical_texts"]
        assert all(row["source_id"] for row in event["people"])
        assert all(row["source_id"] for row in event["places"])
        assert all(row["source_id"] for row in event["historical_texts"])


def test_self_relations_remain_pending(service):
    with service._connection() as connection:
        total = connection.execute("SELECT COUNT(*) FROM person_relations WHERE person_a_id=person_b_id").fetchone()[0]
        pending = connection.execute("SELECT COUNT(*) FROM data_review WHERE issue_type='self_relation' AND review_status='pending'").fetchone()[0]
    assert total == 57
    assert pending == total


def test_link_qa_rejects_known_temporal_conflicts(service):
    with service._connection() as connection:
        rows = connection.execute("""
            SELECT e.name_zh_cn,w.title,ht.chapter,et.link_quality_status,et.temporal_score
            FROM event_text et JOIN events e ON e.id=et.event_id
            JOIN historical_texts ht ON ht.id=et.historical_text_id
            LEFT JOIN works w ON w.id=ht.book_id
            WHERE e.id IN ('event-anlu-xuanzong-shu','event-anlu-changan','event-chuhan-xingyang')
        """).fetchall()
        assert all(row[3] in {"verified", "reviewed"} and row[4] > 0 for row in rows)
        assert {row[2] for row in rows} <= {"唐纪四十", "唐纪三十四", "汉纪一"}
        assert connection.execute("SELECT COUNT(*) FROM event_text WHERE temporal_score=0").fetchone()[0] == 0
        assert connection.execute("SELECT COUNT(*) FROM event_text_candidates WHERE link_quality_status='rejected'").fetchone()[0] == 3


def test_person_place_link_qa(service):
    with service._connection() as connection:
        hongmen = connection.execute("""
            SELECT p.canonical_name_zh_cn,ep.person_id,ep.link_quality_status
            FROM event_person ep JOIN people p ON p.id=ep.person_id
            WHERE ep.event_id='event-hongmen'
        """).fetchall()
        assert ("范增", "curated-person-fan-zeng", "reviewed") in hongmen
        assert all(row[1] != "cbdb-person-649432" for row in hongmen)
        rejected_places = connection.execute("""
            SELECT COUNT(*) FROM event_place
            WHERE link_quality_status='rejected' AND place_id IS NULL
        """).fetchone()[0]
        assert rejected_places == 5
        assert connection.execute("""
            SELECT COUNT(*) FROM people
            WHERE id IN ('cbdb-person-135152','cbdb-person-135353','cbdb-person-20609','cbdb-person-25403','cbdb-person-94373','cbdb-person-379873')
              AND canonical_name_zh_cn IN ('袁绍','刘备','孙权','诸葛亮','郭子仪','安禄山')
        """).fetchone()[0] == 6
