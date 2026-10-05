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
