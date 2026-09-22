# Product Intent Coverage（V11-A）

逐项审计真实代码（`grep` / 测试 / 运行验证）。状态只取：
`IMPLEMENTED` / `PARTIAL` / `NOT_IMPLEMENTED` / `BLOCKED_EXTERNAL`。

| # | 意图 | 状态 | 证据（文件 / 测试） |
| --- | --- | --- | --- |
| 1 | Personal Hub Home | **IMPLEMENTED** | `HomePage.tsx` `home-hub`（Ask AI + Continue/Learn/Knowledge/Home）；截图 `docs/qa/v11/*/home.png` |
| 2 | Global Ask AI | **IMPLEMENTED** | `AIPanel` 全局面板；App 顶栏 `app-bar-ai`；Home ask 框；`agent.run` 单入口 |
| 3 | History | **IMPLEMENTED** | `personal_ai/history.rs` + `HistoryPage.tsx` |
| 4 | History On-demand Enrichment | **IMPLEMENTED** | `history/enrichment` + `EnrichmentPanel.tsx`（V5） |
| 5 | Travel | **IMPLEMENTED** | `personal_ai/travel.rs` + `TravelPage.tsx` |
| 6 | Geography | **IMPLEMENTED** | `personal_ai/geography.rs` + `GeographyPage.tsx` |
| 7 | Language | **IMPLEMENTED** | `personal_ai/language.rs` + `LanguagePage.tsx`（含 6 个 panel） |
| 8 | Personal Memory | **IMPLEMENTED** | `memory/` + `MemoryPanel.tsx`；写入需确认（ConfirmMemory） |
| 9 | Documents | **IMPLEMENTED** | `documents/` + `DocumentsPanel.tsx` |
| 10 | Files | **IMPLEMENTED** | `files/` + `FilesPanel.tsx`；根外访问拒绝 |
| 11 | Knowledge Retrieval | **IMPLEMENTED** | `knowledge/retrievers.rs` + `RetrievalAugmenter` 注入 |
| 12 | Home Server | **IMPLEMENTED** | `server/service.rs` + `ServerPage.tsx` |
| 13 | Safe Actions | **IMPLEMENTED** | `server/action.rs`（票据 → 确认 → 执行 → 审计） |
| 14 | MCP | **IMPLEMENTED** | `mcp/` + stdio/http transports + adapter → ToolRegistry |
| 15 | Multi-Agent | **IMPLEMENTED** | `agents/orchestrator.rs`（depth=1、bounded、capability intersect） |
| 16 | Decision Layer | **IMPLEMENTED** | `agents/decision*.rs`（engine/rule/jev/eval/telemetry） |
| 17 | PWA | **IMPLEMENTED** | `manifest.webmanifest` / `sw.js` / icons / update lifecycle / offline banner |
| 18 | Desktop Layout | **IMPLEMENTED** | `layout.ts` device=desktop；QA 1440×900 0 overflow |
| 19 | Tablet Layout | **IMPLEMENTED** | 168px 导航 + landscape/portrait 分别处理；QA 1024×768 / 768×1024 0 overflow |
| 20 | Mobile Layout | **IMPLEMENTED** | 底部导航 + 单列 + AI bottom-sheet/fullscreen + safe-area；QA 390×844 / 360×800 0 overflow |
| 21 | Persistent Conversations | **IMPLEMENTED** | `personal_ai/conversation.rs` + `conversation_store.rs`（24 tests） |
| 22 | Cross-device Continuity | **PARTIAL** | 后端 ConversationStore 持久化 + 服务端共享（同一 Home Server 可跨设备读）；**UI 的会话列表面板未接**（New/Recent/Resume/Rename/Archive 后端齐，前端入口待接） |
| 23 | Multimodal Input | **IMPLEMENTED** | `ContentPart`（Text/Image/Audio/DocumentRef/BoardSnapshot）+ `ModelCapabilities` 门禁；真实 vision 模型 = **BLOCKED_EXTERNAL（部署输入）** |
| 24 | Study Board | **IMPLEMENTED** | `study-board` 模块（4 工具）+ `StudyBoardSqliteStore` + `StudyBoardPage.tsx`（pen/eraser/undo/redo/clear/snapshot/Ask AI） |
| 25 | Language Audio / Speaking | **IMPLEMENTED** | `language/speech.rs`（SpeechProvider + 定性反馈，无伪分数）+ `tts.ts` Web Speech + `SpeakPanel.tsx`；真实 ASR Provider = **BLOCKED_EXTERNAL（未接厂商）**，本地闭环用 Web Speech |
| 26 | Global Search | **IMPLEMENTED** | `search/`（9 源端口 + degrade）+ `GlobalSearchPage.tsx`；各模块端口注册 = **PARTIAL**（服务可用，源待组合根注册） |
| 27 | First-run Setup | **PARTIAL** | Readiness 页给出 13 项状态与「未配置」指引；无向导式首次配置流程 |
| 28 | Diagnostics | **IMPLEMENTED** | `ReadinessService::diagnostics()` + `SystemReadinessPage` 「运行诊断」 |
| 29 | Backup | **IMPLEMENTED** | `backup/` + manifest + sha256 + drill |
| 30 | Restore | **IMPLEMENTED** | `BackupService::restore`（隔离目标 + 校验 + 路径封闭） |
| 31 | Production Deployment | **IMPLEMENTED** | `PRODUCTION_V11.md` + `DEPLOY_MACOS12.md` + launchd + 启动校验；真实证书/域名 = **BLOCKED_EXTERNAL（部署输入）** |

## 关键缺口（诚实记录）

| 缺口 | 影响 | 计划 |
| --- | --- | --- |
| 会话 UI 面板（New/Recent/Resume/Rename/Archive） | 跨设备恢复需手动带 session_id | 后端齐；前端面板在 `AIPanel` 加会话抽屉 |
| GlobalSearch 各模块端口注册 | 搜索当前只返回空 + degraded（如实） | 组合根按模块注册 `GlobalSearchPort` |
| Readiness 探测器的真实实现（DB/ToolRegistry/备份目标） | 现在由前端本地推导兜底 | 组合根注入真实 probe |
| 真实 vision / ASR Provider | 多模态只能用文字 | 部署方选模型；抽象层已就绪 |
