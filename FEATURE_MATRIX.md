# FEATURE_MATRIX.md

> 历史浏览器验证曾覆盖 16 个页面（1440×900）；不代表 Sprint 11.1 候选的 release binary 交互验收。
> 「数据」列区分**真实数据**与**空/假数据**——空状态本身不算功能可用。

## 页面总览

| # | 页面 | Route | 主要功能 | 数据来源 | Web 端 | 测试 | 问题 |
|---|---|---|---|---|---|---|---|
| 1 | Home | `#home` | 今日概览 / 快捷入口 / Continue | platform today | ⚠️ 部分 | — | 依赖 learning_*（已修） |
| 2 | Review Center | `#review` | 跨模块 SRS 复习 | platform review_cards | ✅ 已修 | — | 无卡片时仅空态 |
| 3 | History | `#history` | 时间轴 / 时期 / 人物 / 事件 / AI 解读 | duckdb | ✅ | 7 个 Rust + 契约测试 | enrichment 5 个 HTTP 端点已接，需登录网关后使用 |
| 4 | Geography | `#geography` | 地图 / 地形 / 3D | geography.db | ◐ API 已接 | Geography/History HTTP tests | 数据量与覆盖度未核实；原生完整交互仍待验收 |
| 5 | Language | `#language` | 学习卡片 / 课程 / 复习 / 错题 | language.db + learning.db | ✅ | 8 个 E2E | — |
| 6 | Study Board | `#study-board` | 手写板 / 笔刷 | `study_boards.db` | ✅ | 5 个 server + 5 前端 | — |
| 7 | News | `#news` | 今日 / 分类 / 搜索 | news.db | ✅ | 14 个 server | 候选来自维护目录并按已订阅 URL 过滤；当前数量依数据而定 |
| 8 | RSS | `#rss` | 订阅 / 阅读 | rss | ✅ | 前端 transport 测试 | `/api/v1/rss/*` 已接，需先登录局域网网关；`fetch_article_url` 拒绝私网地址 |
| 9 | Travel | `#travel` | 城市探索 / AI 行程 | travel.db | ◐ API 已接 | Rust/HTTP/desktop 行程持久化回归 | 行程草稿已由后端持久化；路线估算仍在 React，AI provider 未配置，尚无 live Web 服务 E2E |
| 10 | Markdown | `#markdown` | 编辑 / 工作区 / 大纲 | 本地文件系统 | ❌ | 无 | 对话框已修但 Web 无法实现 |
| 11 | Collections | `#collections` | 专题合集 | platform collections | ✅ 已修 | 3 个 server | — |
| 12 | Knowledge | `#knowledge` | Memory / Docs / Files | 多个 db | ✅ | 3 个 server oneshot + transport 测试 | 读写/扫描/读取全部有 HTTP 端点；仅 `files_open`（唤起系统程序）属桌面能力，网页端显示能力说明 |
| 13 | Graph | `#graph` | 跨模块知识图谱 | platform graph | ✅ 已修 | — | 无 root 时节点少 |
| 14 | Search | `#search` | 全局检索 | platform + memory/docs/files | ⚠️ | 无 | `global_search` 已接；无结果 |
| 15 | Server | `#server` | 家庭服务器 | server_actions.db | ✅ | gateway + server_api + 前端 transport | 读开放；重启仍需网关 admin 标记 + 确认票据 + 审计 |
| 16 | System | `#system` | 就绪状态 / 诊断 | readiness service | ◐ API 已接 | application probes/tests | 13 项检查模型；vision probe 当前 false，尚无 HTTP route integration test |

## 能力覆盖

| 能力 | 状态 |
|---|---|
| LearningEvent | ✅ 平台统一，Language 已接入 |
| LearningProgress / 掌握度 | ✅ 平台统一（`MasteryCalculator`） |
| 复习 SRS | ✅ 平台统一（Review Center + Language 共用） |
| 全局搜索 | ✅ 已接 History / Language / News / Memory / Docs / Files |
| 合集 Collections | ✅ 已接 |
| 全局检索 Graph | ✅ 已接 |
| AI Provider | ⚠️ **未配置**，仅验证「诚实拒绝」 |
| PWA / 移动端 | ✅ 有 Service Worker 与响应式布局 |

## 已修复

