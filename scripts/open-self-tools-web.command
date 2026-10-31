#!/usr/bin/env bash
# 双击启动：本地数据服务 + 网页前端，并打开浏览器。
# 首次双击若被 Gatekeeper 拦截，右键 → 打开。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

exec bash scripts/self-tools-web.sh --open
