# V8 · MCP Integration & Remote Identity — 实施状态

> 实时状态（Gates -1–11）。基线：V7 冻结 HEAD `d147776`（608 全绿）。
> 已提交 4 个 checkpoint；文档与审查修复在工作树。

## 测试总览（真实执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **681 passed / 0 failed**（V7 基线 608，+73） |
| `cargo test --workspace --all-targets` | 通过 |
| `cargo check -p devtoolbox-desktop --tests` | 0 error / 0 warning |
| `npx tsc --noEmit -p apps/desktop/ui/tsconfig.json` | 0 error |
| `npm run build`（ui） | built |

测试分布：core 157（mcp 16）/ application 327（mcp 23）/ infrastructure 144 /
desktop 13 / apps/mcp 33（lib 24 + bin 1 + 集成 8）/ server 7。

## Gate 状态

| Gate | 内容 | 状态 | 证据 |
| --- | --- | --- | --- |
| -1 | V7 Freeze | ✅ | V7 已 5 commits（含 prompt + docs）；工作树干净；608 基线绿 |
| 0 | Spec + Audit + Plan | ✅ | `MCP_V8_PLAN.md`；全仓 grep 确认**零** MCP/OAuth 资产；`ToolRegistry` 适配面干净 |
| 1 | MCP Core Contracts | ✅ | `core/src/mcp/{principal,exposure,audit}.rs`；`McpPrincipal / ToolExposure / McpAuthorizationPolicy / RemoteIdentityProvider` 编译 + 16 单测 |
| 2 | ToolRegistry Adapter | ✅ | `McpToolAdapter` 从 `ToolSpec` 派生；catalog 自动同步测试锁定（无第二张表） |
| 3 | Local STDIO | ✅ | `apps/mcp --stdio`；stdout 只走协议（测试逐行断言 JSON 可解析）；日志 stderr；本地 = LocalTrusted |
| 4 | Remote Identity | ✅ | `RemoteIdentityProvider` trait + `DenyAll` + `StaticTokenIdentityProvider`（Fake，CI 用）；`AuthFailure` 四态；过期/无效/无凭证区分 |
| 5 | Authorization + Exposure | ✅ | discovery 与 execution 共用 `authorize_tool`；未列出 = 不暴露；SYSTEM 远程不可见；宽 scope 不覆盖 server.action |
| 6 | Streamable HTTP | ✅ | `POST /mcp`；默认 loopback；启动门禁拒绝无 auth 的远程绑定；body 上限；转发头不当成本地 |
| 7 | SafeAction Integration | ✅ | MCP SYSTEM → 票据不执行；**session 绑定**（§103，本轮新增）；未注册服务 = 授权拒绝；8 集成测试 |
| 8 | MCP Settings + Audit UI | ✅ | `settings.server.mcp`；`mcp_status` 命令（无 secret）；`AuditSource` 列 + UI 过滤；MCP 状态卡 |
| 9 | Security Review | 🔄 | 独立 reviewer 运行中（结论待回） |
| 10 | Conformance + Regression | ✅ | 681 全绿；`--all-targets` 通过；V5/V6/V7 无退化 |
| 11 | Docs | 🔄 | `MCP_V8.md` / `ADR-007` 已写；status + final report 待审查结论补齐 |

## 实现选择（与计划的偏差，均有理由）

1. **SYSTEM 判定看暴露分组而非 registry risk**：V7 起 `services.restart` 在
   `ToolRegistry` 注册为 Read（registry 门禁只放行 Read+SafeWrite），SYSTEM 语义由
   `ExposureGroup::SystemAction` 表达（ADR-006）。MCP 沿用同一约定，
   `risk_matches_exposure` 允许 `Read ↔ SystemAction`（表比实现严）。
2. **remote 不可见先于 scope 检查**：未授权方得到 `tool_not_exposed` 而不是
   `insufficient_scope`——不泄露「该工具需要什么 scope」。
3. **未注册服务 = 授权拒绝**（`unknown_service`）而非 invalid_params：让 client 能
   区分「没权限」与「调用形式错」。
4. **Phase-1 `apps/mcp` 组合根 fail-closed**：空 registry + DenyAll identity +
   空服务表（§152：能力小优于能力错）；完整装配留待身份层就绪。
5. **`AuditSource` 用 `Mutex` 内部可变**：`SafeActionService` 无 Clone 且被 Arc 共享，
   MCP 装配时打标签需要共享修改（而非消耗式 builder）。

## 未做（明确范围外）

- OAuth/OIDC Authorization Server（只留抽象 + Fake）；
- Resources / Tasks / Prompts / Sampling / Elicitation / MCP Apps；
- 远程 LAN 暴露（默认关闭，待身份层）；
- `apps/mcp` 的完整 store 装配。

## 回滚

删除 `crates/{core,application}/src/mcp`、`apps/mcp`、`settings.server.mcp`、
`AuditSource` 与 `mcp_status` 命令即可回到 V7 形态；`ToolRegistry` /
`PersonalAgent` / V6/V7 安全边界零改动。
