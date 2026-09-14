# Architecture Backlog

> 记录架构一致性审计（Gate F）与只读审计（Gate G/H/I）的发现与建议。
> 只读审计不改变任何行为；修复项均已评估为「低风险、明显、局部、行为不变」。
> 更新：2026-09-14（+ Gate 6 前端 Client Boundary 实施，前端裸 invoke 清零）。

---

## 0. 批次快照（本次审计基线）

- 前端命令层：46 个 Tauri 命令（THIN 15 / MODERATE 26 / FAT 5）
- 前端调用面：History / Geography / RSS / Language / Travel / Markdown / Workspace / Settings 已全部收敛到 `transport.ts` + feature client
- Rust 测试：214 个全通过（core 67 / application 48 / infrastructure 99）
- `cargo check --workspace` 零警告；`tsc --noEmit && vite build` 通过

---

## 1. Gate F — 一致性审计（已完成项 + 待办）

### 1.1 已修复（本次批次，行为不变）

| # | 发现 | 修复 | 风险 |
|---|------|------|------|
| F1 | `core::history` 模块全量孤儿（无 consumer、无测试、无 serialization 依赖） | Gate 4 D1 删除（mod/model/recommendation + `pub mod history;`） | 无（已验证 0 引用、0 导出、编译通过） |
| F2 | 前端 `features/history/types/history.ts` 仅剩 1/11 个体面类型活着 | Gate 4 D2：`HistoryGeoNavigationRequest` 内联进 HistoryPage，删除整个 types 目录 | 无（tsc 通过） |
| F3 | `types.ts` 中 20 个 History 旧 DTO + `CommandFailure` + `GeoCompareView`/`CompareMetric` 零消费者 | Gate 4 D2 删除；`SemanticHistoryHome` 保留 | 无 |
| F4 | 4 个 Tauri 命令零消费者（含 Rust 侧） | Gate 4 D3 删除命令 + 专属 helper/DTO/错误变体：`convert_lines_to_tasks`、`GeographyService::{map,compare}`、`manifests()`、`ManifestInfo`、`core::geography::compare`、`ApplicationError::GeographyCompare` | 无（212 测试通过） |
| F5 | `language_manifests` 死亡链路波及 TS `ManifestInfo`（而 `DatasetManifest`/`DatasetReport` 被 `SourceInfo`/`StarterReport` 复用，保留） | D3 一并处理 | 无 |
| F6 | `geographyClient.ts` 注释陈述了两个已删命令 | 注释更新为「已删除」 | 无 |

### 1.2 待办（未在本批次处理，建议后续 Gate）

| # | 发现 | 建议 | 优先级 |
|---|------|------|--------|
| F10 | `application` 依赖 `infrastructure`（workspace path dep，桌面端适配器复用所致）；trait 实现在 adapter（`apps/desktop/src/history_query.rs`）是显式例外 | Server 侧建立 adapter 层可翻转依赖；短期接受，写进 CURRENT_ARCHITECTURE §1.3 | 中 |
| F11 | `crates/application/src/bin/language_data.rs`：CLI 二进制随 lib crate 一起编译；server 化时需决定归属 | 仅记录 | 低 |
| F12 | ~~前端仍有 **44 处**非 client 裸 `invoke` 调用点（13 个文件）~~ | ✅ **Gate 6 已关闭**：RSS / Language / Travel / Markdown / Workspace / Settings 全部迁入 feature client；裸 `invoke` **44 → 0**，`@tauri-apps/api/core` 仅 `transport.ts` 1 处 | **已完成** |
| F13 | Travel 6 个命令中 4 个为 `#[tauri::command] async fn`（RSS 2 个在 rss 模块），命令层与 application 的异步边界尚未统一 | Server 化时需先把 async 收进 application | 低 |
| F14 | `HistoryQueryPort` 30 个只读方法含 `search`/`home` 聚合；HTTP 化时需要一次陆上的 response DTO 设计 | 记录 | 低 |

---

## 2. Gate G — Travel 审计（只读，未改行为）

现状：
- `travel_research_start`（82 行）是唯一 FAT 命令：provider 装配（`providers_for`/`build_providers`）、session 注册表（`AppState.travel_sessions`）、后台任务、缓存命中判断；
- 缓存命中判断包含字符串命中文本（`命中缓存攻略` 文案），对业务行为敏感；
- `travel_snapshots` 等 5 个命令为薄转发；AMap/QWeather/Llm provider 通过 infrastructure HTTP client 注入。
- `travel_cache`/`travel_snapshot` 关键状态都由 `Arc<Mutex<HashMap>>` 持有于 desktop 层 —— 是 HTTP server 迁移的最大阻碍（进程内状态语义）。

