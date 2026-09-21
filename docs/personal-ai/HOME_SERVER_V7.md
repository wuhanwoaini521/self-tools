# SELF-TOOLS V7 · HOME SERVER & SAFE AUTOMATION — 终版架构

> 状态：✅ 已实施（Gates -1–10 PASS，Gate 11 文档产出中；测试 **606 / 606**，
> 基线 V6 冻结 508）。计划见
> [`HOME_SERVER_V7_PLAN.md`](HOME_SERVER_V7_PLAN.md)；
> 状态见 [`V7_OVERNIGHT_STATUS.md`](V7_OVERNIGHT_STATUS.md)；
> 决策记录见 [`ADR-006-safe-actions.md`](../architecture/ADR-006-safe-actions.md)。

---

## 0. 一句话

给 Personal AI 加一个**只读优先**的「家庭服务器」模块：能看懂系统指标、注册服务与
应用状态、读取有界脱敏日志；唯一写操作（重启已注册服务）必须走
「请求 → 授权 → 用户确认 → 执行 → 审计」，**模型永远不执行系统操作**。

```text
BEFORE: AI 对家庭服务器一无所知；任何系统操作都不存在受控入口
AFTER : server.get_status / services.get_logs / apps.open（READ，自动）
       services.restart → confirmation_required → 用户确认 → 执行 → 审计
```

---

## 1. V6 Freeze（Gate -1）

| 项 | 结果 |
| --- | --- |
| 工作树 | V6 全部改动已作为 5 个 checkpoint 提交（`84ac5a7`…`3320bbe`），`git status` 干净 |
| 基线回归 | `cargo test --workspace` = **508 passed / 0 failed** |
| V6 资产 | Memory / Documents / Files / Knowledge Retrieval / 安全修复（Gate 8 的 8 项）均在 |
| V7 起点 | HEAD `3320bbe`，干净工作树 |

## 2. 分层与依赖（不变式保持）

```text
core            server/{mod,metrics,health,registry,logs,action}.rs（纯契约 + 纯函数）
application  →  core only（端口 + 域服务 + SafeActionService + personal_ai/server 模块）
infrastructure →  core（macOS 指标 / launchd / 日志尾读 / SQLite 审计）
apps/desktop   =  唯一组合根（适配器 + 8 条命令 + build_hub 装配）
```

实测：`grep devtoolbox_infrastructure crates/application/src` = 0；
`std::process` 在 core/application = **0**（仅注释提及）；
`Command::new` 只出现在 `crates/infrastructure/src/server/**`（3 处，全部固定
executable + 字面量 argv）。

## 3. Track A · Server 域与指标（Gates 1-2）

| 契约 | 位置 | 要点 |
| --- | --- | --- |
| `SystemMetrics` / `CpuMetrics` / `MemoryMetrics` | `core/src/server/metrics.rs` | 任何字段不可用都是合法状态（`None` ≠ 0.0） |
| `StorageMetrics` / `VolumeKind` | 同上 | 临时 / 虚拟卷默认隐藏（§17）；`tightest_storage()` 回答「哪个盘快满了」 |
| `HealthStatus` / `HealthReason` / `Thresholds` | `core/src/server/health.rs` | **四态**（Healthy/Degraded/Unhealthy/Unknown）；原因可解释；阈值配置化且 clamp |
| `evaluate_storage` / `evaluate_memory` | 同上 | 纯函数，可单测；无可见卷 → `Unknown`（不乐观） |

application 侧：`SystemMetricsProvider` / `ServiceProbePort` /
`ApplicationProbePort` / `LogTailPort` 四端口 + `ServerService`
（`status()` 产 compact summary：hostname / uptime / CPU / memory /
最紧卷 / health / services / apps）。

**平台隔离**：macOS 用 `sysctl -n hw.memsize` / `sysctl -n kern.boottime` /
`df -k -P` / `sw_vers -productVersion`（全部固定参数）；Linux 读
`/proc`；其它平台返回 `Unknown`。**部分指标缺失不 panic**（Gate 2 PASS）。

## 4. Track B · Service / App Registry + Logs（Gates 3-4）

