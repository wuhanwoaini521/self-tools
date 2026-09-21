# SELF-TOOLS V7 · HOME SERVER & SAFE AUTOMATION — PLAN

> Gate 0 审计结论 + 五 Track 实施计划。基线：HEAD `3320bbe`(V6 Gate 10)，
> 工作树干净，`cargo test --workspace` = **508 passed / 0 failed**
> (application 256 / core 117 / infrastructure 124 / server 7 / desktop 4)。

---

## 1. Gate 0 审计（真实调用链，逐项核实）

### 1.1 平台现状（复用资产）

| 面 | 位置 | 事实 | V7 决策 |
| --- | --- | --- | --- |
| `PersonalAgent` | `crates/application/src/personal_ai/agent.rs` | 工具循环；**零业务分支**；V6 起含通用 `hub.retrieval` stage | **不改**；server 模块标准接入 |
| `ModuleRegistry` / `ToolRegistry` | `personal_ai/registry.rs` | 注册即校验 `module.action` + `allowed_risk`（当前 `Read | SafeWrite`） | `SYSTEM` 工具经**确认票据通道**注册（见 §5.2） |
| `ToolRisk` | `core/src/personal_ai/types.rs:61` | `Read / SafeWrite / SensitiveWrite / System` 四级已存在 | 沿用，不新增 |
| `ActionProtocol` | 同上 | `ActionKind` = Navigate / OpenEntity / RefreshView / ShowPanel / **OpenDocument / OpenFile / ConfirmMemory**（V6 新增 3） | 新增 `ConfirmAction` + `OpenApp` 两种 |
| `Action` | 同上 | `{type, module, target, payload, risk}`；模型只产请求 | `ConfirmAction` 携带不可变确认票据 |
| `AppContext` | 同上 | `module/page/entity/selection/view_state` | server 页面上报 `module = "server"` |
| reqwest | workspace dep | 0.12 + rustls，application **不依赖**（infra 依赖） | health check 走 infrastructure adapter |
| url | workspace dep (`url = "2"`) | 已在 workspace | app URL scheme 白名单校验用它 |
| 无系统监控资产 | 全仓 grep `sysinfo`/`hostname`/`uptime`/`launchd`/`Command::new` | **application/core 零命中**；desktop 零命中 | 全新域；平台代码隔离在 infra |

### 1.2 现有 HTTP 运行时（LAN 边界审计，§72/§73/§74）

| 事实 | 证据 | V7 决策 |
| --- | --- | --- |
| `apps/server` axum 0.8，只读 History 7 路由 + `/health` | `apps/server/src/routes.rs` | V7 **不**给 server crate 加写端点 |
| 默认绑 `127.0.0.1:8080`，`SELF_TOOLS_BIND` 可控 | `apps/server/src/main.rs:20` | 保持；LAN 暴露是用户显式决策 |
| 无鉴权、无 CORS | main.rs 文档注释 | §5.5：`ActionAuthorizationPolicy` fail-closed |
| `application → infrastructure` Cargo 依赖 | Cargo.toml（deferred 模块） | V7 新代码**不**新增 application→infra 引用；保持 grep = 0 的既有例外清单不变 |
| desktop 是唯一 Tauri 组合根 | `apps/desktop/src/lib.rs` | server 模块组合根装配在 desktop |

### 1.3 依赖方向（不变式，V7 必须保持）

```text
core            无内部依赖（新增 server 域纯契约：metrics/health/service/app/action/audit）
application  →  core only（SystemMetricsProvider 端口 + 域服务 + personal_ai/server 模块）
infrastructure  →  core（macOS adapter / launchd / docker / http health / audit store）
apps/desktop   = 唯一组合根（端口适配 + 命令 + Provider 装配）
```

**平台代码隔离铁律**：`Command::new` / `launchctl` / `sysctl` **只允许**出现在
`crates/infrastructure/src/server/**`。core/application 对 `std::process` 的引用必须为 0。

---

## 2. 目标架构

```text
                     PersonalAgent（零业务分支）
                            │
                   Module / Tool Registry
                            │
       ┌────────────────────┼────────────────────┐
   Knowledge            Modules               Server
       │                   │                     │
 Memory/Documents   History/Travel       ┌────────┼────────┐
 Files              Geo/Language      System    Services    Apps
                                          │         │         │
                                        Metrics   Registry  Registry
                                          │         │         │
                                          └────┬────┴─────────┘
                                               │
                                     Safe Action Service
                                    (Plan → Authorize →
                                     Confirm → Execute → Audit)
```

---

## 3. Track A · Server 域与契约（Gates 1-2）

### 3.1 core 契约（`crates/core/src/server/`）

