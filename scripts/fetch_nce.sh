#!/usr/bin/env bash
#
# 下载新概念英语 NCE1–NCE4 全部课时的字幕 + 音频到本地。
#
# 用法：
#   bash scripts/fetch_nce.sh                # 下载到 ~/self-tools-nce
#   bash scripts/fetch_nce.sh /path/to/dir   # 下载到指定目录
#   bash scripts/fetch_nce.sh /path NCE4     # 只下一册（调试用）
#
# 数据来源：公开仓库 byuc/NCE-Flow（每课一对同名 .lrc 字幕 + .mp3 音频）。
#   - 音频在 Git LFS：必须走 media 端点才拿得到真实字节（raw 只返回指针文件）；
#   - 字幕是普通文件：走 raw 端点。
#
# 本脚本只负责把文件拿到手；导入在应用里做：
#   Language → English → 学习资料 → 选择教材文件夹 → 扫描 → 确认导入。
#
# 体积约 300MB / 276 课。可重复运行（已存在的文件跳过，断点续传）。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="${1:-$HOME/self-tools-nce}"
ONLY_BOOK="${2:-}"
REPO="byuc/NCE-Flow"
MEDIA="https://media.githubusercontent.com/media/$REPO/main"
RAW="https://raw.githubusercontent.com/$REPO/main"

if [ -n "$ONLY_BOOK" ]; then
  BOOKS=("$ONLY_BOOK")
else
  BOOKS=(NCE1 NCE2 NCE3 NCE4)
fi

echo "新概念英语下载器"
echo "  目标目录：$TARGET_DIR"
echo "  册数：${BOOKS[*]}"
echo

command -v curl >/dev/null || { echo "需要 curl，请先安装"; exit 1; }
command -v python3 >/dev/null || { echo "需要 python3，请先安装"; exit 1; }
mkdir -p "$TARGET_DIR"

for book in "${BOOKS[@]}"; do
  mkdir -p "$TARGET_DIR/$book"
  echo "→ ${book}：读取课表…"
  curl -sS --max-time 90 "https://api.github.com/repos/$REPO/contents/$book" \
    | python3 "$SCRIPT_DIR/fetch_nce_book.py" "$book" "$TARGET_DIR" "$MEDIA" "$RAW"
done

echo
echo "下载完成。导入步骤："
echo "  1) 打开 self-tools → Language → English"
echo "  2) 首页点右上角「学习资料」"
echo "  3) 「选择教材文件夹」→ 选中 $TARGET_DIR"
echo "  4) 先「扫描看看」确认无误，再「确认导入」"