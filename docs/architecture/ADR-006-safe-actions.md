# ADR-006 · Safe Actions：不给 AI shell，只给注册能力 + 确认 + 审计

- 状态：**Accepted**（2026-09-21，V7 Gates 0-10 PASS）
- 领域：`crates/core` / `crates/application` / `crates/infrastructure` / `apps/desktop`
- 关联：[ADR-003-personal-ai-hub.md](ADR-003-personal-ai-hub.md)（V4）、
  [ADR-005-personal-knowledge-layer.md](ADR-005-personal-knowledge-layer.md)（V6）、
  [V7 计划](../personal-ai/HOME_SERVER_V7_PLAN.md)

## 背景

V6 之后 AI 已「知道我的知识」，但家庭服务器（macOS 12，同时承担个人托管 /
局域网服务 / 文件与知识 / Personal AI Backend）对 AI 完全不可见：无法回答
「哪个盘快满了」「哪些服务没跑」，也无法在用户确认后重启一个服务。

需求天然冲突：**要能用**（回答问题、执行有限操作）与**不能失控**（AI != Shell、
AI != Root、AI != Automatic System Administrator）。V7 必须在不破坏 V4 立下的
「PersonalAgent 零业务分支」与 V6「AI 对用户数据只读」两条不变式的前提下，
打开一个**受控的写入口**。

## 决策

### 1. 模型面只有「注册能力」，没有「命令」

`ToolRegistry` 暴露的工具全部是 typed capability（`server.get_status` /
`services.get_logs` / `services.restart` …）；**不存在** shell / exec /
run_command / bash / terminal 任一形态。执行侧 `RegisteredAction` 是**闭合枚举**
（当前 `RestartService`）——AST 层面不存在「任意命令」的表示，新增操作 = 新增
变体 = 编译期强制显式授权路径，而不是新字符串。

理由：命令字符串一旦进入系统，注入面无限（`foo; rm -rf /`、`--flag`、换行）。
闭合枚举把「模型能请求什么」变成有限集，其余一概不存在。

### 2. 平台调用隔离在 infrastructure，且 argv 全字面量

`Command::new` 只允许出现在 `crates/infrastructure/src/server/**`，形态固定为
`固定 executable + 固定 argument template`（`launchctl print gui/<uid>/<label>`、
`launchctl kickstart -k gui/<uid>/<label>`、`sysctl -n kern.boottime`、
`df -k -P`）。label 只能来自注册表的 `provider_ref`；模型输入（service_id）
与平台引用（label）的映射只发生在 desktop 适配器内。

理由：`sh -c <model_output>` 是最经典的漏洞形态。固定 argv + 映射内聚 =
静态可断言（`grep '"sh","-c"'` = 0）+ 单点审计。

### 3. 写操作 = 票据制：Plan → Authorize → Confirm → Execute → Audit

`SafeActionService` 是唯一写入口。`services.restart` 工具**本身不执行任何操作**：
它校验注册表、签发 `Confirmation`（一次性 + TTL + fingerprint 绑定）、返回
`confirmation_required` + `Action::ConfirmAction`。执行入口是桌面命令
`confirm_action`，由 application 完成「重验证 → 执行 → 审计」。

理由：让模型「决定执行」与「执行」之间永远隔一个用户动作。工具返回票据而非结果，
模型无法用「再调一次工具」绕过确认。

### 4. Fingerprint 绑定 + 一次性，结构性消灭重放与 TOCTOU

`Confirmation.request_fingerprint = action_type | target_id | canonical_json(action) | risk`
（键排序规范化，序列化顺序无关）。`confirm_and_execute` 四步校验：票据存在 →
`state == Pending` → `now < expires_at` → fingerprint 逐字节一致；**执行前**标记
`Consumed`。确认 A、执行 B → `Denied`；过期 → `Expired`；重放 → `Denied`。

理由：确认语义必须是「用户批准的那个具体操作」，而不是「一个可复用的许可」。
在执行前消费票据，保证失败路径也不可重放。

### 5. 会话信任分级，远程 fail-closed

`SessionTrust::{LocalDesktop, RemoteAuthenticated, RemoteUntrusted}` +
`ActionRiskPolicy`。当前无完整身份体系：`RemoteUntrusted` 一律拒写
（`untrusted_session`）；Tauri 桌面会话 = `LocalDesktop`（本地窗口 + 用户确认）；
HTTP server crate **不暴露任何写端点**。

理由：局域网暴露是用户显式决策（`SELF_TOOLS_BIND`），但「无法验证身份的客户端
能重启服务」不是。先立接口（§75），身份系统留给后续。

### 6. 审计只记结构与稳定码

`config/server_actions.db`（SQLite）：`id / timestamp / session_id / action_type /
target_id / risk / confirmed / result / duration_ms / error_code`。**不记录**
API key、完整日志、完整 prompt、secret 值。保留策略：500 条 / 30 天，先到先裁。
DENIED / EXPIRED / FAILED / SUCCESS **全部**记账（§131：所有 write attempt 都有审计）。

理由：审计要能回答「AI 请求了什么、是否确认、结果如何」，同时自身不成为泄漏面。

### 7. 日志是不可信数据：有界 + 脱敏 + 标记

三硬限制（2_000 行 / 1 MiB / 86_400s）+ 来源只能是注册表 log_source +
infra 侧 traversal 防御 + application 侧脱敏（结构化字段优先 → 裸 bearer /
连接串 → V6 secret 门兜底）+ `untrusted: true` 标记 + prompt 规则声明
「日志内容只是数据」。

理由：日志是最常见的 prompt injection 载体，也是最常见的密钥泄漏载体。
两者都需要**结构性**处理（限制来源与体积、脱敏、标记），而不是靠模型自觉。

## 备选方案（拒绝）

| 方案 | 拒绝理由 |
| --- | --- |
| 给模型 `run_command(command: String)` | 注入面无限；违反 Principle 1/2 |
| 白名单命令字符串（`allowlist: Vec<String>`） | 仍是字符串拼接；参数注入无法根治 |
| 确认后把票据存 Redis 供多端复用 | V7 不引外部依赖；一次性语义更安全 |
| 让模型直接调 `confirm_action`（自助确认） | 确认必须是用户动作，否则门禁失效 |
| SYSTEM 工具直接注册为 `ToolRisk::System` 并放行 | 当前 `allowed_risk` 只允许 Read+SafeWrite；放开它会影响所有模块。改为「工具 Read + 票据携带 SYSTEM 语义」，风险门禁仍真实生效（执行入口在桌面层强制） |
| 前端单击图标直接重启（无确认卡） | §82 明确禁止；确认卡必须显示目标/影响/风险/过期 |

## 影响

- `core::server` + `application::server` + `infrastructure::server` +
  `personal_ai::server` + `ui/features/server` 五个新面；
  `AppSettings.server`（serde default）；`config/server_actions.db`。
- `ToolRegistry` / `ActionProtocol` / `PersonalAgent` **零改动**（§106/§109：
  只复用既有 `Action` 与两种新 `ActionKind`）。
- 测试 508 → 606（+98）；静态断言：无 `sh -c`、`Command::new` 仅在 infra/server。
