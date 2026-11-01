# Design System

## Direction

**Modern Flat（2026-11-01）** — 一个成熟、文字优先的个人工作区。当前设计语言取代 2026-10-31 的「Sunrise Glass」：去掉极光/玻璃/发光/手绘贴纸，改为近白画布、白色卡片 + 1px 细线、克制的单一 accent、密集但清晰的信息层级。目标是浏览器、桌面端和手机端看起来是同一套系统，且与 Ember 风格参考图一致。

历史备注：2026-10-31 的 Sunrise Glass 方向（极光场 + 玻璃表面）仍保留在 `theme/sunrise-glass.css`，但已被 `theme/modern-ui.css` 在默认主题下覆盖为扁平面；该文件现在只作为其它主题的兜底层，不再定义视觉方向。

## Visual grammar

- 画布 `--bg`（近白冷灰），卡片 `--panel-raised`（纯白），边界 `--line`（1px 细线），不再使用玻璃模糊、内高光、外部阴影堆叠；仅允许 `--shadow-xs` 级别的极轻投影（抽屉、弹窗、地图卡）。
- 圆角契约：卡片 14–16px、控件 10px、标签/胶囊 999px。不要为单个页面发明新的圆角。
- 强调色只有一个 `--accent`（默认主题 `#5566e6`）。成功/警告/危险只用于语义状态；不再给导航分组或模块分配独立色相。
- 字号阶梯（移动端）：正文 13–14px、次级 12px、微标签 10.5–11px、大数字 24–28px；正文与说明文字对比度 ≥ 4.5:1（`--muted` 4.9:1、`--faint` 4.6:1）。
- 控件契约：`--control-height: 36px`、`--control-radius: 10px`；手机上文本操作/标签页/工具栏按钮 44px、chip 36px、纯图标按钮 44×44。
- 按钮只有四种：主（accent 实底）、次（白底细线）、幽灵（透明 + 文字色）、图标（正方形）。hover 只改变背景/边框颜色，不做位移和缩放。
- 列表行 = 标题（13–14px/600）+ 右侧元信息（11–12px muted）+ 可选描述；间距 10–12px，整行可点。

## Application frame

- 顶栏：桌面保留品牌 + AI 助手 + 搜索 + 设置 + 主题 + 头像；手机收敛为品牌 + 搜索图标 + 头像，设置与主题切换进入「更多」面板。
- 桌面侧栏保持完整分组（17 个入口）；手机底栏只放 4 个主入口（Home / Review / Geography / Language）+「更多」，「更多」面板两列列出全部入口与 Settings，带 `aria-current` / `aria-expanded`，遮罩与 Esc 关闭。
- 移动端每页只有一条滚动链：`.app-content > .page-pane` 负责滚动，`.page-scroll` 不再自行滚动；页面内容高度自适应，`100dvh` + safe-area inset。
- 键盘可访问：所有交互保留 `:focus-visible` 焦点环；面板/弹窗有 role、aria 标签与 Esc 关闭。

## Modern UI layer

- `theme/modern-ui.css` 在 `main.tsx` 中最后导入，是覆盖层：平面画布、单一滚动链、手机底栏与「更多」面板、News/RSS/Markdown/Collections 等页的移动端布局、触控下限、以及各页残留渐变/玻璃的扁平化。
- 页面自有 CSS（如 `geography-explore.css`、`collections.css`、`era-timeline.css`）统一用 `.app-shell .app-content …` 前缀提高特异性，不再用 `!important` 堆叠；新增组件避免使用 `-stat` / `-kpi` / `-tabs` 等会命中全局通用规则的类名子串。
- 主题：默认主题 `data-theme="pencil"` 仍保留 id 与选择行为，但视觉由扁平 token 驱动；`sunrise-glass.css` 只作为其它主题的兜底，`modern-ui.css` 负责去掉其渐变。

## Home

- Home is the one deliberately **flat** surface: near-white canvas (no aurora), white cards with 1px hairlines, 14–16px radius, micro labels, tabular numerals — a data dashboard over the same data.
- Structure: date kicker + greeting + one-line summary, then four KPI tiles (due reviews, average mastery, streak, topics studied today) with meters, then a `1.55fr / 1fr` split — left: continue learning, news, travel; right: review-queue composition, saved knowledge, recent notes.
- Only show measured data: bars exist only for the review-queue composition (overdue / due / upcoming / learning / mastered) and the mastery progress; no fabricated trend series.
- Preserve every loading, empty, error, retry, navigation, and edit action. Responsive: split collapses to one column at ≤1100px; KPI row becomes 2 columns on mobile.

