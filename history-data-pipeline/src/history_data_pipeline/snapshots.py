from __future__ import annotations

import hashlib
import json
import re
import shutil
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


def sha256_file(path: Path, chunk_size: int = 1024 * 1024) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(chunk_size), b""):
            digest.update(chunk)
    return digest.hexdigest()


def md5_file(path: Path, chunk_size: int = 1024 * 1024) -> str:
    digest = hashlib.md5()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(chunk_size), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_version(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", value).strip("-.")
    if not cleaned:
        raise ValueError("版本号不能为空")
    return cleaned


def retrieved_at() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def write_checksum(path: Path, digest: str, filename: str) -> None:
    path.write_text(f"{digest}  {filename}\n", encoding="utf-8")


def write_metadata(snapshot_dir: Path, metadata: dict) -> Path:
    target = snapshot_dir / "metadata.json"
    target.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return target


def snapshot_dir(raw_root: Path, dataset: str, version: str) -> Path:
    return raw_root / dataset / safe_version(version)


def ensure_new_snapshot(path: Path) -> None:
    if path.exists() and any(path.iterdir()):
        raise FileExistsError(f"拒绝覆盖已有 Raw Snapshot: {path}")
    path.mkdir(parents=True, exist_ok=True)


def snapshot_files(root: Path) -> Iterable[Path]:
    yield from (path for path in root.rglob("*") if path.is_file() and path.name not in {"metadata.json", "checksum.sha256"})


def copy_tree_preserving_raw(source: Path, target: Path) -> None:
    for item in source.iterdir():
        destination = target / item.name
        if item.is_dir():
            shutil.copytree(item, destination)
        else:
            shutil.copy2(item, destination)
