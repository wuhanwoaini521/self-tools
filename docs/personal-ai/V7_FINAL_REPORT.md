# SELF-TOOLS V7 · HOME SERVER & SAFE AUTOMATION — IMPLEMENTATION REPORT

- 日期：2026-09-21
- 基线：V6 冻结 HEAD `3320bbe`（工作树干净，`cargo test --workspace` = 508/508）
- 全部改动未 commit（保留工作树，与 V6 一致）

## Status

**PASS**（Gates -1–10 全部 PASS；Gate 9 安全审查结论见 §Security Matrix 与
`V7_OVERNIGHT_STATUS.md` 的审查记录）

---

## Architecture

```text
BEFORE: AI 对家庭服务器一无所知；没有任何受控的系统操作入口
AFTER :                       PersonalAgent（零业务分支）
                                     │
                        Module / Tool Registry
                                     │
        ┌────────────────────────────┼────────────────────────────┐
    Knowledge                     Modules                       Server
        │                            │                             │
  Memory/Documents          History/Travel/Geo/      ┌─────────────┼─────────────┐
  Files                     Language                  │             │             │
                                                SystemMetrics   Services       Apps
                                                       │             │             │
                                                       └──────┬──────┴─────────────┘
                                                              │
                                                    Safe Action Service
                                                   (Plan → Authorize →
                                                    Confirm → Execute → Audit)
```

---

## V6 Freeze

| 项 | 值 |
| --- | --- |
| V6 commits | `84ac5a7` core · `e44a69a` infra · `8f221a5` app · `25c39ef` desktop+ui · `3320bbe` docs（5 个 checkpoint） |
| V6 regression（冻结时） | `cargo test --workspace` = **508 passed / 0 failed** |
| 起始 V7 git status | **clean**（`git status --short` = 0 行） |
| V7 commits（本次） | `158ff44` contracts · `222071f` app services · `0ae094b` infra+module · `76faed3` desktop+ui |

---

## Server Capability Matrix

| 能力 | 状态 | 证据 |
| --- | --- | --- |
| System metrics（hostname/OS/arch/uptime/CPU/cores/mem/load） | **IMPLEMENTED** | `infrastructure/src/server/metrics.rs`；macOS `sysctl`/`sw_vers`，Linux `/proc`；不可用字段 `None` → `Unknown` |
| Memory metrics | **PARTIAL** | macOS 只报 `total`（`used`/`available` 需 `host_statistics` C API，V7 不引依赖）；Linux 完整 |
| Storage metrics | **IMPLEMENTED** | `df -k -P` 解析（含 `map auto_home` 双字段 Filesystem）；`VolumeKind` 过滤临时/虚拟卷 |
| Health（四态 + 可解释原因 + 配置化阈值） | **IMPLEMENTED** | `core/src/server/health.rs`；`evaluate_storage`/`evaluate_memory` 纯函数 |
| Services（registry + 状态 + 重启） | **IMPLEMENTED** | launchd adapter（固定 argv）；`allowed_actions` 白名单；未注册拒绝 |
| Logs（有界 + 脱敏 + 不可信标记） | **IMPLEMENTED** | 2_000 行 / 1 MiB / 86_400s 上限；`LogRedactor` 三层脱敏；`untrusted: true` |
| Applications（registry + health + open） | **IMPLEMENTED** | URL http/https 白名单；`apps.open(app_id)` → `OpenApp` |
| Automation（Track E） | **NOT IMPLEMENTED**（P1，§96 允许） | — |
| Docker adapter | **NOT IMPLEMENTED**（P1，§30 Optional） | — |

---

## Safe Action Matrix

| action | risk | confirmation | executor | audit | rate-limit |
| --- | --- | --- | --- | --- | --- |
| `server.get_status` / `get_cpu` / `get_memory` / `get_storage` / `get_health` | READ | 不需要 | n/a（读） | n/a | n/a |
| `services.list` / `get` / `get_status` / `get_logs` | READ | 不需要 | n/a（读） | n/a | n/a |
| `apps.list` / `get` / `get_status` / `open` | READ | 不需要 | n/a（`open` 只产 Action） | n/a | n/a |
| `services.restart` | **SYSTEM** | **必需**（票据：一次性 + TTL + fingerprint） | `LaunchdServiceControl`（固定 argv） | ✅（SUCCESS/FAILED/DENIED/EXPIRED） | ✅（cooldown 60s + 会话 5 次） |

---

## Platform Matrix

| 平台 | 状态 |
| --- | --- |
| macOS 12（目标） | `sysctl -n hw.memsize` / `kern.boottime` / `vm.loadavg`、`sw_vers -productVersion`、`df -k -P`、`launchctl print/kickstart -k`（全部固定参数模板） |
| Linux（CI/开发） | `/proc/meminfo`、`/proc/uptime`、`/proc/loadavg`、`/etc/os-release`、`df -k -P` |
| Windows（开发机） | 指标 `Unknown`（fail-closed，不编造）；dashboard 正常渲染 |

---

## Security Matrix

