# CURRENT_ARCHITECTURE — Self Tools 现状基线（Gate 0–9，2026-09-15）

> 基线日期：2026-09-13（Overnight Architecture Consolidation 收尾 + Gate 5.5 实施）+ 2026-09-14（Gate 6 实施）
> 方法：所有结论来自真实调用链、文件行号与本次实际执行的验证命令，非文件名推断。
> 上一次架构审计（History V2 Cutover 之前）已归档为历史档案：
> [`docs/migration/09-history-v2-cutover-audit-2026-09-10.md`](docs/migration/09-history-v2-cutover-audit-2026-09-10.md)。
> 一致性/只读审计与 Gate 5.5 分组见 [`ARCHITECTURE_BACKLOG.md`](ARCHITECTURE_BACKLOG.md)；
> 依赖倒置决策见 [`docs/architecture/ADR-001-dependency-inversion-and-composition-root.md`](docs/architecture/ADR-001-dependency-inversion-and-composition-root.md)。

---

## 0. 一句话结论

History V2 Cutover 已经**真实完成**：V1 legacy 在代码中已全部消失，History 只剩
`dist/history.duckdb` 一个只读事实源，缺失时明确报错。

**Gate 8（2026-09-15）**：Travel 依赖倒置完成 —— Provider 契约（Search / WebFetcher /
Llm / TravelData + `ProviderError`）上移到 `crates/core::travel::provider`，infrastructure
只实现、application 只消费；`application → infrastructure` Cargo 依赖清零（实测
`grep devtoolbox_infrastructure crates/application/src` = 0）。

**Gate 9 / 9.5（2026-09-15）**：新增第二个组合根 `apps/server`（axum 0.8）—— 极简
HTTP 运行时，只暴露 History **只读**查询面（7 个 `/api/v1/history/*` 路由 + `/health`），
与桌面端零互通；错误契约 `{"code","message"}` 来自 `ApplicationError`（无 CommandError）；
无鉴权、默认无 CORS；duckdb 路径经 `SELF_TOOLS_HISTORY_DB` / `--history-db` 配置，
缺失即启动失败；默认绑定 `127.0.0.1:8080`，`SELF_TOOLS_BIND` 控制监听地址。

**Gate 2（History Application Boundary，2026-09-12）**：语义聚合从 `apps/desktop/src/lib.rs` 迁入
`crates/application/src/history/`，命令变薄转发，adapter 只留序列化与错误映射（reference 实现）。

**Gate 3 / 3B（Frontend Client Boundary，2026-09-13）**：前端新增 transport 抽象
（`ui/src/transport.ts`）与两个 feature client（`historyClient.ts` / `geographyClient.ts`）。

**Gate 4（Legacy Cleanup，2026-09-13）**：删除全部「已证实 dead」代码（core history、前端孤儿类型、
4 个零消费者命令）。

**Gate 5.5（Dependency Inversion & Composition Root，2026-09-13）**：把「端口（Port）」提升为一等公民——
应用层只依赖端口，桌面组合根（`apps/desktop`）提供适配器实现：

- History（Gate 2 参考实现，本次仅审计，未重排）——已达标；
- **Geography / Documents / Settings / Workspace 本次全部倒置**：Port + port 本地错误在
  `crates/application`（`geography/ports.rs`、`workflows/ports.rs`），适配器在 `apps/desktop`
  （`geography_query.rs`、`composition.rs`），infrastructure 只提供实现；
- 数据契约归一：21 个 history 记录 + settings + `WorkspaceFile` 从 infra 迁到 core
  （`history_records.rs` / `settings.rs` / `workspace.rs`），infra 改为重导出，公开面与 serde 形状不变；
- 错误抽象：port 错误为 port 本地类型（`HistoryPortError` / `GeographyPortError` / `DocumentStoreError` /
  `SettingsStoreError`），`ApplicationError` 变体名不变、用户可见消息逐字保留；
- **组合根只有 `apps/desktop`**（`composition.rs` 64 行 + 两个 `*_query.rs`），不涉及 UI/SQL/业务逻辑。

**Gate 6（Remaining Frontend Client Boundary，2026-09-14）**：前端全部裸 `invoke()` 收敛完毕——