建议（不实施）：
1. 将 session 注册表迁入 `travel_service`（application 层）并自持生命周期，Tauri 命令变薄；
2. 缓存命中判断升级为结构化意图而非字符串匹配（需与业务确认文案稳定性）；
3. server 化时 AMap/QWeather 密钥从 Tauri settings 迁移到 server env。

---

## 3. Gate H — Toolchain 审计（只读）

| 项目 | 现状 | 结论 |
|------|------|------|
| 本机 rustc | 1.98.0（Fedora distro，无 rustup） | 正常；稳定版 |
| `rust-toolchain.toml` | `channel = "stable"`，components: clippy/rustfmt，targets 仅有 Windows | rustup 管理时会装 clippy/rustfmt；无 Linux target 不影响 host 构建 |
| workspace `rust-version` | 1.95 | 与 README「Rust 1.95+」一致，且 ≤ 本机稳定版 → OK |
| cargo fmt/clippy | 本机 **不可用**（distro rust 不带组件，且无 rustup） | 未安装；Gate K 将标注「不可用」 |

产品影响：无。仅开发环境便利性差异（建议通过 rustup 或 distro package 补装，不在仓库内做 rustup 安装）。

---

## 4. Gate I — history-data-pipeline/dist 备份积压（只读 + 建议）

**现状**：`history-data-pipeline/dist/` 累积 `history.previous.<UTC 时间戳>.duckdb` 113 个文件（约 22 GB，单个 240–253 MB），另有遗留 `history.previous.duckdb`（12 MB，旧格式）。

**原因（代码定位）**：`src/history_data_pipeline/real_build.py:118-127` 与 `backbone/build.py:440-448` 中，构建前先把 `history.duckdb` 轮转为 `history.previous.duckdb`，再把旧 `history.previous.duckdb` 按时间戳归档到 `history.previous.<ts>.duckdb`，**没有任何保留上限或删除逻辑**；每次成功构建都会永久增加一个 ~250MB 文件。`Backbone build` 路径同样有 `history.previous`（对齐首选逻辑），但存在两个执行路径导致并发轮转时文件交错（时间戳去重后缀 `{stamp}.{suffix}` 仅实现在 real_build）。

**建议的 retention policy（本轮不实施，写入 backlog）：**
1. 保留最近 N=10 个构建（覆盖回滚需求的合理窗口），超过则删除最旧；
2. 或把已累积的 `history.previous.*` 档案迁移到 `history-data-pipeline/backups/`（.gitignore 外）；
3. 在 `real_build.py` 与 `backbone/build.py` 两条路径统一轮转逻辑（含时间戳冲突后缀），并写测试；
4. 归档删除不采用 `rm -rf`（删除前 dry-run + 保留计数），运行时机：每次构建完成后；
5. dist/ 目录建议移出 git 跟踪（当前 submodule 内，是否被跟踪待确认 —— 见下）。

> 注：`history-data-pipeline` 是 submodule（`e4518b7`）。对 pipeline 的修改属于另一个仓库的 scope —— 本轮只审计不修改。

---

## 5. Gate 5 — Server-readiness 评估（审计结论）

架构前提: application（无 Tauri 依赖）已含全部核心用例；仅 adapter 差异（Tauri command vs HTTP handler）。

| 能力 | 当前命令分层 | HTTP/Web 迁移可行性 | 备注 |
|------|------|------|------|
| History | 7 命令全部薄（≤8 行） | ✅ **READY** | `HistoryQueryPort` 纯只读,无 IO 外置;可直接映射 GET 资源 |
| Geography | 4 薄命令 | ⚠️ MOSTLY | 服务层已就绪;AMap/地图交互属前端 |
| Language | 11 命令 (含 install) | ⚠️ MOSTLY | `language_data` CLI 归属待定；安装路径需要 server 文件系统语义 |
| RSS | 6 命令 (async 2) | ⚠️ MOSTLY | fetch 已在 application;需要把 async 边界移到 application 内 |
| Documents / Settings / Workspace | 6 薄命令 | ❌ NOT | 路径语义依赖桌面文件系统;HTTP 需要工作区设计(如 server 挂载/上传块) |
| Travel | 6 命令 (FAT 2) | ❌ NOT | 会话状态机、provider 装配、字符串缓存判断均在桌面层;最大 backlog 项 |

总体:`HTTP: MOSTLY`（History 全就绪,2/3 功能 MOSTLY,Travel/Workspace 需重构）。不实现任何 server 代码（本轮范围限制）。

