# self-tools 全量体检问题清单

> 基线 commit：`fa9f3e5`
> 体检方式：真实运行（apps/server 真实 SQLite 数据 + 浏览器真实渲染）
> 覆盖：16 个一级页面 × 2 视口(1440×900/1280×800) × 2 主题(浅/深) = 64 次页面检查 + 226 次按钮点击

## 汇总

| 级别 | 数量 |
|---|---|
| P0 阻断 | 0 |
| P1 核心功能损坏 | 2 |
| P2 明显影响体验 | 3 |
| P3 视觉/小交互 | 待定 |

---

## P1

### [P1-01] Markdown「Save」在非桌面运行时崩溃

- **模块**：Markdown
- **问题**：`persist()` 在无打开文档时直接调用 Tauri dialog 的 `save()`，**漏了 `isTauriRuntime()` 守卫**（同文件的 `openNativeDialog()` 有守卫）。
- **类型**：Crash / Runtime / Regression
- **复现**：浏览器打开 Markdown 页 → 点 `Save`
- **实际**：`TypeError: Cannot read properties of undefined (reading 'invoke')`
- **预期**：明确提示「网页端无法保存到本地文件系统」，不崩溃
- **原因**：早期修复 `open()` 静默失败时（2345494）只补了 `open()`，漏了同文件 `persist()` 里的 `save()`
- **栈**：`@tauri-apps/plugin-dialog.save` → `__TAURI_INTERNALS__.invoke` undefined
- **状态**：✅ 已修并验证

### [P1-02] System 页谎报「database 未配置」

- **模块**：System（SystemReadinessPage）
- **问题**：页面对 13 项能力显示状态，其中 `database` 显示「未配置」，但数据库实际正常运行（正在读取真实数据）。detail 写「由后端 ReadinessService 提供（尚未装配）」。
- **类型**：Functional / Data（误导性报告）
- **复现**：打开 System 页
- **实际**：`关键 database → 未配置`，9 项能力全部「尚未装配」
- **预期**：真实反映后端就绪状态
- **原因**：`crates/application/src/readiness/service.rs` 的 `ReadinessService` 已完整实现并有测试，但**从未装配到任何 Tauri 命令 / HTTP 端点**；前端 `localChecks()` 只返回 4 项，UI 按 `REQUIRED_IDS`(13) 渲染，缺失项回落到 `not_configured`
- **状态**：✅ 已修并验证

---

## P2

### [P2-01] News 卡片破图

- **模块**：News
- **问题**：39 张 BBC 新闻图 `naturalWidth === 0`，数据库中存的 BBC CDN 链接真实返回 404（BBC 会轮换图片地址）。
- **类型**：UI / Data
- **复现**：打开 News 页
- **实际**：图片位置显示破图占位
- **预期**：图片加载失败时回落到占位图/标题色块，不显示破图图标
- **原因**：`img` 没有 `onError` 兜底
- **状态**：✅ 已修并验证

### [P2-02] 首次启动被强制刷新一次

- **模块**：PWA / 全局
- **问题**：Service Worker 首次接管触发 `window.location.reload()`，首屏加载后约 3.3 秒整页重载一次。
- **类型**：UX / State
- **复现**：清空 SW 后首次打开（每次 SW 更新也会触发）
- **实际**：`controllerchange` → `reload()`，此时页面上未保存的状态会丢失
- **预期**：首次安装 SW 不应强制刷新（没有旧 SW 需要替换）
- **原因**：`pwa.ts:96` 的 `controllerchange` 监听没有判断「变更前是否已有 controller」
- **状态**：✅ 已修并验证

### [P2-03] Geography 3D 地形依赖外网瓦片

- **模块**：Geography
- **问题**：3D terrain 瓦片来自 `tiles.mapterhorn.com`（已 404/不可达），离线时地图区域空白。
- **类型**：UI / Responsive（离线能力）
- **复现**：进入地形视图
- **实际**：瓦片请求 404
- **预期**：瓦片不可用时给出明确提示，而不是空白
- **状态**：✅ 已修并验证

---

## 已排除（误报，记录以免重复排查）

- History / Review / Home 的「无反应按钮」：审计脚本按钮索引失效导致点错元素；实际手工复核均正常（打开详情面板/弹窗，DOM 变化发生在 `.page-pane` 之外）。
- Search 的「唤起 AI / 系统设置」：实际打开了全局面板，页面 DOM 不变属正常。
- Server 页「空白」：实际是诚实能力声明「浏览器预览不支持家庭服务器，请在桌面端使用」，不是空白页。
- Study / Collections 「空白」：实际是诚实空态（0 笔 / 0 合集）。