| 契约 | 要点 |
| --- | --- |
| `ServiceDescriptor` | id / display_name / description / provider_type / **provider_ref** / health_check / log_sources / **allowed_actions** / tags |
| `ApplicationDescriptor` | id / name / description / url / health_url? / service_id? / category / tags；`is_valid()` 强制 http(s) URL |
| `is_valid_id` | `^[a-z0-9][a-z0-9._-]{0,63}$` —— 契约层封死 `foo; rm -rf /`、`--label`、换行 |
| `is_http_url` | 白名单 http/https；拒绝 `javascript:` / `file:` / `data:` / `shell:` |

- **Registered Only**：`resolve()` 未注册 → `unknown_service` / `unknown_app`；
  id 形态非法 → `invalid_service_id` / `invalid_app_id`（稳定码，不回显输入）。
- **模型可见面只有 id**：`provider_ref`（launchd label）由 infrastructure 映射，
  工具 schema 里不存在该字段。
- **allowed_actions 白名单**：未声明 `restart` → `action_not_allowed`；
  空白名单 → 全部拒绝。
- **Logs**：来源只能是 `descriptor.log_sources`；三硬限制
  （lines ≤ 2_000 / bytes ≤ 1 MiB / age ≤ 86_400，默认 200 / 64 KiB / 300s）；
  infra 侧 `contains_traversal` 纵深防御；**脱敏在 application**
  （`LogRedactor`：结构化字段优先 → 裸 bearer/连接串 → V6 secret 门兜底）；
  **不可信标记**（`untrusted: true` + prompt 规则）。

## 5. Track C · Safe Action 层（Gates 5-6，V7 核心）

```text
plan(request, trust)
  ├─ 1) registry.resolve(target)         → 未注册 / id 非法 → Denied
  ├─ 2) policy.authorize(action, trust)  → RemoteUntrusted → Denied（fail-closed）
  ├─ 3) cooldown / session limit         → 超限 → Denied
  └─ 4) risk.requires_confirmation()
        ├─ false → execute_now（V7 暂无此类写操作）
        └─ true  → issue Confirmation（一次性 + TTL + fingerprint）

confirm_and_execute(confirmation_id, request)
  ├─ 票据存在？                       否则 Denied
  ├─ state == Pending？               否则 Denied（重放）
  ├─ now < expires_at？               否则 Expired
  ├─ fingerprint == request.fingerprint()？  否则 Denied（TOCTOU / 确认 A 执行 B）
  ├─ 标记 Consumed（执行前）
  ├─ control.restart(label)            → Success / Failed
  └─ audit（含 DENIED / EXPIRED / FAILED / SUCCESS）
```

| 机制 | 实现 |
| --- | --- |
| 闭合操作集 | `RegisteredAction` 枚举（当前仅 `RestartService`）——**AST 层面无命令字符串** |
| 指纹 | `action_type\|target_id\|canonical_json(action)\|risk`（键排序规范化） |
| 一次性 | `Consumed` 在执行前标记 → 重放 Denied |
| TTL | `settings.server.confirmation_ttl_secs`，默认 60（30–120） |
| 授权 | `ActionRiskPolicy` trait + `DefaultActionRiskPolicy`（RemoteUntrusted 拒写） |
| 频率 | 每目标 cooldown（默认 60s）+ 每会话上限（默认 5） |
| 审计 | `config/server_actions.db`（SQLite）；只存 id/时间/session/action_type/target_id/risk/confirmed/result/duration/error_code；**不含正文、密钥、日志、prompt**；保留 500 条 / 30 天，先到先裁 |
| 工具面 | `services.restart` 注册为 **Read** 风险的工具，本体只签发票据并回 `confirmation_required` + `Action::ConfirmAction`；SYSTEM 语义在票据里，由桌面命令强制 |

## 6. Track D · PersonalAgent 集成 + UI（Gates 7-8）

- **零核心改动**：`register_server` 与 V5/V6 模块同构；
  `grep 'if module == "server"' agent.rs` = 0。
- 14 个工具：`server.{get_status,get_cpu,get_memory,get_storage,get_health}`、
  `services.{list,get,get_status,get_logs,restart}`、
  `apps.{list,get,get_status,open}`。