| 模块 | 问题 | 状态 |
|---|---|---|
| Language | 自建第二套复习/进度；写入路径全断；Mastered 不可达 | ✅ |
| Language | 跟读无反馈（`speaking::score` 写好却从未被调用） | ✅ |
| Language | 复习只做「认词」，不练「用句子」 | ✅ |
| History / News / AI / Settings | Web 端 `isTauriRuntime` 早退 + 内部错误串泄漏 | ✅ |
| Learning OS | 8 个 client 未迁移 + 无 HTTP 端点 | ✅ |
| Markdown | 原生对话框失败静默 | ✅ |
| Study Board | 笔迹只存 localStorage，网页端与 AI 各存各的 | ✅ |
| Study Board | 列表笔画数恒为 0（SQLite 层硬编码） | ✅ |
| Travel | 主按钮失色 / 图标被撑成巨幅 | ✅ |
| Travel | 行程草稿只在 localStorage；桌面与 Web 无共享持久化 | ✅ `/api/v1/travel/plans` + SQLite，旧缓存只迁移一次 |
| LAN / Web | 无认证 API 只能 loopback；Vite 只监听本机，pad 无法访问 | ✅ `scripts/lan-gateway.mjs` 密码网关 + Host/Origin 重写；`scripts/start-lan-web.ps1` 一键启动 |
| LAN / Web | 登出只清 Cookie、旧会话仍有效；WebSocket 升级头被过滤 | ✅ 服务端撤销会话；升级请求保留 `Connection`/`Upgrade` 并透传 101 |
| RSS / History 解读 | 8+5 个命令只在桌面桥 | ✅ HTTP 端点 + 前端映射，移除 Web 早退 |
| RSS / History 解读 | 新端点无专门回归测试；文章抓取可被重定向到私网 | ✅ 13 端点 oneshot + SSRF 私网/DNS/重定向测试；抓取逐跳校验 |
| CI / 契约 | Web 构建、视觉回归、网关测试与 OpenAPI 漂移未进门禁 | ✅ `web-quality` / `gateway-node-tests` / `contract` jobs；`mise run contract:check` / `test:gateway` |
| CI / History 产物 | 不 checkout submodule 时 Tauri bundle resource 缺失，三平台 `cargo check` 失败；0 字节占位又让桌面启动 panic（E2E code=101）与 7 个 DuckDB 测试 FAILED | ✅ `build.rs` 自动补占位；`HistoryDuckDbRepository::open_or_attach` 让缺失/不可读降级为「History 数据不可用」；fixture 相关测试把 0 字节占位视为缺失 |
| CI / 桌面 E2E | 产品壳改版后 E2E 仍用 `功能导航` / `.app-nav-item` / 英文页面名，全部在第一步超时 | ✅ 一级入口 + 分区子导航 + 更多页面的当前 IA；深链仍覆盖 markdown/knowledge/server/system/graph；AI 空态改用浮动气泡 |
| Knowledge / Web | Memory 写入 / 确认 / 拒绝 / 归档 / 更新、documents/files 扫描与读取只在桌面桥 | ✅ `POST /api/v1/memory*`、`/documents/scan|read`、`/files/scan|metadata|read_text`、`GET /knowledge/status`；`documents/status` 补 `roots`、`files/status` 补明细；路径仍走 `FileService::authorize`，无任意写删；PATCH 请求体改为按端点显式声明 |
| AI 会话 / Web | 会话列表 / 新建 / 改名 / 归档 / 删除 / 追加消息只有 Tauri 命令，抽屉在网页端直接显示「需要桌面运行时」 | ✅ `/api/v1/conversations*` 六个端点复用 `ConversationService` 与 `conversations.db`；`conversationClient` 注入 `CommandTransport`，抽屉移除 Web 早退；非法 role 400 |
| AI 进度 / Web | 网页端收不到 `agent-progress`（`transport.subscribe` 在 HTTP 下是 no-op），也没有断线重放 | ✅ `GET /api/v1/ai/runs/{trace_id}`（状态/游标）+ `/events`（SSE，`event: progress`、`id: seq`、15s 心跳、游标过期 409）；桌面 Tauri 与网页 HTTP 共用 runtime `AiJobEvents` 的同一份载荷/seq 规则（各自进程内扇出）；`AIPanel` 按 `seq` 幂等 |
| Server / Web | 家庭服务器面板与重启操作只有 Tauri 命令；网页端硬阻断，且服务端无法区分登录者 | ✅ `/api/v1/server/{status,services,apps,actions/recent,logs,mcp}` 读开放；写端点要求网关 HMAC 标记 + admin 角色（`RemoteGatewayAdmin`），票据一次性、绑 principal、过期即废，跨 principal 确认/取消 Denied；审计来源 `gateway`，日志先 clamp 再脱敏并标 `untrusted`；直连 8080 写请求 401 |
| 设计系统 | Pixel-Lift 硬阴影 + 结构 token 分散在 4 个主题 | ✅ |

## 未修复（按影响）

1. **仍有多组 desktop-only 命令**：Markdown/本地文件与 `files_open` 仍通过 Tauri；Server（只读面板 + 重启票据）、Geography、RSS、History enrichment、Knowledge（`files_open` 除外）与 Conversation CRUD 已有 HTTP 映射。当前未枚举可复核的“未映射命令总数”。
2. System Readiness 已有 13 项后端检查模型和服务端 route；vision probe 当前 false，PWA/设备会话值由前端拥有，HTTP route 集成覆盖仍需补齐。
3. Geography HTTP 路由使用共享 `GeographyStore`，覆盖 home/search/detail/favorite 和 History crosswalk；本机数据行数及内容覆盖度未核查。
4. Geography/History 完整原生交互、Settings 凭据边界与 live Travel provider 仍未作为本轮 UI 验收项。
