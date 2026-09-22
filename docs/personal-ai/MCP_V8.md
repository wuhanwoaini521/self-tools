# SELF-TOOLS V8 · MCP INTEGRATION & REMOTE IDENTITY — 终版架构

> 状态：✅ 已实施（Gates -1–11 PASS；独立安全审查 3 项发现全部修复，测试 **687 / 687**）。
> 计划见 [`MCP_V8_PLAN.md`](MCP_V8_PLAN.md)；决策记录见
> [`ADR-007-mcp-as-adapter.md`](../architecture/ADR-007-mcp-as-adapter.md)。

---

## 0. 一句话

把 V4–V7 积累的 typed capabilities（`ToolRegistry`）通过**标准 MCP 协议**暴露给
外部 AI Client，同时**不新开任何权限通道**：MCP 只是 transport / protocol adapter，
风险门禁、确认、审计、限流全部复用既有层。

```text
BEFORE: self-tools 的能力只能被内部 PersonalAgent 使用
AFTER : PersonalAgent ─┐
                       ├─► ToolRegistry ◄─ MCP STDIO（本地 Client）
                       │                 ◄─ MCP HTTP（默认 loopback）
                       └─ 同一套 exposure / risk / SafeAction / audit
```

---

## 1. 核心决策：MCP 是适配器，不是业务层

| 原则 | 落实 |
| --- | --- |
| MCP Tool != Business Logic | `apps/mcp` 只做协议编解码 + 传输；工具语义全在 `application::mcp` → `ToolRegistry` |
| ToolRegistry = Source of Truth | `McpToolAdapter::all_definitions()` 每次从 `registry.specs()` 派生；**无第二张 tool list**（Gate 2 测试锁定） |
| 稳定命名 | MCP tool 名 = 内部名（`history.search` / `server.get_status`）；无 `mcp_*_v2` 体系 |
| Schema 不手写 | `inputSchema` 直接取 `ToolSpec::input_schema`；仅补 `type: object` 归一化 |
| Risk 不丢失 | annotations 暴露 risk / module / exposureGroup / requiresConfirmation |
| PersonalAgent 零改动 | MCP 直接调 `ToolRegistry::execute`，不经过 PersonalAgent |

## 2. 暴露与授权（V8 安全核心）

```text
tools/list  ──► authenticate ──► McpAuthorizationPolicy::authorize_tool(每个 tool)
                                                     │
tools/call   ──► authenticate ──► authorize_tool ────┤
                                    （同一函数！discovery 与 execution 同权，§18）
                                                     │
                              payload 上限 ──► exposure group 判定
                                                     │
                              SYSTEM? ──► SafeActionService::plan（票据，不执行）
                                        └─► ToolRegistry::execute
```

| 环节 | 规则 |
| --- | --- |
| 身份 | `McpPrincipal{principal_id, client_id, transport, trust, authenticated, scopes}`；`AuthFailure` 四态（unauthenticated / invalid_token / expired_token / insufficient_scope）+ provider_unavailable |
| 传输信任 | STDIO / loopback = `LocalTrusted`（**仍过全部门禁**，§22）；HTTP 已认证 = `RemoteAuthenticated`；其它 = `RemoteUntrusted` → **0 tools**（§40） |
| Scope | 8 个（selftools.read / {memory,documents,files,server}.read / history.enrich / memory.write / server.action）；`selftools.read` 只覆盖 module read，**不覆盖** server.action |
| 暴露表 | **白名单**：未列出 = 不暴露（`default_exposure` → None）；SYSTEM 对远程不可见（§77）；knowledge 工具需各自 scope（§70） |
| SYSTEM | 判定依据是 `ExposureGroup::SystemAction`（而非 registry risk，见 ADR-006）；首次调用返回 `confirmation_required` + 票据；**执行只能由 self-tools UI 确认**（§56） |
| 票据 | 一次性 + TTL + fingerprint(action_type|target_id|canonical(params)|risk) + **session 绑定**（§103：Client A 的票据 Client B 不能用） |
| 审计 | `McpAuditEntry`（request_id / principal / client / transport / tool / risk / decision / result / duration）；`AuditSource` 区分 desktop/mcp；**不含 token / 正文** |