| 问题 | 答案 |
| --- | --- |
| AI 是否可以执行任意 shell？ | **NO**（无 shell/exec/run 工具；`RegisteredAction` 闭合枚举；`grep '"sh","-c"'` = 0） |
| AI 是否可以传 arbitrary args？ | **NO**（argv 全字面量；label 只来自注册表 `provider_ref`） |
| AI 是否可以操作未注册 service？ | **NO**（registry + plan 双层；adapter 对未映射 id 也拒） |
| AI 是否可以读取任意 log path？ | **NO**（只接受 descriptor 内 log_source；traversal 双侧防御） |
| AI 是否可以打开任意 URL？ | **NO**（`apps.open` 只接 app_id；http/https 白名单） |
| SYSTEM action 是否可以绕过确认？ | **NO**（`confirm_and_execute` 唯一写入口；票据四步校验） |
| Confirmation 是否可重放？ | **NO**（执行前标记 `Consumed`；重放 → Denied） |
| 确认后参数是否可改变？ | **NO**（fingerprint 覆盖 type/target/参数/risk） |
| Remote untrusted client 是否可执行 write？ | **NO**（`untrusted_session`；HTTP server 无写端点） |
| 日志中的 prompt injection 是否可能触发工具？ | **NO**（`untrusted` 标记 + prompt 规则 8/9；测试断言零副作用） |

---

## PersonalAgent Core Diff

**NO**——没有为 Server 加入任何业务 if/else。

- `agent.rs` 零改动（V6 的通用 `hub.retrieval` stage 保持不变）；
- `ToolRegistry` / `ActionProtocol` 零改动（只复用 `Action` + 两种新 `ActionKind`）；
- server 模块按 `ModuleDescriptor + 14 tools + ContextProvider + register_server`
  标准接入（与 geography/language/knowledge 同构）。

---

## Tests

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **608 passed / 0 failed** |
| `cargo test -p devtoolbox-core` | 141 passed |
| `cargo test -p devtoolbox-application` | 302 passed |
| `cargo test -p devtoolbox-infrastructure` | 144 passed |
| `cargo test -p devtoolbox-desktop --lib` | 13 passed（含 2 条装配集成测试） |
| `cargo check -p devtoolbox-desktop --tests` | 0 error / 0 warning |
| `npx tsc --noEmit -p apps/desktop/ui/tsconfig.json` | 0 error |
| `npm run build`（ui） | built |

Gate 9 审查 4 项发现全部修复：**build_hub 漏注册 server 模块（高危）**——
曾导致 14 个工具在真实桌面运行里不存在；现已补注册并新增 2 条装配集成测试
（真实 store + tempdir 构建 hub，断言模块与工具可达）。另修
`ServerSettings` 6 项配置未接线（TTL/cooldown/上限硬编码、审计无限增长）、
`LogRedactor` 兜底门被占位符整体禁用导致同行第二个 secret 泄漏、
`launchctl kickstart` 假成功。

Safe Action 十例（§100）全部有测试：READ 无确认 / SYSTEM 需确认 / 无确认拒绝 /
过期拒绝 / 重放拒绝 / target 不一致拒绝 / 未知服务拒绝 / 成功审计 / 失败审计 /
频率限制。

---

## Regression

- V6（Memory / Documents / Files / Knowledge Retrieval / File Security /
  Secret Protection）：508 个既有测试全部通过，无退化。
- V5（History / Travel / Geography / Language / ChatModelProvider）：全部通过。
- History pipeline：`apps/server` 未改动，其测试通过。

---

## Known Issues

1. macOS `CpuMetrics.usage_ratio` 与 `MemoryMetrics.used/available` 为 `None`
   （需 `host_statistics` C API；健康评估按 `Unknown` 降级，不编造）。
2. `services.start` / `services.stop` 未实现（P1）。
3. Automation Registry 未实现（P1，§96 允许）。
4. 无完整身份体系：远程写操作默认 disabled（`RemoteUntrusted`）。
5. `df -k -P` 的 Capacity 列未参与解析（只用 1K-blocks/Used/Available），
   极端畸形的 df 输出会被跳过而不是误判。

---

## Git Status

4 个 V7 checkpoint 已提交（`158ff44` / `222071f` / `0ae094b` / `76faed3`）；
prompt 规则补充与 3 份文档在工作树（与 V6 一致：保留未提交）。

---

## V8 Readiness

| 问题 | 答案 |
| --- | --- |
| 现有 Tool Registry 是否可以暴露为 MCP？ | **是**。`ToolRegistry` 已是 typed capability 注册表（name + JSON Schema + risk + module），与 MCP tool 定义同构；`specs()` 可直接映射。 |
| Server tools 是否已经具备清晰 capability boundary？ | **是**。14 个工具全部 typed；无命令字符串；provider_ref 不泄漏到模型面。 |
| Safe Action 是否可以作为 MCP 写操作保护层？ | **是**。`SafeActionService` 与传输无关（plan/confirm 是纯 application 语义 + 端口执行），MCP 侧只需提供「确认回调」把票据交给用户。 |
| 是否还需要重构 PersonalAgent？ | **不需要**。V4–V7 四代演进中 `agent.rs` 始终零业务分支；新增能力全部走模块机制。 |
| 是否可以进入 MCP Integration 阶段？ | **可以**（前提：先补远程身份体系，否则 MCP 写操作只能对 `LocalDesktop` 开放）。 |