- 新增 6 个 feature client：`rssClient`（8 方法）/ `languageClient`（14）/ `travelClient`（7）/
  `markdownClient`（3）/ `workspaceClient`（1）/ `settingsClient`（2），全部遵循
  「`interface XClient` + `createXClient(transport = tauriTransport)` + 导出 singleton」模式，
  物理放在各自 feature 目录（参考 `historyClient` / `geographyClient`），无 api.ts 上帝对象；
- 前端裸 `invoke`：44 → **0**；`@tauri-apps/api/core` import 全仓库**仅 1 处**（`transport.ts`）；
  `isTauriRuntime()` 分支 15 个消费者文件全部保留（Browser preview 行为不变，原因见 §3）；
- 验证：`npm run build`（tsc + vite）/ `cargo check --workspace --all-targets` /
  `cargo test --workspace`（214）全绿；本 gate 未触碰任何 Rust 代码。

当前架构的**剩余偏差（实测，均已在 BACKLOG §6.2 分级）**：
1. ~~**前端 44 处裸 `invoke()` 调用点**~~（Gate 6 已消除：0 处）—— ✅ 关闭；
2. **Travel / Language / RSS 未倒置**（deferred）：application 仍在 16+ 处直接使用 infra 类型（`LanguageStore`、
   `FeedRepository`、travel providers）—— P0/P1；
3. `Cargo.toml` 的 `application → infrastructure` path 依赖保留（deferred 模块使用）—— P0（server 前置）。

---

## 1. 当前架构（已核实）

### 1.1 Workspace 结构

```text
Cargo.toml                  workspace members: apps/desktop, crates/{core,application,infrastructure}
                            edition 2024, rust-version 1.95（= README 声明，仅低于本机稳定版）
apps/desktop/               唯一 Tauri 入口 + 唯一组合根
  src/lib.rs                980 行：46 个 command adapter + AppState + setup + 组合接线
  src/composition.rs        64 行（Gate 5.5）：组合根成员 —— DocumentStoreAdapter / SettingsStoreAdapter
  src/geography_query.rs    108 行（Gate 5.5）：GeographyQueryPort 桌面适配器
  src/history_query.rs      132 行（Gate 2）：HistoryQueryPort 桌面适配器（reference adapter）
  src/main.rs               15 行
  ui/src/                   React 19 + Vite 7 + TS 5
    src/transport.ts                                    CommandTransport + tauriTransport（Gate 3；唯一 `@tauri-apps/api/core` import 点）
    src/settingsClient.ts                           SettingsClient（get/put，Gate 6）
    src/workspaceClient.ts                         WorkspaceClient（list，Gate 6）
    src/features/history/historyClient.ts         7 命令薄 client（Gate 3）
    src/features/geography/geographyClient.ts     4 命令薄 client（Gate 3B）
    src/features/rss/rssClient.ts                 8 命令薄 client（Gate 6）
    src/features/language/languageClient.ts       14 命令薄 client（Gate 6）
    src/features/travel/travelClient.ts           7 命令薄 client（Gate 6）
    src/features/markdown/markdownClient.ts       3 命令薄 client（Gate 6）
crates/core/                纯领域 + 跨 crate 数据契约
  history_records.rs        21 个只读结果记录（Gate 5.5，原在 infra/duckdb.rs）
  settings.rs               AppSettings / GeographySettings / TravelSettings / TravelSearchBackend / ThemeMode / MarkdownView（Gate 5.5）
  workspace.rs              WorkspaceFile（Gate 5.5）
  geography/ language/ travel/ task_state/...  领域模块
crates/application/         use case + 端口定义：workflows / rss_workflows / geography / language / travel / history
  history/ports.rs          HistoryQueryPort（31 方法）+ HistoryPortError（reference）
  geography/ports.rs        GeographyQueryPort（10 方法）+ GeographyPortError（Gate 5.5）
  workflows/ports.rs        DocumentStorePort（3）+ SettingsStorePort（3）+ errors + #[cfg(test)] fakes（Gate 5.5）
  language/ports.rs         LanguageStorePort（Gate 7.5）
  rss/ports.rs              RssRepositoryPort / FeedFetcherPort（Gate 7.6）
  travel/ports.rs           TravelStorePort（Gate 8）+ mocks/tests（FakeStore，无 SQLite）
  src/bin/language_data.rs  CLI（随 lib 编译，server 化时再定归属）
crates/infrastructure/      SQLite / DuckDB / 文件 / HTTP / 数据导入（只实现，不持端口/不持契约）
history-data-pipeline/      独立 submodule（Python），产出 dist/history.duckdb
```

