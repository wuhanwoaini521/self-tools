# READY FOR MORNING ACCEPTANCE — V10 → V11

> self-tools V11（Decision Intelligence + Production Hardening + PWA + Personal Hub）
> 早上 10 分钟验收说明。真实验收路径 = 本文件；架构细节见
> `docs/personal-ai/DECISION_LAYER_V10.md` 与 `docs/operations/PRODUCTION_V11.md`。

## Overall Status

| 项 | 状态 |
| --- | --- |
| V10 Decision Intelligence Layer | **PASS**（DecisionEngine / Rule / Jev adapter / shadow / fallback / eval / telemetry） |
| V11 Engineering（config/secrets/lifecycle/health/logs/backup/restore/failure injection） | **PASS** |
| V11 Product（PWA / adaptive UX / conversations / multimodal / study board / language / search / readiness） | **PASS** |
| Security review（V4–V11） | **PASS**（0 Critical / 0 High / Medium 全修） |
| Full regression | **PASS**：990 Rust tests / 0 failed；`cargo check --all-targets` 0 warning；frontend tsc+build PASS；history pipeline validate OK |
| Real Jev | **BLOCKED_EXTERNAL**（无 API key → 纯 Rule 模式；shadow/fallback 已由 Fake 全覆盖） |
| Real LLM key（`settings.json` 的 `ai` 段） | **未配置**（Travel 段有 key，但 PersonalAgent 只读 `ai` 段）→ 配 `ai.base_url + ai.model` 即用 |

## Production Status

| 项 | 状态 |
| --- | --- |
| 生产二进制 | `target/release/devtoolbox-server`（HTTP）/ `devtoolbox-mcp`（MCP） |
| launchd 生命周期 | 生成/安装/卸载/status/start/stop/restart 全部实现（`render_launchd_plist` + `LaunchdCommand`） |
| 优雅退出 | 8 阶段固定顺序（stop accepting → cancel agents → expire → flush audit → close DB → shutdown MCP → shutdown HTTP → release locks） |
| 崩溃恢复 | `runtime/unclean-shutdown` 标记 + `CrashRecoveryReport`（interrupted / expired / rebuildable） |
| 健康 | liveness / readiness / degraded 三态；外部依赖故障 = degraded（不是 down） |
| 备份/恢复 | sha256 manifest + SQLite `VACUUM INTO` 快照 + 恢复演练（真实库） |

## How To Start（真实命令，从仓库推导）

### 开发（本机 Windows）

```bash
# 后端测试/检查
cargo test --workspace                 # 990 passed
cargo check --workspace --all-targets   # 0 warning

# UI（浏览器模式）
npm --prefix apps/desktop/ui install
npm --prefix apps/desktop/ui run dev     # http://127.0.0.1:1420

# UI 单测
npm --prefix apps/desktop/ui test        # vitest

# 只读 History HTTP 服务（Gate 9 试点）
cargo run -p devtoolbox-server -- --bind 127.0.0.1:8080
```

### 生产（macOS 12 家庭服务器）

```bash
cargo build --release -p devtoolbox-server -p devtoolbox-mcp
npm --prefix apps/desktop/ui ci && npm --prefix apps/desktop/ui run build

export SELF_TOOLS_HOME="$HOME/Library/Application Support/self-tools"
mkdir -p "$SELF_TOOLS_HOME"/{config,data,cache,logs,backup,runtime}
chmod 700 "$SELF_TOOLS_HOME" "$SELF_TOOLS_HOME/config"

# 填 config/settings.json（ai / decision / knowledge / server.mcp）
SELF_TOOLS_HOME="$SELF_TOOLS_HOME" target/release/devtoolbox-server --mode production

# launchd（显式安装，见 docs/operations/DEPLOY_MACOS12.md §6）
target/release/devtoolbox-server --launchd generate > ~/Library/LaunchAgents/ai.self-tools.server.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/ai.self-tools.server.plist
```

## URL

