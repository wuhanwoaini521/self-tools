# 开源项目 UI 研究：旅行 / 地理 / 历史

日期：2026-10-31
范围：只研究「信息架构与交互模式」，不引入依赖、不复制实现；落地改动保持前端范围。

## 方法

- GitHub 仓库检索（stars 排序）+ 仓库 README 阅读，挑出三个领域里真实在用、维护活跃的项目。
- 提炼能映射到 self-tools 现有数据契约（`TravelPlan` / `GeographyHome` / `SemanticHome`）的模式。
- 每条结论都标注「我们采用了什么 / 为什么不做」。

## 旅行

### 参考项目

- [liketrek/TREK](https://github.com/liketrek/TREK)（15.3k★，TypeScript）：day plan 是可编辑的一等对象——「drag places, notes and bookings between days with undo」「drop a map marker straight onto a day」；Explore 按类别给附近地点；路线支持按天自动排序（nearest neighbour + 2-opt）；天气用 Open-Meteo；手机端有独立布局。
- [seanmorley15/AdventureLog](https://github.com/seanmorley15/AdventureLog)（3.8k★）：以「行程/收藏/统计」为主的 trip tracker，卡片化列表 + 详情页。
- [itskovacs/trip](https://github.com/itskovacs/trip)（2k★）：极简 POI 地图 + 行程规划，先给地图和地点，再谈行程。

### 提炼

1. 用户第一次打开时，不应该面对一张空表单；先给「可编辑的起点」（模板 / 热门城市 / 最近行程）。
2. 表单填写过程中要持续反馈「将按什么条件整理」，而不是等提交才知道。
3. 行程本身应以「天」为单位组织，地图与列表联动；这一点我们的 `TravelGuide` 已有，属于后续深化项。

### 本轮采用

- 新增「行程速览」：填了城市后用一句话 + 条件 chips 说明将按什么整理（天数/日期/必去/偏好）。
- 新增「起步行程」：第一次打开且没有任何行程时，给三条可点击模板（杭州 3 天 / 成都 4 天 / 西安 2 天），只填表单，不发请求、不写数据。
- 实现：`apps/desktop/ui/src/features/travel/TravelPage.tsx` + `features/travel/travel.css`。
- 第三轮（行程工作区）：按 TREK 的 day plan 模式，每天可折叠（单日 + 全部收起/展开），天数筛选条吸顶；每天可「在地图查看」跳到路线地图并预选该天。实现：`features/travel/TravelGuide.tsx` + `features/travel/travel.css`。
- 顺手修正：通用规则 `.app-content [class*="-stat"]` 会把 `day-stat-chip` / `day-stats-right` / `travel-plan-status-copy` 误当统计卡，已用同特异性规则还原。

### 暂不做

- 拖拽排序、地图选点、自动路线优化：需要新的前端交互层与地图联动，属于下一轮。
- 预算 / 打包清单：没有对应后端契约，前端不造数据。

## 地理

### 参考项目

- TREK 的 Explore：按类别浏览附近地点，地点卡带照片/评分/开放时间，地图与列表联动。
- MapLibre GL 官方示例：图层切换、terrain、marker 聚合，都是「地图是主角，控制是轻量浮层」。
- 我们的 `GeoMap` / `MapLibreMap` 已实现基础/行政/3D 地形/河流图层，缺的是「地点索引与结果呈现」。

### 提炼

1. 地图页需要一条「可点击的地点索引」，让用户不靠搜索也能开始探索。
2. 搜索结果要能直接读出类型与关键属性，而不是只有名字。
3. 选中实体时给一张轻量信息卡，避免立刻跳走。

### 本轮采用

- Explore 搜索结果卡片化：类型计数侧栏 + 卡片网格（类型徽标、中英文名、摘要、查看详情），空态给出 4 个已知地点建议，搜索中/错误态完整。
- 地图浮动信息卡：点选实体后在视口左下浮起玻璃卡（类型、名称、坐标、2–3 条真实 properties、收藏状态、查看详情），Esc / × 关闭；第二次点击同一 marker 或「查看详情」进入详情。
- 「精选地点 / 收藏」横向索引轨道：严格使用 `home.featured` 与 `home.favorite_ids` 交集；只有 id 的收藏只提示数量，不造名称。原「精选地理线索」改为「最近查看」（`home.recent`）。
- 索引 ↔ 地图双向联动（第二轮）：索引卡 / 搜索结果卡右上角定位按钮 → 选中实体、平滑移动到该坐标并滚动到地图卡（只滚地理页滚动容器）；地图选中实体时对应卡片高亮。
- 实现：`features/geography/GeographyPage.tsx`、`features/geography/geography-explore.css`、`GeoMap.tsx`（新增 `focus` 观察中心）、`MapLibreMap.tsx`。

## 历史

### 参考项目

- [ybogdanov/history-timeline](https://github.com/ybogdanov/history-timeline)（354★，"Map of Contemporaries"）：按年份比例绘制水平时间带，年份刻度尺，当前项高亮，hover 显示「此刻在世的人」；作者列出的改进项包括缩放与人物联动。
- [Chronasorg/chronas](https://github.com/Chronasorg/chronas)：历史地图 + 时代滑杆，强调「时代切换」是主交互。
- TimelineJS：事件卡片挂在水平时间线上，适合叙事型内容。

### 提炼

1. 朝代/时代切换不应该只有一个下拉或文字链接，应该是可见的、按时间比例的水平带。
2. 时间带要有年份刻度与当前项高亮，点击即切换。
3. 事件详情继续用卡片，挂在时间线之下，而不是塞进时间带。

### 本轮采用

- 新增 `EraTimeline`：按真实 `start_year`/`end_year` 比例分段的时代带；公元前负年份、约略年代虚线、开放起点（start_year=null）单独标注；重叠时期按首次适配分轨（31 个时期全部可点）；当前项 accent 高亮 + `aria-current`；方向键/Home/End 键盘导航；手机轨道横向滚动并自动把当前时期滚入视野。
- PeriodDetail 顶部三卡压缩空白并补「事件/人物/政权」事实；详情桌面左栏粘性玻璃事实卡、右栏正文，≤1100px 单列顺序化。
- 实现：`features/history/EraTimeline.tsx`、`features/history/era-timeline.css`、`eraTimeline.test.ts`（6 个布局单测）、`HistoryPage.tsx`、`PeriodDetail.tsx`。
- 第八轮（地理探索小步）：今日探索加「换一个」随机入口；地图选中实体时索引轨道自动横向滚动到对应卡片。
- 第七轮（跨天拖动 + 行程概览）：拖动支持跨天移动（改 assignedDays 并插入目标天），日程顶部加「天数 / 地点数 / 必去完成度 / 总里程」概览条；数据全部来自本地行程状态。
- 第六轮（拖动排序）：按 TREK 的 day plan「drag places between positions」模式，在同一天内用左侧抓手拖动重排景点，复用 `selectedKeys` 顺序；抓手对辅助技术隐藏、触屏隐藏，键盘仍用上/下箭头。
- 第五轮（Story 顺序导航）：Story 阅读页底部加「上一篇 / 下一篇」，优先用首页精选 Story 池（可跨时期），当前 Story 不在池中时退回本时期 Story；同时把章节滚动限制在历史页容器内。
- 第四轮（阶段密度）：在「历史阶段」上方加「阶段事件密度条」——宽度按 `start_year/end_year` 比例、高度按 `event_count`，点击跳到该章节并立即高亮；只使用 `periodDetail.stages` 已有字段，不额外请求。同时把章节滚动限制在历史页自己的滚动容器内（此前 `scrollIntoView` 会把应用外壳一起滚走）。

## 下一步（按价值排序）

1. Travel：拖拽排序与地图选点（需要新的交互层，下一轮评估）。
2. History：时期级事件密度仍缺字段（`SemanticPeriod` 没有 event_count）；当前只在「当前时期详情」里做了阶段密度，跨时期密度需要后端补字段。
3. 技术债：`geography-explore.css` 的 `!important` 仍偏多（为退出旧纸张按钮皮肤），可收敛为一个作用域 reset。
