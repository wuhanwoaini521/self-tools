# History UI V2 改造分析与实施计划

## 当前 History UI

当前入口为 `apps/desktop/ui/src/features/history/HistoryPage.tsx`，由 `App.tsx` 挂载。页面使用 `history_home`、`history_period_nodes`、`history_detail` 等命令，底层对应 `config/history.db` 的旧 `HistoryStore`；时代视觉、故事路线和部分文案还来自前端 `historyEras.ts` 与 `data/historyStories.ts`。

现有交互包括时代切换、搜索、人物/地图/关系/故事视图、通用详情页、收藏和跨模块地图跳转。旧详情模型把 Event、Person、Place 组织成通用 HistoryNode/HistoryRelation，无法表达 Semantic Layer 的 Story → Event → Person / Place → HistoricalText → Source 链路。

主要问题：

- 正式 History 页面没有消费 `history-data-pipeline/data/normalized/history.duckdb` 的语义表。
- 前端存在手写故事、时代说明和节点路线，不能作为正式历史事实来源。
- Story Flow 没有以 DuckDB `story_events.sequence` 为唯一顺序源。
- Event Detail 没有直接呈现 `event_person`、`event_place`、`event_text` 和 `event_relations` 的完整质量状态。
- HistoricalText 三层文本、Source Provenance 和 `needs_linking` 没有对应 UI。
- 返回路径只维护一个旧详情状态，不能保留 Story Flow 中的当前 Event。

## 可以复用

- `App.tsx` 的 History 挂载位置和跨 Geography 的导航 adapter。
- Tauri `invoke` 调用方式、现有主题变量、Phosphor Icons、响应式断点与通用按钮/表单样式。
- `HistorySearch` 的键盘交互思路，但结果类型将改为 Semantic Query 结果。
- `history-data-pipeline/HISTORY_SEMANTIC_QUERY_CONTRACT.md` 作为 TypeScript 与 Rust DTO 的边界。
- 已存在但未接入页面的 `HistoryDuckDbRepository`，在其上补齐最小只读聚合查询。

## 应该废弃

- History V2 正式路径不再使用 `historyStories.ts`、`historyEras.ts` 中的历史事实和路线数据。
- 不再让 `HistoryPage` 依赖旧 `HistoryStore` 的 `HistoryNode` / `HistoryDetailView` 作为正式数据模型。
- 不再把 Story 伪装成普通 Dashboard 卡片或把 Event Relation 画成强因果流程图。
- 保留旧 SQLite Store 与其测试作为其他兼容代码，但不再由 V2 页面读取。

## 新页面架构

### History Overview

入口调用 DuckDB `history_semantic_home`，读取全部 Period 与可用 Story。时间导航支持横向滚动、滚轮、拖动与键盘左右键；没有 Story 的时期只显示“这个时期的数据正在整理中”。

### Period Explorer

以 Period、年份范围和一句话说明为首屏，再列出该 Period 下真实可用的 Story。Period 与 Dynasty/Regime 信息来自 DuckDB，不在组件中补写历史内容。

### Story Explorer

调用 `history_semantic_story_detail` 获取 Story、`story_events`、聚合人物、地点、史料和 Source。Flow 严格按服务端返回的 `sequence` 渲染；点击 Event 打开右侧 Context Drawer，保留 Flow 上下文和当前选中位置。

### Event Detail

Drawer 调用 `history_semantic_event_detail`，展示事件时间、摘要、前置/后续事件、event_person、event_place、EventText 与 Source。地点无可靠链接时显示“地点待考”，不生成坐标。

### Person Detail

从 Event 或搜索进入 Person Detail。读取 DuckDB Person、人物关系、PersonPlace、参与 Event、相关 Story 和 Source，并在当前 Story / Event 语境中保留返回位置。人物关系默认按类别折叠/筛选，不渲染全量网络图。

### HistoricalText Reader

从 Event 的真实 EventText 进入阅读器，按 `original_text`、`original_simplified`、`translation_zh_cn` 提供“原文 / 简体 / 译文 / 对照”模式。`quality_status` 或 Source quality 为未复核时仅显示轻提示，并提供来源与数据说明折叠区。

### Place Detail

Place 作为辅助连接信息展示。可靠 Place 显示历史名、现代名和坐标；`place_id = NULL` 且 `link_status = needs_linking` 时保留原始地点名并显示“位置待考 / 尚未完成地理关联”，不放入现代地图。

## 数据流

```text
history.duckdb
  → HistoryDuckDbRepository（只读 SELECT）
  → Tauri history_semantic_* commands
  → HistoryPage 的 query hook / 状态
  → Overview / Period / Story Flow / Drawer / Reader components
```

## 实施顺序

1. 补齐 Rust DuckDB DTO、聚合查询和最小 Tauri commands。
2. 替换 HistoryPage 的数据模型与状态，移除正式路径上的前端历史事实常量。
3. 实现 Overview、Period、Story Flow、Event Drawer、Person、Place、HistoricalText 和 Source 区块。
4. 增加 loading / empty / error / quality 状态与返回位置保留。
5. 运行 TypeScript 构建、Rust 检查、现有测试和三条 QA Story 验收。

