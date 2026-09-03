# History UI V2 实施报告

完成时间：2026-09-03

## 修改内容

### 前端

- 重写 `apps/desktop/ui/src/features/history/HistoryPage.tsx`，正式页面不再读取旧 `HistoryStore`、`historyEras.ts` 或 `historyStories.ts`。
- 新增 `apps/desktop/ui/src/features/history/semanticTypes.ts`，类型与 `HISTORY_SEMANTIC_QUERY_CONTRACT.md` 的 Semantic DTO 对齐。
- 新增现代历史图谱式布局：History Overview、横向可滚动时间长河、Period Overview、编辑式 Story Flow、Event Context Drawer、Person Detail、Place Detail、HistoricalText Reader、Source 折叠区。
- HistoricalText 明确区分“原文 / 简体 / 译文 / 对照”，读取 `original_text`、`original_simplified`、`translation_zh_cn`；未复核译文使用轻提示。
- `needs_linking` 地点保留原始地点名，显示“位置待考 / 尚未完成地理关联”，不生成现代坐标。
- Story Flow 直接使用服务端返回的 `sequence` 顺序；Event Relation 只作为前置、后续或推进说明，不将 `precedes` 伪装成强因果。
- 首页 History 推荐也改为调用 `history_semantic_home`，不再从旧 `history_home` 读取历史事实。

### Rust / Tauri

- 扩展 `HistoryDuckDbRepository` 的只读 DTO 和查询：Story、StoryEvent、Event、EventPerson、EventPlace、EventRelation、EventText、Person、PersonRelation、PersonPlace、Source。
- 新增最小必要命令：

  - `history_semantic_home`
  - `history_semantic_period`
  - `history_semantic_story`
  - `history_semantic_event`
  - `history_semantic_person`
  - `history_semantic_search`

- `AppState` 持有 `history.duckdb` 的只读 Repository，路径固定解析为 `history-data-pipeline/data/normalized/history.duckdb`。
- 没有新增 HTTP Server；没有修改 Raw/Staging 数据；旧 SQLite HistoryStore 保留给兼容代码，但 V2 页面不再调用它。

### 文档与测试

- 新增改造分析：[HISTORY_UI_V2_PLAN.md](HISTORY_UI_V2_PLAN.md)
- 新增本实施报告：[HISTORY_UI_V2_IMPLEMENTATION.md](HISTORY_UI_V2_IMPLEMENTATION.md)
- Semantic QA 依据：[reports/SEMANTIC_LINK_QA.md](reports/SEMANTIC_LINK_QA.md)
- Semantic Layer 依据：[reports/HISTORY_SEMANTIC_LAYER_REPORT.md](reports/HISTORY_SEMANTIC_LAYER_REPORT.md)

## 数据流

```text
history-data-pipeline/data/normalized/history.duckdb
  → HistoryDuckDbRepository（只读 SELECT）
  → Tauri history_semantic_* command
  → React HistoryPage query state
  → Overview / Period / Story Flow / Drawer / Reader
```

页面没有读取 Story Sample JSON；Sample 仍只用于开发调试和视觉验证。

## 已支持

- Period / 时间长河：BCE 与 CE 统一格式化，支持横向滚动、滚轮、拖动、点击、键盘左右键。
- Story：按 Period 查看真实可用 Story，显示时间范围、简介、QA 状态和统计信息。
- Event：按 `story_events.sequence` 阅读，侧栏展示人物、地点、前后事件、史料和来源。
- Person：展示 canonical name、生卒、简介、参与 Event、人物关系、来源。
- Place：展示可靠地点信息；未可靠关联地点显示 `needs_linking` 状态，不放入现代地图。
- HistoricalText：原文、简体、译文、对照四种模式。
- Source：以用户可读的 Dataset、Version、License、Quality 折叠显示；开发 ID 不直接暴露。
- Search：使用结构化查询支持 Person、Story、Event、Work 分组结果。
- Loading / empty / error：已覆盖首页、查询失败、时期无 Story、Drawer 读取过程。
- 返回位置：Story Flow 与 Event Drawer 在同一页面状态中保留，关闭详情后仍停留在原阅读位置。

## 未支持 / 有意保留的范围

- Work 当前支持搜索结果展示，但没有独立 Work Detail 页面；史料阅读从 EventText 进入。
- PersonRelation 使用限量列表展示，暂未做分页和大型 Network Graph。
- Place 当前为辅助信息，不接管 Geography；只有已有可靠坐标的 Place 才显示坐标。
- 当前仅有 3 条通过 QA 的 Story：没有 Story 的 Period 显示“这个时期的数据正在整理中”，不会补造内容。
- 仍未实现 AI 总结、全文搜索引擎、Vector Search、3D/GIS 历史地图和 Story 自动生成。
- 本次环境未启动完整 Tauri 桌面窗口，因此没有提交运行时截图；已完成前端生产构建与真实 DuckDB Repository 测试。

## 三条 Story 验证

数据源 QA 报告中三条 Story 均为 `usable = true`，并已由 Tauri Repository 测试读取：

| Story | Story Flow | Person / Place | HistoricalText | 结果 |
|---|---|---|---|---|
| 楚汉争霸 | 9 个事件，按 sequence | 人物可读；未可靠地点保留待考 | 真实 EventText | 通过 |
| 三国格局形成 | 8 个事件，按 sequence | 可从官渡之战进入曹操；赤壁地点待考 | 可进入三层史料阅读 | 通过 |
| 安史之乱 | 9 个事件，按 sequence | 可从长安失守进入郭子仪等人物；地点状态受 QA 约束 | 未出现已修复的跨时代文本错链 | 通过 |

## 测试

- `npm run build`：通过，TypeScript `tsc --noEmit` 与 Vite production build 均通过。
- `cargo fmt --all -- --check`：通过。
- `cargo check -p devtoolbox-desktop`：通过。
- `cargo test -p devtoolbox-infrastructure semantic_repository_reads_curated_story_flow_and_evidence -- --nocapture`：通过。
- Rust Infrastructure 全量测试：97 passed。
- Python Semantic / Pipeline / Query 测试：25 passed。
- `git diff --check`：通过，无空白错误。

## 最终判断

History UI V2 已接入真实 DuckDB Semantic Layer，并支持“时间 → Story → Event → Person / Place → HistoricalText → Source”的核心探索链路。当前 `READY_FOR_HISTORY_UI_V2 = true` 的三条验收 Story 均可进入和阅读；未完成的实体关联会以明确质量状态呈现，不会被前端伪装成已确认事实。
