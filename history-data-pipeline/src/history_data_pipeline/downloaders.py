from __future__ import annotations

import re
import shutil
import zipfile
from dataclasses import dataclass, replace
from pathlib import Path

from .config import PipelinePaths
from .http import absolute_links, download_to, fetch_json, fetch_text, verify_url
from .snapshots import (
    ensure_new_snapshot,
    md5_file,
    retrieved_at,
    safe_version,
    sha256_file,
    snapshot_dir,
    snapshot_files,
    write_checksum,
    write_metadata,
)


@dataclass(frozen=True)
class ResolvedSource:
    dataset: str
    version: str
    url: str
    license: str
    filename: str
    official_page: str
    expected_sha256: str | None = None
    expected_size: int | None = None
    official_md5: str | None = None
    official_sha1: str | None = None
    notes: str = ""


CBDB_LATEST = "https://github.com/cbdb-project/cbdb_sqlite/raw/refs/heads/master/latest.json"
CTEXT_PAGE = "https://ctext.org/tools/linked-open-data"
NIUTRANS_API = "https://api.github.com/repos/NiuTrans/Classical-Modern"
WIKI_PAGES = {
    "wikipedia": "https://dumps.wikimedia.org/zhwiki/latest/",
    "wikisource": "https://dumps.wikimedia.org/zhwikisource/latest/",
}


def _listing_version(html: str, fallback: str = "latest") -> str:
    dates = re.findall(r"(\d{2})-(\w{3})-(\d{4})", html)
    if not dates:
        return fallback
    months = {name: index for index, name in enumerate(("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"), 1)}
    day, month, year = max(dates, key=lambda item: (int(item[2]), months.get(item[1], 0), int(item[0])))
    return f"{year}{months.get(month, 0):02d}{int(day):02d}"


def resolve_cbdb() -> ResolvedSource:
    latest, _ = fetch_json(CBDB_LATEST)
    archive_url = latest["huggingface_url"]
    filename = Path(archive_url.split("?", 1)[0]).name
    generated = latest.get("generated_at_utc", "")
    version = re.sub(r"[^0-9]", "", generated[:10]) or Path(filename).stem.rsplit("_", 1)[-1]
    return ResolvedSource("cbdb", version, archive_url, "见 CBDB 项目许可说明；待快照确认", filename, CBDB_LATEST, latest.get("sha256"), notes=f"SQLite 文件名由 latest.json 返回：{latest['sqlite_filename']}；generated_at_utc={generated}；SHA-256 针对 SQLite 文件")


def resolve_ctext() -> ResolvedSource:
    html, _ = fetch_text(CTEXT_PAGE)
    links = absolute_links(CTEXT_PAGE, html, r"ctext_datawiki-[^/]+\.ttl\.zip$")
    if not links:
        raise RuntimeError("CText 官方页面未发现 ctext_datawiki-*.ttl.zip")
    url = links[0]
    filename = Path(url.split("?", 1)[0]).name
    match = re.search(r"ctext_datawiki-(\d{4}-\d{2}-\d{2})\.ttl\.zip", filename)
    version = match.group(1) if match else "latest"
    return ResolvedSource("ctext", version, url, "CC BY-NC-SA 3.0", filename, CTEXT_PAGE, notes="Dataset 日期取官方页面当前列出的文件日期")


def resolve_niutrans() -> ResolvedSource:
    payload, _ = fetch_json(NIUTRANS_API)
    branch = payload.get("default_branch", "main")
    version = payload.get("pushed_at", "")[:10].replace("-", "") or branch
    url = f"https://github.com/NiuTrans/Classical-Modern/archive/refs/heads/{branch}.zip"
    return ResolvedSource("classical-modern", version, url, "MIT（仓库 LICENSE；数据文件另须保留各目录 数据来源.txt）", f"Classical-Modern-{branch}.zip", NIUTRANS_API, notes="动态读取官方仓库默认分支；保存完整仓库压缩包并解包")


def resolve_wikimedia(dataset: str) -> ResolvedSource:
    page = WIKI_PAGES[dataset]
    html, _ = fetch_text(page)
    links = absolute_links(page, html, r"pages-articles-multistream\.xml\.bz2$")
    if not links:
        links = absolute_links(page, html, r"pages-articles\.xml\.bz2$")
    if not links:
        raise RuntimeError(f"{page} 未发现文章 XML dump")
    url = links[0]
    filename = Path(url).name
    version = _listing_version(html)
    official_md5, official_sha1 = None, None
    for checksum_url, kind in ((absolute_links(page, html, r"md5sums\.txt$"), "md5"), (absolute_links(page, html, r"sha1sums\.txt$"), "sha1")):
        if checksum_url:
            checksum_text, _ = fetch_text(checksum_url[0])
            match = re.search(rf"([0-9a-fA-F]{{{'32' if kind == 'md5' else '40'}}})\s+\*?{re.escape(filename)}\s*$", checksum_text, re.MULTILINE)
            if match:
                if kind == "md5":
                    official_md5 = match.group(1).lower()
                else:
                    official_sha1 = match.group(1).lower()
    return ResolvedSource(dataset, version, url, "CC BY-SA 4.0 + GFDL 1.3（Wikimedia 文本贡献许可）", filename, page, expected_size=None, official_md5=official_md5, official_sha1=official_sha1, notes="只选择 pages-articles；不下载 images/history/logging")


