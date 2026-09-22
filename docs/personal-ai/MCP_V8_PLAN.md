# SELF-TOOLS V8 · MCP INTEGRATION & REMOTE IDENTITY — PLAN

> Gate 0 审计结论 + 五 Track 实施计划。基线：V7 冻结 HEAD `d147776`
> （工作树干净，`cargo test --workspace` = **608 passed / 0 failed**）。

---

## 1. Gate 0 审计（逐项核实）

### 1.1 现有资产（复用 vs 新建）

| 面 | 位置 | 事实 | V8 决策 |
| --- | --- | --- | --- |
| MCP | 全仓 grep `mcp`（大小写不敏感） | **零命中**（仅日志脱敏正则里的 `authorization`/`bearer` 字样） | 全新域；薄协议 adapter |
| OAuth / OIDC / session auth / API token | 全仓 grep | **零命中** | 只建 `RemoteIdentityProvider` **抽象** + Fake 实现；不造用户名密码系统（§29） |
| `ToolRegistry` | `application/src/personal_ai/registry.rs` | `specs()` / `spec(name)` / `async execute(&ToolCallRequest)`；注册期 `allowed_risk` 门禁（Read + SafeWrite） | **MCP 的 source of truth**；不复制 schema |
| `ToolSpec` | `core/src/personal_ai/types.rs` | `{name, description, input_schema, risk, module}` | MCP tool definition 直接派生 |
| `ToolExecutor` | registry.rs:27 | `fn spec() + async execute(arguments)` | MCP `tools/call` 走同一条执行路径 |
| SafeAction | `application/src/server/action.rs` | plan / confirm_and_execute / audit / cooldown；票据一次性 + fingerprint | MCP SYSTEM 复用，**不新开**执行路径 |
| HTTP 基建 | `apps/server`（axum 0.8 + tokio） | 只读 History 7 路由；默认 `127.0.0.1:8080`；无 CORS/鉴权 | Streamable HTTP transport 复用其运行时形态 |
| Rust SDK 生态 | workspace deps | 无 MCP SDK | **薄协议 adapter**（§1）：手写 JSON-RPC 2.0 帧 + 协议不变量测试，不为 MCP 引外部 crate |

### 1.2 目标规范

- **MCP 2026-07-28**（当前正式版）：JSON-RPC 2.0 over STDIO / Streamable HTTP；
  `initialize` → `tools/list` → `tools/call`；
  stateless request/response core（§49：不建永久 MCP session server state）。
- 实现策略：**薄协议 adapter**。协议帧（`McpRequest`/`McpResponse`/`McpError`）+
  方法分发 + schema 派生；工具语义全部委派 `ToolRegistry`。
  不实现：Resources（P1）、Tasks（P2）、Prompts、Sampling、Elicitation（§61-§65）。

### 1.3 依赖方向（不变式）

```text
core            mcp/{principal,exposure,audit}.rs（纯契约：principal / scopes / exposure / audit）
application  →  core only（McpToolAdapter + McpExposurePolicy + McpAuthorizationPolicy
                          + RemoteIdentityProvider trait + McpAuditPort）
infrastructure →  core（FakeIdentityProvider / token store；无平台耦合）
apps/desktop   =  组合根 + MCP Settings UI + 确认 UI 扩展
apps/server    =  MCP HTTP transport（复用既有 axum 运行时）
```

`application → infrastructure` = 0 保持；`std::process` 在 core/application = 0 保持。

---

## 2. 目标架构

```text
                    self-tools
                        │
                  ToolRegistry（source of truth）
                        │
        ┌───────────────┼────────────────┐
        │               │                │
   PersonalAgent     MCP STDIO       MCP HTTP
        │               │                │
   (内部 AI)       Local Client     Remote Client
                        │                │
                        └────────┬───────┘
                                 │
                          Authorization
                          （principal + scope + exposure）
                                 │
                            Risk Policy
                                 │
                          SafeAction Layer
                                 │
                            Confirmation
                                 │
                               Audit
```

