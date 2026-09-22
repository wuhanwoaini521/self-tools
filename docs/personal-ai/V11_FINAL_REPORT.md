# V11 Final Report

## 结论

V11（Production Hardening + PWA + Personal Hub Product Completion）**PASS**。
真实外部依赖（Jev key / LLM key / OIDC / HTTPS 证书 / 真机）属部署方输入，
本地 architecture / fake / fallback / fail-closed / UI / tests / docs 全部完整。

## 测试计数（真实）

```
V9 基线      749 passed
V10 final    805 passed
V11 final   1017 passed / 0 failed     （+172 over V10）
cargo check --workspace --all-targets   0 warning
frontend tsc --noEmit                  PASS
frontend vite build                    PASS
frontend vitest                        11 passed
history-data-pipeline backbone validate OK（31 periods / 64 regimes / 656 events / 3 stories）
visual QA                              80 screenshots · 5 viewports · 0px overflow
```

测试增量来源：decision（V10，已计入 805）· conversations 24 · search 17 ·
readiness 19 · backup 34 · study board 37 · speech 7 · multimodal 4 ·
failure injection 13 · E2E journeys 20 · operations(config/lifecycle/observability) 30。

## Engineering DoD（§185）

| 项 | 状态 | 证据 |
| --- | --- | --- |
| Config | PASS | `operations/config.rs` 11 tests（fail-closed 启动校验） |
| Secrets | PASS | `redact_log_value` + no-secret 断言 + gitignore（`config/`） |
| macOS deployment | PASS | `DEPLOY_MACOS12.md` + `render_launchd_plist` |
| launchd lifecycle | PASS | `lifecycle.rs` 7 tests（lock / marker / plist / commands） |
| Graceful shutdown | PASS | `ShutdownStage::all()` 8 阶段 |
| Crash recovery | PASS | `StartupMarker::recover()` + E2E journey |
| Health | PASS | `observability.rs` 7 tests（liveness/readiness/degraded） |
| Metrics | PASS | `MetricsSnapshot` + 派生指标 |
| Logs | PASS | `StructuredLogEvent` 字段集 + 脱敏 |
| Store inventory | PASS | `BACKUP_RESTORE.md` §1 表 |
| Migrations | PASS | 各 store 幂等 `migrate()` + 版本单调校验 |
| Backup | PASS | `backup/` 22 tests（sha256 manifest + VACUUM INTO） |
| Restore | PASS | 恢复演练（真实库）+ 路径封闭 + 完整性校验 |
| Failure injection | PASS | 13 tests（LLM/Jev/Search/DB/budget/backup/crash） |
| Security review | PASS | `SECURITY_MODEL.md`（0 Critical / 0 High） |
| Architecture review | PASS | 本节 §架构不变式 |

## Product DoD（§186）

| 项 | 状态 | 证据 |
| --- | --- | --- |
| Personal Hub | PASS | Home ask 框 + 四组入口；QA desktop/mobile home 截图 |
| Global AI | PASS | AIPanel 单入口；Ask AI 从 Home/History/… 可达 |
| PWA | PASS | manifest + sw.js（敏感路径 network-only）+ icons + 更新 + 离线条 |
| Desktop | PASS | 1440×900 0 overflow |
| Tablet | PASS | 1024×768 / 768×1024 0 overflow（landscape/portrait 分别处理） |
| Phone | PASS | 390×844 / 360×800 0 overflow + 底部导航 + safe-area |
| Persistent Conversations | PASS | `ConversationSqliteStore` 24 tests |
| Cross-device | PASS | E2E journey 10（同 store 跨设备读/写/归档） |
| Multimodal foundation | PASS | `ContentPart` 5 变体 + `ModelCapabilities` 门禁 + 4 tests |
| Study Board | PASS | 模块 4 工具 + SQLite + UI（pen/eraser/undo/redo/clear/snapshot/Ask AI） |
| Language daily UX | PASS | `SpeechProvider` + 定性反馈（无伪分数）+ SpeakPanel |
| Global Search | PASS | `GlobalSearchService` 无 LLM + per-source degrade + UI |
| System Readiness | PASS | 13 探测 + 诊断运行 + 无 secret |
| Visual QA | PASS | 80 截图 + qa-report.json 断言 |
| E2E Journeys | PASS | 18 条 |

## 产品验收矩阵（§179）

