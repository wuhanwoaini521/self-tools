# self-tools 统一个人应用改造方案（只读核查 · 未实施）

> ⚠️ **部分过期（2026-11-18）**：本方案基于 `93fe71e`，其后 `main` 已前进 186 个提交，
> 其中已经落地了本方案 P1 想做的事：`apps/server`（axum，版本化 `/api/v1`）、`crates/runtime`
> （装配层，含 `AppCore` / `server_adapters`）、`apps/mcp`，以及 News / Knowledge / Learning /
> TravelPlan / Personal AI 等模块。**因此第 4、5 节的"新建 app-api + 加壳"不再需要**。
> 仍有效的部分：数据归属与同步方向（第 4 节表格）、Notion/Tavily/额度账本尚未落地的判断、
> 以及"命令层与 HTTP 端点行为一致性"的验收思路。
> Windows 基线的实际修复见 `docs/p0-windows-fixes.md`。

- 编写日期：2026-11-17
- 上一版：`C:\Users\Hansen\dev\hermes-research\docs\hub-integration-assessment.md`（首选 hermes-research，**已作废**，理由见第 0 节）
- 本版结论：**承载项目确定为 self-tools（DevToolbox）**，目标形态是「一套 UI + 一套业务 crate + 两个薄壳（Tauri 桌面 / axum Web）」。
- 本文件不含任何密钥；只说明凭据"是否已配置"。

---

## 0. 决定变更说明

你指定 self-tools 作为长期主要项目，这改变了判断的一个前提：我上一版把"Web 单一入口 + 定时后台"当作硬需求，因此排除了桌面壳。既然 self-tools 是主项目，正确做法不是换承载，而是**把它的桌面壳降级成"两个壳之一"**——业务逻辑本来就不在 Tauri 层（`crates/application` 完全不依赖 tauri，已核实），所以加一个 HTTP 壳是加固而不是重构。

被放弃的是 hermes-research 作为仓库载体的方案；**它的代码价值没有丢**：`hrlib` 里的搜索预算、robots 预检、正文抽取、去重与引用校验规则，会成为 Rust 实现的对照规格与测试黄金样本（第 5 节 P2/P4 写明了怎么用）。

---

## 1. 承载项目现状（全部实测）

| 项 | 实测结果 | 命令/依据 |
|---|---|---|
| 仓库 | `E:\Code\self-github\self-tools`，git `main` @ `93fe71e`，工作区干净 | `git status --porcelain` 空 |
| 结构 | Rust workspace 4 包：`crates/{core,application,infrastructure}` + `apps/desktop` | `Cargo.toml` |
| 业务层是否绑定 Tauri | **否**。`crates/application/Cargo.toml` 无 tauri 依赖；Tauri 只出现在 `apps/desktop` | 依赖清单 |
| 规模 | Rust 21,035 行（184 个 `#[test]`）；前端 TS/TSX 15,039 行；`apps/desktop/src/lib.rs` 1,436 行、54 个 `#[tauri::command]` | `wc -l`、`grep -c` |
| 前端调用面 | 79 处 `invoke`，分布在 18 个文件；全部是 `invoke<T>("cmd_name", {args})` 形式 | `grep -rn invoke` |
| Tauri 专属 API（非 invoke） | 仅 3 处：`plugin-dialog` 的 `open/save`、`plugin-opener` 的 `openUrl`（2 文件） | `grep -rn @tauri-apps` |
| 运行时判定 | `isTauriRuntime() = "__TAURI_INTERNALS__" in window`（`ui/src/utils.ts:3`） | 读码 |
| UI 路由 | 无 router，`App.tsx` 用 `useState<PageId>` 做 tab 切换 | `App.tsx:100` |
| UI 构建 | Vite 7 + React 19，dev 端口 1420（`--host 127.0.0.1`），产物 `ui/dist`，Tauri `frontendDist` 指向它 | `vite.config.ts`、`tauri.conf.json` |
| **本机可运行性** | **跑不起来**：`cargo`/`rustc` 不存在；无 `target/`；`apps/desktop/ui` 无 `node_modules`；项目 `config/` 为空 → 从未在本机运行 | `command -v cargo` 空、`ls` |
| **编译链** | **缺失且受限**：无 Visual Studio / MSVC（`vswhere.exe` 不存在）、无 Windows SDK、无 clang/gcc；`net session` 拒绝访问、`whoami /groups` 无 Administrators → 当前账号非管理员 | 见第 7 节风险 R1 |
| WebView2 | 已存在（`Program Files (x86)/Microsoft/EdgeWebView/Application`） | 目录存在 |
| WSL | WSL2 + Ubuntu 已注册（用户级，无需管理员） | `wsl.exe --status`、`HKCU\...\Lxss` |
| 存储布局 | 每模块一个 SQLite：`config/{dashboard,travel,history,geography,language}.db`（`history` 另有 DuckDB 语义层） | `crates/infrastructure/**/store.rs` |
| LLM | `OpenAiCompatibleLlmProvider`，`base_url` 可指 `http://localhost:11434/v1`（代码注释明确写了 Ollama） | `crates/infrastructure/src/travel/llm.rs` |
| 搜索 | `SearchProvider` trait + Bing 中国(主)/百度(备)/SearXNG；**无 Tavily** | `crates/infrastructure/src/travel/search.rs:45` |
| Notion | **无任何实现**（全仓库 grep 无 notion） | `grep -ri notion` 仅命中无关文档 |
| 图片 | `CityGuide` 无图片字段（`grep image/photo/media` 于 `guide.rs` 为空） | 读码 |
| 数据 | 本机零业务数据（`config/` 空）→ **无迁移成本** | `ls config` |

