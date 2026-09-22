# SELF-TOOLS V8 · MCP INTEGRATION & REMOTE IDENTITY — FINAL REPORT

- 日期：2026-09-22
- 基线：V7 冻结 HEAD `d147776`（608 passed / 0 failed，工作树干净）
- 已提交 5 个 checkpoint；文档与审查修复在工作树。

## Status

**PASS**（Gates -1–11；独立安全审查 3 项发现全部修复）

---

## V7 Freeze

| 项 | 值 |
| --- | --- |
| V7 commits | `158ff44` / `222071f` / `0ae094b` / `76faed3` / `d147776`（5 个，含 prompt 规则 + 4 份文档） |
| V7 regression（冻结时） | `cargo test --workspace` = **608 / 0** |
| 起始 V8 git status | **clean** |

---

## Protocol

| 项 | 值 |
| --- | --- |
| 目标 MCP 版本 | **2026-07-28**（当前正式版；JSON-RPC 2.0 over STDIO / Streamable HTTP） |
| SDK | **无**（Rust 生态无成熟 2026-07-28 SDK）→ **薄协议 adapter**（Plan §1） |
| Transport | STDIO（newline-delimited JSON）+ Streamable HTTP（`POST /mcp`，stateless） |
| 方法面 | `initialize` / `ping` / `tools/list` / `tools/call`；Resources / Prompts / Sampling / Tasks = P1/P2 未实现 |

---

## Tool Mapping

| 指标 | 值 |
| --- | --- |
| 内部工具 | 由 `ToolRegistry` 派生（Phase-1 组合根为 0；完整装配后为 V4–V7 全量） |
| MCP 暴露工具 | **白名单**（`default_exposure`）；未列出 = 不可达 |
| 隐藏工具 | `files.*` 写操作、`shell.*`、`db.*`、`http.request`、`documents.scan`、`services.start/stop` |
| 风险分布 | Read 组（basic/knowledge/module read）/ SafeWrite（memory.save, history.ensure_enrichment）/ SystemAction（services.restart，远程不可见） |

**Gate 2 证明**：MCP catalog 由 `registry.specs()` 派生，新增/删除工具自动同步
（测试 `catalog_tracks_registry_contents` 锁定），不存在第二张 tool list。

---

## Transport Matrix

| Transport | 状态 |
| --- | --- |
| STDIO | **IMPLEMENTED**（`self-tools mcp --stdio`；stdout 纯度测试；本地 = LocalTrusted，仍过全部门禁） |
| Streamable HTTP loopback | **IMPLEMENTED**（默认 `127.0.0.1:8787`；免 bearer，但以**真实对端 IP** 判定 + 无代理头） |
| Streamable HTTP remote | **DISABLED**（`remote_enabled` 默认 false；启动门禁要求同时配置 identity provider） |

---

## Auth Matrix

| 项 | 状态 |
| --- | --- |
| identity | `RemoteIdentityProvider` trait + `DenyAllIdentityProvider`（拒一切）+ `StaticTokenIdentityProvider`（CI Fake） |
| issuer / audience 校验 | 抽象已留（`McpPrincipal.issuer` / `expires_at`）；OAuth/OIDC provider 未实现 |
| scope | 8 个；`selftools.read` 只覆盖 module read，**不覆盖** `server.action` |
| discovery filtering | ✅ discovery 与 execution 共用 `authorize_tool` |
| execution filtering | ✅ 同一函数；未注册工具 → `unknown_tool`；未暴露 → `not_exposed`；scope 不足 → `insufficient_scope` |
| credential handling | bearer 只从 `Authorization` header；token 不进日志 / 审计 / 错误体（测试断言） |

---

## Risk Matrix

| Risk | MCP 行为 |
| --- | --- |
| READ | 有对应 read scope → 直接执行 |
| SAFE_WRITE | 有 module write scope → 执行（仍过 registry `allowed_risk`） |
| SENSITIVE_WRITE | registry 门禁拒绝；MCP 暴露表不接受该分组 |
| SYSTEM | **不执行**：返回 `confirmation_required` + 票据；只能由 self-tools UI 确认；票据一次性 + TTL + fingerprint + **session 绑定** |

---

## Security Matrix

