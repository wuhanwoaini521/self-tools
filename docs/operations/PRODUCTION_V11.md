# V11 Production · 运维手册（PRODUCTION_V11）

> 目标：self-tools 长期、安全、可恢复地运行在家庭 macOS 12 Server。
> 部署步骤见 `DEPLOY_MACOS12.md`；备份/恢复见 `BACKUP_RESTORE.md`；排障见 `TROUBLESHOOTING.md`。

## 1. 生产拓扑

```text
Home macOS 12 Server
 ├── launchd (ai.self-tools.server, KeepAlive + ThrottleInterval=10)
 │    └── self-tools backend (release binary；--mode production)
 ├── static frontend bundle (dist/) + PWA assets（manifest / sw.js / icons）
 ├── local MCP (STDIO；仅本机)
 └── optional remote MCP（默认 OFF；需身份提供者）

SELF_TOOLS_HOME（默认 ~/Library/Application Support/self-tools）
 ├── config/   settings.json（0600；含 LLM/Jev key；gitignored）
 ├── data/     业务库（SQLite）
 ├── cache/    可重建索引 / 派生数据
 ├── logs/     结构化日志
 ├── backup/   备份目标（manifest.json + 快照）
 └── runtime/  instance.lock / unclean-shutdown
```

路径全部经 `AppPaths` 解析（`crates/core/src/operations/mod.rs`），
`SELF_TOOLS_HOME` 可覆盖；**不硬编码任何个人目录**。

## 2. 配置与校验

启动时 `validate_startup`（`crates/core/src/operations/config.rs`）fail-closed：

| 类别 | 规则 |
| --- | --- |
| 端口 | 必须 1..=65535（0 非法）；`host:port` 结构 |
| 绑定 | 生产模式非 loopback 需 `SELF_TOOLS_ALLOW_REMOTE=1` |
| 目录 | data/config 必须可创建且可写（探针文件） |
| 上限 | `max_indexed_files` / `max_document_bytes` / `max_read_chars` / `max_workers` 非 0；`max_workers ≤ 32` |
| 阈值 | `confirmation_ttl_secs ∈ 30..=120`；cpu/memory/disk 比率 ∈ 0..=1；`jev_timeout_secs ∈ 1..=60` |
| 远程 MCP | `remote_enabled` 但无身份提供者 → 拒绝启动 |
| 文件根 | `file_roots` / `document_roots` 必须存在 |
| 决策模式 | `rule \| jev_shadow \| jev_active`；未配置 key → 强制 rule |

校验输出**不含** secret（测试 `violations_never_contain_secret_values` 固化）。

## 3. Secret 处理

| Secret | 位置 | 禁止 |
| --- | --- | --- |
| LLM key | `config/settings.json`（gitignored） | 前端响应 / 日志 / git / debug dump |
| Jev key | 同上；`jev_configured()` 只暴露布尔 | 同上 |
| OIDC 凭据 | 同上 | 同上 |
| MCP 凭据 | plist 引用的 env 文件（0600），不写进 plist | 同上 |

日志双保险：`StructuredLogEvent` 只接受事件标签/稳定错误码/计数值；
`redact_log_value` 对 key/token/secret/password/cookie/authorization 字段替换 `[REDACTED]`。

## 4. 健康与可观测

| 端点/对象 | 语义 |
| --- | --- |
| liveness | 进程响应（`HealthState::Alive`） |
| readiness | config / DB / ToolRegistry / 核心 store / 必需路径 |
| degraded | 外部依赖（LLM / Jev / Search / MCP remote）不可用 → DEGRADED，**不是** DOWN |
| HTTP 映射 | ready/alive/degraded = 200；down = 503 |

结构化日志字段：`timestamp_ms / level / component / request_id / session_id /
trace_id / source / event / duration_ms / result / fields`。

指标（`MetricsSnapshot`）：请求数、错误数、tool 调用与延迟、agent runs、
编排率、决策 provider 命中、Jev fallback、tokens、MCP 调用、SafeAction、
健康失败、备份结果。派生 `avg_tool_latency_ms` / `orchestration_rate`。

## 5. 生命周期

plist 生成（不自动安装）：

```bash
self-tools --launchd generate > ~/Library/LaunchAgents/ai.self-tools.server.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/ai.self-tools.server.plist
launchctl kickstart -k gui/$(id -u)/ai.self-tools.server   # restart
launchctl bootout gui/$(id -u)/ai.self-tools.server        # uninstall
```

优雅退出顺序（`ShutdownStage::all()`，固定 8 步）：
stop accepting → cancel agents → expire pending work → flush audit →
close databases → shutdown MCP → shutdown HTTP → release locks。

崩溃恢复（`StartupMarker`）：启动检测 `runtime/unclean-shutdown`：
未完成 AgentRun → `interrupted`；过期 confirmation → `expired`；
stale lock → 清理；cache/index → 标记 rebuildable。恢复报告进启动日志。

单实例锁（`InstanceLock`）：第二个实例拒绝启动（避免两个进程共开 SQLite）；
stale lock（pid 不可解析/不存在）自动接管。

## 6. 韧性矩阵

| 故障 | 行为 | 测试 |
| --- | --- | --- |
| LLM down | 受控错误；Documents/Files/Memory/Server/Search 仍工作 | `llm_down_returns_controlled_error_not_panic` |
| LLM hang | 受预算/超时约束返回 | `llm_hang_is_bounded_by_worker_timeout` |
| Jev down | Rule 接管 + `fallback` 遥测 | `jev_down_falls_back_to_rule_and_request_succeeds` |
| Jev 恶意选择 | worker clamp → Direct | `jev_hostile_choice_cannot_escalate` |
| Search 源 down | 只降级该源 | `search_source_down_degrades_only_that_source` |
| 预算耗尽 | 安全停止（`budget_exhausted`） | `budget_exhausted_stops_orchestration_safely` |
|  unclean 退出 | 恢复分类 + 可重建标记 | `crash_marker_survives_unclean_exit_and_recovers` |
| 备份目标不可用 | 受控错误，服务不受影响 | `backup_failure_does_not_affect_service` |
| MCP 未授权 | fail-closed | `mcp_unauthorized_is_fail_closed` |
| 超大请求 | 截断 256 字符 | `bad_document_row_is_a_controlled_error` |
| agent 超时 | 有界返回（timed_out） | `agent_timeout_marks_run_timed_out` |

## 7. 安全默认（§52）

Remote MCP OFF · SYSTEM 确认 ON · 文件任意访问不可能 · Multi-Agent 有界
（depth=1、max_workers≤32、max_agents/max_steps/max_tokens/max_duration 全部强制）·
Decision fallback 开（不可关） · Jev 失败回落开（不可关） · 远程写 fail-closed。
