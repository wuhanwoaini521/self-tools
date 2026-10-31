# Design System

## Direction

**Sunrise Glass** — a personal workspace for collecting and returning to work across learning, notes, reading, travel, and AI. The 2026-10-31 refinement replaces the earlier warm-paper treatment with a luminous field moving from sky blue through violet into apricot and soft yellow, translucent glass surfaces with crisp bright edges, quiet depth, and clear graphite text. The interface stays familiar, calm, and fast to scan.

## Visual grammar

- Keep the existing Manrope family and active theme tokens. Use a restrained type scale, readable weights, and sentence-case labels. Headings use tight tracking (`-0.02em` to `-0.025em`); avoid decorative or handwritten faces.
- The aurora field is painted by the shell, not by content: a blue → violet → apricot light field with a soft horizon glow at the bottom. It must never sit behind long reading text.
- Glass surfaces use high-opacity panel colour (`70%–88%`), a modest backdrop blur (`18px–28px`), a `1px` cool edge, and a bright inset top highlight. Keep reading content crisp: cards are frosted, article bodies are not.
- Use the active theme's accent for the selected route, primary actions, and focus rings. Primary actions use the accent gradient when the theme defines `--accent-grad-a/b`; otherwise the theme accent is used as the fallback so the control never becomes transparent. Do not assign different hues to navigation groups or Home modules. Reserve success, warning, and error colours for their semantic states.
- Keep information in aligned rows with a clear title, concise context, and action. Let real content and icon labels identify a module.

## Application frame

- Retain the existing persistent top bar, route-backed side navigation, mobile bottom navigation, search, and settings actions.
- The top bar, navigation rail, search field, settings button, and mobile bottom bar are frosted glass surfaces with a crisp bright top edge. The desktop navigation remains a stable rail with one consistent active state across all groups.
- Keep the user's current theme and stored preference intact. Keep controls familiar and keyboard-accessible, with visible focus and sufficiently large mobile targets.

## Home

- Home inherits the shell field. Lead with the current date, the existing resume/review action, and the existing learning progress.
- Present the streak and mastery values as supporting facts, not as separate decorative dashboard cards.
- Group the existing learning, saved knowledge, travel, reading, and notes data into typographic sections and flat rows. Preserve every loading, empty, error, retry, navigation, and edit action.
- Keep the Home layout responsive: a two-column scan path on wide screens and a single column on mobile.

## Language → English

- The English workspace is the primary focus surface of the sunrise field: the greeting and page title sit directly on the aurora; cards, metric chips, and the single focused “continue” card are frosted glass.
- The focused card keeps a warm apricot-to-violet tint; the primary action is the accent gradient with a soft accent shadow.
- Travel English (`旅行英语`) is a self-contained learning surface inside the same route: a segmented glass tab bar for 场景 / 词汇 / 语法 / 阅读 / 学习进度, a responsive card grid (three columns on wide screens, one on mobile), inline sentence rows with a speech control, example blocks for vocabulary, corrected/incorrect grammar pairs, answer options with correct/wrong states, and progress metric tiles.
- Travel English content is static typed content; learning records are written only through the existing `learning_record_event` contract. The UI never fakes review scheduling, a wrong-answer queue, or browser-local progress.
- The English home exposes the Travel English entry in the no-course, imported-course, and dashboard-error states, because the material does not depend on New Concept English import.

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
- `theme/sunrise-glass.css` is imported last and only paints: the aurora field, glass chrome, and the Language English / Travel English surfaces. Other light themes receive a warm haze from their own palette; dark themes (`warm-editorial-dark`, `nord`, `catppuccin-macchiato`) receive a low-opacity violet/apricot version of the same field and keep their own accent for actions.
- The layer honours `prefers-reduced-transparency` (solid surfaces, no field) and `prefers-reduced-motion` (no transitions). Do not create raster assets for this scope.
- This refinement covers the shared application frame, Home, the Language English workspace (including Travel English), and the global AI entry. Other feature-page content remains outside scope and keeps its existing markup.

## Research influences

- [Memos](https://github.com/usememos/memos/releases/tag/v0.31.0): quick capture and date-, tag-, calendar-, and map-based retrieval.
- [AppFlowy](https://github.com/AppFlowy-IO/AppFlowy/releases/tag/v0.14.11): consistent content views, feed previews, and direct quick actions.
- [Logseq](https://github.com/logseq/logseq/releases/tag/2.0.2): text-first hierarchy and study-oriented workflows.

These references inform retrieval, hierarchy, and consistency. Do not copy their branding, community features, team/admin emphasis, or graph-first navigation.
