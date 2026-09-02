from __future__ import annotations

import os
from pathlib import Path

from .database import TABLES


def export_parquet(database: Path, output_dir: Path) -> None:
    try:
        import duckdb
    except ImportError as exc:
        raise RuntimeError("缺少 DuckDB，请先安装 requirements.txt") from exc
    output_dir.mkdir(parents=True, exist_ok=True)
    with duckdb.connect(str(database), read_only=True) as connection:
        for table in TABLES:
            target = output_dir / f"{table}.parquet"
            temporary = output_dir / f".{table}.parquet.tmp"
            if temporary.exists():
                temporary.unlink()
            connection.execute(f"COPY {table} TO ? (FORMAT PARQUET, COMPRESSION ZSTD)", [str(temporary)])
            os.replace(temporary, target)
