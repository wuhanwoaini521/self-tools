#!/usr/bin/env python3
"""下载新概念英语某一册的字幕与音频（由 scripts/fetch_nce.sh 调用）。

每课一对同名文件：
  - `.mp3` 在 Git LFS → 走 media 端点（raw 只会返回指针文件）
  - `.lrc` 是普通文件 → 走 raw 端点

已存在且非空的文件会跳过，因此可以重复运行（断点续传）。
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time
import urllib.parse


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: fetch_nce_book.py <book> <target_dir> <media_base> <raw_base>",
              file=sys.stderr)
        return 2
    book, target_dir, media_base, raw_base = sys.argv[1:5]
    listing = json.load(sys.stdin)
    items = [item for item in listing if item["name"].endswith((".lrc", ".mp3"))]
    total = len(items)
    done = 0
    failed: list[str] = []

    for item in items:
        name = item["name"]
        dest = os.path.join(target_dir, book, name)
        if os.path.exists(dest) and os.path.getsize(dest) > 0:
            done += 1
            continue
        base = media_base if name.endswith(".mp3") else raw_base
        url = f"{base}/{book}/" + urllib.parse.quote(name)
        # GitHub 会对高频请求限流（返回空响应）：重试 3 次并退避。
        ok = False
        for attempt in range(3):
            subprocess.run(
                ["curl", "-sSL", "--max-time", "120", "-o", dest, url],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if os.path.exists(dest) and os.path.getsize(dest) > 0:
                ok = True
                break
            time.sleep(1.5 * (attempt + 1))
        if ok:
            done += 1
        else:
            # 0 字节文件留着会被误认为「已下载」，删掉。
            if os.path.exists(dest):
                os.remove(dest)
            failed.append(name)
        if done % 20 == 0:
            print(f"  {done}/{total}", flush=True)

    print(f"  {book}: 成功 {done}/{total}")
    if failed:
        print(f"  {book}: 失败 {len(failed)} 个（重跑本脚本即可续传）")
        for name in failed[:5]:
            print(f"    - {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())