| 环境 | URL |
| --- | --- |
| 开发 | `http://127.0.0.1:1420/`（Vite dev） |
| 生产后端默认 bind | `127.0.0.1:8080`（`apps/server/src/main.rs` `DEFAULT_BIND`；`SELF_TOOLS_BIND` 可覆盖） |
| **家庭最终访问** | `https://self-tools.local/`（部署方经反向proxy + 本地 CA / 受信任证书；**不是** `http://192.168.x.x`） |

## HTTPS Status

| 项 | 状态 |
| --- | --- |
| 代码要求 | `DeployMode::Production.requires_secure_context()` → PWA/Service Worker 只在安全上下文注册 |
| Readiness 检查 | `pwa_secure_context`（未 HTTPS = degraded 并说明原因） |
| 部署方输入 | 反向proxy / 本地 CA / 受信任证书（三选一，见 `DEPLOY_MACOS12.md` §4） |
| 当前仓库 | 未含具体证书（部署方提供）→ **BLOCKED_EXTERNAL（部署输入）** |

## How To Install PWA

**Desktop Chromium**
1. 打开 `https://self-tools.local/`
2. 地址栏右侧「安装」图标（或 `⋮ → 安装 self-tools`）
3. 确认 → standalone 窗口启动

**iPad Safari**
1. Safari 打开 `https://self-tools.local/`
2. 分享按钮（↑）→ 「添加到主屏幕」
3. 「添加」→ 主屏图标启动，standalone 模式

**iPhone Safari**
1. Safari 打开 `https://self-tools.local/`
2. 底部「分享」→ 「添加到主屏幕」
3. 「添加」→ 启动后无浏览器 UI（`apple-mobile-web-app-capable`）

> 离线时 App Shell 可打开并显示离线条；AI / 联网搜索 / 远程 MCP / 服务器操作明确不可用（`sw.js` 敏感路径 network-only）。

## AI Provider Status / Vision / Jev / MCP

| 项 | 状态 | 说明 |
| --- | --- | --- |
| AI Provider | **未配置**（`settings.json` 无 `ai` 段） | System → Readiness 显示「AI Provider 未配置」；填 `ai.base_url + ai.model (+ api_key)` 即用 |
| Vision | **Unsupported**（未配置模型 → `ModelCapabilities::text_only()`） | 发图片会收到「当前模型不支持图片」的受控拒绝（不假装分析） |
| Jev | **Not Configured**（无 key）→ 模式强制 `rule` | `decision.mode` 可设 `jev_shadow`/`jev_active`；配 key 后 shadow 先行 |
| MCP Local | **Ready**（`mcp.enabled` + `stdio_enabled` 默认开） | `tools/list` 走 registry |
| MCP Remote | **Disabled**（默认关；无身份提供者则拒绝启动） | fail-closed |
| Search | **Ready**（`GlobalSearchService` 无 LLM 依赖；LLM down 也能搜） | 未注册源时如实报告 degraded |
| Backup | **Ready**（manifest + 快照 + 演练） | 目标不可用时受控错误，不影响服务 |

## Tests（真实运行结果）

```
cargo test --workspace                     990 passed / 0 failed
  devtoolbox-application                   469 passed
  devtoolbox-core                          277 passed
  devtoolbox-infrastructure                182 passed
  devtoolbox-desktop / mcp / server         29 + 14 + 4 + 8 + 7 passed
cargo check --workspace --all-targets        0 warning
frontend: tsc --noEmit                      PASS
frontend: vite build                        PASS
frontend: vitest                            11 passed
history-data-pipeline: backbone validate    OK（periods=31 regimes=64 events=656 stories=3）
visual QA: 5 viewports × 16 routes = 80 screenshots, 0px overflow
```

V9 基线 749 → V11 最终 **990**（+241）。

## Git Status

```bash
git status --short     # clean（最后一个 commit：a711442 + 文档 commits）
git log --oneline -6
```