> 结论：这是一份**写得完整但从未在本机跑过**的工程。184 个 Rust 测试与 600+ 断言都是"代码里存在"，不是"跑通过"。P0 的第一件事就是消除这个未知。

---

## 2. 目标形态：一套 UI + 一套业务 + 两个薄壳

```
                      ┌─────────────────────────────┐
   浏览器 / 手机  ───► │  apps/server (axum + tokio) │──┐
                      └─────────────────────────────┘  │
                                                      ├──► crates/app-api      ← 新增：命令层（传输无关）
                      ┌─────────────────────────────┐  │      ├─ AppState / AppPaths
   Tauri 窗口     ───► │  apps/desktop (Tauri 2)     │──┘      └─ 54 个命令体（从 lib.rs 搬出）
                      └─────────────────────────────┘
                                   │
                    crates/application（用例/编排/任务）
                                   │
                    crates/core（领域规则，纯函数）
                                   │
                crates/infrastructure（SQLite / DuckDB / reqwest / 新：Notion·Tavily·robots·额度）
```

关键点：**不是两套应用**。UI 只有一份（`apps/desktop/ui`，同一个 Vite 工程出两个产物：Tauri 内嵌 + 浏览器直开），业务只有一套（application/core），两个壳各自翻译一次。

### 2.1 为什么这样满足"一个入口"

- 一个前端入口：`http://127.0.0.1:8787`（Web）与桌面窗口加载的是同一份 `ui/dist`，导航、主题、页面完全一致。
- 一个后端应用：两个壳都调用 `crates/app-api` 的同一批函数；不存在"桌面走一套、Web 走另一套"的分叉。
- 统一配置/日志/任务：落在 `crates/application`（任务）与 `config/`（设置），两个壳共用。

### 2.2 传输层设计（决定改造成本的关键）

现状：79 处 `invoke("name", {args})`，散在 18 个文件。

做法：新增 `ui/src/lib/ipc.ts`，导出**同名同签名**的 `invoke`：