## Language → English

- （2026-11-01 更新）English 工作区已改为扁平：指标卡与「继续学习」卡为白底细线，主按钮为纯 accent 实底；原先的极光问候区与暖色渐变已由 `modern-ui.css` 覆盖。以下两条描述保留为历史记录。
- 历史：The English workspace is the primary focus surface of the sunrise field…（已被上式取代）
- Travel English (`旅行英语`) is a self-contained learning surface inside the same route: a segmented glass tab bar for 场景 / 词汇 / 语法 / 阅读 / 学习进度, a responsive card grid (three columns on wide screens, one on mobile), inline sentence rows with a speech control, example blocks for vocabulary, corrected/incorrect grammar pairs, answer options with correct/wrong states, and progress metric tiles.
- Travel English content is static typed content; learning records are written only through the existing `learning_record_event` contract. The UI never fakes review scheduling, a wrong-answer queue, or browser-local progress.
- The English home exposes the Travel English entry in the no-course, imported-course, and dashboard-error states, because the material does not depend on New Concept English import.

## 英语学习：先回忆，再核对

目标是用「主动回忆」代替「中英文同时铺开」，同时把长列表拆小。实现位于
`features/language/english/RevealLine.tsx`、`TravelEnglishSection.tsx`、
`SentenceStage.tsx`、`ReadingStage.tsx`。

- 例句、短语、词汇例句、逐句精听与精读的译文默认隐藏：英文下方是虚线胶囊「点击显示译文」，
  点英文或胶囊才展开译文，再点译文可收起。
- 每组提供「显示全部译文 / 隐藏全部译文」；逐句精听与精读还支持单句点开。
- 列表分批：旅行场景首批 4 个、词汇首批 24 个、语法首批 3 节、短文首批 4 篇，
  底部「显示更多（还剩 N）」继续加载；场景的「先开口说」每张卡先给 3 句，其余折叠。
- 译文展开/收起只影响本页展示状态，不写入学习记录，也不改动后端 SRS 语义。

