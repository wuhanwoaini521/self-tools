from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class PipelinePaths:
    root: Path

    @property
    def raw(self) -> Path:
        return self.root / "data" / "raw"

    @property
    def staging(self) -> Path:
        return self.root / "data" / "staging"

    @property
    def normalized(self) -> Path:
        return self.root / "data" / "normalized"

    @property
    def exports(self) -> Path:
        return self.root / "data" / "exports"

    @property
    def reports(self) -> Path:
        return self.root / "data" / "reports"

    @property
    def logs(self) -> Path:
        return self.root / "data" / "logs"

    @property
    def database(self) -> Path:
        return self.normalized / "history.duckdb"

    def ensure(self) -> None:
        for path in (self.raw, self.staging, self.normalized, self.exports, self.reports, self.logs):
            path.mkdir(parents=True, exist_ok=True)


DATASETS = ("cbdb", "chgis", "ctext", "classical-modern", "wikipedia", "wikisource")
