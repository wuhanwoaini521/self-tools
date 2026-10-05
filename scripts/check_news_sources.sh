#!/usr/bin/env bash
# 复核推荐新闻源目录。
#
# ## 为什么需要它
#
# 2026-10-05 的实测：11 个种子源里 **6 个已经死了**（404 / 401 / 返回 HTML），
# 还有 1 个（人民网）**不报错但内容停在 2025-06**。用户只看到「今日新闻少得可怜」，
# 没有任何线索指向「这些地址早就失效了」。
#
# 目录（`crates/core/src/news.rs` 的 `RECOMMENDED_SOURCES`）里每条都带 `verified_on`，
# 这个脚本就是用来**更新那个日期**的：跑一遍，把 dead 的换掉，把还活着的日期刷新。
#
# 用法：
#   scripts/check_news_sources.sh          # 全部列出
#   scripts/check_news_sources.sh --quiet  # 只列出不正常的
#   scripts/check_news_sources.sh --url <feed>   # 只查一个地址（排查用）
#
# 判定口径（刻意保守，避免误报）：
#   DEAD  = 连续 3 次都 404/401/403，或始终不是 feed —— 需要换源；
#   FLAKY = 时好时坏 / 限流 / 超时 —— 不用换源，但要在意；
#   STALE = 能拉但最新条目超过 10 天 —— 源还活着但已停更（最容易被忽略的一种坏）；
#   ok    = 正常。
set -uo pipefail

QUIET=0
ONLY_URL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --quiet) QUIET=1; shift ;;
    --url) ONLY_URL="${2:-}"; shift 2 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

UA="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CATALOG="$REPO_ROOT/crates/core/src/news.rs"
STALE_DAYS=10
ATTEMPTS=3

if [[ -n "$ONLY_URL" ]]; then
  URLS="$ONLY_URL"
else
  # 从目录里抽 feed 地址（只取含 rss/feed/atom 的，避开站点首页）。
  URLS=$(grep -oE 'https?://[^"]+' "$CATALOG" | grep -E 'rss|feed|atom' | sort -u)
fi

# 抓一次：输出 "<http_code>\t<body>"（body 存临时文件，避免超长字符串进变量）。
TMP_BODY=$(mktemp)
trap 'rm -f "$TMP_BODY"' EXIT

fetch() {
  local url="$1"
  curl -sL -m 12 -A "$UA" -o "$TMP_BODY" -w '%{http_code}' "$url" 2>/dev/null
}

# 头部 400 字节里找 feed 标记（先落到变量再 grep：见下方 pipefail 注释）。
is_feed_body() {
  local head
  head=$(head -c 400 "$TMP_BODY" 2>/dev/null)
  # `grep -q` 提前退出会给上游发 SIGPIPE；配合 `set -o pipefail` 会让整条管道
  # 的退出码变成 141，从而把「明明是 feed」判成「不是 feed」。所以用变量中转。
  if printf '%s' "$head" | grep -qiE '<rss|<feed|<\?xml'; then
    return 0
  fi
  return 1
}

# 最新条目距今天数（用 python3 解析 RFC822/ISO8601，比 date 的 -f 稳）。
latest_age_days() {
  python3 - "$TMP_BODY" <<'PY' 2>/dev/null || echo ""
import re, sys, time, datetime
raw = open(sys.argv[1], "rb").read(400_000).decode("utf-8", "ignore")
m = re.search(r"<(?:pubDate|updated|published)>([^<]+)<", raw)
if not m:
    print(""); raise SystemExit
text = m.group(1).strip()
parsed = None
for fmt in ("%a, %d %b %Y %H:%M:%S %z", "%a, %d %b %Y %H:%M:%S %Z", "%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M:%SZ"):
    try:
        parsed = datetime.datetime.strptime(text, fmt)
        break
    except ValueError:
        continue
if parsed is None:
    try:
        parsed = datetime.datetime.fromisoformat(text.replace("Z", "+00:00"))
    except ValueError:
        print(""); raise SystemExit
if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=datetime.timezone.utc)
print(int((time.time() - parsed.timestamp()) // 86400))
PY
}

FAILED=0
FLAKY=0
STALE=0
OK=0
PROBLEMS=()

printf '%-38s %-6s %-6s %-10s %s\n' "URL" "HTTP" "FEED" "LATEST" "状态"
while read -r url; do
  [[ -z "$url" ]] && continue

  codes=()
  feed_yes=0
  for attempt in $(seq 1 "$ATTEMPTS"); do
    code=$(fetch "$url")
    codes+=("$code")
    if [[ "$code" == "200" ]] && is_feed_body; then
      feed_yes=1
      break
    fi
    [[ "$attempt" -lt "$ATTEMPTS" ]] && sleep 1
  done
  code="${codes[-1]}"
  age=$(latest_age_days)
  shown_age="-"
  [[ -n "$age" ]] && shown_age="${age}天前"

  status="ok"
  if [[ "$feed_yes" == "0" ]]; then
    if [[ "$code" =~ ^(404|401|403|410)$ ]]; then
      status="DEAD(http $code)"
      FAILED=$((FAILED + 1))
      PROBLEMS+=("$url  → $status")
    else
      # 200/429/000 都归到「不稳定」：限流与超时不该被当成「源死了」。
      status="FLAKY(http ${codes[*]})"
      FLAKY=$((FLAKY + 1))
      PROBLEMS+=("$url  → $status（限流或超时，不用换源）")
    fi
  elif [[ -n "$age" ]] && (( age > STALE_DAYS )); then
    status="STALE(最新 ${age} 天前)"
    STALE=$((STALE + 1))
    PROBLEMS+=("$url  → $status（源还活着但已停更）")
  else
    OK=$((OK + 1))
  fi

  if [[ "$QUIET" == "1" && "$status" == "ok" ]]; then
    continue
  fi
  printf '%-38s %-6s %-6s %-10s %s\n' \
    "$(printf '%s' "$url" | cut -c1-38)" "$code" \
    "$([[ "$feed_yes" == "1" ]] && echo yes || echo no)" "$shown_age" "$status"
done <<< "$URLS"

echo
echo "活着: $OK   死了: $FAILED   不稳定: $FLAKY   停更: $STALE"
if (( FAILED > 0 || STALE > 0 )); then
  cat <<'TIP'

处理方式（**不要**只把地址写进代码就完事）：
  1. 换一个当前能用的同类源，替换 `crates/core/src/news.rs` 的 `RECOMMENDED_SOURCES`；
  2. 把已死的旧 URL 记进 `crates/infrastructure/src/news_store.rs` 的
     `OUTDATED_SEED_SOURCES`（老用户的库里会有，迁移靠它自动替换）；
  3. 把替换/存活条目的 `verified_on` 更新为今天；
  4. 跑 `cargo test -p devtoolbox-infrastructure health_tests` 确认迁移逻辑仍成立。
TIP
fi
printf '\n%s\n' "${PROBLEMS[@]}"
(( FAILED > 0 )) && exit 1
exit 0