---

## 3. Track A · MCP Core Contracts + Adapter（Gates 1-2）

### 3.1 core 契约（`crates/core/src/mcp/`）

| 类型 | 内容 |
| --- | --- |
| `McpPrincipal` | `principal_id / client_id / transport / scopes / trust_level / authenticated / metadata`（§20） |
| `McpTransport` | `Stdio / Http` |
| `McpTrustLevel` | `LocalTrusted / RemoteAuthenticated / RemoteUntrusted`（§21） |
| `McpScope` | 字符串新类型 + 解析（`selftools.read` / `memory.read` / `documents.read` / `files.read` / `server.read` / `history.enrich` / `memory.write` / `server.action`）（§36） |
| `ExposureGroup` | `BasicRead / KnowledgeRead / ModuleRead / SafeWrite / SystemAction`（§67） |
| `ToolExposure` | tool 名 → 所需 scope + group + 是否 remote 可见 |
| `McpAuditEntry` | `request_id / timestamp / principal_id / client_id / transport / tool / risk / decision / duration_ms / result`（§78） |

### 3.2 application（`crates/application/src/mcp/`）

| 组件 | 职责 |
| --- | --- |
| `McpToolAdapter` | `tool_definitions(principal) -> Vec<McpToolDef>`（name/description/inputSchema 从 `ToolSpec` 派生，**零手写**）；`call(principal, request) -> McpToolCallResult` |
| `McpExposurePolicy` | tool 名 → exposure + 必需 scope + remote 可见性（默认表 + `settings.mcp` 覆盖） |
| `McpAuthorizationPolicy` | `authorize(principal, tool_spec) -> Decision{Allowed, Denied{reason}}`；reason 用 §41 四态（`unauthenticated` / `invalid_token` / `expired_token` / `insufficient_scope`） |
| `RemoteIdentityProvider` | trait：`authenticate(credential) -> Result<McpPrincipal, AuthError>`；`AuthError` = 上述四态 |
| `McpAuditPort` | trait；记录 §78 字段 |
| `McpService` | 编排：authenticate → authorize(discovery) → schema validate → `ToolRegistry::execute` → audit |

**风险语义（§37/§53-§55）**：

| tool risk | MCP 行为 |
| --- | --- |
| READ | 有对应 read scope → 直接执行 |
| SAFE_WRITE | 有 module write scope → 执行（仍过 registry 的 `allowed_risk`） |
| SENSITIVE_WRITE | registry 门禁本就拒绝 → MCP 同样拒绝（不改内部策略） |
| SYSTEM | **永不直接执行**：进 `SafeActionService::plan`；首次返回 `confirmation_required`（§55）；执行只能由 self-tools UI 确认（§56/§57） |

### 3.3 Gate 2 PASS 判据

新增/删除 Tool 时 MCP catalog 自动同步——因为 `tool_definitions` 每次从
`registry.specs()` 派生。测试：注册一个临时工具 → 立即出现在 MCP catalog。

---

## 4. Track B · Local STDIO（Gate 3）

- 入口：`self-tools mcp`（二进制 `apps/mcp` 或 desktop 的 `--mcp-stdio` 子命令；
  按仓库结构选择，倾向**独立小二进制** `apps/mcp`）。
- principal：`LOCAL_TRUSTED`（§22），**仍过 Tool Risk enforcement**（不等于 root）。
- exposure：READ 全允许；SAFE_WRITE 按 tool；SYSTEM 走 SafeAction。
- **stdout 只走协议**；日志走 stderr（§25）。测试断言 stdout 帧干净（§104）。
- JSON-RPC 2.0：`initialize` / `tools/list` / `tools/call` / `ping`；
  未知方法 → `-32601 Method not found`；未知 tool → `-32602 Invalid params`。
- 配置示例由当前 executable 路径动态生成（§95，不硬编码用户路径），
  **不自动修改**其他 AI client 配置（§96）。

