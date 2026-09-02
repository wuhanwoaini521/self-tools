from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path


def log_event(log_dir: Path, filename: str, event: str, detail: str = "") -> None:
    log_dir.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    suffix = f" {detail}" if detail else ""
    with (log_dir / filename).open("a", encoding="utf-8") as stream:
        stream.write(f"{timestamp} {event}{suffix}\n")