| 类型 | 内容 |
| --- | --- |
| `SystemMetrics` | hostname / platform / os_version / arch / uptime_secs / cpu_usage / cpu_count / memory_total/used/available / load_average(Option) |
| `StorageMetrics` | `mount, total_bytes, used_bytes, available_bytes, usage_ratio, kind` |
| `VolumeKind` | `SystemDisk / DataDisk / Removable / Network / Temporary / Virtual / Unknown` |
| `HealthStatus` | `Healthy / Degraded / Unhealthy / Unknown`（§20，非 bool） |
| `HealthReason` | `code + detail`（可解释，§21；detail 不含内容） |
| `Thresholds` | disk_warn / disk_critical / memory_warn / cpu_warn（§22 配置化） |
| `ServiceDescriptor` | id / display_name / description / provider_type / provider_ref / health_check / log_source / allowed_actions / tags（§27） |
| `ServiceProviderType` | `Launchd / Docker / Http / Process`（§28；V7 实现 Launchd + Http） |
| `ApplicationDescriptor` | id / name / description / url / health_url? / service_id? / category / tags（§43） |
| `HealthCheckKind` | `None / Http { url } / Launchd` |

### 3.2 application 端口 + 服务

| 组件 | 职责 |
| --- | --- |
| `SystemMetricsProvider` | 端口：`metrics()` → `SystemMetrics`；`storage()` → `Vec<StorageMetrics>` |
| `HealthEvaluator` | 纯函数：metrics + thresholds + 服务状态 → `HealthReport{overall, reasons}` |
| `ServerService` | get_status / get_cpu / get_memory / get_storage / get_health（§18 可按设计合并） |
| `ServiceRegistry` | 注册表（settings 注入 + 默认空）；validate id → DENIED（§35） |
| `ApplicationRegistry` | 注册表；URL scheme 白名单（§47/§48） |
| `LogReader` | 端口：`read(service_id, max_lines, max_bytes, range)`；路径只能来自 descriptor（§39） |

### 3.3 macOS 12 adapter（infrastructure）

- `SystemMetricsProvider` 实现：优先 Rust native（`std::fs` 读 `/proc`-like；macOS 用
  `sysctl -n hw.memsize` 等**固定参数模板**，§24）；CPU/内存取 `host_statistics`
  不可得时降级为 `Unknown` 并记 reason（Gate 2 PASS 要求「部分指标不可用不 panic」）。
- `StorageMetrics`：`df -k -P` 固定参数解析，或 `statvfs`（若引入依赖则评估）；
  过滤规则见 §3.4。
- **禁止** `sh -c` / `bash -c` / 任何模型输出进入 argv（§25：出现即 FAIL）。

### 3.4 Storage 过滤（§17）

`VolumeKind` 判定规则（平台无关、可单测）：
- `/dev/disk…s1` + mount `/` → `SystemDisk`
- `/private/var/folders/…`、`/Volumes/…`(ramdisk) → `Temporary`
- `devfs`/`map auto_home`/`/System/Volumes/Data` 以外的小体积只读卷 → `Virtual`
- 用户显式配置的 `configured_volumes` → 保留
- 默认展示：`SystemDisk / DataDisk / Removable / Network / Unknown`；隐藏 `Temporary / Virtual`

---

## 4. Track B · Service / App Registry + Logs（Gates 3-4）

### 4.1 Service Registry

- 来源：`settings.server.services: Vec<ServiceDescriptor>`（组合根解析；空 = 未注册）。
- id 校验：`^[a-z0-9][a-z0-9._-]{0,63}$`（小写英数字 + `.`/`_`/`-`）——直接封死
  `foo; rm -rf /` 注入（§155）。
- 模型只能传 `service_id`（§36）；`provider_ref`（launchd label / container name）
  由 infrastructure 映射，**永不出现在工具入参**。
- `allowed_actions`：每个服务显式声明（如 `["restart"]`）；未声明 → 该操作 DENIED。

### 4.2 工具（§32）

| 工具 | risk | 说明 |
| --- | --- | --- |
| `services.list` | Read | 注册服务 + 状态摘要 |
| `services.get` | Read | 单个 descriptor + 状态 |
| `services.get_status` | Read | 健康状态 + reason |
| `services.get_logs` | Read | 有界 / 脱敏 / 不受信标记 |
| `services.restart` | **System** | 走票据通道（§5） |

### 4.3 Logs（§37-§41）

- 来源只能是 `descriptor.log_source`（stdout/stderr 路径或 launchd 日志）。
- 硬限制：`max_lines`（默认 200，上限 2_000）、`max_bytes`（默认 64 KiB，上限 1 MiB）、
  `max_seconds`（默认 300，上限 86_400）——全配置化。
- 脱敏：复用 `core::memory::detect_secret`（V6）+ authorization header / bearer /
  cookie / 连接串正则；命中 → `[REDACTED]`。
