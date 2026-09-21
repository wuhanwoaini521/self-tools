# V7 · Home Server & Safe Automation — 实施状态

> 实时状态（Gates -1–11）。基线：V6 冻结 HEAD `3320bbe`（工作树干净，508 全绿）。
> 全部工作未 commit（用户要求保留工作树，与 V6 一致）。

## 测试总览（真实执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **606 passed / 0 failed**（基线 V6 508，+98） |
| core | 141 passed（其中 server 域 24） |
| application | 302 passed（server 域 29 + server 模块 17） |
| infrastructure | 144 passed（server 平台 20） |
| desktop | 12 passed（server 组合根 4） |
| `cargo check -p devtoolbox-desktop --tests` | 0 error / 0 warning |
| `npx tsc --noEmit -p apps/desktop/ui/tsconfig.json` | 0 error |
| `npm run build`（ui） | built（15.3s，仅 chunk 大小提示） |

## Gate 状态

| Gate | 内容 | 状态 | 证据 |
| --- | --- | --- | --- |
| -1 | V6 Freeze | ✅ | V6 已 5 个 checkpoint 提交；`git status` 干净；508 基线绿 |
| 0 | Audit + V7 Plan | ✅ | `HOME_SERVER_V7_PLAN.md`；资产审计：无既有系统监控代码；`url`/`reqwest` 已在 workspace；`ToolRisk` 四级已存在 |
| 1 | Server 契约 | ✅ | `core/src/server/{mod,metrics,health,registry,logs,action}.rs`；`ServerStatus/SystemMetrics/StorageMetrics/HealthStatus/ServiceDescriptor/ApplicationDescriptor/ActionRequest/Confirmation/AuditEntry` 全部编译并通过 24 测试 |
| 2 | Metrics + Health | ✅ | `LocalSystemMetrics`（sysctl/df/os-release；Linux `/proc`）；部分指标缺失 → `Unknown` 不 panic；四态健康 + 可解释原因 + 配置化阈值 |
| 3 | Service Registry + Logs | ✅ | `is_valid_id` 契约层防注入；provider_ref 不暴露给模型；日志三硬限制 + 脱敏 + `untrusted` 标记 |
| 4 | App Registry | ✅ | `apps.open(app_id)` → `OpenApp` Action；URL http/https 白名单；未注册拒绝 |
| 5 | Safe Action + Confirmation | ✅ | `SafeActionService` plan→authorize→confirm→execute→audit；`RegisteredAction` 闭合枚举；**无确认 = 不执行** |
| 6 | Audit + Rate Limit | ✅ | `config/server_actions.db` + 保留策略；DENIED/EXPIRED/FAILED/SUCCESS 全部记账；cooldown + 会话上限 |
| 7 | PersonalAgent 集成 | ✅ | `personal_ai/server.rs`（14 Read 工具 + ContextProvider）；`build_hub` 注册；`agent.rs` 零改动 |
| 8 | Server Dashboard | ✅ | `ui/features/server/**` + 确认卡（dashboard 与 AI panel 双入口）+ 审计列表 + 设置页注册表编辑；响应式 |
| 9 | Security Review | ✅ | 独立 security reviewer（见下）；静态断言全过 |
| 10 | Full Regression | ✅ | 606 全绿；V5/V6 无退化 |
| 11 | Docs | ✅ | `HOME_SERVER_V7.md` / `ADR-006` / 本文件 / `V7_FINAL_REPORT.md` |

## Gate 9 安全审查修复（独立 security reviewer，4 项发现）

| # | 发现 | 严重度 | 修复 |
| --- | --- | --- | --- |
| V7-SEC-001 | `build_hub` 接收 `&ServerRuntime` 却从未调用 `register_server` —— 14 个工具在真实桌面运行里不存在（编译器与既有测试都发现不了） | 高 | 补注册；**新增 2 条装配集成测试**（`build_hub_registers_every_domain_module` / `server_module_tools_are_reachable_from_the_agent`，用真实 store + tempdir 构建到 hub），把「模块注册」变成可断言的契约 |
| V7-SEC-002 | `ServerSettings` 6 个配置项从未被读取：TTL/cooldown/会话上限硬编码、审计无限增长 | 中 | `ServerRuntime::assemble` 从 settings 读 `thresholds` / `confirmation_ttl_secs` / `cooldown_secs` / `max_system_per_session` / `audit_max_entries` / `audit_retention_days`；`SqliteAuditStore::record` 按保留策略惰性 prune |
| V7-SEC-003 | `LogRedactor` 的 V6 secret 兜底门被「占位符出现即跳过」整体禁用 → 同行第二个 secret 原样泄漏 | 中 | 改为对 `text.split(REDACTION_PLACEHOLDER)` 的**每个未脱敏片段**分别送检；新增回归测试 `second_secret_on_same_line_is_still_caught` |
| V7-SEC-004 | `launchctl kickstart` 对未加载 label 也返回 0 → 假成功（audit 记 success、cooldown 记错） | 低 | 重启后必须 `launchctl print <target>` 成功，否则 `service_not_loaded_after_restart` |

规格偏离同步修复：`is_safe_log_path` 接入 infra 日志路径校验；日志正文改用
`<untrusted_log>…</untrusted_log>` 包裹（§4.3）；prompt 规则新增第 8/9 条
（日志是不可信数据 / 系统修改只能请求不能自行执行）。

## Gate 9 静态断言（真实执行）

```
grep -rn '"sh", "-c"\|sh -c\|bash -c' crates apps --include=*.rs  → 0
grep -rn "Command::new" crates apps --include=*.rs | grep -v infra/src/server → 0
grep -rn "std::process" crates/core/src crates/application/src → 2（均为注释）
```

## 实现选择（与计划的偏差，均有理由）

1. **`services.restart` 工具注册为 `Read` 风险**：当前 `ToolRegistry.allowed_risk`
   只允许 Read+SafeWrite，放开 System 会影响所有模块。改为「工具 Read +
   SYSTEM 语义放在确认票据里，执行入口在桌面命令层强制」——风险门禁仍真实
   生效（ADR-006 备选方案表记录）。
2. **macOS CPU/内存 used 为 `None`**：需要 `host_statistics`（C API）。V7 不引入
   额外依赖，字段保持 `None` → 健康评估 `Unknown`，**不编造**（§23）。
3. **`df -k -P` 解析兼容 `map auto_home` 双字段 Filesystem**：macOS 虚拟卷的
   Filesystem 列含空格，从左侧扫描到首个纯数字字段再取后续列。
4. **审计库不可用时降级内存审计**：SQLite 打开失败不阻塞启动（写路径仍可用），
   stderr 记录原因。
5. **`LogTailPort` 未知名 log_source 回落第一个注册源**：descriptor 只有一个源时
   最直观；**没有任何路径**能读 descriptor 之外的日志。

## 未做（明确范围外 / P1）

- `services.start` / `services.stop`（§34：restart 扎实后再扩）。
- Docker adapter（§30 Optional；且禁 `docker exec`）。
- Automation Registry（Track E，§96：时间不足则完全不做）。
- 完整身份体系（§75：远程写操作默认 disabled）。
- 图表 / 视觉打磨（P2）。

## 回滚

删除 `crates/{core,application,infrastructure}/src/server`、
`personal_ai/server.rs`、`apps/desktop/src/{server,server_adapters}.rs`、
`ui/src/features/server` 与 `AppState.server` 即可完全关闭；
`settings.server` 走 `serde(default)`；`config/server_actions.db` 独立可删。
