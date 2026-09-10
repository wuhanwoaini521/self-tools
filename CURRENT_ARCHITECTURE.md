# CURRENT_ARCHITECTURE — History V2 现状审计（Gate 1）

> 审计日期：2026-09-10
> 审计范围：`self-tools`（主仓库）+ `history-data-pipeline`（子仓库）的真实调用链。
> 方法：全部结论来自代码调用链与数据产物核实，非文件名推断。

---

## 0. 一句话结论

History 前端（`HistoryPage.tsx`）**已经完全切换到 Semantic V2 API**，只消费 `history_semantic_*` 命令；
但 V1 legacy 链路（`history.db` SQLite + `HistoryStore` + 5 个 `history_*` 命令）**仍然注册并随应用启动打开**，
与 V2 DuckDB 形成"两套数据源同时运行"的状态。V1 前端代码（`views/` `components/` `details/` `hooks/`）已是无人引用的死代码。

另外 `data/normalized/history.duckdb` 回退仍然存在（**2 处**），必须在 V2 Cutover 中移除。

---

## 1. 前端数据来源

唯一活动入口：`apps/desktop/ui/src/features/history/HistoryPage.tsx`。

- `App.tsx:22` 只 import `HistoryPage`，history feature 其余文件没有被任何页面引用。
- HistoryPage 调用的 Tauri 命令（全部为 V2 Semantic）：
  - `history_semantic_home` (`HistoryPage.tsx:59`)
  - `history_semantic_period` (`:66`)
  - `history_semantic_story` (`:71`)
  - `history_semantic_event` (`:77`)
  - `history_semantic_person` (`:84`)
  - `history_semantic_search` (`:101`，180ms 防抖)
- 页面没有使用 `history.db` / `HistoryStore` / `history_home` / `history_search` / `history_period_nodes` /
  `history_detail` / `history_toggle_favorite` 中的任何一个。

```text
HistoryPage.tsx (V2 UI)
   ↓ invoke
history_semantic_* (Tauri commands, lib.rs:879-1297)
   ↓
HistoryDuckDbRepository (crates/infrastructure/src/history/duckdb.rs)
   ↓ (只读 SELECT)
history-data-pipeline/dist/history.duckdb   ← 唯一 build artifact
```

## 2. Semantic API 数据来源

- `semantic_history_path()` (`apps/desktop/src/lib.rs:176-181`) 解析 DuckDB 路径：
  候选 1：`history-data-pipeline/dist/history.duckdb`
  候选 2（fallback）：`history-data-pipeline/data/normalized/history.duckdb`
- `AppState.history_duckdb` 在 `setup()` (`lib.rs:1420-1424`) 打开，连接方式为只读
  (`duckdb.rs:381-391` `with_connection` → `AccessMode::ReadOnly`)。
- 所有 SELECT 都指向 `dist/` 的表：`periods / regimes / stories / story_events / events /
  event_person / event_place / event_relations / event_evidence / event_text / people / places / works /
  historical_texts / sources / person_aliases / person_relations / person_place / story_*`。

## 3. legacy API 是否仍在被使用

**被注册，但前端没有消费者。**

- 命令仍注册于 `invoke_handler` (`lib.rs:1465-1469`)：`history_home / history_search / history_period_nodes / history_detail / history_toggle_favorite`。
- 仍与 `HistoryService`（`crates/application/src/history/service.rs:30-140`）+ `HistoryStore`（`crates/infrastructure/src/history/store.rs`）绑定，数据源是项目 config 目录下的 SQLite `history.db`。
- 前端引用它们的只有死代码：
  - `hooks/useHistoryNavigation.ts:43,75,97`
  - `hooks/useHistorySearch.ts:55`
  - `views/GraphView.tsx:47`
- `App.tsx`、`HistoryPage.tsx` 均不调用；`components/`、`views/`、`details/`、`hooks/` 全部为未引用文件（`HistoryPage` 未 import 其中任何一个）。

## 4. `history.db`（SQLite）是否仍有实际消费者

**作为知识库：没有真实用户。** 作为用户状态：同样没有（V2 UI 无收藏/最近查看/进度功能）。