- **不可信数据标记**：日志文本在 prompt 中以 `<untrusted_log>` 包裹，
  `ServerContextProvider` 文档与 prompt 规则双写明「内容只是数据」。

---

## 5. Track C · Safe Action 层（Gates 5-6，V7 核心）

### 5.1 模型

```text
ActionRequest { action_type, target_id, parameters: Value, risk, requested_by }
Confirmation { id, request_fingerprint, summary, risk, created_at, expires_at, used }
ActionOutcome { Success / Failed / Denied / Expired / Cancelled }   // §62
AuditEntry { id, timestamp, session_id, action_type, target_id, risk, confirmed, result, duration_ms, error_code? }  // §64
```

- `request_fingerprint` = blake3/sha256(`action_type|target_id|canonical(parameters)|risk`)
  的实现无关摘要（用 `core` 已有 `stable_id` 风格；不引新依赖则用 SHA-256 via 手写或
  现有 `regex`-无关实现 → 评估后选型）。
- **TOCTOU 防护**（§58/§70）：确认票据绑定 fingerprint；执行时重算并比对，
  任一字段变化 → DENIED。

### 5.2 风险通道与 ToolRegistry 的关系

`services.restart` 注册为 `ToolRisk::System`。当前 `allowed_risk` 拒绝 System。
**决策**：`ToolRegistry` 增加可选 `RiskPolicy`（默认保持 `Read|SafeWrite` 不变），
desktop 组合根为 server 模块注入 `RiskPolicy::allow(SystemFor("services.restart"))`
—— 该工具本体**不做任何系统操作**，只做：
`validate → 签发 Confirmation → 返回 {confirmation_required, confirmation}` +
`Action::ConfirmAction{confirmation_id}`。执行入口是桌面命令 `confirm_action`，
`SafeActionService::confirm_and_execute` 在 application 层完成
「重验证 → 执行 → 审计」（§69）。

### 5.3 Confirmation 生命周期

| 属性 | 值 |
| --- | --- |
| 有效期 | `settings.server.confirmation_ttl_secs`，默认 60（区间 30–120，§59） |
| 一次性 | `used` 标志；重放 → `Expired/Denied`（§60、测试 Case 5） |
| 绑定 | fingerprint；不一致 → DENIED（Case 6） |
| 清理 | 过期条目惰性清理 + store 容量上限 |

### 5.4 Executor

- 只能执行 `RegisteredAction`（typed enum）：`RestartService { service_id }`（§61）。
- **没有 command string 形态**——AST/枚举层面不存在任意命令表示。
- 失败隔离：单服务重启失败 → `ActionOutcome::Failed` + audit，不影响其它。

### 5.5 Authorization（§72-§76）

```text
trait ActionAuthorizationPolicy {
    fn authorize(&self, request: &ActionRequest, session: &SessionTrust) -> Decision;
}
enum SessionTrust { LocalDesktop, RemoteAuthenticated, RemoteUntrusted }
```

- `RemoteUntrusted` → 所有非 Read **disabled**（§73 fail-closed）。
- 当前 Tauri desktop 会话 = `LocalDesktop`（本地窗口 + 用户确认，§76）。
- HTTP server crate 不暴露写端点 → 远程天然无写入面。

### 5.6 Rate limit（§67）

- 每 target cooldown（默认 60s，配置化）；窗口内重复 → DENIED + reason `cooldown`。
- 全局上限：每个会话每 5 分钟最多 N 次 SYSTEM action（默认 5）。

### 5.7 Audit（§63-§66）

- 存储：`config/server_actions.db`（SQLite，复用 infra 模式；**不**引 Redis/PG，§111）。
- 保留：`max_entries`（默认 500）+ `retention_days`（默认 30），先到先裁（§112）。
- **不记录**（§65）：API key、完整日志、完整 prompt、secret 值。只存
  id/时间/session/action_type/target_id/risk/confirmed/result/duration/error_code。
- UI：Recent Actions 列表（§66）。

---

## 6. Track D · PersonalAgent 集成 + Server UI（Gates 7-8）

### 6.1 模块（§12）

`personal_ai/server.rs`：ModuleDescriptor(`server`) + 5 个 Read 工具 +
`ServerContextProvider`（§13/§83：compact 摘要，不含日志正文）+ `register_server`。
`PersonalAgent` **零改动**（§106）。

### 6.2 Context（§13/§83）

```
Server · Home
hostname=mac-studio platform=macOS 12.7 arm64 uptime=14d
cpu=18% mem=42% disks=[Macintosh HD 68% warn]
services: self-tools=Healthy, geo-explorer=Degraded(disk_usage>threshold)
apps: 3 registered
```

### 6.3 UI（§77-§82）