---

## 6. Gate 5.5（实施）— 依赖倒置与组合根（Dependency Inversion & Composition Root）

决策记录：[`docs/architecture/ADR-001-dependency-inversion-and-composition-root.md`](docs/architecture/ADR-001-dependency-inversion-and-composition-root.md)。
原则：端口（Port）定义在 application（用例方），适配器（Adapter）实现放在桌面组合根（desktop）；
**不新增 crate**（否决 `crates/ports`）；错误抽象为 port 本地错误；DTO 归 core；
组合根只在 `apps/desktop`。

### 6.1 已实施（P0 全部完成，行为不变）

| 模块 | Port（application） | Adapter（desktop） | 原 infra 直接引用 |
|---|---|---|---|
| History（audit，未重排） | `history/ports.rs`：`HistoryQueryPort` 31 方法 + `HistoryPortError` | `history_query.rs`（既有） | 已清零（Gate 2） |
| Geography | `geography/ports.rs`：`GeographyQueryPort` 10 方法 + `GeographyPortError` | `geography_query.rs`（新） | **清零** |
| Documents / Settings / Workspace | `workflows/ports.rs`：`DocumentStorePort`（3）+ `SettingsStorePort`（3）+ `#[cfg(test)] fakes` | `composition.rs`（新） | **清零** |
| 数据契约 | core 新增 `history_records.rs`（21 个记录）/`settings.rs`/`workspace.rs`；infra 改为重导出，公开面不变 | — | — |
| 错误抽象 | `ApplicationError` 变体名不变；`Infrastructure` 变体改为 `{ path, message: String }`；消息文本逐字保留 | — | — |

证据：`cargo test --workspace` **214 passed**（core 67 / application 48 / infrastructure 99）；
`cargo check --workspace --all-targets` 零警告；`npm --prefix apps/desktop/ui run build` PASS。

### 6.2 剩余依赖（P0/P1/P2 分级）

| 优先级 | 项 | 现状 | 处置 |
|---|---|---|---|
| ~~**P0**~~ | `application → infrastructure` Cargo 依赖 | ✅ **已拆除**（Gate 8） | 闭：`grep devtoolbox_infrastructure crates/application/src` = 0，Cargo.toml 依赖移除 |
| ~~**P0**~~ | Travel 模块倒置 | ✅ **已倒置**（Gate 8：provider 契约入 core，`TravelStorePort` 入 application，desktop 适配器注入） | 闭合：见 CURRENT_ARCHITECTURE §6.1（Gate 8） |
| ~~**P1**~~ | Language 模块依赖 | ✅ **已倒置**（Gate 7.5：`LanguageStorePort` + `now_unix` 入 application） | 关闭 |
| ~~**P1**~~ | RSS async 边界 | ✅ **已倒置**（Gate 7.6：`RssRepositoryPort` / `FeedFetcherPort` + models 入 core） | 关闭 |
| **P2** | `application/src/bin/language_data.rs` | CLI 随 lib 编译，server 归属未定 | Gate 9 未纳入；保持 TODO（server 或独立 bin） |
| **P2** | 组合根增长监测 | `composition.rs` 64 行 + 两个 `*_query.rs` | 若超单文件 ~150 行，拆为 `composition/` 目录（每域一文件），避免 God Object |


---

## 7. Gate 6 — Remaining Frontend Client Boundary（已完成）

**状态：✅ PASS（2026-09-14）**

目标：在不改变 UI、Tauri command contract、DTO、错误行为与 Browser Preview 行为的前提下，把剩余前端直接 Tauri 调用收敛到统一 Client / Transport 边界。

### 7.1 实施结果

| 项 | Gate 6 前 | Gate 6 后 |
|---|---:|---:|
| Feature / Page 裸 `invoke()` | 44 | **0** |
| `@tauri-apps/api/core` 直接 import | 多处分散 | **1**（仅 `transport.ts`） |
| 已有 feature client | History / Geography | **History / Geography / RSS / Language / Travel / Markdown / Workspace / Settings** |
| 新增业务功能 | 0 | 0 |
| Rust/Tauri command 修改 | — | **0** |

新增 client：

- `features/rss/rssClient.ts`：8 方法；
- `features/language/languageClient.ts`：14 方法；
- `features/travel/travelClient.ts`：7 方法；
- `features/markdown/markdownClient.ts`：3 方法；
- `workspaceClient.ts`：1 方法；
- `settingsClient.ts`：2 方法。

所有 client 统一遵循既有 reference pattern：