注意：**它仍被打开**。`setup.rs:1416-1418` 在应用启动时执行 `HistoryStore::open(config/history.db).expect(...)`，
失败会 panic 整个应用。也就是说 `history.db` 当前是一个"启动即实例化、但无人查询"的 SQLite 库。

表结构 (store.rs): `history_periods / history_nodes / history_relations / history_sources / history_facts /
history_favorites / history_views / history_searches`。种子数据为 ~24 个内置人物/事件/朝代/地点文档
（含"唐"的长篇概述），这些内容已完全被 DuckDB 的 618 事件替代。

## 5. `history.duckdb` 的实际消费者

- 消费者：`history_semantic_*` 全部 6 个命令（整个 V2 UI）。
- 数据库路径：`history-data-pipeline/dist/history.duckdb`（回退 `data/normalized`，见 §6）。
- 内容（manifest.json 2026-09-10 构建）：periods=31, regimes=64, events=618（critical=62, major=555）,
  stories=3, event_relations=1057, event_people=398 条链接, event_places=26, event_evidence=130,
  people=234, places=3, works=49, historical_texts=0（dist 版不含全文）, sources=8。

**产品完整性现状（post-merge 口径，即加载 V2.1 合入后的 618 事件）——本审计最重要的发现：**

| 维度 | 全部 618 | Critical 62 |
|---|---:|---:|
| summary | 618/618 | 62/62 |
| background | 51/618 | 51/62 |
| result | 77/618 | 51/62 |
| ≥1 人物关联（linked） | 261/618 | 61/62 |
| ≥1 地点关联（linked） | **3/618** | **0/62** |
| evidence | 77/618 | 51/62 |
| relations | 607/618 | 62/62 |
| source_reference | 618/618 | 62/62 |

即：**地点层几乎完全缺失**，62 个 critical 事件没有任何一个带已关联地点；11 个 critical 缺
background/result/evidence。这是"有总量、缺关联、页面空"的直接证据。

## 6. `normalized/history.duckdb` 的 fallback 现状

**存在 2 处 fallback，都必须移除：**

| 位置 | 逻辑 |
|---|---|
| `apps/desktop/src/lib.rs:176-181` | `dist/history.duckdb` 缺失 → 回退 `data/normalized/history.duckdb`（`semantic_history_path` 的 CANDIDATES 数组） |
| `crates/infrastructure/src/history/duckdb.rs:1107-1121` | 测试 helper `available_dist_repository()`：dist → normalized → 无则返回 `None`（silent skip） |

`data/normalized/history.duckdb` 是 legacy Layer2 的中间产物（575 MB，含 NiuTrans 语料），
新 backbone build 已不写它（`data/normalized` 只作 legacy 查询 fallback / `--knowledge` 引用源）。

原则期望：正式应用只读取 `dist/history.duckdb`；文件缺失时明确报开发错误，绝不悄悄回退到 normalized。

## 7. 已废弃的 History Commands

| Command | Rust 定义 | 前端使用 |
|---|---|---|
| `history_home` | lib.rs:522 | 无（V1 UI 死代码） |
| `history_search` | lib.rs:1300 | 无（只 useHistorySearch.ts:55 死代码） |
| `history_detail` | lib.rs:1320 | 无（useHistoryNavigation.ts:43 / GraphView.tsx:47 死代码） |
| `history_period_nodes` | lib.rs:1310 | 无（useHistoryNavigation.ts:97 死代码） |
| `history_toggle_favorite` | lib.rs:1330 | 无（useHistoryNavigation.ts:75 死代码） |

建议：这些命令 + `HistoryStore` + `history.db` 一起清除；`HistoryService` 若确认无消费者删掉。

## 8. 两套数据源同时运行？

**是。** `setup.rs` 同时：

1. `HistoryStore::open(config/history.db)` → `AppState.history_store`（SQLite，V1 知识库）
2. `HistoryDuckDbRepository::open(dist/history.duckdb)` → `AppState.history_duckdb`（DuckDB，V2 知识库）

