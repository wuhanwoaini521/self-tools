"""中国历史离线数据仓库 V1。"""

from .database import SCHEMA_SQL, build_database

__all__ = ["SCHEMA_SQL", "build_database"]
