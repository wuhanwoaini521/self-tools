from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

REVIEW_STATUSES = (
    "pending",
    "confirmed_source_error",
    "confirmed_normalization_error",
    "accepted",
    "fixed_in_canonical",
    "ignored",
    "needs_more_sources",
)


def ensure_review_table(database: Path) -> int:
    """Create the review table and register date anomalies without changing facts."""
    import duckdb

    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    with duckdb.connect(str(database)) as connection:
        connection.execute(
            """
            CREATE TABLE IF NOT EXISTS data_review (
              id VARCHAR PRIMARY KEY, entity_type VARCHAR NOT NULL, entity_id VARCHAR NOT NULL,
              field_name VARCHAR NOT NULL, issue_type VARCHAR NOT NULL, current_value VARCHAR,
              source_value VARCHAR, review_status VARCHAR NOT NULL, review_note VARCHAR,
              reviewed_by VARCHAR, created_at VARCHAR NOT NULL, reviewed_at VARCHAR
            )
            """
        )
        connection.execute(
            """
            INSERT OR IGNORE INTO data_review
              (id, entity_type, entity_id, field_name, issue_type, current_value,
               source_value, review_status, review_note, reviewed_by, created_at, reviewed_at)
            SELECT
              'review-birth-death-' || p.id, 'person', p.id, 'birth_year/death_year',
              'birth_after_death', CAST(p.birth_year AS VARCHAR) || ' > ' || CAST(p.death_year AS VARCHAR),
              NULL, 'pending', '仅登记异常；未判断来源或规范化哪一方错误。请对照源记录复核。',
              NULL, ?, NULL
            FROM people p
            WHERE p.birth_year IS NOT NULL AND p.death_year IS NOT NULL AND p.birth_year > p.death_year
            """,
            [now],
        )
        return connection.execute("SELECT COUNT(*) FROM data_review WHERE review_status = 'pending'").fetchone()[0]