---

## P2

### [P2-04] PWA 每页产生一条 404（死代码）

- **模块**：PWA / 全局
- **问题**：`PwaBanner` 启动时 fetch `/api/health` 取后端版本，但该路由**在两种运行时都不存在**（Tauri 无 HTTP 服务端；server 只有 `/health` 且不返回版本）。于是每打开任何页面都留下一条 404，且永远拿不到版本。
- **类型**：Dead Code / Network
- **修复**：删除该探测。前后端在桌面端是同一个包发布，不存在版本错配场景；`versionNotice(null)` 本就返回 null。
- **状态**：✅ 已修（全域 4xx 请求归零）

---

## Phase 7 跨模块流程结果

| Flow | 结果 |
| --- | --- |
| A Dashboard → Language → Lesson → 学词 → 进度 | 6/6 PASS |
| B AI 未配置时诚实告知 | 1/1 PASS |
| C News 列表 → 打开详情 | 2/2 PASS |
| D History → 事件 → 返回 | 3/3 PASS |
| E Settings → 主题切换立即生效 | 4/4 PASS |

跨模块合计 **16/16 通过**。

---

## 真实桌面应用验证（Tauri 二进制 + 真实 SQLite）

- 应用成功启动，渲染真实数据（连续 3 天 / 今日已学 107 项 / 掌握度 44% / 4 篇真实新闻）
- 导航在真实 WebView 中工作（Collections / Travel / RSS / System 均正常渲染）
- **System 页修复在真实应用中得到确认**：数据库显示「就绪 · 数据目录可访问」，
  不再是「由后端 ReadinessService 提供（尚未装配）」

---

## 已知限制（未解决，如实记录）

| 项 | 说明 |
|---|---|
| Geography 3D 地形瓦片 | 依赖 `tiles.mapterhorn.com` 外网服务，已 404。地形视图离线时空白。数据层（知识/实体/搜索）不受影响。 |
| News 源站图床 | BBC CDN 图片地址已失效（真实 404）。已加 6s 超时兜底，不再泄漏 `<img>`；但图片本身拿不到。 |
| Server 页网页端 | 显示「浏览器预览不支持家庭服务器，请在桌面端使用」——诚实声明，非空白页。 |
| macOS GUI 自动化 | WKWebView 无法被 Playwright 驱动（safaridriver 需交互式授权会挂起）。桌面端验证采用「真实二进制启动 + 屏幕截图 + Rust 层命令测试」。 |

---

## Phase 8：学习板持久化（Study Board）

| 发现 | 处理 |
|---|---|
| 画板笔迹只存浏览器 `localStorage`：换浏览器/清缓存就没，网页端与桌面端各存各的 | ✅ 新增 `StudyBoardService` 用例，桌面命令 + HTTP 端点共用，数据落 `config/study_boards.db`；旧 localStorage 数据首次进入画板时自动导入并清掉本地副本 |
| AI 看到的 `study-board.list` 与用户眼前那块板不是同一块 | ✅ 工具层改为调用同一份用例（`personal_ai::study_board` → `StudyBoardService`），ToolResult 形状、文本与错误 reason 均未变（28 个既有测试全通过） |
| 列表里每块板的笔画数恒为 0 | ✅ `StudyBoardSqliteStore::list_boards` 曾硬编码 `stroke_count: 0`（列表页显示「0 笔」但打开有笔），改为由 strokes 文本现算 + 回归测试 |

验证：Rust 全量测试通过（含 5 个新 HTTP 黑盒用例）、clippy 零警告、
前端 71 passed、`tsc --noEmit` 通过；真实起 server 用 curl 走完
save → list → get → snapshot 往返，非法 id 400 / 不存在 404。

---

## Phase 9：新闻源「静默失败」专项（2026-10-05）

用户反馈：「新闻页面有些数据应该抓取不全」。实际查库后发现**四个独立缺陷**，
其中三个是**静默失败**（不报错、看起来正常、数据其实不对）：