V1 链路在 UI 上无消费者（死代码），但进程内仍存在。用户状态（favorites/views/searches）只有 V1 表，
而 V2 UI 未用。推荐落地：删掉 V1 知识库职责，`history.db` 若仍需收藏/观看建议为独立的
`user_history_state.db`（仅用户状态，不装历史知识）。

## 9. 相关信息与风险提示（供后续 Gate 使用）

- 子仓库主流程：`backbone build`（cli.py:389-397）→ `build_backbone` (backbone/build.py:565)。
- `backbone coverage` 命令已存在，产出 `BACKBONE_COVERAGE.md` + `backbone_coverage.json`
  （backbone/coverage.py），但**只有事件/故事数量，没有产品完整性（people/places/evidence/source/relations）**。
- `backbone/quality.py` 有 100 分质量评分（9 维），关键事件 ≥90 为 AUTO_ACCEPT —— 这是"Product Completeness"的种子，但需要按 G1/层级扩展。
- `backbone/qa_report.py` 有 gap detection（GAP_THRESHOLD_YEARS=40），但只报告年份缺事件，不按事件完整性/优先级排队列。
- Event schema 只有 `summary_zh_cn / background_zh_cn / result_zh_cn`（无 process/impact 字段），与任务定义的
  `has_process / has_impact` 不一致 —— 需要在 schema、build、前端三处同步扩展（注意 additionalProperties:false）。
- 地点层是整个产品最薄弱的一层：`places` 知识库仅 3 个，critical 事件地点关联为 0。
- Evidence：dist 130 条，均有 work/term/chapter_hint；`historical_text_id` 未锚定（needs_linking）。
`event_evidence.linked=0`，`pending_knowledge=26, needs_linking=104`。

## 10. Submodule 状态

```text
git submodule status:  7c63bd680f312862c0cc1bca410c6805e3472b20 history-data-pipeline (heads/main)
submodule HEAD:        7c63bd6 tools(audit): add dataset completeness / connectivity / evidence-link audit scripts
```

主仓库 gitlink 与子仓库当前 HEAD 一致（无需更新 pointer）。dist 构建于子仓库 commit `3f26aa0`
（比当前 HEAD 旧 1 个 commit：最近 HEAD 只加了 audit 脚本，不改数据）。

---

## 11. 审计方法说明（可复核）

- 前端调用链：`App.tsx:22`；`HistoryPage.tsx` 全部 invoke 语句 (`:59,:66,:71,:77,:84,:101`)；`hooks/useHistorySearch.ts:55`
  `hooks/useHistoryNavigation.ts:43,75,97`；`views/GraphView.tsx:47`。
- Rust 命令注册：`apps/desktop/src/lib.rs:1492-1497`（invoke_handler 列表）。
- 路径解析与 fallback：`lib.rs:160-215`（semantic_history_path）；`duckdb.rs:1107-1123`（test helper）。
- 数据产物：`history-data-pipeline/dist/manifest.json`（counts / summary_counts / reference_resolution）。
- 事件完整性快照：用 `backbone/loader.load_backbone`（含 V2.1 合入）遍历 618 事件统计，
  见第 5 节数字。

---

## 12. 结论与下一步（Gate 2）

| 问题 | 结论 |
|---|---|
| 前端数据来源 | HistoryPage → semantic commands ✅ V2 |
| semantic API 数据来源 | dist/history.duckdb（读取）✅ 但 fallback normalized 需移除 ❌ |
| legacy API 使用 | 注册但无活跃消费者 → 可删（命令+service+store） |
| history.db 实际消费者 | 仅启动时打开，无 UI 消费者 → 可删除（若保留用户状态则独立成 user_history_state.db） |
| normalized fallback | **存在 2 处**，需移除，改为"缺失明确报 develop error" |
| 两套数据源 | **是**，需收敛到只有 DuckDB |

Gate 2 执行顺序建议：先改消费者（前端死代码定位 + 测试迁移）→ 删 legacy 命令 → 改 `dist` 缺失错误提示 → 删 fallback → 删 `HistoryStore` / `history.db`。