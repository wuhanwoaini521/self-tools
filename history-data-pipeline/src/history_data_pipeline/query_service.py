from __future__ import annotations

import json
from contextlib import contextmanager
from pathlib import Path
from time import perf_counter
from typing import Any, Iterator


def _jsonish(value: Any) -> Any:
    if not isinstance(value, str):
        return value
    try:
        return json.loads(value)
    except (TypeError, ValueError):
        return value


def _rows(connection, sql: str, parameters: list[Any] | None = None) -> list[dict[str, Any]]:
    result = connection.execute(sql, parameters or [])
    columns = [item[0] for item in result.description]
    return [dict(zip(columns, row)) for row in result.fetchall()]


class HistoryQueryService:
    """Read-only query facade for the normalized History DuckDB."""

    def __init__(self, database: Path):
        self.database = Path(database)
        if not self.database.exists():
            raise FileNotFoundError(f"数据库不存在: {self.database}")

    @contextmanager
    def _connection(self) -> Iterator[Any]:
        import duckdb

        with duckdb.connect(str(self.database), read_only=True) as connection:
            yield connection

    @staticmethod
    def _name_variants(value: str) -> list[str]:
        variants = [value]
        try:
            from opencc import OpenCC

            variants.extend([OpenCC("s2t").convert(value), OpenCC("t2s").convert(value)])
        except ImportError:
            fallback = str.maketrans("刘苏轼备", "劉蘇軾備")
            variants.append(value.translate(fallback))
        return list(dict.fromkeys(item for item in variants if item))

    def _sources_for(self, connection, entity_type: str, entity_id: str) -> list[dict[str, Any]]:
        return _rows(
            connection,
            """
            SELECT esm.source_id, esm.external_id, esm.match_type, esm.confidence,
                   s.dataset, s.dataset_version, s.license, s.original_url,
                   s.raw_path, s.staging_path, s.quality_status AS source_quality_status
            FROM entity_source_mapping esm
            LEFT JOIN sources s ON s.id = esm.source_id
            WHERE esm.entity_type = ? AND esm.entity_id = ?
            ORDER BY esm.source_id, esm.external_id
            """,
            [entity_type, entity_id],
        )

    def get_sources(self, entity_type: str | None = None, entity_id: str | None = None) -> list[dict[str, Any]]:
        with self._connection() as connection:
            if entity_type and entity_id:
                return self._sources_for(connection, entity_type, entity_id)
            if entity_type:
                return _rows(connection, "SELECT * FROM sources WHERE dataset = ? ORDER BY dataset", [entity_type])
            return _rows(connection, "SELECT * FROM sources ORDER BY dataset")

    def get_source(self, entity_type: str, entity_id: str) -> list[dict[str, Any]]:
        """Return the complete source/license provenance for one canonical entity."""
        return self.get_sources(entity_type, entity_id)

    def get_review_queue(self, status: str | None = "pending") -> list[dict[str, Any]]:
        with self._connection() as connection:
            if status is None:
                return _rows(connection, "SELECT * FROM data_review ORDER BY created_at, id")
            return _rows(connection, "SELECT * FROM data_review WHERE review_status = ? ORDER BY created_at, id", [status])

    def get_dataset_stats(self) -> dict[str, Any]:
        return self.get_stats()

    def search_people(self, query: str, limit: int = 20) -> list[dict[str, Any]]:
        variants = self._name_variants(query)
        clauses = []
        params: list[Any] = []
        for value in variants:
            clauses.append("(canonical_name_zh_cn = ? OR name_raw = ? OR search_name = ? OR canonical_name_zh_cn LIKE ? OR search_text LIKE ?)")
            params.extend([value, value, value, f"%{value}%", f"%{value}%"])
        params.append(max(1, min(limit, 100)))
        with self._connection() as connection:
            return _rows(
                connection,
                f"""
                SELECT id, canonical_name_zh_cn, name_raw, traditional_name, birth_year, death_year,
                       birth_precision, death_precision, gender, period_ids, regime_ids, dynasty_ids,
                       intro_zh_cn, quality_status, created_from_source, search_name, search_aliases,
                       search_text, pinyin, initials,
                       CASE WHEN canonical_name_zh_cn IN ({','.join('?' for _ in variants)}) THEN 0 ELSE 1 END AS match_rank
                FROM people
                WHERE {' OR '.join(clauses)}
                ORDER BY match_rank, canonical_name_zh_cn, id
                LIMIT ?
                """,
                variants + params[:-1] + [params[-1]],
            )

    def _person_details(self, connection, row: dict[str, Any], include_relations: bool = False, include_places: bool = False) -> dict[str, Any]:
        person = dict(row)
        person.pop("match_rank", None)
        for field in ("period_ids", "regime_ids", "dynasty_ids"):
            person[field] = _jsonish(person[field])
        person["aliases"] = _rows(connection, "SELECT alias, alias_zh_cn, alias_type, source, source_id, external_id FROM person_aliases WHERE person_id = ? ORDER BY alias", [person["id"]])
        person["source_mappings"] = self._sources_for(connection, "person", person["id"])
        if include_relations:
            person["relations"] = self._relations_for(connection, person["id"])
        if include_places:
            person["places"] = self._places_for(connection, person["id"])
        return person

    def get_person(self, query: str, relations: bool = False, places: bool = False) -> dict[str, Any] | None:
        with self._connection() as connection:
            rows = self.search_people(query, limit=20)
            if not rows:
                return None
            return self._person_details(connection, rows[0], relations, places)

    def get_person_aliases(self, person_id: str) -> list[dict[str, Any]]:
        with self._connection() as connection:
            return _rows(connection, "SELECT * FROM person_aliases WHERE person_id = ? ORDER BY alias", [person_id])

    def _relations_for(self, connection, person_id: str) -> list[dict[str, Any]]:
        return _rows(
            connection,
            """
            SELECT pr.person_a_id, a.canonical_name_zh_cn AS person_a_name,
                   pr.person_b_id, b.canonical_name_zh_cn AS person_b_name,
                   pr.relation_type, pr.start_year, pr.end_year, pr.description,
                   pr.source_ids, pr.confidence,
                   CASE WHEN pr.person_a_id = ? THEN 'outgoing' ELSE 'incoming' END AS direction,
                   COALESCE(sa.dataset, sb.dataset) AS source_dataset
            FROM person_relations pr
            LEFT JOIN people a ON a.id = pr.person_a_id
            LEFT JOIN people b ON b.id = pr.person_b_id
            LEFT JOIN sources sa ON sa.id = REPLACE(REPLACE(REPLACE(pr.source_ids, '[', ''), ']', ''), '''', '')
            LEFT JOIN sources sb ON sb.id = REPLACE(REPLACE(REPLACE(pr.source_ids, '[', ''), ']', ''), '''', '')
            WHERE pr.person_a_id = ? OR pr.person_b_id = ?
            ORDER BY pr.relation_type, pr.person_a_id, pr.person_b_id
            """,
            [person_id, person_id, person_id],
        )

    def get_person_relations(self, person_id: str) -> list[dict[str, Any]]:
        with self._connection() as connection:
            return self._relations_for(connection, person_id)

    def _places_for(self, connection, person_id: str) -> list[dict[str, Any]]:
        return _rows(
            connection,
            """
            SELECT pp.person_id, pp.place_id, pl.canonical_name_zh_cn AS place_name,
                   pl.historical_name, pl.modern_name, pl.longitude, pl.latitude,
                   pl.place_type, pl.valid_from, pl.valid_to, pp.relation_type,
                   pp.start_year, pp.end_year, pp.source_id, pp.external_id,
                   pp.quality_status
            FROM person_place pp
            LEFT JOIN places pl ON pl.id = pp.place_id
            WHERE pp.person_id = ?
            ORDER BY pp.relation_type, pp.start_year NULLS LAST, pp.place_id
            """,
            [person_id],
        )

    def get_person_places(self, person_id: str) -> list[dict[str, Any]]:
        with self._connection() as connection:
            return self._places_for(connection, person_id)

    def get_work(self, query: str, limit: int = 20) -> list[dict[str, Any]]:
        variants = self._name_variants(query)
        clauses = ["title = ? OR title_zh_cn = ? OR title LIKE ? OR title_zh_cn LIKE ?" for _ in variants]
        params = [item for value in variants for item in (value, value, f"%{value}%", f"%{value}%")]
        params.append(max(1, min(limit, 100)))
        with self._connection() as connection:
            rows = _rows(connection, f"SELECT * FROM works WHERE {' OR '.join(clauses)} ORDER BY title, id LIMIT ?", params)
            for row in rows:
                row["author_ids"] = _jsonish(row["author_ids"])
                row["source_ids"] = _jsonish(row["source_ids"])
                row["sources"] = self._sources_for(connection, "work", row["id"])
            return rows

    def get_historical_texts(self, work: str | None = None, limit: int = 20) -> list[dict[str, Any]]:
        params: list[Any] = []
        where = ""
        if work:
            variants = self._name_variants(work)
            where = "WHERE " + " OR ".join("(w.title = ? OR w.title_zh_cn = ? OR w.title LIKE ? OR w.title_zh_cn LIKE ?)" for _ in variants)
            params.extend(item for value in variants for item in (value, value, f"%{value}%", f"%{value}%"))
        params.append(max(1, min(limit, 100)))
        with self._connection() as connection:
            rows = _rows(
                connection,
                f"""
                SELECT ht.id, ht.title_zh_cn, ht.book_id, w.title AS work_title, ht.chapter, ht.section,
                       ht.original_text, ht.original_simplified, ht.translation_zh_cn, ht.notes_zh_cn,
                       ht.intro_zh_cn, ht.translation_type, ht.translation_source, ht.quality_status,
                       ht.source_id, ht.alignment_quality
                FROM historical_texts ht
                LEFT JOIN works w ON w.id = ht.book_id
                {where}
                ORDER BY ht.title_zh_cn, ht.chapter, ht.id
                LIMIT ?
                """,
                params,
            )
            for row in rows:
                row["source"] = _rows(connection, "SELECT id,dataset,dataset_version,license,raw_path,staging_path FROM sources WHERE id = ?", [row["source_id"]])
            return rows

    def get_stats(self) -> dict[str, Any]:
        with self._connection() as connection:
            tables = [row[0] for row in connection.execute("SHOW TABLES").fetchall()]
            counts = {table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] for table in tables}
            complete = connection.execute("SELECT COUNT(*) FROM historical_texts WHERE original_text IS NOT NULL AND original_simplified IS NOT NULL AND translation_zh_cn IS NOT NULL").fetchone()[0]
            equal = connection.execute("SELECT COUNT(*) FROM historical_texts WHERE translation_zh_cn IS NOT NULL AND translation_zh_cn = original_simplified").fetchone()[0]
            source_totals = {
                "people_with_mapping": connection.execute("SELECT COUNT(DISTINCT esm.entity_id) FROM entity_source_mapping esm JOIN people p ON p.id=esm.entity_id WHERE esm.entity_type='person'").fetchone()[0],
                "works_with_mapping": connection.execute("SELECT COUNT(DISTINCT esm.entity_id) FROM entity_source_mapping esm JOIN works w ON w.id=esm.entity_id WHERE esm.entity_type='work'").fetchone()[0],
                "historical_texts_with_source": connection.execute("SELECT COUNT(*) FROM historical_texts WHERE source_id IS NOT NULL AND source_id IN (SELECT id FROM sources)").fetchone()[0],
            }
            anomalies = connection.execute("SELECT COUNT(*) FROM people WHERE birth_year IS NOT NULL AND death_year IS NOT NULL AND birth_year > death_year").fetchone()[0]
            orphan_relations = connection.execute("SELECT COUNT(*) FROM person_relations pr LEFT JOIN people a ON a.id=pr.person_a_id LEFT JOIN people b ON b.id=pr.person_b_id WHERE a.id IS NULL OR b.id IS NULL").fetchone()[0]
            orphan_places = connection.execute("SELECT COUNT(*) FROM person_place pp LEFT JOIN people p ON p.id=pp.person_id LEFT JOIN places pl ON pl.id=pp.place_id WHERE p.id IS NULL OR pl.id IS NULL").fetchone()[0]
            self_relations = connection.execute("SELECT COUNT(*) FROM person_relations WHERE person_a_id = person_b_id").fetchone()[0]
            review_queue = connection.execute("SELECT COUNT(*) FROM data_review WHERE review_status = 'pending'").fetchone()[0] if 'data_review' in tables else 0
            return {"counts": counts, "historical_texts_complete": complete, "translation_equals_simplified": equal, "source_coverage": source_totals, "birth_after_death": anomalies, "broken_person_relations": orphan_relations, "broken_person_places": orphan_places, "self_person_relations": self_relations, "review_queue_pending": review_queue}

    def timed_queries(self) -> dict[str, float]:
        timings: dict[str, float] = {}
        calls = {
            "person_exact_name": lambda: self.search_people("曹操", 1),
            "aliases_by_person_id": lambda: self.get_person_aliases("cbdb-person-30257"),
            "relations_by_person_id": lambda: self.get_person_relations("cbdb-person-30257"),
            "places_by_person_id": lambda: self.get_person_places("cbdb-person-30257"),
            "historical_texts_by_work": lambda: self.get_historical_texts("史记", 10),
            "stats": self.get_stats,
        }
        for name, call in calls.items():
            started = perf_counter()
            call()
            timings[name] = round((perf_counter() - started) * 1000, 2)
        return timings