| # | 发现 | 证据 | 状态 |
|---|---|---|---|
| 1 | 11 个种子源里 **6 个已死** | 新华网 404、财联社 404、第一财经 404、路透中文 301→401、36 氪返回 HTML、澎湃 302→首页 | ✅ 目录重新策展为 12 个**实测可用**源（中新网分频道 / 东方财富 / IT之家 / 极客公园 / 联合国新闻…），每条带 `verified_on` |
| 2 | 人民网**不报错但已停更** | 一直返回 200，但最新条目停在 **2025-06-05**（14 个月），静默贡献 100 篇旧闻 | ✅ 新增 `SourceHealth::Stale`（最新文章 > 10 天）+ `Disabled` + 停用原因；不再被算成「一切正常」 |
| 3 | 摘要渲染成碎片 | 中国新闻网的 `description` 实际是 `\r\n伪科普、加速包、` 与 `\r\n据网络平台数据` | ✅ 新增 `summarySnippet()`：清洗空白、短于 24 字或无句末标点 → 显示「源站未提供摘要」；长摘要按**句边界**截断（不再被 CSS 从中间切） |
| 4 | **网页端根本没有刷新入口** | `news_refresh_now` 只是 Tauri 命令，transport 无映射，浏览器点刷新只会得到「尚无网页端接口」 | ✅ 补 `POST /api/v1/news/refresh`，与桌面端同一个 `NewsIngestPort::refresh` |

顺手修的两个相关缺陷：

- **跨源同标题重复**：同一条通稿在「中国新闻网」与「中国新闻网·财经」各存一条，
  列表里连着两条同样的标题。按标题保留最早入库的那条（按源查看不受影响）。
- **`list_sources` 遇到 `site_url` 为空的行会整体报错**（用 `String` 读 NULL）：
  `Sqlite("Invalid column type Null at index: 5")` —— 任何没有站点首页的源都会让源列表查询失败。

## 防止再次腐烂

- `scripts/check_news_sources.sh`：逐个拉取、验证是否 feed、报告最新条目时间；
  判定分 DEAD / FLAKY（限流，别误判）/ STALE / ok，并给出修复步骤。
- 老用户不需要手动处理：`NewsRepository::open` 里的迁移会按 URL 自动把死源换成
  新目录里已验证的源（`OUTDATED_SEED_SOURCES`），并把停更源标记为已停用。
- 迁移先做**只读检查**再写：没有待修行时完全不碰写锁（否则桌面端与网页端同时
  开着会撞 SQLITE_BUSY）。


---

## Phase 10：History 页面「不像这个应用」（2026-10-05）

用户反馈：**「这 UI 有问题，字体背景不对劲」**（附 History 时期详情截图）。

### 根因一：History V2 有自己的一套配色，完全无视主题

`.history-v2` 在**模块作用域内重新定义**了全局 token：

```css
.history-v2 {
  --history-v2-paper: #f6f2e9;   /* 暖米白纸面 */
  --history-v2-ink:   #23251f;   /* 墨黑 */
  --history-v2-accent: #4e8a83;  /* 低饱和青绿 */
  /* 然后把 --bg / --panel / --text / --accent 全指向自己 */
  --bg: var(--history-v2-paper);
  --text: var(--history-v2-ink);
  …
}
```

后果：
- **换主题时 History 纹丝不动** —— 无论选 pixel-light / warm-editorial / nord /
  catppuccin / 深色，它永远是那张米白纸面，看起来像「另一个应用」；
- 浅色主题下 `--bg #f6f2e9` 与 `--panel #fbf9f2` 只差 3 个色阶 → **卡片和页面底色
  糊成一片**（实测人物卡 `#faf8f3` 落在 `#f7f5ef` 上，肉眼几乎看不出边界）；
- 深色主题更糟：米白纸面 + 浅色文字的组合基本不可读。

### 根因二：13 处硬编码衬线字体

`.history-v2-*` 的标题与专名用
`font-family: Georgia, "Songti SC", "Noto Serif SC", "SimSun", serif`，
而全应用是 `var(--font-sans)`（Manrope + 系统无衬线）—— 换到 History 就换了一套字体。

### 改动

1. **History 不再有自己的配色**：`--history-v2-*` 全部由全局 token 派生
   （`--bg` / `--panel` / `--text` / `--accent` / `--line`…），
   并删掉那三段「按主题硬编码 accent」的覆盖块；`.history-page` / `.history-empty`
   同样改为从 `--accent` 派生。**换主题，History 跟着换。**
2. **13 处衬线栈 → `var(--font-sans)`**（层次靠字号/字重，不再靠字体家族）。
3. **章节标题的分隔线**从「墨色」改为 `--line`：深色主题下墨色≈白，会变成刺眼白线。
4. **两个浅色主题补回层次**（这是「背景不对劲」的第二层原因）：
   - `warm-editorial`：`bg #f7f5ef → #f3f0e7`，`panel → #fbf9f4`，
     `panel-raised → #fffdf8`，`line → #dcd4c3`（此前三者只差 2–3 色阶）；
   - `pixel-light`：`bg → #f4f4f3`，`panel → #fcfcfc`。
