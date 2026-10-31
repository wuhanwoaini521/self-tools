# 三模块前端改进记录（2026-10-31 夜）

范围：Travel / Geography / History 三个模块的**前端体验**改进，依据对开源项目的研究（见 [`oss-ui-research.md`](./oss-ui-research.md)）。
约束：FRONTEND ONLY；不改后端业务逻辑、API 契约、持久化；不新增依赖；保留工作区既有改动。

## 研究来源

- 旅行：[liketrek/TREK](https://github.com/liketrek/TREK)（day plan、Explore、拖动排序）、[seanmorley15/AdventureLog](https://github.com/seanmorley15/AdventureLog)（trip 模板）、[itskovacs/trip](https://github.com/itskovacs/trip)（极简 POI 地图）
- 历史：[ybogdanov/history-timeline](https://github.com/ybogdanov/history-timeline)（按年份比例的时间带、密度）、[Chronasorg/chronas](https://github.com/Chronasorg/chronas)（时代为主轴）、TimelineJS（事件卡挂时间线）
- 地理：TREK Explore 的「地图 + 地点索引」、MapLibre GL 图层切换模式

## 改动清单

### Travel

| 改动 | 文件 |
|---|---|
| 行程速览（填城市后显示天数/日期/必去/偏好条件条） | `TravelPage.tsx`、`travel.css` |
| 起步行程（首次打开给三条可点击模板，只填表单不发请求） | `TravelPage.tsx`、`travel.css` |
| 每天可折叠（单日 + 全部收起/展开）、天数筛选条吸顶 | `TravelGuide.tsx`、`travel.css` |
| 每天「在地图查看」（切到地图 Tab 并预选该天） | `TravelGuide.tsx` |
| 景点拖动排序（同天重排 + 跨天移动；触屏隐藏抓手，键盘用上下箭头） | `TravelGuide.tsx`、`travel.css` |
| 行程概览条（天数/地点数/必去完成度/总里程） | `TravelGuide.tsx`、`travel.css` |
| 修复 `[class*="-stat"]` 误命中 `day-stat-chip` / `day-stats-right` / `travel-plan-status-copy` | `travel.css` |

### Geography

| 改动 | 文件 |
|---|---|
| 搜索结果卡片化（类型计数侧栏 + 卡片网格 + 空态建议） | `GeographyPage.tsx`、`geography-explore.css` |
| 地图浮动信息卡（Esc/× 关闭，不遮挡缩放控件） | `GeographyPage.tsx`、`geography-explore.css` |
| 探索索引轨道（精选 / 收藏，收藏只显示可推导名称） | `GeographyPage.tsx`、`geography-explore.css` |
| 索引/搜索结果 → 地图定位（`GeoMap` 新增 `focus`），选中态同步 | `GeographyPage.tsx`、`GeoMap.tsx`、`geography-explore.css` |
| 地图选中 → 索引轨道自动横向滚动到对应卡片 | `GeographyPage.tsx` |
| 今日探索「换一个」随机入口 | `GeographyPage.tsx`、`geography-explore.css` |

### History

| 改动 | 文件 |
|---|---|
| 比例时代时间轴（多轨处理重叠时期、公元前负年份、约略年代虚线、键盘切换、手机自动定位） | `EraTimeline.tsx`、`era-timeline.css`、`eraTimeline.test.ts` |
| 时期页重构（顶部卡片压缩补事实、桌面粘性事实栏） | `PeriodDetail.tsx`、`era-timeline.css` |
| 阶段事件密度条（条宽=跨度、条高=事件数、点击跳转并立即高亮） | `PeriodDetail.tsx`、`era-timeline.css` |
| 章节滚动只滚历史页容器（不再带动应用外壳） | `PeriodDetail.tsx` |
| Story 上/下一篇导航（优先首页精选池，可跨时期） | `HistoryPage.tsx`、`era-timeline.css` |
| 跨时期事件密度（`periods[].event_count` 增量字段 + 时间轴顶部密度柱） | `history_records.rs`、`duckdb.rs`、`semanticTypes.ts`、`EraTimeline.tsx`、`era-timeline.css`、`eraTimeline.test.ts` |

## 验证证据

- `cd apps/desktop/ui && npx tsc --noEmit` → 0 错误
- `npx vitest run` → 22 个测试文件 / 133 个测试全过（新增 `eraTimeline.test.ts` 8 个、`geoTimeline.test.ts` 4 个）
- `cargo check --workspace --all-targets` 与 `cargo test -p devtoolbox-infrastructure --lib -- history`（11 项）通过
- `npx vite build` → 通过
- Playwright（channel=chrome，真实本地数据 `http://127.0.0.1:1422`）：
  - 桌面 1440×900 / 手机 390×844（含 hasTouch）逐页检查：`document.scrollWidth <= innerWidth`、`window.scrollY === 0`
  - 交互：历史切唐、密度段点击、Story 上/下一篇；地理搜索→详情→返回、索引定位、地图信息卡、换一个；旅行起步行程、空白行程折叠、在地图查看、同天/跨天拖动
  - 唯一网络失败是外部 Maptyhorn DEM 瓦片请求被中止（外部源，与代码无关）
- 截图目录：`output/visual-qa/modules-round1` … `modules-round8`、`output/visual-qa/final-sweep`

## 已知边界 / 后续

1. **跨时期事件密度**：已完成——`PeriodResult`/`SemanticPeriod` 新增 `event_count`（后端 SQL 按 `events.period_id` 统计，纯增量），时代时间轴顶部渲染密度柱；无计数时不渲染。
2. **Geography 的 `!important` 技术债**：`geography-explore.css` 中约 100+ 处 `!important`，用于退出旧「纸张按钮」皮肤（`.app-shell button:not(...)` 特异性 0,3,1）。彻底收敛需要把组件选择器整体提升特异性，风险较高，留作专项。
3. **真实数据状态覆盖**：夏/商/周等时期 `stages` 为空、部分时期没有 Story；密度条与 Story 导航按设计在无数据时不渲染。空白行程没有坐标，地图会明确说明不绘制推测路线。
4. **Travel 跨天拖动**在触屏不可用（抓手隐藏），触屏继续用「安排在 Day X」下拉。
5. 浏览器 QA 曾出现 `qa-bridge: unsupported command news_sources` 提示（仅 QA 桥环境，产品运行时不出现）。