### 1.2 各模块真实调用链（已核实）

| 模块 | 前端入口 | Tauri 命令 | Application 层 | Infrastructure |
|---|---|---|---|---|
| Markdown | `markdownClient.ts`（Gate 6）+ `workspaceClient.ts` | `read_document` / `write_document` / `list_workspace` / `cycle_task_lines` | `workflows.rs`（经 `DocumentStorePort`） | `document_store.rs` / `workspace_scanner.rs` |
| RSS | `rssClient.ts`（Gate 6）：RssPage + App | `list_rss_*` / `add_rss_feed` / `refresh_rss_feeds` / `fetch_article_url` … | `rss_workflows.rs`（直引 infra 类型，P1） | `rss_store.rs` / `feed_fetcher.rs` |
| Travel | `travelClient.ts`（Gate 6）：TravelPage + SettingsDialog | `travel_research_start` / `_progress` / `_recent_guides` / `_load_guide` | `TravelResearchService`（直引 infra provider，P0） | `travel/*.rs` |
| Geography | **`geographyClient.ts`**（Gate 3B） | **4 个**：`geography_home` / `geography_search` / `geography_detail` / `geography_toggle_favorite` | `GeographyService`（经 `GeographyQueryPort`） | `geography/store.rs` |
| Language | `languageClient.ts`（Gate 6）：8 个面板 + App + Settings | `language_*` | `LanguageService`（直引 infra store，P1） | `language/store.rs` + import/* |
| **History** | **`historyClient.ts`**（Gate 3） | **7 个** `history_semantic_*`（全薄） | `history/`：`HistoryService` + `HistoryQueryPort` | `history/duckdb.rs`（reference 链条 §1.3） |

> Gate 6 后：前端**所有**页面/外壳的 Tauri 调用都经过 8 个 feature client（history / geography / rss /
> language / travel / markdown + root 的 workspace / settings），无任何裸 `invoke`；
> 后端分层状态不变：History / Geography / Documents / Settings / Workspace 已倒置（经 Port），
> RSS / Language / Travel 仍为「service 直引 infra 类型」（deferred，见 BACKLOG §6.2）。

### 1.3 History 现状（V2，唯一事实源，Reference 实现链条）

```text
HistoryPage.tsx                     → historyClient.ts（7 方法 = 7 命令）
                                         ↓ tauriTransport.invoke<T>("history_semantic_*")
transport.ts  tauriTransport        （switch 点：未来 PWA 用 HTTP transport 替换）
   ↓ TauriJSON 契约不变（命令名/参数/返回形状/错误码冻结）
history_semantic_{home,period,story,event,person,work,search}   lib.rs（薄转发）
   ↓ HistoryService.home() / period_detail() / …                      crates/application/src/history/service.rs
   ↓ HistoryQueryPort（31 个只读方法，port 本地错误类型）               crates/application/src/history/ports.rs
HistoryQueryAdapter（newtype 适配器，这是 ADR-001 的 reference 形态）   apps/desktop/src/history_query.rs
   ↓
HistoryDuckDbRepository（只读 SELECT，infra 重导出 core 记录）          crates/infrastructure/src/history/duckdb.rs
   ↓
history-pipeline/dist/history.duckdb  （V2 唯一事实源）
```

- 路径解析：`semantic_history_path()` 只认 `history-data-pipeline/dist/history.duckdb`，无 fallback；
  缺失时在 setup 报 `history_data_missing`（不在命令映射内，Gate 2 后未变）。
- 错误链：`InfrastructureError →(adapter) HistoryPortError →(service) ApplicationError::History（transparent，消息不变）→
  CommandError { code: "history_error" }`；无静默 fallback。
- 用例决策（全在 application，命令零决策）：`semantic_source_ids()` 去重排序、`search()` 组序
  person→story→event→work、period_detail/work_detail 解析、空输入返回空。
- DTO 所有权在 2026-09-13 迁到 core（`devtoolbox_core::history_records`），infra `pub use` 重导出保持公开面。

### 1.4 Geography（Gate 3B 迁移 + Gate 5.5 倒置后）

```
GeographyPage.tsx      → geographyClient.ts（4 方法）→ transport
lib.rs: geography_home（cursor） / geography_search（type） / geography_detail（id） /
        geography_toggle_favorite（id）        ← 全薄转发
GeographyService（home/search/detail/toggle_favorite）
    ↓ 依赖端口：GeographyQueryPort（10 方法，application 侧定义）
GeographyQueryAdapter（desktop：newtype 包装 `Arc<Mutex<GeographyStore>>`，错误映射为 GeographyPortError）
    ↓
GeographyStore（SQLite，用户数据；infra 只实现）
```

错误链：`InfrastructureError →(adapter) GeographyPortError → ApplicationError::Geography { message } →
 CommandError { code: ... }`；消息逐字保留（`Geography { message }` 无 source 字段）。

### 1.5 依赖倒置与组合根（Gate 5.5「After」图，全部为真实当前结构）

```
                    ┌──── application（端口定义） ────┐      ┌── desktop 组合根（适配器）──┐      ┌─ infrastructure（实现）──┐
                    │                                  │      │                               │      │                          │
 HistoryService ──▶ │ HistoryQueryPort（31）           │◀────│ HistoryQueryAdapter            │────▶│ HistoryDuckDbRepository   │
                    │                                  │      │                               │      │                          │
 GeographyService ─▶│ GeographyQueryPort（10）         │◀─────│ GeographyQueryAdapter          │────▶│ GeographyStore            │
                    │                                  │      │  （Arc<Mutex<GeographyStore>>）│      │                          │
 workflows::        │ DocumentStorePort（3）           │◀─────│ DocumentStoreAdapter           │─────▶│ document_store::           │
  load/save/scan    │ SettingsStorePort（3）           │◀──────│ SettingsStoreAdapter           │─────▶│ read_utf8 / write_utf8_...  │
                    │  （port 本地错误，`#[cfg(test)]`  fakes）│（composition.rs 组合根成员）    │      │ settings_store::SettingsStore│
                    └──────────────────────────────────┘      └───────────────────────────────┘      └──────────────────────────┘
```

- **端口**只存在于 application（用例消费者侧）；实现（SQLite/DuckDB/文件）在 infra；
- **适配器 + 组合**存在于两个平台组合根：`apps/desktop`（Tauri）与 `apps/server`（Gate 9）；
- **模型真相**：Gate 8 后 application 对 `devtoolbox_infrastructure` **零引用**
  （`grep -rn devtoolbox_infrastructure crates/application/src` = 0），无 deferred；
- **fakes**位于端口对（`workflows/ports.rs#[cfg(test)]`），应用测试不触真实文件系统（fake port 驱动）。

---

## 2. 依赖规则（Gate 5 审计 + Gate 5.5 实施后）

### 2.1 实际依赖（来自各 `Cargo.toml`，Gate 5.5 后）

```text
devtoolbox-core            → regex / serde / serde_json / thiserror（无内部依赖）
devtoolbox-infrastructure  → devtoolbox-core
devtoolbox-application     → devtoolbox-core（Gate 8：infra 依赖已拆除）
devtoolbox-desktop         → application + core + infrastructure + tauri
devtoolbox-server          → application + core + infrastructure + axum 0.8 + tokio（Gate 9，与 desktop 互不依赖）
```

### 2.2 与目标架构的偏差（已核实 + 处置，语义同 BACKLOG §6.2）

| 目标 | 现状 | 处置 |
|---|---|---|
| Application → Core；Infrastructure 只做实现 | **全部模块已倒置**（History / Geography / Workflows / Language / RSS / Travel） | ✅ Gate 5.5 + 7.5 + 7.6 + 8；`application` 对 infra 零引用（grep = 0） |
| `application → infrastructure` Cargo 依赖 | **不存在**（Gate 8 从 Cargo.toml 拆除） | ✅ 关闭（原 P0；HTTP server 前置条件已满足） |
| 命令分层 | Travel 命令已变薄（Gate 7 session 入 application） | ✅ Gate 7 + 8 |
| 平台组合根 | desktop 与 server（Gate 9）两个根，互不依赖 | ✅ Gate 9 |
| 平台错误契约 | Tauri=CommandError（桌面）；HTTP=`{"code","message"}`（服务端映射 ApplicationError） | ✅ Gate 9 |

### 2.3 命令分层（2026-09-13 实测）

| 类 | 数量 | 示例 | 备注 |
|---|---|---|---|
| THIN（≤6 行） | 15 | history_semantic_home / language_languages / get_settings / cycle_task_lines … | 只剩序列化转发 |
| MODERATE（7–20） | 26 | history_semantic_* / geography_* / language_search / add_rss_feed … | 纯转发，部分含 DTO 构造 |
| FAT（>20） | 5 | **travel_research_start（82）** / fetch_article_url（39） / test_travel_qweather（26） / travel_research_progress（21） / test_travel_llm（21） | 唯一行为块在 adapter；见 BACKLOG G |

> Gate 4 后 46 个命令注册，本次 Gate 5.5 未增删任何命令 —— 行为冻结保持。

---

## 3. 运行时边界

- **唯一 Tauri 入口**：`apps/desktop/src/lib.rs`；`invoke_handler` 注册 **46 个命令**。
- **注册与调用一致**：前端 46 个命令名全部已注册，无 FE→BE 名称不匹配。
- **4 个无前端调用者的命令已删**（Gate 4）：`convert_task_lines`、`geography_compare`、`geography_map`、`language_manifests`。
- **前端 Client / Transport**：8 个 feature client（history / geography / rss / language / travel /
  markdown + workspace / settings）+ `transport.ts`；裸 `invoke()` 计数 **0**（Gate 6 后）。
- **组合根**：`apps/desktop`（唯一）；`apps/desktop/src/composition.rs + *_query.rs` 只做装配/映射，无业务逻辑。


### 3.1 Gate 6 — Remaining Frontend Client Boundary（2026-09-14）

**目标**：把 React Feature / Page 中剩余的 Tauri 直接调用全部收敛到 `Feature Client → CommandTransport → tauriTransport`，在不改变 Browser Preview 行为的前提下，让业务 UI 不再直接依赖 `@tauri-apps/api/core`。

**实施结果**：

- 新增 6 个 client：
  - `features/rss/rssClient.ts`：8 方法；
  - `features/language/languageClient.ts`：14 方法；
  - `features/travel/travelClient.ts`：7 方法；
  - `features/markdown/markdownClient.ts`：3 方法；
  - `workspaceClient.ts`：1 方法；
  - `settingsClient.ts`：2 方法；
- 延续 Gate 3 / 3B 既有 `historyClient` / `geographyClient` 模式：
  `interface XClient + createXClient(transport = tauriTransport) + singleton`；
- 前端裸 `invoke()`：**44 → 0**；
- `@tauri-apps/api/core` 直接 import：收敛为 **1 处**，仅存在于 `ui/src/transport.ts`；
- `isTauriRuntime()` 仍保留在 15 个消费者文件中，用于冻结既有 Browser Preview / capability guard 行为；本 Gate 没有把平台判断伪装成新的“抽象”而改变行为；
- `App.tsx` 中后端调用按领域归属进入对应 client，没有创建 `api.ts` / `backendClient` 之类上帝对象；
- 本 Gate **未修改任何 Rust/Tauri command、DTO、数据库、错误码、UI 或用户 workflow**。

当前前端后端访问链统一为：

```text
React Feature / App shell
        ↓
Feature Client
        ↓
CommandTransport
        ↓
tauriTransport
        ↓
Tauri command
```

未来增加 HTTP / Home Server 时，目标是复用现有 Feature Client，只增加新的 Transport；**本 Gate 未实现 `HttpTransport`**。

验证结果：

- `npm --prefix apps/desktop/ui run build`：✅ PASS（`tsc --noEmit` + Vite）；
- `cargo check --workspace --all-targets`：✅ PASS，零 warning；
- `cargo test --workspace`：✅ PASS，**214 passed / 0 failed**；
- Rust 代码：本 Gate 未修改。

---

## 4. 数据库所有权（已核实，未变）

| 数据 | 文件 | 打开位置 | 职责 |
|---|---|---|---|
| RSS | `config/dashboard.db` | infra FeedRepository | 用户数据（读写） |
| Travel 缓存 | `config/travel.db` | TravelStore | 用户数据（读写） |
| Language | `config/language.db` | LanguageStore | 用户数据 + 导入（读写） |
| Geography | `config/geography.db` | GeographyStore（经 adapter 注入） | 用户数据（读写） |
| **History** | `history-data-pipeline/dist/history.duckdb` | lib.rs setup 只读 | **只读知识库唯一事实源** |

`config/` 被 `.gitignore` 忽略；每域一个 Source of Truth。

---

## 5. 迁移/进程状态

| Gate | 状态 | 证据 |
|---|---|---|
| Gate 0 Baseline | ✅ | 本文档 + 档案 |
| Gate 1 Boundary Audit | ✅ | `docs/migration/09-history-v2-cutover-audit-2026-09-10.md` |
| Gate 2 Application Boundary | ✅ History | `crates/application/src/history/`（service+ports+15 tests）；7 命令全薄 |
| Gate 3 Frontend Boundary | ✅ History | `transport.ts` + `historyClient.ts`；build PASS |
| Gate 3B Frontend（可选） | ✅ Geography | `geographyClient.ts`；4 命令收敛；build PASS |
| Gate 4 Legacy Cleanup | ✅ 全 | §6 删除清单；零引用搜索 + 编译 + 212→214 测试 |
| Gate 5 Server-ready Audit | ✅ 审计（无代码） | BACKLOG §5：HTTP:MOSTLY |
| **Gate 5.5 Dependency Inversion & Composition Root** | ✅ **实施完成** | ADR-001；四组端口+适配器；DTO 归 core；214 tests 全绿；倒置模块 infra 零引用 |
| `application → infrastructure` Cargo 依赖 | **不存在**（Gate 8 从 Cargo.toml 拆除） | ✅ 关闭（原 P0；HTTP server 前置条件已满足） |
| Gate F/G/H/I 审计 | ✅ 只读 | BACKLOG |
| 下一步（BACKLOG §6.2） | ⏳ | P0：Travel 边界（FAT→MOD）；P1：Language/RSS 倒置；前端裸 invoke 迁移已完成（P2 关闭） |

---

## 6. 已删除的死代码清单（Gate 4，2026-09-13，未变）

| 位置 | 规模 | 验证方式 |
|---|---|---|
| `crates/core/src/history/`（model.rs+recommendation.rs+mod.rs） | 363 行 | 0 消费者；core 测试 67 全过 |
| `ui/src/features/history/types/history.ts` | 84 行 | 11 导出中仅 1 活（已内联），其余零引用 |
| `ui/src/types.ts`（History 旧 DTO 块 + CommandFailure + GeoCompareView + …） | ~200 行 | 全仓库 0 引用；保留活性类型 |
| 命令链路：`convert_lines_to_tasks` / `geography_map` / `geography_compare` / `language_manifests` | ~150 行 + core 117 + TS | 前后端 0 引用；注册表同步移除 |

---

## 7. 本次验证结果（2026-09-14，Gate 6 最终验证）

| 检查 | 结果 |
|---|---|
| `cargo check --workspace --all-targets` | ✅ PASS（零警告，含 tests/benches/examples targets） |
| `cargo test --workspace` | ✅ PASS — **214 passed, 0 failed**（core 67 + application 48 + infrastructure 99；desktop 0） |
| `npm --prefix apps/desktop/ui run build` | ✅ PASS（tsc --noEmit && vite build，仅既有 chunk 体积警告） |
| `cargo fmt --check` / `clippy` | ⚠️ **本机不可执行**（发行版 rust 1.98.0 无 rustup/rustfmt/clippy 组件）；与 Gate H 结论一致，标记「不可用」 |
| 前端测试 | 不存在（无 runner），UI 验证 = tsc + vite build |

工具链事实（Gate H，只读）：`rust-toolchain.toml` = `stable` + clippy/rustfmt（windows target）；workspace rust-version=1.95；本机 1.98.0 → 无产品影响。

---

## 8. Gate 8 — Travel 依赖倒置完成（2026-09-15 验证）

**状态：✅ PASS**

目标（backlog §6.2 P0）：消除 `application → devtoolbox-infrastructure` 最后的重依赖。

### 8.1 实施事实（本次验证）

- **契约上移 core**：`crates/core/src/travel/provider.rs`（新增）持有 `SearchProvider` /
  `WebFetcher` / `LlmProvider` / `TravelDataProvider`、`SearchOptions` /
  `TravelDataRequest` / `TravelRouteRequest` / `TravelRoute`、`ProviderError{ kind, message }`
  （Display 前缀与原 `InfrastructureError::Travel*` 逐字一致，`is_llm_transport_error`
  等字符串判定不改）；infra `travel/mod.rs` 改为 re-export，实现文件零自定义契约；
- **存储端口**：`crates/application/src/travel/ports.rs` —— `TravelStorePort`（6 方法，String 错误）；
- **错误模型**：`ApplicationError::Travel(TravelFailure { kind: TravelErrorKind, message })`
  （Search / Fetch / Llm / Data / Store）；`error.rs` 不再引入 infra 类型；
- **测试**：`travel/tests.rs` 重写为 FakeTravelStore（内存），无 SQLite、无临时目录；
- **组合**：desktop `TravelStoreAdapter`（composition.rs）绑定 `TravelStorePort`，
  `travel_research_service` 经适配器注入；命令错误 code 映射不变
  （Search→travel_search_failed / Fetch→travel_fetch_failed / Llm→travel_llm_failed /
  Data→travel_data_failed / Store→travel_error）。

### 8.2 验证

- `grep -rn devtoolbox_infrastructure crates/application/src` → **0**；应用 Cargo 依赖已移除；
- `cargo check --workspace --all-targets` 零错误零警告；`cargo test --workspace` 222 passed / 0 failed；
- `cargo check -p devtoolbox-server`（新成员）通过。

---

## 9. Gate 9 / 9.5 — HTTP 只读 History 试点（apps/server，2026-09-15 验证）

**状态：✅ PASS**

### 9.1 交付面（与桌面零互通）

| 契约 | 实现 |
|---|---|
| 路由 | `GET /health`、`GET /api/v1/history/{home,search,periods/:id,events/:id,people/:id,works/:id,stories/:id}`（只读） |
| 错误契约 | `{code,message}`，源自 `ApplicationError`；HTTP 层绝不出现 CommandError；404/400 同契约 |
| 鉴权 / CORS | 无鉴权（Gate 边界）；未注册 CORS → 默认无跨源（仅显式 allowlist 才可能放行，当前不提供） |
| duckdb 路径 | `SELF_TOOLS_HISTORY_DB` 或 `--history-db`；**缺失 = 启动失败**（exit 1），无静音 fallback |
| 绑定 | 默认 `127.0.0.1:8080`；`SELF_TOOLS_BIND`（env）或 `--bind`（CLI）可改 |
| 日志 | `RUST_LOG`（默认 info）：启动 / 绑定 / 知识库就绪 / 请求错误 / 关闭，无密钥 |
| 关闭 | SIGINT / SIGTERM 优雅退出（axum graceful_shutdown） |
| 测试 | 路由 × oneshot（无真 TCP）7 passed：/health、home、search（含缺参 400）、5 个明细 404、查询失败 500、未知路由 404 |

### 9.2 冒烟（真进程）

默认 8080 + `dist/history.duckdb` 起服 → `/health` `{"status":"ok"}` → `/home`（真实「夏」
period JSON）→ 缺参 400 契约 → 无效 id 404 → SIGTERM 退出码 0 → 无残留进程（`ps comm` 无
devtoolbox-server）。缺失文件路径同理拒绝（exit 1）。

---

## 10. 下一个 Gate 建议（更新，2026-09-15）

- **Gate 10 / 10.1（Overnight 门册后续）**：HTTP Profile 面扩展（travel/rss/language 只读
  REST）与可选 CORS allowlist env；
- 前端 `HttpTransport`（Gate 6 已收敛 transport 层，可插拔）；
- `application/src/bin/language_data.rs` CLI 归属（server / 独立 bin）保持 TODO。

## 11. 复核方法（可自行重跑）

```bash
git status --short && git submodule status
grep -n "generate_handler" -A 8 apps/desktop/src/lib.rs        # 注册表（46）
rg -n '\binvoke\b' apps/desktop/ui/src --glob '!transport.ts' --glob '!*Client.ts' | rg -v 'isTauriRuntime|tauriInvoke' | wc -l   # 应为 0（Gate 6 后）
rg -n '^import .*@tauri-apps/api/core' apps/desktop/ui/src    # 应只有 transport.ts:10
rg -n 'isTauriRuntime' apps/desktop/ui/src | wc -l            # 59 处 / 15 消费者文件（浏览器预览守卫）
grep -rn "devtoolbox_infrastructure::" crates/application/src --include='*.rs'   # 应只有 deferred(language/rss/travel)
grep -rn "HistoryQueryPort\|GeographyQueryPort\|DocumentStorePort\|SettingsStorePort" --include='*.rs' crates apps | grep -v target/
grep -rn -E 'convert_task_lines|geography_compare|geography_map|language_manifests|crates/core/src/history' apps crates | grep -v target/   # 应 0 输出
cargo check --workspace --all-targets && cargo test --workspace
npm --prefix apps/desktop/ui run build
```