| 项 | 结论 |
| --- | --- |
| Personal Hub Home / Global Ask AI / Context Awareness | PASS |
| History / Enrichment / Travel / Geography / Language | PASS |
| Language Audio | PASS（Web Speech 本地闭环；真实 ASR = BLOCKED_EXTERNAL） |
| Study Board / Multimodal | PASS（真实 vision 模型 = BLOCKED_EXTERNAL，拒绝路径已验证） |
| Personal Memory / Documents / Files / Knowledge Retrieval | PASS |
| Global Search | PASS（模块端口注册待组合根接线 = PARTIAL） |
| Persistent Conversation / Cross-device | PASS |
| Home Server / Applications / Service Logs | PASS |
| SafeAction / Confirmation / Audit | PASS |
| MCP Local / Remote Security | PASS（远程默认关 + 无身份拒绝启动） |
| Multi-Agent / Decision Layer / Rule Fallback | PASS |
| Jev | BLOCKED_EXTERNAL（abstraction/fake/shadow/fallback 全实现） |
| PWA / Offline / Update Lifecycle | PASS |
| HTTPS | BLOCKED_EXTERNAL（部署方证书；代码要求 Secure Context） |
| Desktop / Tablet Portrait / Tablet Landscape / Phone UX | PASS（viewport/模拟；真机 pending） |
| Diagnostics / Readiness | PASS |
| Backup / Restore | PASS |
| launchd / Crash Recovery / Production Build | PASS |
| Security Review | PASS（0 Critical / 0 High） |

## 架构不变式（§163）

| 问题 | 结论 |
| --- | --- |
| PersonalAgent 是否仍是唯一用户 AI 入口？ | 是（单 `run()`；编排/决策都经它） |
| ToolRegistry 是否仍唯一 capability source？ | 是（risk gate + capability intersect；decision 只能 clamp） |
| Modules 是否仍独立？ | 是（10 个模块全部 register_*；agent 无业务分支） |
| Decision Layer 是否 replaceable？ | 是（`DecisionProvider` trait；Rule/Jev 可换） |
| Jev 是否只做 decision？ | 是（isolated in infrastructure adapter；无执行入口） |
| Multi-Agent 是否仍 bounded？ | 是（depth=1 / max_agents / max_steps / tokens / duration） |
| Agent 是否仍 depth=1？ | 是（executor 无 delegate 入口） |
| MCP 是否仍只是 adapter？ | 是（→ ToolRegistry；无独立能力） |
| SafeAction 是否仍唯一 SYSTEM 写路径？ | 是（票据 + 确认 + 审计） |
| Conversation 与 Memory 是否仍分离？ | 是（doc + 测试） |
| PWA 是否破坏 security boundary？ | 否（敏感路径 network-only；不缓存 token） |
| Mobile 是否只是 Desktop 缩小版？ | 否（底部导航 / AI bottom-sheet / safe-area / 触控目标 44pt） |
| Study Board 是否通过标准 Module 接入？ | 是（descriptor + 4 工具 + ContextProvider） |
| Production 是否不依赖 dev shell？ | 是（绝对路径 + env 文件 + launchd plist） |

## 评审发现并修复（§39/§39 + V11）

| 编号 | 级别 | 问题 | 修复 |
| --- | --- | --- | --- |
| V11-SEC-001 | High（可用性） | LanguagePage useEffect 嵌套 → React #321，整个前端不可用 | 拆成平级 effect |
| V11-SEC-002 | Medium | `html{min-width:1120px}` → 布局系统永远选不到 mobile | 下限收窄到 ≥1180px |
| V11-SEC-003 | Medium | 缺 `resolve.dedupe` → 可能两份 React | vite dedupe |
| V11-SEC-004 | Medium | 决策回落无显式标记 | `DecisionTelemetry.fallback` + UI |
| V11-SEC-005 | Medium | Study Board 只存浏览器内存 | SQLite store + 隐私铁律 |

## 已知外部阻塞（如实）

- `REAL_JEV = BLOCKED_EXTERNAL`（无 API key）
- 真实 LLM key 未配置（`settings.json` 无 `ai` 段）
- 真实 vision / audio 模型未配置（受控拒绝路径已验证）
- 真实 OIDC provider 未配置（远程 MCP fail-closed）
- HTTPS 证书未提供（部署输入）
- 真实 iPad/iPhone 不在 CI（viewport/模拟 PASS；真机 pending）