5. **顺手补上 `.history-v2-state` 的样式**（载入 / 错误态此前**完全没有 CSS**）：
   「历史数据知识库未就绪」原本是一段裸文字贴在底色上，现在是有图标、有排查指引的
   居中状态，并补了 loading  spinner。

### 验证

真实 Chrome + 真实 duckdb 数据 + 真实 server，逐主题截图对比：

| 主题 | 修复前 | 修复后 |
| --- | --- | --- |
| warm-editorial | 米白纸面 + 衬线专名 + 卡片糊成一片 | 主题暖调 + 无衬线 + 卡片边界清晰 |
| default（深色） | 同一张米白纸面（基本不可读） | 深色面 + 主题蓝 accent |
| pixel-light | 同上（层次塌陷） | 卡片与底色分得开 |

截图存 `apps/output/audit/history-theme-{warm-editorial,default,pixel-light}.png`、
`history-error-state.png`；另跑了 Home / News / Study / Settings 四页的主题切换巡检
（`sweep-*.png`），没有出现被新 token 破坏的页面。


---

## Phase 11：控件不统一（2026-10-05）

用户反馈：**「部分按钮、字体、下拉、分类等还是这种感觉的样式，不太统一」**（附 Knowledge 筛选行截图：
搜索框 + 「全部分类」下拉 + 刷新按钮三者高低、字体、边框各不相同，下拉还是 macOS 原生控件）。

### 巡检结果（不是凭感觉，是逐页量的）

写脚本把 11 个页面里所有 `select / input / textarea / button` 的计算样式收上来统计：

| 对象 | 修复前 | 修复后 |
| --- | --- | --- |
| `<select>` | 高度 34 / 41 / **58**px，字号 14px，`appearance: auto`（macOS 原生箭头） | **36px / 13px / `none`** |
| `<input>` | 高度 26 / 28 / 34 / **42** / 44px，字号 14–16px | **36px / 13px / `none`** |
| 行内按钮 | 与同行控件不同高（30 / 33 / 34 / 42 / 58px） | **36px**，与同行控件一致 |
| 按钮圆角 / 字号 | 6 / 10 / 16px 三种圆角，10–16px 七种字号 | 行内控件统一 10px / 13px（卡片型大按钮不动） |

最刺眼的一处：Knowledge 那一行里 **select 52px + 搜索按钮 22px + 输入框 42px**。

### 根因

1. **没有人定义「控件该多大」**：每个页面各写各的 padding/height/border-radius，
   同一行控件自然对不齐；
2. **`appearance` 全是 auto**：macOS 会给 `<select>` 画原生控件（灰底 + 上下箭头），
   在任何主题里都和设计语言不像；
3. **flex 行默认 `align-items: stretch`**：36px 的下拉会被同行最高的搜索框拉成 52px
   —— 这是「同一行高低不齐」的另一半原因；
4. 部分页面「外层盒子有边框 + 内层 input 也有边框」，输入框看起来像被套了个相框。

### 改动

1. **控件契约**（`:root` 四个变量，作为唯一依据）：
   `--control-height: 36px` / `--control-radius` / `--control-font: 13px` / `--control-pad-x: 12px`。
2. **基础控件层**：`appearance: none` + 统一高度/字号/圆角 + hover / disabled / focus 状态；
   数字输入的步进箭头与搜索框的原生清除按钮一并去掉。
3. **下拉箭头**：按主题给两套内联 SVG（`--control-arrow`）+ `color-scheme: light|dark`
   —— 深色主题里下拉列表和日期选择器也跟着是暗的。
4. **行内按钮用 `:has()` 精确圈定**：只统一「与输入框/下拉并排的按钮」，
   卡片型大按钮（列表项、文章卡）不受影响。
5. **行内 flex 行 `align-items: center`**，并把 Travel 自己写死的 42px/14px 交回契约。
6. **搜索框去双边框**：内层 input 透明无边框，外观由外层盒子承担。

### 过程中踩到的两个坑（都记在代码注释里）

- `mask-image` 画下拉箭头会把 `<select>` 的**文字也一起遮掉**（只剩一个箭头可见）→ 改用按主题的 `background-image`。
- 深色主题块的选择器是 `:root, :root[data-theme="default"]`（含裸 `:root`），
  往里插 token 会**命中所有主题** —— 第一次插进去，浅色主题也拿到了深色箭头。

### 验证

真实 Chrome 逐页量：select / input / input[search] / input[date] **全部 36px / 13px /
appearance: none**；7 个「输入框 + 按钮」的行（news / rss / knowledge / search / travel /
geography）**偏差 0**。浅色与深色主题各截图确认
（`ui-warm-editorial-knowledge.png`、`ui-warm-editorial-dark-knowledge.png` 等）。