```text
Feature / App shell
    ↓
Feature Client
    ↓
CommandTransport
    ↓
tauriTransport
    ↓
Tauri command
```

没有创建 `api.ts` / `backendClient` 上帝对象，也没有引入 RPC / DI / middleware framework。

### 7.2 Runtime guard 结论

`isTauriRuntime()` 仍存在于 15 个消费者文件中，原因是它们承载既有 Browser Preview / capability guard 语义。本 Gate 选择**冻结行为**，没有为了追求计数归零而机械替换平台判断。

后续若进入 PWA / HTTP Transport Gate，应按“能力检测（capability）”而不是“平台身份”逐点评估；Gate 6 不提前实现。

### 7.3 验证

- `cargo check --workspace --all-targets`：✅ PASS，零 warning；
- `cargo test --workspace`：✅ PASS，**214 passed / 0 failed**；
- `npm --prefix apps/desktop/ui run build`：✅ PASS（`tsc --noEmit` + Vite）；
- 本 Gate 未修改 Rust 代码、数据库、DTO、Tauri command contract 或 UI。

### 7.4 Backlog 影响

- **F12：关闭**；
- 前端 Client Boundary 不再是 HTTP / PWA 的阻塞项；
- 下一主要 P0 保持为 **Travel Application Boundary**；
- Language / RSS 依赖倒置仍为 P1；
- `HttpTransport` / Home Server / PWA 仍未实现。

---

## 8. 查看后续建议（下一批优先）

1. **Travel Command Layer (FAT→MOD)**：session 状态入 application；这是唯一 FAT 行为块；
2. **RSS async 边界** 统一（F13）；
3. **Pipeline retention 实现**（Gate I 提案）在未来 submodule 仓库内进行;
4. **PWA/browser preview**：前端裸 `invoke` 已在 Gate 6 清零；后续重点改为 `HttpTransport` / capability guard 设计，而不是继续迁 client；
5. **Server 上线前置**:先把 HISTORY 作为第一 REST 模块（契约 = 现有 command signature）。


## 9. Gate 8 — Travel 应用边界（实施，2026-09-15 ✅）

见 [CURRENT_ARCHITECTURE §8](CURRENT_ARCHITECTURE.md)（先于本轮追加）。

- `crates/core/src/travel/provider.rs`：4 个 Provider 契约 + `ProviderError`
  （Display 与原 `InfrastructureError::Travel*` 逐字一致）；infra 只实现（re-export）；
- `crates/application/src/travel/ports.rs`：`TravelStorePort`（6 方法）；
  `travel/tests.rs` 改为 FakeTravelStore（内存），无 SQLite、无临时目录；
- `ApplicationError::Travel(TravelFailure{kind,message})`；desktop 错误 code 映射不变；
- 验证：`cargo check --workspace --all-targets` ✅ / `cargo test --workspace` 222 ✅ /
  `npm --prefix apps/desktop/ui run build` ✅ / `grep devtoolbox_infrastructure crates/application/src` = 0 ✅。

## 10. Gate 9 / 9.5 — HTTP 只读 History 试点（已实现，2026-09-15 ✅）

见 [CURRENT_ARCHITECTURE §9](CURRENT_ARCHITECTURE.md)。`apps/server`（axum 0.8）
与 desktop 零互通：

- 7 个只读 History 端点 + `/health`；错误契约 `{code,message}` 源自 `ApplicationError`；
- 无鉴权；未注册 CORS（默认无跨源）；默认绑定 `127.0.0.1:8080`，
  `SELF_TOOLS_BIND` / `--bind` 可改；`SELF_TOOLS_HISTORY_DB` / `--history-db` 配置
  duckdb 路径，缺失 = 启动失败（exit 1，无静音 fallback）；`RUST_LOG` 控日志；
  SIGINT / SIGTERM 优雅退出；
- 测试：routes×oneshot（无真实端口）7 个全覆盖；真机冒烟（health / home 真实数据 /
  400 / 404 / SIGTERM 退出 0 / 无残留进程）✅。

## 11. 更新后的下批建议优先（2026-09-15）

1. **HTTP Profile 面扩展**（Gate 10 / 10.1）：travel / rss / language 只读 REST，可选 CORS allowlist env；
2. **前端 `HttpTransport`**：Gate 6 已把 invoke 收敛到 transport 层，PWA 只换实现；
3. **Pipeline retention**（Gate I）：仍在 pipeline 仓库内推进；
4. **`language_data.rs` CLI 归属**：随下一 server gate 定案；
5. **Directory health**：组合根与 server 的边界再复核（Gate 12 候选）。

（全文完）