---

## 5. Track C/D · Remote Identity + Streamable HTTP（Gates 4-6）

### 5.1 Identity（§26-§42）

```text
trait RemoteIdentityProvider: Send + Sync {
    fn authenticate(&self, credential: &McpCredential) -> Result<McpPrincipal, AuthFailure>;
}
enum AuthFailure { Unauthenticated, InvalidToken, ExpiredToken, InsufficientScope, ProviderUnavailable }
```

- bearer **只从 `Authorization` header** 读（§32）；token 禁止进日志/审计（§79/§149）。
- 若配置 OAuth/OIDC：校验 issuer + audience/resource（§33/§34）；
  credential 按 `client_id` 隔离（§35）。
- **auth backend 出错 → DENY**（§42 fail-closed）。
- V8 只提供抽象 + Fake（CI 用，§106）；不实现 Authorization Server（§29）。

### 5.2 Authorization + Exposure（Gate 5）

- `tools/list` 与 `tools/call` **都**过 `McpAuthorizationPolicy`（§18/§19/§127）：
  未授权 client 连 catalog 都看不到私人工具。
- 无 `memory.read` → 看不到 memory 工具（§70/§148）；SENSITIVE 记忆即使有 scope
  也不默认暴露（§71，复用 V6 `is_model_visible`）。
- `server.action` scope **不等于**可执行 restart（§38）：仍要 SafeAction + 确认。
- `REMOTE_UNTRUSTED` → **0 tools**（§40）。

### 5.3 Streamable HTTP（Gate 6）

- 默认 bind `127.0.0.1`（§44）；LAN 需显式 `enable_remote_mcp = true` **且**
  identity provider 已配置，否则启动 fail 或降级 loopback（§45/§46）。
- 传输：`POST /mcp`（JSON-RPC 帧）；`GET /mcp` 用于 SSE 由 SDK 取舍——V8 只做
  request/response（stateless core，§49）。
- 限制：max body / max arg size / max response size / request timeout /
  tool timeout（§83-§85）。
- rate limit：per principal + per tool（§81）；SYSTEM 复用 V7 cooldown（§82）。

---

## 6. Track E · SafeAction 集成 + UI（Gates 7-8）

- MCP `services.restart` → `SafeActionService::plan` → 返回
  `confirmation_required { confirmation_id, summary, target, risk, expires_at }`（§55）。
- **外部 client 不能自己说 `confirmed=true`**（§56）：票据只能由 self-tools UI 的
  `confirm_action` 命令消费。
- 确认 UI 显示外部 client 元数据（client id / principal / tool / target / risk）（§58）。
- 结果获取：第一版「重新调用」或 `confirmation status`（§59 选最简单方案）。
- **跨 client 隔离**（§103）：票据绑 `session_id = client_id`；
  Client B 拿 Client A 的票据 → Denied。
- UI：Settings 增加 MCP 区（§89-§91：状态 / STDIO / HTTP / remote / bind / port /
  auth 状态；**secret 只显示 configured/not configured**）；Pending Actions 展示
  外部请求（§94）；审计 UI 支持 `source = MCP` 过滤（§93）。

---

## 7. 安全模型 / Threat Model

| 威胁 | 缓解 |
| --- | --- |
| 未认证发现私人工具 | discovery 也过 authorization（§18） |
| 未认证调用 | authenticate 前置；fail-closed |
| scope 提升 | tool → 必需 scope 显式映射；缺 → Denied |
| SYSTEM 绕过确认 | MCP 无执行入口；只有 UI 确认消费票据 |
| 票据重放 / 跨 client | 一次性 + fingerprint + client 绑定（V7 + §103） |
| bearer 泄漏 | 只从 header 读；脱敏正则覆盖；审计不含 token |
| SSRF / 任意 URL | MCP 不暴露任意 HTTP 工具；health URL 仍来自 registry（V7） |
| 任意文件 / shell | V6/V7 边界不变（Allowed Roots / deny / 闭合枚举） |
| schema bypass | `ToolRegistry::validate_args` 前置 + domain validation |
| payload DoS | body/arg/response 上限 + timeout |
| prompt injection（Documents/Files/Logs/Web） | 仍标记 untrusted（§87）；transport 不改变语义 |

