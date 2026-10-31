#!/usr/bin/env bash
# Ask where to save the NCE files, confirm, then run the resumable downloader.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_DIR="${NCE_DOWNLOAD_DIR:-$HOME/self-tools-nce}"

if [[ -t 0 ]]; then
  printf '新概念英语 NCE1–4 将下载到哪个目录？\n保存目录 [%s]: ' "$DEFAULT_DIR"
  IFS= read -r TARGET_DIR
  TARGET_DIR="${TARGET_DIR:-$DEFAULT_DIR}"
else
  if [[ -z "${NCE_DOWNLOAD_DIR:-}" ]]; then
    echo "需要交互式终端输入保存目录，或设置 NCE_DOWNLOAD_DIR=/path/to/folder。" >&2
    exit 2
  fi
  TARGET_DIR="$NCE_DOWNLOAD_DIR"
fi

# Expand a leading ~/ without eval, preserving spaces and other special characters.
if [[ "$TARGET_DIR" == "~" ]]; then
  TARGET_DIR="$HOME"
elif [[ "$TARGET_DIR" == "~/"* ]]; then
  TARGET_DIR="$HOME/${TARGET_DIR#\~/}"
fi

printf '\n下载位置：%s\n已有文件会保留并断点续传。确认开始下载？[y/N] ' "$TARGET_DIR"
if [[ -t 0 ]]; then
  IFS= read -r CONFIRM
else
  CONFIRM="${NCE_DOWNLOAD_CONFIRM:-}"
fi
case "$CONFIRM" in
  y|Y|yes|YES) ;;
  *) echo '已取消。'; exit 0 ;;
esac

exec bash "$SCRIPT_DIR/fetch_nce.sh" "$TARGET_DIR"