| 问题 | 答案 |
| --- | --- |
| 未经认证能否发现私人 tools？ | **NO**（discovery 同权；不可信远程 = 0 tools） |
| 未经认证能否调用 tools？ | **NO**（authenticate 前置，fail-closed） |
| 没有 scope 能否调用对应 tool？ | **NO**（`insufficient_scope`；未暴露优先于 scope 检查，不泄露 scope 名） |
| 有 SYSTEM scope 能否绕过确认？ | **NO**（票据是唯一路径；外部 client 无自批准参数） |
| MCP 能否执行 arbitrary shell？ | **NO**（无 shell/exec 工具；`RegisteredAction` 闭合枚举） |
| MCP 能否访问任意文件？ | **NO**（`files.*` 仍走 V6 Allowed Roots + deny + symlink 防护） |
| MCP 能否读取任意 log？ | **NO**（只接受注册表内 log_source；traversal 双侧防御） |
| MCP 能否调用任意 URL？ | **NO**（无任意 URL 工具；health URL 来自 ApplicationRegistry + http/https 白名单） |
| Confirmation 是否可 replay？ | **NO**（执行前标记 Consumed；重放 → Denied） |
| Client A 是否可复用 Client B ticket？ | **NO**（session 绑定；本轮修复了本地固定 client_id 的退化） |
| Bearer token 是否可能进入日志？ | **NO**（只走内存；审计结构无 token 字段；错误不回显） |

---

## PersonalAgent Core Diff

**NO** —— 未为 MCP 重构或添加任何业务逻辑。

- `agent.rs` 零改动；`ToolRegistry` 零改动；
- MCP 直接调 `ToolRegistry::execute`（与 PersonalAgent 同一执行路径）；
- 架构从「MCP → PersonalAgent → Tool」改为「MCP 与 PersonalAgent 平级 → ToolRegistry」。

---

## Tests

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **687 passed / 0 failed** |
| `cargo test --workspace --all-targets` | 通过 |
| `cargo check -p devtoolbox-desktop --tests` | 0 error / 0 warning |
| `npx tsc --noEmit` | 0 error |
| `npm run build`（ui） | built |

新增 79 个测试：core mcp 16 / application mcp 25 / transport 28（STDIO 7 + HTTP 12 +
protocol 8）/ 集成 8 / 其它扩展。含：stdout 纯度逐行断言、启动门禁、超大 body、
非对象 arguments、工具超时、截断语义、唯一 client id、真实对端 IP 回归。

---

## Regression

- V4（Registry / Tool / Action / UiBlock）：Provider 契约测试通过。
- V5（History / Travel / Geography / Language / ChatModelProvider）：全部通过。
- V6（Memory / Documents / Files / Allowed Roots / Secret Protection）：全部通过，
  边界未被 MCP 绕过（MCP 未新增文件/shell 能力）。
- V7（Server / Services / Logs / Apps / SafeAction / Confirmation / Audit / Rate Limit）：
  全部通过；MCP SYSTEM 走同一 `SafeActionService`（§129）。
- History pipeline：`apps/server` 未改动，7 测试通过。

---

## Known Issues

1. 远程身份：仅有抽象 + Fake；真实 OAuth/OIDC 接入前远程写操作关闭（§152 允许）。
2. `apps/mcp` Phase-1 组合根是 fail-closed 空能力集；完整 store 装配待身份层。
3. `StaticTokenIdentityProvider` 的 token 比较非 constant-time（本地/Fake 场景，
   生产 provider 需保证）。
4. Resources / Tasks / Prompts / Sampling / Elicitation 未实现（P1/P2）。
5. HTTP 的 loopback 信任依赖 `into_make_service_with_connect_info` 已被接线；
   若将来前置反向代理，需要显式关闭 `loopback_trusted` 或校验代理头（现有
   `has_proxy_headers` 已作为兜底）。

---

## Git Status

5 个 V8 checkpoint：
```
23803b5 core+app: MCP contracts, registry adapter, identity and exposure policy (Gates 0-2)
f1c4a4b mcp: STDIO and streamable HTTP transports as thin protocol adapters (Gates 3/6)
96b7b51 app+mcp: bind confirmations to their session and route MCP SYSTEM through V7 SafeAction (Gate 7)
61c7eaa core+desktop+ui: MCP settings, audit source tagging, and MCP status UI (Gate 8)
9e6f450 security: fix V8 review findings — real peer IP, unique client ids, HTTP arg shape, tool timeout (Gate 9)
```
MCP_V8.md / V8_OVERNIGHT_STATUS.md / 本报告 / ADR-007 在工作树（保留未提交，与 V6/V7 一致）。

---

## V9 Readiness

| 问题 | 答案 |
| --- | --- |
| MCP 是否已成为稳定外部 capability layer？ | **是**。薄 adapter + 白名单暴露 + 与内部同权的授权链。 |
| ToolRegistry 是否仍是唯一 source of truth？ | **是**（catalog 派生 + 测试锁定；无第二套 schema）。 |
| 是否可以让其他 Agent 通过 MCP 使用 self-tools？ | **可以**（本地 STDIO 立即可用；远程待身份层）。 |
| SafeAction 是否覆盖本地与远程 Client？ | **是**（票据唯一执行路径 + session 绑定 + cooldown + 审计来源标记）。 |
| 下一阶段是否可以进入 Multi-Agent Orchestration？ | **可以**，前提：先补远程身份（OAuth/OIDC provider）与 MCP 完整 store 装配。 |
| PersonalAgent core 是否仍无需重构？ | **是**（V4→V8 四代零业务分支）。 |