- `ServerContextProvider`：compact 摘要（hostname / 平台 / uptime / CPU / 内存 /
  最紧卷 / health 原因码 / 服务 id / 应用 id）——**不含日志正文**（测试锁定）。
- **UI**：`features/server/`（Dashboard + 确认卡 + 审计列表），响应式
  （≤640px 单列、按钮全宽）；AI Panel 内的 SYSTEM 确认卡同样显示
  目标 / 风险 / 影响 / 过期倒计时；`SettingsDialog` 的注册表编辑
  （服务 / 应用 / TTL / cooldown / 上限）+ id 与 URL 形态校验。

## 7. 安全矩阵（Gate 9）

| 问题 | 答案 | 证据 |
| --- | --- | --- |
| AI 可执行任意 shell？ | **NO** | 无 shell/exec/run 工具；`RegisteredAction` 闭合枚举；`grep '"sh","-c"'` = 0 |
| AI 可传 arbitrary args？ | **NO** | argv 全字面量；label 只来自注册表 provider_ref |
| AI 可操作未注册 service？ | **NO** | registry + plan 双层拒绝；adapter 对未映射 id 也拒 |
| AI 可读任意 log path？ | **NO** | 只接受 descriptor 内的 log_source；traversal 双侧防御 |
| AI 可打开任意 URL？ | **NO** | `apps.open` 只接 app_id；URL 白名单 http/https |
| SYSTEM 可绕过确认？ | **NO** | `confirm_and_execute` 是唯一写入口；票据四步校验 |
| Confirmation 可重放？ | **NO** | 执行前标记 `Consumed`；重放 → Denied |
| 确认后参数可改变？ | **NO** | fingerprint 覆盖 type/target/参数/risk；不一致 → Denied |
| Remote untrusted 可写？ | **NO** | `SessionTrust::RemoteUntrusted` → `untrusted_session`；HTTP server 无写端点 |
| 日志注入可触发工具？ | **NO** | `untrusted` 标记 + prompt 规则；测试断言零副作用 |

## 8. 测试（606）

| 面 | 数量 | 覆盖 |
| --- | --- | --- |
| core server | 24 | id 校验 / 指标 / 四态健康 / 阈值 clamp / registry / URL 白名单 / 指纹 / 一次性 / 过期 / 授权 / 日志限制 |
| application server 域 | 29 | 指标降级 / 阈值告警 / 注册表过滤 / 脱敏（结构化 + 兜底 + 占位符）/ **Safe Action 全部 10 例** |
| application server 模块 | 17 | 模块面 / 无 shell 工具 / context 不含日志 / READ 工具 / 日志有界脱敏不可信 / 注入零副作用 / apps 白名单 / restart 零执行 + 票据 / 未注册拒绝 / 指纹不符拒绝 |
| infrastructure server | 20 | 平台采样不 panic / `df -P` 解析（含 `map auto_home`）/ 卷分类 / launchd 状态解析 / 探活降级 / 日志尾读边界 / 审计 round-trip + 保留策略 |
| desktop 组合根 | 4 | 空注册表 fail-closed / 确认流 / 未信任会话拒绝 / 未知服务拒绝 |
| 既有（V5/V6） | 508 | 无退化 |

## 9. 已知限制（P1）

- `services.start` / `services.stop` 未实现（V7 只做 restart，§34）。
- Docker adapter 未实现（§30 Optional；且禁 `docker exec`）。
- macOS 的 CPU 使用率 / 内存 used 需要 `host_statistics`（C API），当前为
  `None` → 健康评估按 `Unknown` 降级，**不编造**（§23「非必要不引入依赖」）。
- Automation Registry（Track E）未实现（§96：时间不足则完全不做）。
- 无完整身份体系：远程写操作默认 disabled（§73/§75）。

## 10. Rollback

删除 `crates/{core,application,infrastructure}/src/server`、
`personal_ai/server.rs`、`apps/desktop/src/{server,server_adapters}.rs`、
`ui/src/features/server` 与 `AppState.server` 即可完全关闭；`settings.server`
走 `serde(default)`；`config/server_actions.db` 独立可删。