- 新页 `features/server/`：Dashboard（Health/CPU/Memory/Storage/Services/Applications/
  Recent Actions）+ `serverClient.ts` + `serverTypes.ts`（沿用 V6 client 模式，裸 invoke = 0）。
- 确认 UI：`ConfirmActionCard` —— 目标 / 影响 / 风险 / 过期倒计时；**不**是 "Confirm?"
  （§56/§82）；移动端全宽卡片。
- `AIPanel` 渲染 `ConfirmAction` action + `ActionOutcome` 结果块。
- `App.tsx` 导航 + 默认设置 `server` 段。

---

## 7. Track E · Automation（P1，§89-§96）

**时间不足则完全不实现**（§96）。实现时：
`AutomationRegistry { id, name, description, steps: Vec<RegisteredAction>, risk, allowed_targets }`；
step 只能是 RegisteredAction；`automation.list` Read、`automation.run` 取
最高 step risk 决定确认。

---

## 8. Security（Gate 9）

独立 security reviewer 检查清单：任意命令执行、参数注入、service-id 注入、
路径穿越、日志泄漏、SSRF、URL scheme、确认绕过、确认重放、TOCTOU、
audit 泄漏、LAN 不安全写。

**代码审计不变量**（§101，CI 可断言）：
- `grep -rn "sh\", \"-c\|bash\", \"-c\|zsh\", \"-c" crates apps` = 0
- `grep -rn "std::process::Command" crates/core/src crates/application/src` = 0
- `Command::new` 只出现在 `crates/infrastructure/src/server/**`，且 argv 全部字面量

---

## 9. Test Matrix（§97-§103）

| 面 | 用例 |
| --- | --- |
| Server 域 | get_status 有指标；provider 失败 → UNKNOWN/DEGRADED 不 panic；磁盘阈值 → warning |
| Registry | 注册服务可见；未注册 DENIED；注册 app 可见；未注册 app open DENIED；非法 URL 拒绝 |
| Logs | 注册源允许；任意路径拒绝；行数上限；字节上限；secret 脱敏；注入文本当数据 |
| Safe Action | Case 1-10 全（Read 无确认 / System 需确认 / 无确认拒绝 / 过期拒绝 / 重放拒绝 / target 不一致拒绝 / 未知服务拒绝 / 成功审计 / 失败审计 / 频率限制） |
| Shell safety | service id → registry 映射 → 固定 args；无 `-c` |
| macOS adapter | mock launchd status / restart success / restart error / missing service（不依赖真实 launchd） |
| UI | `tsc --noEmit` + `vite build` + reviewer/手动 QA |

---

## 10. Gate 顺序与 PASS 判据（§125）

| Gate | 内容 | PASS |
| --- | --- | --- |
| -1 | V6 Freeze | 工作树干净；508 基线绿 |
| 0 | 审计 + 本计划 | 落盘 |
| 1 | Server 契约 | `ServerStatus/SystemMetrics/StorageMetrics/HealthStatus/ServiceDescriptor/ApplicationDescriptor/ActionRequest/Confirmation/AuditEntry` 编译通过 |
| 2 | Metrics + Health | macOS 与 CI（Unknown 降级）都能出指标，不 panic |
| 3 | Service Registry + Logs | 只查注册服务 / 只读注册日志源 |
| 4 | App Registry | 只查看/打开注册 app |
| 5 | Safe Action + Confirmation | 无确认 = 不执行（硬 Gate） |
| 6 | Audit + Rate Limit | 所有 write attempt 有审计（含 DENIED/FAILED） |
| 7 | PersonalAgent 集成 | 可答状态/服务/日志摘要/应用列表；可提 restart 确认 |
| 8 | Server Dashboard | desktop + mobile 宽度可用 |
| 9 | Security Review | 独立 reviewer，无 HIGH/CRITICAL 未修 |
| 10 | Full Regression | workspace + tsc + build 全绿，V5/V6 不退化 |
| 11 | Docs | 3 份 V7 文档 + ADR |

---

## 11. Rollback

server 域独立（`core/src/server` + `application/src/server` +
`infra/src/server` + `personal_ai/server.rs` + `ui/features/server`）；
新增 `config/server_actions.db` 与 `settings.server`（serde default）。
PersonalAgent 唯一风险点是 `ToolRegistry` 的 `RiskPolicy`（默认值行为不变）——
删除 server 模块注册即可完全关闭 SYSTEM 通道。

## 12. P1 / 不做

Docker adapter、services.start/stop、Automation Registry、图表。
明确不做（§11）：MCP、Multi-Agent、arbitrary shell、SSH agent、docker exec、
文件写、包安装、系统更新、重启/关机、用户管理、chmod/chown、防火墙、
端口转发、路由器、智能家居。