---

## 8. Test Matrix

| 面 | 用例 |
| --- | --- |
| Conformance（§97） | tool discovery / tool call / invalid method / invalid tool / invalid args / structured result / authorization |
| Exposure（§98） | local trusted 见 READ；remote unauth 无私人工具；无 scope 隐藏；有 read scope 可见；无 `server.action` 不可见 |
| Authorization（§99） | valid / expired / wrong issuer / wrong audience / insufficient scope / unknown client / provider unavailable |
| HTTP security（§100） | remote disabled → 不能 LAN bind；remote 无 auth → 启动失败/降级；非法 Authorization → denied；超大 body → rejected |
| Risk（§101） | READ 直接；SAFE_WRITE scope 不足 → Denied；SYSTEM 即使有 scope → `confirmation_required` |
| Confirmation（§102） | MCP SYSTEM → ticket；错 client confirm → denied；过期 → denied；target 篡改 → denied；UI 确认 → execute once + audit |
| Cross-client（§103） | Client A 票据 Client B 用 → denied |
| STDIO（§104） | stdout 协议干净 / 日志 stderr / tools list / tools call / invalid call |
| HTTP（§105） | Fake transport：negotiate / list / call / auth / timeout（不依赖 Internet） |
| Gate 2 | 新增工具 → MCP catalog 自动同步 |

---

## 9. Gate 顺序与 PASS 判据

| Gate | 内容 | PASS |
| --- | --- | --- |
| -1 | V7 Freeze | 工作树干净；608 基线绿 |
| 0 | Spec + audit + Plan | 落盘；确认零 MCP/OAuth 资产 |
| 1 | MCP Core Contracts | `McpPrincipal / McpToolAdapter / McpExposurePolicy / McpAuthorizationPolicy` 编译 + 单测 |
| 2 | ToolRegistry Adapter | catalog 自动派生（无第二张 tool list） |
| 3 | Local STDIO | 本地 client 可 list + call read tool；stdout 干净 |
| 4 | Remote Identity | 可区分 authenticated / unauthenticated / principal / scope |
| 5 | Authorization + Exposure | discovery 与 execution 都 enforce scope |
| 6 | Streamable HTTP | 默认 loopback；无 auth 不能 remote |
| 7 | SafeAction 集成 | MCP 无法绕过 Confirmation / Audit / Rate Limit |
| 8 | Settings + Audit UI | UI 可看状态 / pending actions / audit 过滤 |
| 9 | Security Review | 独立 reviewer；无 HIGH/CRITICAL 未修 |
| 10 | Conformance + Regression | 协议测试 + workspace 全绿 + V5/V6/V7 不退化 |
| 11 | Docs | `MCP_V8.md` / `V8_OVERNIGHT_STATUS.md` / `V8_FINAL_REPORT.md` + `ADR-007` |

---

## 10. Scope 控制

**本轮不做**（§10）：Multi-Agent、A2A、公有云部署、智能家居、router 控制、
SSH agent、包管理、OS update、重启/关机、支付、发邮件；Resources（P1）、
Tasks（P2）、Prompts、Sampling、Elicitation、MCP Apps。

**降级策略**（§152）：若完整 Remote Auth 无法安全完成 →
**V8 = Local MCP only**（STDIO + loopback HTTP，remote disabled），
绝不交付未认证 LAN MCP。

## 11. Rollback

`core/src/mcp` + `application/src/mcp` + `apps/mcp`（或 desktop 子命令）+
`ui` MCP 设置区全部可独立删除；`settings.mcp` 走 `serde(default)`；
`ToolRegistry` / `SafeAction` / V6 边界零改动 → 删除即回到 V7 形态。