## 3. Transport

| Transport | 状态 | 要点 |
| --- | --- | --- |
| STDIO（`self-tools mcp --stdio`） | ✅ | stdout 只走协议帧；日志 stderr；本地 = LocalTrusted；`serve_with()` 复用调用方 runtime |
| Streamable HTTP（`--http`） | ✅ | `POST /mcp` 单端点、stateless；默认 `127.0.0.1:8787`；loopback 免 bearer，其余必须 `Authorization: Bearer`；**启动门禁**：非 loopback 需 `remote_enabled` + 已配置 identity，否则启动失败（§46）；body 上限 1 MiB |
| 远程 LAN | ⛔ 默认关 | `remote_enabled = false`；接入真实 OAuth/OIDC provider 前不开放（§29/§152） |

## 4. 明确不做（P1/P2）

Resources（`selftools://server/status` 等）、Tasks extension、Prompts、Sampling、
Elicitation、MCP Apps、Authorization Server 实现（只留 `RemoteIdentityProvider`
抽象 + DenyAll + StaticToken Fake）。

## 5. 分层与依赖（不变式保持）

```text
core            mcp/{principal,exposure,audit}.rs（纯契约）
application  →  core only（adapter / policy / auth / service）
infrastructure →  core（SQLite 审计 + Fake identity）
apps/mcp       =  protocol + transport（stdio/http/compose）
apps/desktop   =  组合根 + MCP 设置 UI + 确认 UI
```

`application → infrastructure` = 0；`std::process` 在 core/application = 0。

## 6. 测试矩阵

| 面 | 数量 | 覆盖 |
| --- | --- | --- |
| core mcp | 16 | scope 覆盖 / 宽 scope 不越权 / bearer 解析 / 审计无 token / 暴露表白名单 |
| application mcp | 23 | catalog 派生 / discovery+execution 同权 / 未暴露隐藏 / SYSTEM 远程隐藏 / 过期拒绝 / 风险漂移保护 |
| MCP transport | 24 | STDIO 7（含 stdout 纯度）/ HTTP 10（auth、oversize、startup gate、转发头）/ protocol 8 |
| SafeAction 集成 | 8 | MCP SYSTEM → 票据 / 禁止自批准 / 跨 client 拒绝 / 目标篡改拒绝 / 重放拒绝 / scope 隐藏 / 未注册拒绝 / 审计完整 |
| 既有（V4–V7） | 620 | 无退化 |

## 7. 已知限制（P1）

- 远程身份：只有抽象 + Fake；真实 OAuth/OIDC 接入前远程写操作关闭（§152）。
- Phase-1 `apps/mcp` 组合根是 fail-closed 空能力集；完整装配待身份层。
- Resources / Tasks / Prompts / Sampling 未实现。
- `StaticTokenIdentityProvider` 的 token 比较非 constant-time（生产 provider 需保证）。

## 8. Gate 9 安全审查结论

独立 security reviewer 按 A–H 清单审查：**auth bypass / bearer 泄漏 / SYSTEM 判定 /
票据生命周期 / 确认响应内容 / STDIO 纯度 全部通过**；3 项发现已修复：

| # | 严重度 | 发现 | 修复 |
| --- | --- | --- | --- |
| MCP-C | 中 | loopback HTTP 与 STDIO 用固定 `client_id` → §103 跨 client 票据隔离与 §67 会话限额退化为单一全局 session | 每请求 nonce 唯一 client_id（HTTP）+ 每进程唯一（STDIO） |
| MCP-H | 低 | HTTP `tools/call` 不校验 `arguments` 是对象（STDIO 有） | 两条传输一致：非对象 → `INVALID_PARAMS` |
| MCP-E | 低 | `request_timeout_ms` 未接线；`truncate_json` 先全量序列化 | `tokio::time::timeout` 包住工具执行；截断只保留前缀并记录总长 |

另外本轮自行修复：loopback 信任判定原先只看「无转发头」（可被 LAN 客户端利用），
改为 axum `ConnectInfo<SocketAddr>` **真实对端 IP** + 代理头兜底，并补 2 条回归测试。
