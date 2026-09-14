# ADR-001 — 依赖倒置与组合根（Dependency Inversion & Composition Root）

- 状态：**已接受（Accepted）**
- 日期：2026-09-13
- 范围：Gate 0 架构系列 Gate 5.5（Server-readiness 第二站）
- 关联：`CURRENT_ARCHITECTURE.md`（§1.5 / §2）、`ARCHITECTURE_BACKLOG.md`（§7 Gate 5.5）

---

## 1. Context（背景）

自 Gate 0 基线以来，依赖分层存在一个已知偏差（BACKLOG F10 / CURRENT §2.2）：

```text
devtoolbox-application → devtoolbox-infrastructure（直接依赖具体 Store 类型）
```

多数 application Service 通过 `Arc<Store>` 具体类型构造（`GeographyService(Arc<GeographyStore>)`、
`LanguageService(Arc<LanguageStore>)`、workflows 自由函数直接调用 `read_utf8` 等 infra 自由函数）。
只有 History 在 Gate 2 建立了例外：Port 定义在 application（`HistoryQueryPort`），实现（`HistoryQueryAdapter`）
在桌面 adapter 层，绕开了 application → infra 的依赖。

本 ADR 处理以下既有事实：

- `application` 与 `infrastructure` 之间存在**反向依赖**（Cargo 依赖方向固定为 structural 单向，
  但逻辑方向 application 需要 infra 的服务能力）；
- 服务器化（未来 HTTP server / PWA）要求 application 不绑定任何具体 adapter；
- 多个「data-neutral」DTO（history-result records、settings structs、workspace file）所有权在 infra，
  但契约上是跨层的，`infrastructure` 的依赖方向（→ core）使得 application 无法经 infra 获得它们
  （crate 依赖单向，`infra → core` 已存在，application 不能依赖 infra 拿 DTO，否则application→infra
  就永远拆不掉）。
- 本机验证契约：behavior freeze（§27）—— UI/commands/args/DTO/DB schema/数据/History 全部不变，
  仅当编译需要时才允许拼字级变化；`cargo fmt`/`clippy` 本机不可执行。

## 2. Decision（决定）

采纳**“Port 在 application、Adapter 在桌面组合层、SOA 分层保持原有 crate 布局”**的方案
（即用户方案/有限备选中的 “Solution C”），不做任何新 crate（不新增 `crates/ports`）。

1. **Port 归属**：所有端口 trait 与 port 本地错误类型定义在 application：
   - `crates/application/src/history/ports.rs` — `HistoryQueryPort`（31 个只读方法）+ `HistoryPortError`（Gate 2 已有，本次仅把错误换为 port 本地类型，方法签名改为 `Result<_, HistoryPortError>`）；
   - `crates/application/src/geography/ports.rs`（新）—— `GeographyQueryPort`（10 个方法：`all_entities/recent_ids/favorite_ids/map_snapshot/search/entity/record_view/relations_for/sources/toggle_favorite`）+ `GeographyPortError`；
   - `crates/application/src/workflows/ports.rs`（新）—— `DocumentStorePort`（read/write/scan_markdown）+ `DocumentStoreError`；`SettingsStorePort`（path/load/save）+ `SettingsStoreError`；`#[cfg(test)] pub mod fakes` 提供 InMemory Fakes。
2. **Adapter 在桌面（组合层）**：
   - `apps/desktop/src/history_query.rs` 仍是 `HistoryQueryAdapter`（Gate 2 已有，仅错误映射换新类型）；
   - `apps/desktop/src/geography_query.rs`（新）—— `GeographyQueryAdapter` 包装 `Arc<Mutex<GeographyStore>>`；
   - `apps/desktop/src/composition.rs`（新）—— `DocumentStoreAdapter` 与 `SettingsStoreAdapter`，为组合根成员。
3. **DTO 所有权迁移到 core**（application-neutral contracts，避免 infra 持有「上层数据契约」）：
   - `crates/core/src/history_records.rs`（新）—— 21 个 History 结果记录
   - `crates/core/src/settings.rs`（新）—— `TravelSearchBackend / TravelSettings / GeographySettings / AppSettings / ThemeMode / MarkdownView`
   - `crates/core/src/workspace.rs`（新）—— `WorkspaceFile`
   - `infrastructure` 改为 `pub use devtoolbox_core::…` 重导出，公开面零变化；
   - `serde` 契约（JSON 形状）不变。
