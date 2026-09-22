# V11 安全模型与最终安全评审（SECURITY_MODEL）

覆盖 V4–V11 最终系统，不是单看 V11 diff。结论：**0 Critical / 0 High**；
Medium 已修（见 §3）；真实外部依赖限制项见 §4。

## 1. 边界总览

```text
Web/PWA（HTTPS） → Device Session（authenticated）
   → PersonalAgent（唯一用户 AI 入口）
        → DecisionEngine（只选策略；不授权）
        → ToolRegistry（唯一 capability source；risk gate: Read+SafeWrite）
             → Modules（History/Travel/Geography/Language/Memory/Documents/
                        Files/Knowledge/Study Board/Server）
                  → SafeAction（唯一 SYSTEM 写路径：票据 → 确认 → 执行 → 审计）
   MCP（adapter；本地 STDIO / 可选远程 + 身份）
```

## 2. 逐项评审

### 2.1 Secret 处理
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| LLM/Jev/OIDC/MCP key 只进 gitignored `settings.json` | PASS | `SettingsStore` 路径在 config；`.gitignore` |
| 前端响应不含 key | PASS | `AiStatus` 只有 configured/model；`jev_configured()` 只暴露布尔 |
| 日志不含 key/token/正文 | PASS | `redact_log_value` 双保险 + `decision_security_tests` 的 no-secret 断言 |
| plist 不含 key | PASS | `render_launchd_plist` 测试断言无 API_KEY/Bearer；env 文件外置 |

### 2.2 文件系统与 symlink
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| 任意文件访问不可能 | PASS | `FileAccessPolicy` 只允许显式 `file_roots` |
| symlink 逃逸 | PASS | `LocalFileSystem` 规范路径后校验根（V6 测试） |
| 读取有界 | PASS | `max_read_chars`；超限受控错误 |

### 2.3 Documents / Memory 写入
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| Memory 写入需确认 | PASS | `memory.save` 走 `ConfirmMemory` Action；用户确认才落库 |
| worker 不可写 Memory | PASS | profiles `denied_tools` 含 `memory.save`；`decision_security_tests` |
| Conversation ≠ Memory | PASS | doc + 测试；记忆服务不读会话存储 |
| Study Board 不自动进 Memory | PASS | `study_board` 工具路径无 memory 写；测试断言 |

### 2.4 MCP / 远程暴露
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| 远程默认 OFF | PASS | `McpSettings::default()` `remote_enabled=false` |
| 远程无身份 → 拒绝启动 | PASS | `validate_startup` `remote_mcp_without_auth` |
| 非 loopback HTTP 绑定需身份 | PASS | `validate_startup` `mcp_bind_unsafe` |
| MCP SYSTEM 走 SafeAction | PASS | V8 Gate 7（`with_system_actions` + 确认票据） |
| 错误不回显 token | PASS | `mcp::auth::tests::error_text_never_echoes_token`（V8 已有） |

### 2.5 Device Session / Web
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| LAN ≠ trusted | PASS | 远程设备同样要 authenticated + authorized + confirmation + audit |
| 无 query token / 硬编码密码 | PASS | 会话 id 经 `is_valid_task_id` 校验；无永久明文 token |
| CORS 不允许 `*` | PASS | HTTP 面未设通配 CORS（私有 API fail-closed） |
| CSRF | PASS | 写操作走 SameSite cookie + 确认票据（不与 GET 混用） |

### 2.6 SafeAction / Confirmation / Audit
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| SYSTEM 动作唯一写路径 | PASS | `services.restart` 工具不执行，只签发票据（V7 §56） |
| 票据不可变 + 绑定 session | PASS | V8 Gate 7（确认票据绑定 session） |
| 票据重放 | PASS | 一次性消费 + TTL（30..120s）+ 冷却 |
| 审计完整 | PASS | `server_actions.db` 记录每次确认与结果 |

### 2.7 Agent 能力提升
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| capability escalation | PASS | `DelegatedCapabilitySet::intersect` + `is_subset_of` 双重保险 |
| depth = 1 | PASS | executor 无 delegate 入口；profiles `can_delegate=false` |
| Decision 不能提权 | PASS | `decision_security_tests`（7 条）+ `failure_injection_tests` 的 hostile 用例 |
| Decision 不能 bypass SafeAction | PASS | 决策层只返策略；SYSTEM 语义只在票据 |

### 2.8 隐私 / 日志 / 备份 / PWA
| 检查 | 结论 | 证据 |
| --- | --- | --- |
| 日志隐私 | PASS | §2.1 |
| 备份文件权限 | PASS | `chmod 700 home / 600 settings`（DEPLOY_MACOS12） |
| SQLite 不用裸 copy | PASS | `VACUUM INTO` / Backup API；drill 测试 |
| 恢复不覆盖真实数据 | PASS | restore 目标隔离 + 路径封闭 |
| PWA 缓存不存敏感数据 | PASS | `sw.js` 对 `api/memory/document/file/conversation/study/safe-action/mcp/auth/token/session` 走 network-only |
| Service Worker 不缓存 token | PASS | 同上（network-only + 不写任何 cache） |
| 多模态上传隐私 | PASS | 图片默认不进 Memory；保存需显式用户动作（§106） |
| Study Board 隐私 | PASS | 笔迹是私有数据；`strokes_summary` 有界且不含坐标 |

## 3. 已修复的 Medium 项（V11 评审中发现）

| 编号 | 问题 | 修复 |
| --- | --- | --- |
| SEC-001 | `LanguagePage` useEffect 嵌套（运行时 React #321，整个应用不可用） | 拆成两个平级 effect |
| SEC-002 | `html { min-width:1120px }` 使布局系统永远选不到 mobile（移动端可用性 + 潜在的 UI 欺骗风险） | 桌面下限收窄到 `@media (min-width:1180px)` |
| SEC-003 | `resolve.dedupe` 缺失 → peer 依赖可能引入第二份 React | vite.config 显式 dedupe |
| SEC-004 | 决策遥测缺少「回落」显式标记（排障时无法区分 rule 原生 vs fallback） | `DecisionTelemetry.fallback` + UI 展示 |
| SEC-005 | Study Board 缺后端持久化（只存浏览器内存 → 换设备丢失 + 隐私不可控） | `StudyBoardSqliteStore` + 隐私铁律测试 |

## 4. 已知外部限制（如实记录）

| 项 | 状态 | 当前行为 |
| --- | --- | --- |
| Real Jev API | `BLOCKED_EXTERNAL`（无 key） | 纯 Rule 模式；key 配置后可 `jev_shadow` → `jev_active` |
| 真实 LLM key | 部署方提供 | 未配置 → 受控 `ModelUnavailable`，非 AI 功能不受影响 |
| 真实 OIDC provider | 部署方提供 | 未配置 → 远程 MCP 拒绝启动（fail-closed） |
| 真实 HTTPS 证书 | 部署方提供 | 未配置 → Readiness 页 `pwa_secure_context` 显示 degraded |
| 真实 iPad/iPhone | 需真机 | 视口/浏览器模拟 PASS；真机手动验收 pending |

## 5. 结论

**0 Critical / 0 High / Medium 全部修复。** 真实外部依赖（Jev key / LLM key /
OIDC / 证书 / 真机）属部署方输入，本地 architecture / fake / fallback /
fail-closed / tests / docs 全部完整。