def _download_one(paths: PipelinePaths, source: ResolvedSource, *, extract: bool = False) -> Path:
    verify_url(source.url)
    target_dir = snapshot_dir(paths.raw, source.dataset, source.version)
    ensure_new_snapshot(target_dir)
    part = target_dir / f".{source.filename}.part"
    target = target_dir / source.filename
    try:
        download_to(source.url, part, expected_size=source.expected_size)
        part.replace(target)
        digest = sha256_file(target)
        if source.expected_sha256 and digest.lower() != source.expected_sha256.lower():
            raise RuntimeError(f"SHA-256 校验失败: {target.name}")
        md5 = md5_file(target)
        if source.official_md5 and md5 != source.official_md5:
            raise RuntimeError(f"官方 MD5 校验失败: {target.name}")
        write_checksum(target_dir / "checksum.sha256", digest, target.name)
        metadata = {
            "dataset": source.dataset,
            "version": safe_version(source.version),
            "downloaded_at": retrieved_at(),
            "source_url": source.url,
            "official_page": source.official_page,
            "license": source.license,
            "filename": source.filename,
            "sha256": digest,
            "md5": md5,
            "size": target.stat().st_size,
            "expected_sha256": source.expected_sha256,
            "expected_size": source.expected_size,
            "official_md5": source.official_md5,
            "official_sha1": source.official_sha1,
            "notes": source.notes,
        }
        write_metadata(target_dir, metadata)
        if extract and zipfile.is_zipfile(target):
            extract_dir = target_dir / "repository"
            extract_dir.mkdir()
            with zipfile.ZipFile(target) as archive:
                archive.extractall(extract_dir)
        return target
    except Exception:
        if part.exists():
            part.unlink()
        shutil.rmtree(target_dir, ignore_errors=True)
        raise


def download_dataset(paths: PipelinePaths, dataset: str) -> Path:
    paths.ensure()
    if dataset == "cbdb":
        source = resolve_cbdb()
        # latest.json 的 SHA-256 是解包后的 SQLite，不是官方下载 ZIP；先验 ZIP，
        # 再对 SQLite 做官方摘要比对。
        target = _download_one(paths, replace(source, expected_sha256=None))
        with zipfile.ZipFile(target) as archive:
            archive_size = target.stat().st_size
            archive_sha256 = sha256_file(target)
            sqlite_names = [name for name in archive.namelist() if name.endswith(".sqlite3")]
            if not sqlite_names:
                raise RuntimeError("CBDB 官方压缩包内没有 SQLite 文件")
            sqlite_name = sqlite_names[0]
            archive.extract(sqlite_name, target.parent)
            extracted = target.parent / sqlite_name
            canonical = target.parent / source.filename.removesuffix(".zip")
            if extracted != canonical:
                extracted.replace(canonical)
            digest = sha256_file(canonical)
            if source.expected_sha256 and digest.lower() != source.expected_sha256.lower():
                raise RuntimeError("CBDB SQLite SHA-256 校验失败")
            (target.parent / "checksum.sha256").write_text(f"{archive_sha256}  {target.name}\n{digest}  {canonical.name}\n", encoding="utf-8")
            metadata = __import__("json").loads((target.parent / "metadata.json").read_text(encoding="utf-8"))
            metadata.update({"filename": canonical.name, "sha256": digest, "size": canonical.stat().st_size, "archive_filename": target.name, "archive_size": archive_size, "archive_sha256": archive_sha256})
            write_metadata(target.parent, metadata)
        return target.parent
    if dataset == "ctext":
        return _download_one(paths, resolve_ctext())
    if dataset == "niutrans":
        return _download_one(paths, resolve_niutrans(), extract=True)
    if dataset in WIKI_PAGES:
        return _download_one(paths, resolve_wikimedia(dataset))
    raise ValueError(f"未知数据集: {dataset}")


class CHGISManualImporter:
    """只接收用户已获许可的官方 CHGIS 文件，不自动访问或绕过许可页。"""

    def __init__(self, paths: PipelinePaths):
        self.paths = paths

    def import_directory(self, input_dir: Path, version: str, license_text: str) -> Path:
        if not input_dir.is_dir():
            raise FileNotFoundError(f"CHGIS 输入目录不存在: {input_dir}")
        if not license_text.strip():
            raise ValueError("CHGIS 必须显式提供许可/授权记录")
        self.paths.ensure()
        target = snapshot_dir(self.paths.raw, "chgis", version)
        ensure_new_snapshot(target)
        shutil.copytree(input_dir, target / "files", dirs_exist_ok=False)
        files = list(snapshot_files(target))
        if not files:
            shutil.rmtree(target)
            raise ValueError("CHGIS 输入目录为空")
        checksums = [(str(path.relative_to(target)), sha256_file(path), path.stat().st_size) for path in files]
        (target / "checksum.sha256").write_text("".join(f"{digest}  {name}\n" for name, digest, _ in checksums), encoding="utf-8")
        write_metadata(target, {
            "dataset": "chgis", "version": safe_version(version), "downloaded_at": retrieved_at(),
            "source_url": "https://yugong.fudan.edu.cn/CHGIS/sjxz.htm", "official_page": "https://yugong.fudan.edu.cn/CHGIS/bqsm.htm",
            "license": license_text, "filename": "files/", "sha256": None,
            "size": sum(size for _, _, size in checksums), "file_count": len(files),
            "manual_import": True, "notes": "用户确认已获官方许可；禁止默认公开再分发",
        })
        return target


class CBDBDownloader:
    def download(self, paths: PipelinePaths) -> Path:
        return download_dataset(paths, "cbdb")


class CTextDownloader:
    def download(self, paths: PipelinePaths) -> Path:
        return download_dataset(paths, "ctext")


class ClassicalModernDownloader:
    def download(self, paths: PipelinePaths) -> Path:
        return download_dataset(paths, "niutrans")


class WikipediaDumpDownloader:
    def download(self, paths: PipelinePaths) -> Path:
        return download_dataset(paths, "wikipedia")


class WikisourceDumpDownloader:
    def download(self, paths: PipelinePaths) -> Path:
        return download_dataset(paths, "wikisource")