```ts
// ui/src/lib/ipc.ts
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if ("__TAURI_INTERNALS__" in window) {
    const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
    return tauriInvoke<T>(cmd, args);
  }
  const res = await fetch(`/api/cmd/${cmd}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  const payload = await res.json();
  if (!res.ok) throw payload;            // 保持 {code,message} 形状，前端错误处理零改动
  return payload as T;
}
```

于是 18 个文件的改动只有 **import 一行**（`@tauri-apps/api/core` → `../../lib/ipc`），79 处调用点一行不动。`plugin-dialog` / `plugin-opener` 各包一层 `lib/platform.ts`（Web 模式用 `<input type=file>` 与 `window.open`，并新增一个 `POST /api/fs/list` 供工作区选择）。

服务端 `POST /api/cmd/{cmd}` 用一张显式分发表（不是反射），54 个命令逐个接线、逐个补测试——这样"未接线的命令"在编译期就是错误，不会变成运行期 404。

---

## 3. 目标模块 ↔ 现有实现对应表

| 目标模块 | self-tools 现有 | 复用方式 | 必须新写 |
|---|---|---|---|
| **新闻资讯** | RSS 全套：`feeds`/`articles` 表、feed-rs 解析、后台刷新、全文抓取（`crates/infrastructure/src/rss_store.rs`、`application/src/rss_workflows.rs`、`ui/features/rss/`） | 解析与抓取直接复用；新闻模块作为 RSS 之上的"加工层"，**不改 articles 表结构**，新建 `news_*` 表 | 分类与关键词过滤、单源限流、跨源标题去重、跨日已读、批量一句话摘要、每日归档 md、Notion 发布、条级已读/收藏 |
| **旅行探索**（问题 → 带来源图文） | Travel：`query_planner` → Bing/百度 → 自研正文提取 → LLM 抽事实 → `SourceAuthority` 分级 → `verify_facts` 多源验证 → `CityGuide` 落 SQLite（`crates/{core,infrastructure}/src/travel/`） | 全链路复用；把"城市"泛化为"主题/问题"（现有 `TravelResearchRequest` 需要放宽为 `TopicRequest`） | Tavily 作为首选 provider、robots 预检、正文质量门、`sources` 中央表、**图片链路**（当前无任何图片字段）、跨主题来源复用 |
| **知识学习** | 无对应模块。可借：`language` 的间隔复习/进度机制、`out/` 产物的"已读"语义 | 复习调度、进度视图的数据形状可参照 `language` | 全部（topics / reads / feedback / 建议），但可做薄 |
| **旅行计划** | `CityGuide` 已有 `itinerary_days` / `ItineraryStop` / `AccommodationArea.budget`（`core/src/travel/guide.rs`），但**只生成、不可编辑**；无待办清单 | 字段形状直接沿用；`travel_guides` 表已有缓存 24h 机制 | 用户可编辑的 `trips/trip_days/trip_items/trip_budget/trip_todos` CRUD、导出（md/ics）、与 explore 来源互链 |
| **公共能力** | LLM（OpenAI 兼容，可指 Ollama）、reqwest 共享 client、SQLite/DuckDB 适配器 | 保留 | `TavilyProvider`（实现现有 `SearchProvider` trait）、`notion.rs`、`robots.rs`、`quota.rs`（Tavily 额度账本）、本地回环守卫（对齐 hrlib 的"拒绝非回环"）、正文缓存表 |
| **任务/日志/配置** | 任务：`travel_sessions: HashMap<session, events>` + 前端轮询（`travel_research_start/progress`）；配置：`settings_store.rs`；日志：无 | 轮询模式沿用 | `jobs` 表 + 通用 `job_start/job_status/job_cancel`；每 job 一份日志文件；`/api/health`；配置页 |

### 3.1 明确的复用边界（避免重复造）

| 共享对象 | 落点 | 规则 |
|---|---|---|
| 网页正文 | 新表 `source_pages`（`hub.db`）+ 现 `HttpWebFetcher` | 按规范化 URL 唯一；news/explore/learn/trips 全部引用 `source_id`，不再各抓一遍 |
| 搜索 | `SearchProvider` 链 + `quota` 表 | 每次搜索先记账；免费额度用尽直接失败，不静默换服务 |
| 模型 | 现 `LlmProvider` + 新增串行队列 | 8 GB 显存下同一时刻只跑一个生成；`base_url` 非回环时按"个人模式"拒绝 |
| 任务 | `jobs` 表 + tokio task | 所有抓取/生成都在任务里；HTTP 只入队与查状态 |
| 配置 | `config/settings.json`（现 `AppSettings`）+ 环境变量（密钥） | 密钥只来自环境；不新增 `.notion-token` 式第二份副本 |

---

## 4. 数据归属与同步方向

现有 5 个库**不动结构**；新功能全部落在新库 `config/hub.db`（纯增表，无迁移）。

| 内容 | 唯一事实源 | 副本/出口 | 同步规则 |
|---|---|---|---|
| 网页正文、抓取时间、提取器、状态 | `hub.db.source_pages` | — | 只由 fetch 任务写；其他模块只读 |
| 新闻条目、摘要、已读/收藏 | `hub.db.news_items` | `export/news/<日期>/*.md`、Notion 每日页 | 库 → 文件 → Notion。Notion 页**首次创建后不再覆盖**；同日重跑默认跳过，`force` 追加"版本块" |
| 探索成文 | `hub.db.explore_articles`（正文 + `source_ids`） | `export/explore/<topic>/<ts>/article.md`、Notion 专题页 | 库 → 出口。**Notion 编辑不回流**；需要时显式"从 Notion 导入"，产生 `version+1` |
| 学习记录（主题/已读/反馈/建议） | `hub.db.topics|read_records|feedback` | 可选导出到 `C:\Users\Hansen\OneDrive\Obsidian笔记` | **单向导出**，不解析 Obsidian 回写（避免两套笔记互相覆盖） |
| 旅行计划 | `hub.db.trips|trip_days|trip_items|trip_budget|trip_todos` | md/ics 导出 | 单向导出；Notion 只做"分享快照页" |
| 旅行研究缓存 | 现 `config/travel.db`（保留） | — | 不改；新表通过 `source_id` 关联 |
| 任务与日志 | `hub.db.jobs` + `config/logs/jobs/<id>.log` | — | 页面可读；保留 30 天 |
| 配置 | `config/settings.json`（非密钥）+ 环境变量（密钥） | — | 单一来源；页面只显示"已配置/未配置" |

一句话：**库是事实源，文件与 Notion 都是出口；任何回流必须显式且带版本。**

---

## 5. 阶段计划与验收标准

每个阶段独立可交付、可回退。P0 是硬门。

### P0 先让它在本机跑起来（0.5–1 人日，主要是下载等待）
- 安装 rustup + MSVC Build Tools（含 Windows SDK），`make install` 装前端依赖。
- 跑 `cargo test --workspace`、`npm --prefix apps/desktop/ui run build`、`make dev`。
- **验收**：`cargo test --workspace` 报告 184 项全通过（如实记录实际数字）；`ui build` 成功；桌面窗口能打开并显示 RSS 列表（空库也应正常）。**若 P0 无法完成，后面全部不启动**（见 R1 的三条退路）。

> **P0 已实测（2026-11-17）**，详见 `docs/p0-verification-report.md`：
> 工具链就绪（Rust 1.100 / MSVC 14.44 / SDK 10.0.26100）；`cargo test --workspace` **202 passed / 0 failed**；
> 前端构建通过；桌面端**首次启动崩溃**（`semantic_history_path` 期望 `data/normalized/history.duckdb`，
> 实际库在 `dist/`），补该文件后可正常启动；RSS（真实抓取 20 篇）、Language（Starter Pack +1113 条）、
> History（DuckDB 语义层）、Geography（MapLibre DEM）均实测通过。
> **因此 P1 前插入 P1.0：修语义库路径三级回退 + History 未安装空态。**

### P1 抽出命令层 + 加 Web 壳（3–5 人日）
- 新建 `crates/app-api`：搬 `AppState`/`AppPaths`（把 `AppHandle` 换成注入的 `config_dir`）与 54 个命令体；`apps/desktop/src/lib.rs` 只留 `#[tauri::command]` 转发。
- 新建 `apps/server`（axum 0.8 + tokio）：`/api/health`、`POST /api/cmd/{name}`（显式分发）、静态托管 `ui/dist`、`/api/fs/list`（Web 端选工作区）。
- 前端加 `lib/ipc.ts`、`lib/platform.ts`，改 18 个 import。
- **验收**：① `cargo test --workspace` 仍全绿；② 浏览器打开 `127.0.0.1:8787` 与桌面窗口看到同一批数据（同一 config 目录）；③ 至少这 12 个命令在 HTTP 下逐一实测并记录：`list_rss_feeds`、`latest_rss_articles`、`mark_rss_article_read`、`get_settings`、`put_settings`、`travel_recent_guides`、`travel_load_guide`、`geography_home`、`language_languages`、`history_home`、`read_document`、`write_document`；④ Web 模式下 `openUrl` 用新标签打开、文件选择走 `/api/fs/list`；⑤ 未接线的命令在 Rust 编译期报错（分发是 match 全量）。

### P2 公共能力层（4–6 人日）
- `infrastructure`: `TavilyProvider`（basic、禁 auto_parameters、429/432/433 → 停止）、`robots.rs`、`quota.rs`、`notion.rs`（create/get/append/query）、`source_pages` 表与正文缓存、回环守卫。
- **验收**：① Tavily 用 key 跑一次 basic 搜索落库并记账（额度表 +1，`credits` 写入）；② robots 全站 Disallow 的 URL 在抓取前被跳过；③ `base_url=http://example.com/v1` 被拒绝（单测，不发请求）；④ Notion 连接测试走只读接口（`GET /v1/users/me`），失败只提示不写入；⑤ 上述全部有单测，`cargo test` 覆盖新增模块。

### P3 新闻模块（4–6 人日）
- `news_*` 五张表 + 采集 job + 摘要 + 归档 md + Notion 发布开关；UI 新增「资讯」页（分类、条数、失败源）。
- 对照样本：`C:\Users\Hansen\dev\news-digest\digests\2026-11-17\*.md` 作为**格式与字段**基准（不要求条目一致）。
- **验收**：① 同日重跑：`news_items` 无重复（唯一键 `(category, canonical_url)`），Notion 不产生第二个页面；② 单源最多 2 条、跨日不重复出现（对照 `state/seen.json` 语义）；③ 人为让一个源 429：该源标红，其他分类照常产出；④ `--dry-run` 等价开关下不写库、不推 Notion；⑤ 关闭 Ollama 时降级为纯标题，不产出空摘要。

### P4 探索模块（5–7 人日）
- 把 Travel 的"城市"泛化为"主题"；接 Tavily 首选 + robots + 来源表；成文带 `[资料N]` 引用与引用校验；图片先做"无图也发布"的诚实版。
- 黄金样本：`C:\Users\Hansen\dev\hermes-research\out\*\*/sources.json`（11 组真实抓取结果，含 body/status/extractor）——拷进 `tests/fixtures/` 作为正文抽取与去重的回归样例。
- **验收**：① 同一 URL 在两侧抽取出的正文长度差异 <20%、`status` 判断一致；② 引用校验能识别"文中出现本轮未抓取的链接"并告警；③ Ollama 离线 → 明确失败且**不写 article 记录**；④ 某站 403 → 记录原因、本轮不再试同站；⑤ 一次探索的搜索次数 ≤ 配置上限（额度表可查）。

### P5 学习 + 旅行计划（5–7 人日）
- `topics/read_records/feedback` + 建议；`trips/*` CRUD + 导出；可把探索成文/来源挂到行程项。
- **验收**：① 标记已读/反馈后重启仍在；② "建议下一步"引用的 `source_id` 全部存在（断链 0）；③ 断网下 trips 全部功能可用；④ 删除 trip 后无孤儿行（外键级联断言）；⑤ 导出 md 含日期、预算、待办勾选状态。

### P6 运维面（2–3 人日）
- `/api/health`（Ollama/Tavily/Notion/数据库可写）、任务中心（历史、失败原因、取消、重试）、日志查看、配置页（模型名、预算上限、能力开关）。
- **验收**：① 把模型名改错 → 任务失败原因在页面可读并指向该项；② 改回后无需重启即成功；③ 密钥在页面只显示"已配置"；④ 日志可按 job 过滤。

### P7（可选，需你定）手机与常驻
- server 常驻（启动文件夹 + 托盘/无窗口模式）、局域网访问 + 口令、手机浏览器可用；定时任务仍走系统计划任务或 Hermes cron，**本轮不新建**。

估算合计：**约 24–34 人日**（不含 P7）。这已含 Rust 编译等待；agent 辅助能压缩编码时间，但 `cargo build` 与 duckdb 首次编译（10–20 分钟）压不下去。

---

## 6. 目录级改动清单（实施时按此落）

```
新增
  crates/app-api/            # 命令层（传输无关）：AppState、AppPaths、54 个 handler、DTO
  apps/server/               # axum 壳：main.rs、routes.rs、dispatch.rs、static_fs.rs、auth.rs
  crates/infrastructure/src/notion.rs     # create/append/get/query + 错误码映射
  crates/infrastructure/src/tavily.rs     # impl SearchProvider
  crates/infrastructure/src/robots.rs
  crates/infrastructure/src/quota.rs
  crates/infrastructure/src/hub_store.rs  # hub.db：sources/jobs/news/learn/trips
  crates/core/src/news/      # 过滤器、去重、摘要解析（纯函数，可单测）
  crates/core/src/trips/     # 行程/预算/待办的领域校验
  crates/application/src/news.rs  jobs.rs  learn.rs  trips.rs
  apps/desktop/ui/src/lib/ipc.ts  platform.ts
  apps/desktop/ui/src/features/news/  learn/  trips/  admin/
  tests/fixtures/{sources,feeds}/*        # 从 hermes-research/out 与 news-digest 拷入的只读样本

改动
  apps/desktop/src/lib.rs     # 1436 行 → 约 200 行（只剩 #[tauri::command] 转发 + 插件注册）
  apps/desktop/ui/src/**.tsx  # 18 个文件的 import 行
  apps/desktop/ui/src/App.tsx # 导航加 4 项
  Cargo.toml                  # workspace members 增加 crates/app-api、apps/server

不动
  crates/core|application|infrastructure 既有模块的接口与 184 个测试
  config/*.db 既有 5 个库的结构
  tauri.conf.json 的打包配置（只在需要时加 CSP）
```

---

## 7. 风险 · 未核实 · 待你决定

### 风险（跑了才可能出问题）
- **R1 编译链是硬门（最高优先级）**：本机无 MSVC、无 Windows SDK、账号非管理员。三条退路见下，必须在 P0 解决：
  1. **（推荐）** 用管理员账号安装 VS Build Tools（C++ 工作负载 + Windows SDK）+ rustup，约 6–8 GB、一次性。之后 MSVC 工具链、Tauri 打包、`cargo test` 全部可用。
  2. **免管理员**：MSYS2 MinGW-w64 + `rustup target add x86_64-pc-windows-gnu`。风险：`duckdb` 是 `bundled`（本地编译 C++ 源码，`crates/infrastructure` 依赖），GNU 下大概率编不过；`rusqlite bundled` 走 C 也不一定稳。若走这条，先做 2 小时 spike，失败就把 history 的 duckdb 换成 SQLite 或加 feature gate（这属于改结构，需要你批）。
  3. **WSL 开发**：WSL2 Ubuntu 已就绪，Linux 下装 rustup 不需要 Windows 管理员，可编译并运行 `apps/server`。代价：产物是 Linux 二进制，桌面壳仍需 MSVC；数据在 `/mnt/e` 上跨 9p 访问 SQLite 性能与锁语义都不可信，需要把 config 迁进 WSL（等于多一套部署）。
- **R2 首次编译成本**：`duckdb bundled` + `rusqlite bundled` 的首次构建在 4 核机上可能 10–20 分钟，之后增量仍慢于 Python/Node。迭代节奏会变。
- **R3 搜索链路仍是 HTML 抓取**：Bing/百度 页面结构一变就失效；Tavily 接入后应把它降级为兜底，而不是主路径。
- **R4 8 GB 显存**：Ollama 被 `ai-guard` 卸模型时任务会失败；必须"明确失败 + 可重试"，不能静默降级成编造。
- **R5 并发写**：`AppState` 现在是 `Mutex<FeedRepository>` 等，Web 壳多线程访问时若长事务持锁会阻塞全站；需要 WAL + 短事务，且 LLM 调用必须在锁外。
- **R6 前后端契约漂移**：54 个命令靠 DTO 手写同步，没有 OpenAPI；建议 P1 顺手生成一份 `docs/api-commands.md` 作为契约基线（`cargo test` 断言命令清单与文档一致）。
- **R7 没有 git 之外的保护**：`config/` 里是真实数据（虽然现在空），`.gitignore` 已排除 `config/`，但新增 `export/` 也要排除，否则 md 归档会被提交。

### 未核实（本轮没做，不当结论用）
- `cargo test --workspace` 是否真能 184 项全绿：**未验证**（无工具链）。
- 184 个测试是否覆盖 `apps/desktop/src/lib.rs` 的命令层：**未验证**（测试集中在 core/application/infrastructure）。
- 现有 Bing/百度 搜索解析今天是否仍有效：**未验证**（需编译后联网跑）。
- `duckdb 1.4 bundled` 在 GNU target 下能否编译：**未验证**（R1 退路 2 的前置）。
- Tauri 2 应用在本机能否正常打开（WebView2 在，但缺 MSVC 运行库的可能性未排除）：**未验证**。
- 新增 `hub.db` 与既有 5 库的跨库查询：SQLite 用 `ATTACH` 可行，但**未做原型**；若不合适，改为"一个库多 schema 前缀表"（需你批）。
- Tavily 剩余额度：**未知**（本轮只做过 1 次 basic 自检）。

### 需要你决定
1. **R1 走哪条**：管理员装 MSVC（推荐）／GNU spike（免管理员但可能卡 duckdb）／WSL（免管理员但多一套部署）？
2. **是否现在开始 P0**：装工具链会下载数 GB、耗时较长；P0 完成前不建议动代码。
3. **既有 Python 项目怎么处置**：`news-digest` 与 `hermes-research` 在 Rust 版达到验收后，是**退役**、还是**保留为无 UI 的应急 CLI**？（我建议保留 CLI 到 P4 结束，之后只保留 `hermes-research` 的抓取样本作用）
4. **一个库还是多个库**：新功能全进 `config/hub.db`（推荐），还是沿用"每模块一个库"的现状？
5. **Notion 语义**：只做发布出口（推荐），还是需要双向？
6. **图片**：探索成文要不要配图？要的话是否只允许 Wikimedia/机构图并记录许可？
7. **现有 5 个模块是否都要进 Web 入口**：我默认全进（同一份 UI，零额外成本），但语言学习依赖 TTS/录音，Web 下体验会打折——是否先在 Web 上隐藏 `language` 的听说面板？
8. **端口与安全**：server 默认只听 `127.0.0.1:8787`；局域网/手机访问是否要开（需要口令，且你的路由器 NAT 语义我不清楚）。

---

## 附：本轮实测命令（只读，可复现）

```bash
cd "E:/Code/self-github/self-tools"
git status --porcelain && git log -1 --oneline
cat Cargo.toml apps/desktop/Cargo.toml crates/*/Cargo.toml
wc -l apps/desktop/src/lib.rs && grep -c "#\[tauri::command\]" apps/desktop/src/lib.rs
grep -rn "invoke" apps/desktop/ui/src | wc -l
grep -rn "@tauri-apps" apps/desktop/ui/src
grep -n "isTauriRuntime" -A2 apps/desktop/ui/src/utils.ts
grep -ri "ollama\|tavily\|notion" --include="*.rs" crates apps | sort -u
npx tsc --noEmit -p apps/desktop/ui/tsconfig.json   # 失败原因：依赖未安装，不计为代码缺陷
command -v cargo rustc; ls -d config
ls "/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"   # 不存在
MSYS_NO_PATHCONV=1 cmd.exe /c "net session"          # 拒绝访问 → 非管理员
MSYS_NO_PATHCONV=1 cmd.exe /c "wsl.exe --status"     # Ubuntu, 版本 2
```

未执行：任何安装、`cargo build`、`git` 写操作、DB 写入、Notion 写、cron 变更。