工作树 clean；快照 commits：`a61a0d5`（V10）→ `79f1450`（V11 工程基础）→
`75aa361`（产品完成 + QA harness）→ `b07da8b`（运行期关键修复 + 视觉 QA）→
`a711442`（故障注入）+ 文档 commit。

## 10 分钟验收路线

| # | 步骤 | 期望 |
| --- | --- | --- |
| 1 | 打开 `https://self-tools.local/` | Personal Hub Home：Ask AI 输入框 + Continue / Learn / 我的知识 / 家庭 四组入口 |
| 2 | Ask AI 输入「你好」 | AI 面板打开；未配置模型时明确提示「未配置」，其他功能正常 |
| 3 | History → 事件详情 → 问「遵义会议为什么重要？」 | AppContext 上报当前实体；回答引用该事件 |
| 4 | Geography → 地点 → 问「这里为什么……？」 | 上下文含 location 实体 |
| 5 | Language → Today → 选句 → 发音练习 | 定性反馈（「接近目标」+ 具体漏读/错读），**没有** 95/100 伪分数 |
| 6 | Study → 画一题 → 「问 AI」 | 快照进入上下文（vision 未配置时明确拒绝，不假装分析） |
| 7 | Search → 输入关键词 | 跨模块结果；单源失败只降级该源 |
| 8 | Server → Dashboard → 请求重启服务 | confirmation 票据 → 确认 → 执行 → audit 可见 |
| 9 | 缩窗到手机宽度（或用手机打开） | 底部导航 + 单列；AI 变底部抽屉/全屏；safe-area 不被遮挡 |
| 10 | System → Readiness → 运行诊断 | 13 项检查状态；只显示 configured/not configured，无 secret |

截图：`docs/qa/v11/{desktop,tablet-landscape,tablet-portrait,mobile,small-mobile}/`

## Known External Blockers（真实）

| 阻塞 | 当前 fallback |
| --- | --- |
| Real Jev API 未配置（无 key） | 纯 Rule 决策（V9 行为冻结）；shadow/fallback 已由 FakeJev 全覆盖 |
| Real LLM key 未配置（`ai` 段空） | `UnconfiguredModelProvider` → 受控「未配置」提示；Documents/Files/Memory/Server/Search/Study Board 全部不依赖 LLM |
| 真实多模态模型不可用 | vision=false → 图片输入受控拒绝（明确说明，不假装分析） |
| 真实 OIDC provider 未配置 | 远程 MCP fail-closed（拒绝启动/拒绝访问） |
| HTTPS 证书未提供（部署输入） | Readiness `pwa_secure_context` = degraded 并说明；手机/iPad 需部署方提供证书 |
| 真实 iPad / iPhone 不在 CI | 视口 + 浏览器模拟 PASS；真机手动验收 pending |

## 截图索引

| 界面 | desktop | tablet-landscape | tablet-portrait | mobile |
| --- | --- | --- | --- | --- |
| Personal Hub Home | `desktop/home.png` | `tablet-landscape/home.png` | `tablet-portrait/home.png` | `mobile/home.png` |
| Study Board | `desktop/study-board.png` | `tablet-landscape/study-board.png` | `tablet-portrait/study-board.png` | `mobile/study-board.png` |
| AI Panel | `desktop/ai-panel.png` | `tablet-landscape/ai-panel.png` | `tablet-portrait/ai-panel.png` | `mobile/ai-panel.png` |
| Server Dashboard | `desktop/server.png` | `tablet-landscape/server.png` | `tablet-portrait/server.png` | `mobile/server.png` |
| System Readiness | `desktop/system.png` | `tablet-landscape/system.png` | `tablet-portrait/system.png` | `mobile/system.png` |
| SafeAction Confirmation | `desktop/safe-action.png` | `tablet-landscape/safe-action.png` | `tablet-portrait/safe-action.png` | `mobile/safe-action.png` |

（全部 16 界面 × 5 视口见 `docs/qa/v11/`；`qa-report.json` 含每张的 overflow/device 断言。）