4. **错误抽象**：
   - port 错误为 port 本地类型，如 `HistoryPortError`、`GeographyPortError`（`String` 包裹 infra error 的
     Display，消息逐字保留）；
   - `ApplicationError` 变体名不变（History / Geography 等），`Infrastructure` 变体改为
     `{ path, message: String }`（不再持有 `InfrastructureError` 类型）；Rss/Language/Travel 变体
     保持原样（deferred 模块，允许直引 `InfrastructureError`）。
5. **组合根**：`apps/desktop` 是**唯一**组合根；新组合代码只做装配（构造 adapter、注入 store），
   不含业务逻辑 / SQL / UI。按域拆成小 adapter（`composition.rs` + 各 `*_query.rs`），避免 God Object。
6. **不 scope**：RSS / Language / Travel 本次**不反转**（deferred，见 §5），
   因此 `devtoolbox-application/Cargo.toml → devtoolbox-infrastructure` 的 path dep 仍保留，但
   反转后模块在代码层面对 infra 的具体类型**零引用**（检查手段：`grep devtoolbox_infrastructure:: crates/application/src` 仅剩 deferred 模块）。

## 3. Alternatives（备选方案）

| 方案 | 内容 | 结论 |
|---|---|---|
| A. 新建 `crates/ports` | 端口类型独立一个 crate | 否决 —— 用户第一选择“最少新增 crate”；多一个 crate 的版本/依赖管理成本，FYI 只依赖核心契约，本可以，但不值 |
| B. Port 定义在 infra | 由 infra 提供 trait，application 倒过来依赖 infra 的 trait（“端口在实现侧”） | 否决 —— 这不但倒置了端口方向，且 application→infra 依赖仍存在（端口类型在 infra），不解决 |
| C. 保持现状 | application 直接用 infra 具体类型 | 否决（对已选模块）—— Server-readiness（F10）与后续 HTTP 需要 application 可测可复用；保留为 deferred 模块状态的 justification |
| D. Status quo + adapter 单例外 | 仅 History 做 port、其余维持 | 否决 —— 本次酌情：所有无异议模块一并反转，一次性消除已知偏差 |

## 4. Why（为什么是它）

1. **最少新增 crate**（用户约束）：不改 crate 结构，只在现有 4 crate 内移动/新增代码；
2. **方向正确**：端口 = 领域用例的输入接口 → 归 application；实现（SQL/文件/SQLite）归 infra，
   而连接两者的 adapter 归组装层（desktop —— 真正的可执行环境）；
3. **只需一次改动**递进到 HTTP：同端口 + 同 adapter（不同壳，HTTP handler）即可复用整个 application；
   桌面端 command 保持现在的 adapter 注入形式；
4. **可测试**：application 的测试用 fakes（`workflows::ports::fakes`）、不依赖真实文件系统/DB；
5. **行为不变**：DTO 字段、错误变体名、错误消息文本（String 包裹）、命令签名（包括 settings 的 `#[serde]` 形状）、
   duckdb schema 一切不变 —— History reference implementation 审计后**没有重做**，保持 Gates 2 形态。

## 4. Consequences（结果 & 代价）

**正向（证实）**
- application 的 History/Geography/w改造部署干净：三组代码各零 infra 引用；
- 多路 adapter 检查：`application` 层服务端/桌面端对称，字证实 app 在 desktop-only；
- 新增可测性：48 个 application 测试（含 fake-based workflows 测试）→ workspace 214 个测试全绿。

**代价 / 注意**
- `crates/application/Cargo.toml` 的 infra path dep **保留**（deferred 模块用），
  这是「唯一仍存在的 application→infra 直接引用」，必须在后续 HTTP server gate 落地前清理
- 不建议把 `composition.rs` 写成 God Object——新 port/adapter 一律按域独立成文件（mod 组织）
- 桌面 adapter 存在「四段超薄样板」（port 方法 → store 调用 → 错误映射），与 History adapter 对齐 —— 可接受

## 5. 未来 / 后续工作

1. **Gate 后续（尚未实施）**：Travel / Language / RSS 模块按「port-in-module（travel）→ port-in-module（language/rss）→ 组合根注入」同法倒置；RSS 已有 `feed_fetcher` 仍直引 infra（async 边界保留在 app）；Travel cancel；
2. `application/src/bin/language_data.rs`（CLI 二进制归 lib 编译）的决定供 server gate；
3. HTTP server：复用同一端口、新 adapter 进 server crate 组合根 —— 这一 ADR 是队史的「application 独立于实现」前提。

（全文完）