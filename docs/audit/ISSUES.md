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
- **状态**：待修

### [P1-02] System 页谎报「database 未配置」

- **模块**：System（SystemReadinessPage）
- **问题**：页面对 13 项能力显示状态，其中 `database` 显示「未配置」，但数据库实际正常运行（正在读取真实数据）。detail 写「由后端 ReadinessService 提供（尚未装配）」。
- **类型**：Functional / Data（误导性报告）
- **复现**：打开 System 页
- **实际**：`关键 database → 未配置`，9 项能力全部「尚未装配」
- **预期**：真实反映后端就绪状态
- **原因**：`crates/application/src/readiness/service.rs` 的 `ReadinessService` 已完整实现并有测试，但**从未装配到任何 Tauri 命令 / HTTP 端点**；前端 `localChecks()` 只返回 4 项，UI 按 `REQUIRED_IDS`(13) 渲染，缺失项回落到 `not_configured`
- **状态**：待修

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
- **状态**：待修

### [P2-02] 首次启动被强制刷新一次

- **模块**：PWA / 全局
- **问题**：Service Worker 首次接管触发 `window.location.reload()`，首屏加载后约 3.3 秒整页重载一次。
- **类型**：UX / State
- **复现**：清空 SW 后首次打开（每次 SW 更新也会触发）
- **实际**：`controllerchange` → `reload()`，此时页面上未保存的状态会丢失
- **预期**：首次安装 SW 不应强制刷新（没有旧 SW 需要替换）
- **原因**：`pwa.ts:96` 的 `controllerchange` 监听没有判断「变更前是否已有 controller」
- **状态**：待修

### [P2-03] Geography 3D 地形依赖外网瓦片

- **模块**：Geography
- **问题**：3D terrain 瓦片来自 `tiles.mapterhorn.com`（已 404/不可达），离线时地图区域空白。
- **类型**：UI / Responsive（离线能力）
- **复现**：进入地形视图
- **实际**：瓦片请求 404
- **预期**：瓦片不可用时给出明确提示，而不是空白
- **状态**：待修

---

## 已排除（误报，记录以免重复排查）

- History / Review / Home 的「无反应按钮」：审计脚本按钮索引失效导致点错元素；实际手工复核均正常（打开详情面板/弹窗，DOM 变化发生在 `.page-pane` 之外）。
- Search 的「唤起 AI / 系统设置」：实际打开了全局面板，页面 DOM 不变属正常。
- Server 页「空白」：实际是诚实能力声明「浏览器预览不支持家庭服务器，请在桌面端使用」，不是空白页。
- Study / Collections 「空白」：实际是诚实空态（0 笔 / 0 合集）。

---

## 待补充

（Phase 5 UI 一致性、Phase 7 跨模块流程检查后补充）