开源参考（用 GitHub 检索到的项目）：
- [Lute v3](https://github.com/LuteOrg/lute-v3)：以阅读为核心的语言学习工具，逐词状态 + 句译切换。
- [FreeLingo](https://github.com/artcc/freelingo)：自托管 LLM 语言学习，闪卡 + 间隔重复 + 听说练习。
- [FlashCardPro](https://github.com/Mohfath/FlashCardPro)：Leitner 间隔重复、滑动复习的 Android 闪卡应用。

后续可借鉴：Lute 的「词状态」标记（未知/学习中/已知）、FlashCardPro 的左右滑动复习手势、
FreeLingo 的「先自评再看答案」流程。第 14 项之后如果要写记录，只通过既有
`learning_record_event` 契约完成，不由前端臆造调度。

## Travel

- 规划入口先给起点：第一次打开且没有任何行程时，展示三条可点击的起步行程（只填城市/天数/偏好，不发请求、不写数据）；填了城市后显示「行程速览」条件条（天数、日期、必去数量、偏好），提交前就能确认将按什么整理。
- 行程工作区以「天」为单位组织：每天是可折叠单元（单日收起/展开 + 全部收起/展开），长行程不必一路滚到底；天数筛选条在滚动时吸顶。
- 每天可直接「在地图查看」：切到路线地图 Tab 并预选该天；地图仍只绘制有真实坐标的地点，缺坐标时明确说明不绘制推测路线。
- 景点可拖动排序：同一天内拖到目标位置即重排，拖到另一天的卡片即跨天移动（与上/下箭头、`安排在 Day X` 共用同一份顺序）；抓手对辅助技术隐藏，键盘用上/下箭头；触屏隐藏抓手，避免与滚动冲突。
- 日程顶部有「行程概览」：天数 / 地点数 / 必去完成度 / 总里程，数据来自已加载的行程状态，不发请求。
- 最近行程、待同步草稿、历史攻略库与保存状态保持原有入口与语义。
- 参考 TREK 的 day-plan 起点与 AdventureLog 的 trip 模板，只借鉴信息结构，不复制其交互实现；拖拽排序、地图选点、自动路线优化属于后续项。

## Geography

- Explore 采用「地图 + 可点击索引」：精选地点 / 收藏横向卡片轨道、搜索结果卡片（类型徽标 + 中英文名 + 摘要 + 查看详情 + 按类型计数）、选中实体的地图浮动信息卡（Esc 或 × 关闭，不遮挡缩放控件与 attribution）。
- 索引与地图双向联动：索引卡 / 搜索结果卡右上角的定位按钮把实体选中到地图并平滑移动过去（只滚动地理页自身的滚动容器，不带动应用外壳）；地图选中实体时对应卡片同步高亮，并把索引轨道横向滚到该卡片。
- 今日探索卡提供「开始探索」与「换一个」：换一个从当前 featured 里随机挑一个非今日推荐的地点打开详情，不额外请求。
- 收藏只使用 `favorite_ids ∩ featured` 可推导的数据；只有 id 的收藏显示数量提示，不臆造名称。原「精选地理线索」改为「最近查看」（`home.recent`，含空态）。
- 地图图层切换、3D 地形、离线回退与既有实体详情保持现状；地图点击第一次出信息卡，卡内「查看详情」或再次点击同一 marker 才进入详情。

## History

- 时代以「按年份比例的水平时间带」为主轴：真实 `start_year`/`end_year` 参与比例，公元前用负年份；重叠时期（宋辽金、三国、十六国等）按首次适配分轨，年份位置保持准确；约略年代用虚线边标注；当前时期 accent 高亮并带 `aria-current`，方向键/Home/End 可切换，手机横向滚动并自动把当前时期滚入视野。
- 时期卡片压缩空白并补事实（事件/人物/政权）；详情桌面左栏为粘性事实栏，≤1100px 单列顺序化。
- 历史阶段区顶部有「阶段事件密度条」：条宽=年代跨度、条高=事件数量，点击跳到该阶段并立即高亮；章节滚动只滚动历史页自身容器，不带动应用外壳。
- 时代时间轴上方有「跨时期事件密度」：每个时期的柱子宽度跟随其时代分段、高度=该时期事件数（`periods[].event_count`），点击即可切换时期；无计数时不渲染。
- Story 阅读页底部有「上一篇 / 下一篇」顺序导航：优先使用首页精选 Story 池（可跨时期），当前 Story 不在池中时退回本时期 Story；没有上/下篇时不留空按钮。
- 参考 ybogdanov/history-timeline 的比例时间带与 Chronas 的时代切换；事件密度条、人物定位属于后续项。

## Global AI

- Keep the existing global toggle, keyboard shortcut, context assignment, assistant panel, callbacks, and conversation behaviour.
- Style the desktop entry as a glass pill in the top bar. The existing panel opens directly below it on a high-opacity frosted surface. On mobile, retain the floating 48px control above the bottom navigation and safe-area inset.
- Use one short state-change motion for opening the panel and disable it for reduced-motion preferences.

## Theme behaviour and scope

- Use the existing `--bg`, `--panel`, `--panel-raised`, `--line`, `--text`, `--muted`, `--accent`, `--accent-soft`, `--accent-grad-a`, `--accent-grad-b`, and semantic state tokens. Do not change theme IDs, selection behaviour, routes, feature data, API clients, persistence, or backend contracts.
- The default theme keeps `id="default"` and `data-theme="pencil"`; its palette (`theme/pencil.css`) is updated to the sunrise values: pale periwinkle canvas, white surfaces, graphite text, blue-violet accent, apricot selection highlight.
- `theme/sunrise-glass.css` 仍是极光/玻璃层的来源，但已不是最后导入：`theme/modern-ui.css` 在其后导入，默认主题下把画布、chrome、语言卡等统一压平（2026-11-01 起）。 Other light themes receive a warm haze from their own palette; dark themes (`warm-editorial-dark`, `nord`, `catppuccin-macchiato`) receive a low-opacity violet/apricot version of the same field and keep their own accent for actions.
- The layer honours `prefers-reduced-transparency` (solid surfaces, no field) and `prefers-reduced-motion` (no transitions). Do not create raster assets for this scope.
- This refinement covers the shared application frame, Home, the Language English workspace (including Travel English), and the global AI entry. Other feature-page content remains outside scope and keeps its existing markup.

## Research influences

- [Memos](https://github.com/usememos/memos/releases/tag/v0.31.0): quick capture and date-, tag-, calendar-, and map-based retrieval.
- [AppFlowy](https://github.com/AppFlowy-IO/AppFlowy/releases/tag/v0.14.11): consistent content views, feed previews, and direct quick actions.
- [Logseq](https://github.com/logseq/logseq/releases/tag/2.0.2): text-first hierarchy and study-oriented workflows.

These references inform retrieval, hierarchy, and consistency. Do not copy their branding, community features, team/admin emphasis, or graph-first navigation.
