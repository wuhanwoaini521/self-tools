#!/usr/bin/env bash
# 浏览器 / PWA 方式启动 self-tools：本地数据服务 + 网页前端。
#
#   bash scripts/self-tools-web.sh          # 前台运行，Ctrl+C 停止全部
#   bash scripts/self-tools-web.sh --open   # 额外打开默认浏览器
#   bash scripts/self-tools-web.sh --lan    # 同时允许局域网设备访问（手机/另一台电脑）
#
# 端口已被占用时复用已有进程，不会重复启动；两个进程都已在运行时脚本直接退出。
# 停止：前台按 Ctrl+C。数据读写的是仓库 config/，与桌面开发模式一致。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

API_PORT="${SELF_TOOLS_API_PORT:-8080}"
WEB_PORT="${SELF_TOOLS_WEB_PORT:-1420}"
WEB_HOST="${SELF_TOOLS_WEB_HOST:-127.0.0.1}"
LOG_DIR="$REPO_ROOT/output/web-logs"
OPEN_BROWSER=false
LAN=false
for arg in "$@"; do
  case "$arg" in
    --open) OPEN_BROWSER=true ;;
    --lan) LAN=true ;;
    -h|--help)
      echo "用法: bash scripts/self-tools-web.sh [--open] [--lan]"
      echo "  --open  启动后打开默认浏览器"
      echo "  --lan   监听 0.0.0.0，允许同一 Wi-Fi 下的手机/其它电脑访问"
      exit 0
      ;;
    *)
      echo "未知参数：${arg}（可用 --open / --lan）" >&2
      exit 2
      ;;
  esac
done
if [[ "$LAN" == true && "$WEB_HOST" == "127.0.0.1" ]]; then
  WEB_HOST="0.0.0.0"
fi

mkdir -p "$LOG_DIR"

# 本机在局域网中的 IPv4（macOS：优先 en0/en1/en2 的 Wi-Fi/有线地址）。
lan_ip() {
  local ip=""
  for iface in en0 en1 en2; do
    ip="$(ipconfig getifaddr "$iface" 2>/dev/null || true)"
    [[ -n "$ip" ]] && break
  done
  echo "$ip"
}

port_busy() {
  lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1
}

kill_tree() {
  local pid="$1"
  [[ -z "$pid" ]] && return 0
  local child
  for child in $(pgrep -P "$pid" 2>/dev/null || true); do
    kill_tree "$child"
  done
  kill "$pid" 2>/dev/null || true
}

SERVER_PID=""
WEB_PID=""
WATCHDOG_PID=""

stop_children() {
  local status=$?
  trap - EXIT INT TERM HUP
  kill "$WATCHDOG_PID" 2>/dev/null || true
  kill_tree "$WEB_PID"
  kill_tree "$SERVER_PID"
  wait 2>/dev/null || true
  if [[ -n "$SERVER_PID" || -n "$WEB_PID" ]]; then
    echo
    echo "已停止本地数据服务与网页前端。"
  fi
  exit "$status"
}
trap stop_children EXIT INT TERM HUP

# `make web` / `mise run web` 会让任务运行在独立进程组里：终端 Ctrl+C 可能只打到
# make/mise，本脚本收不到 INT。父进程消失时自行退出并清理，避免留下孤儿服务。
PARENT_PID=$PPID
(
  while kill -0 "$PARENT_PID" 2>/dev/null; do
    sleep 2
  done
  kill -TERM $$ 2>/dev/null || true
) &
WATCHDOG_PID=$!

# ---- 依赖检查（双击启动时不经过 mise，这里补一次）----
if [[ ! -d "$REPO_ROOT/apps/desktop/ui/node_modules" ]]; then
  echo "· 首次运行，安装前端依赖…"
  npm --prefix "$REPO_ROOT/apps/desktop/ui" install --no-audit --no-fund
fi

# ---- 本地数据服务（127.0.0.1:8080）----
if port_busy "$API_PORT"; then
  echo "· 数据服务已在 127.0.0.1:$API_PORT 运行，直接复用。"
elif [[ -x "$REPO_ROOT/target/debug/devtoolbox-server" ]]; then
  echo "· 启动数据服务（target/debug/devtoolbox-server，日志 output/web-logs/server.log）"
  "$REPO_ROOT/target/debug/devtoolbox-server" >"$LOG_DIR/server.log" 2>&1 &
  SERVER_PID=$!
else
  echo "· 未找到预编译服务，改用 cargo run（首次会编译，日志 output/web-logs/server.log）"
  cargo run -p devtoolbox-server >"$LOG_DIR/server.log" 2>&1 &
  SERVER_PID=$!
fi

# ---- 网页前端（127.0.0.1:1420）----
if port_busy "$WEB_PORT"; then
  echo "· 前端已在 :$WEB_PORT 运行，直接复用。"
  if [[ "$LAN" == true ]]; then
    echo "  （若它不是用 --lan 启动的，只监听 127.0.0.1，局域网设备仍然连不上；请先停止再重跑 --lan）" >&2
  fi
else
  echo "· 启动前端（Vite，监听 ${WEB_HOST}:${WEB_PORT}，日志 output/web-logs/ui.log）"
  (
    cd "$REPO_ROOT/apps/desktop/ui"
    exec ./node_modules/.bin/vite --host "$WEB_HOST" --port "$WEB_PORT" --strictPort
  ) >"$LOG_DIR/ui.log" 2>&1 &
  WEB_PID=$!
fi

# ---- 等待前端就绪 ----
ready=false
for _ in $(seq 1 80); do
  if curl -fsS -o /dev/null "http://127.0.0.1:$WEB_PORT/" 2>/dev/null; then
    ready=true
    break
  fi
  if [[ -n "$WEB_PID" ]] && ! kill -0 "$WEB_PID" 2>/dev/null; then
    break
  fi
  sleep 0.5
done

if [[ "$ready" != true ]]; then
  echo "前端没有在 40 秒内就绪，最近日志：" >&2
  tail -n 20 "$LOG_DIR/ui.log" >&2 || true
  exit 1
fi

echo
echo "网页版已就绪： http://127.0.0.1:$WEB_PORT"
if [[ "$WEB_HOST" == "0.0.0.0" || "$WEB_HOST" == "::" || "$WEB_HOST" == "*" ]]; then
  LAN_ADDR="$(lan_ip)"
  if [[ -n "$LAN_ADDR" ]]; then
    echo "局域网访问：  http://$LAN_ADDR:$WEB_PORT  （手机/其它电脑需连同一 Wi-Fi）"
    echo "局域网注意：HTTP 下浏览器不提供 Service Worker，无法离线缓存或安装成桌面 App；"
    echo "            需要安装请改用 HTTPS（mkcert 自签证书，或先在本机 127.0.0.1 用）。"
  else
    echo "未检测到局域网 IPv4（en0/en1/en2）；用 ifconfig 查本机地址后访问 :${WEB_PORT}。" >&2
  fi
fi
echo "添加到桌面：Chrome/Edge 点地址栏右侧「安装」图标；Safari 用 文件 → 添加到程序坞。"
echo "数据目录：$REPO_ROOT/config（与桌面开发模式共用）"
echo "停止：在此窗口按 Ctrl+C，或关闭窗口。"

if [[ "$OPEN_BROWSER" == true ]]; then
  open "http://127.0.0.1:$WEB_PORT"
fi

if [[ -z "$SERVER_PID" && -z "$WEB_PID" ]]; then
  echo "两个进程本来就在运行，脚本退出。"
  exit 0
fi

# 保持前台，直到 Ctrl+C / 关闭窗口 / 子进程退出。
wait
