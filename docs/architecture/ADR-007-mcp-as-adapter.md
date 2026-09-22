# ADR-007 · MCP as Adapter：ToolRegistry 是唯一 Source of Truth，Remote Auth Required

- 状态：**Accepted**（2026-09-22，V8 Gates 0-8 PASS，Gate 9 审查中）
- 领域：`crates/core` / `crates/application` / `apps/mcp` / `apps/desktop`
- 关联：[ADR-005-personal-knowledge-layer.md](ADR-005-personal-knowledge-layer.md)（V6）、
  [ADR-006-safe-actions.md](ADR-006-safe-actions.md)（V7）、
  [V8 计划](../personal-ai/MCP_V8_PLAN.md)

## 背景

V4–V7 已形成成熟的内部能力栈：`ModuleRegistry` → `ToolRegistry`（typed capability +
risk）→ `SafeActionService`（确认 + 审计 + 限流）。用户希望未来能从手机 / 平板 /
其它 AI Client（Pi、Claude）使用 self-tools。

两条路：(a) 为每个外部 Client 写集成；(b) 用标准 MCP 协议暴露既有能力。
(b) 明显正确，但有一个真实风险：**MCP 很容易变成第二条权限通道**——新 server、
新 schema、新执行路径，绕过 V6/V7 的全部边界。

## 决策

### 1. MCP 只是 transport / protocol adapter

`apps/mcp` 只做三件事：JSON-RPC 帧编解码、方法分发、传输（STDIO / Streamable HTTP）。
工具语义**全部**委派 `application::mcp` → `ToolRegistry`。MCP 层没有、也不允许有
任何业务分支（与 PersonalAgent 零业务分支同构）。

### 2. ToolRegistry 是唯一 source of truth

`McpToolAdapter::all_definitions()` 每次从 `registry.specs()` 派生 MCP tool
definition（name / description / inputSchema / annotations）。**不维护第二张 tool
list**；新增/删除工具 → MCP catalog 自动同步（Gate 2 有测试锁定）。

命名保持内部稳定名（`history.search`），不建 `mcp_*_v2` 第二命名体系。

### 3. Discovery 与 Execution 同权

`tools/list` 与 `tools/call` 都过**同一个** `McpAuthorizationPolicy::authorize_tool`：
未授权 client 连 catalog 都拿不到（§18/§19）。这避免「通过 tools/list 探测私人能力」
这一信息泄露面。

### 4. 暴露表是白名单，且 SYSTEM 对远程不可见

`default_exposure(tool)` 是显式表：**未列出 = 不暴露**。分组
`basic_read / knowledge_read / module_read / safe_write / system_action`；
knowledge 工具需各自 scope（memory.read 等，§70）；`services.restart` 归
`system_action` 且 `remote_visible = false`（§77）。

### 5. Risk 语义由暴露分组 + 票据表达，不由 registry risk 表达

V7 起 `services.restart` 在 `ToolRegistry` 注册为 `Read`（因为 registry 的
`allowed_risk` 门禁只放行 Read+SafeWrite）。它的 SYSTEM 语义由
`ExposureGroup::SystemAction` 与 `SafeActionService` 的一次性票据表达（ADR-006）。
MCP 层沿用同一约定：SYSTEM 判定看**暴露分组**，不是 registry risk。

### 6. MCP 无执行入口：SYSTEM 只能由 self-tools UI 确认

`McpService::call_tool` 对 SYSTEM 工具调 `SafeActionService::plan` 并返回
`confirmation_required`；票据一次性 + TTL + fingerprint，且**绑 session**
（MCP principal 的 client_id）——Client A 的票据 Client B 不能用（§103）。
外部 client 没有任何参数能自称「已确认」。

### 7. 远程必须认证，未认证一律 deny；没有 provider 就关闭远程

`RemoteIdentityProvider` 抽象 + `AuthFailure` 四态（unauthenticated / invalid_token /
expired_token / insufficient_scope）+ provider_unavailable；auth 后端出错 →
**DENY**（§42）。bearer 只从 `Authorization` header 读；token 不进日志 / 审计 /
错误体（§79）。

尚无 Authorization Server：远程 MCP **默认关闭**；非 loopback 绑定需要
`remote_enabled = true` 且已配置 identity，否则启动失败（§46）。宁可是
「Local MCP only」也不交付未认证 LAN MCP（§152）。

### 8. 传输层上限与 body 限制在 protocol 之前

body 1 MiB → 解析 → 认证 → 授权 → payload 上限 → registry validate → 执行 →
response 上限。日志 / 文档 / 文件工具的业务限制继续生效，MCP 只**追加**传输层闸门。

## 备选方案（拒绝）

| 方案 | 拒绝理由 |
| --- | --- |
| MCP server 内自建工具实现 | 第二套业务逻辑必然漂移；违反 Principle 1 |
| MCP 直接注入 PersonalAgent | 引入会话/Provider 耦合；MCP client 应直连 ToolRegistry |
| tools/list 不过授权（只保护 call） | tools/list 本身就是能力探测面（§19） |
| 远程未认证也可读 | 「家里局域网」不是身份（§31）；默认关闭 |
| token 走 query param | URL 会进日志/历史（§31） |
| MCP 客户端可自报 confirmed=true | 确认必须是用户动作（§56） |
| 为 MCP 新开 command string 工具 | 回到 V7 已拒绝的任意命令形态 |

## 影响

- 新增 `core::mcp`（纯契约）、`application::mcp`（adapter/policy/auth/service）、
  `apps/mcp`（protocol + stdio + http + compose）。
- `settings.server.mcp`（serde default）；`AuditEntry.source`（desktop/mcp）；
  `SafeActionService` 票据绑 session。
- `ToolRegistry` / `PersonalAgent` / V6 边界**零改动**。
- 测试 +98（core 16 / application 23 / transport 24 / 集成 8 + 若干扩展）。
