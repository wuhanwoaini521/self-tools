from __future__ import annotations

import json
import re
from html import unescape
from urllib.error import HTTPError, URLError
from urllib.parse import urljoin
from urllib.request import Request, urlopen


USER_AGENT = "china-history-data-pipeline/1.0 (+offline snapshot collector)"


def request_bytes(url: str, *, range_header: str | None = None) -> tuple[bytes, dict[str, str]]:
    headers = {"User-Agent": USER_AGENT}
    if range_header:
        headers["Range"] = range_header
    request = Request(url, headers=headers)
    with urlopen(request, timeout=60) as response:
        return response.read(), {key.lower(): value for key, value in response.headers.items()}


def fetch_json(url: str) -> tuple[dict, dict[str, str]]:
    payload, headers = request_bytes(url)
    return json.loads(payload.decode("utf-8")), headers


def fetch_text(url: str) -> tuple[str, dict[str, str]]:
    payload, headers = request_bytes(url)
    return payload.decode("utf-8", errors="replace"), headers


def verify_url(url: str) -> dict[str, str]:
    request = Request(url, method="HEAD", headers={"User-Agent": USER_AGENT})
    try:
        with urlopen(request, timeout=30) as response:
            return {key.lower(): value for key, value in response.headers.items()}
    except (HTTPError, URLError):
        payload, headers = request_bytes(url, range_header="bytes=0-0")
        if not payload and not headers:
            raise RuntimeError(f"官方 URL 无法验证: {url}")
        return headers


def download_to(url: str, target, *, expected_size: int | None = None) -> dict[str, str | int]:
    headers = {"User-Agent": USER_AGENT}
    request = Request(url, headers=headers)
    with urlopen(request, timeout=120) as response, target.open("wb") as output:
        size = 0
        while chunk := response.read(1024 * 1024):
            output.write(chunk)
            size += len(chunk)
    if expected_size is not None and size != expected_size:
        raise RuntimeError(f"下载大小不符合官方目录: expected={expected_size}, actual={size}")
    return {"size": size, "content_type": response.headers.get("Content-Type", "")}


def absolute_links(page_url: str, html: str, pattern: str) -> list[str]:
    links = re.findall(r"href=[\"']([^\"']+)[\"']", html, flags=re.IGNORECASE)
    return [urljoin(page_url, unescape(link)) for link in links if re.search(pattern, link)]
