# V11 生产加固计划（Production Hardening Plan）

对应 Goal §47-§70 / §164-§168。本文件是审计结论 + 实施清单；
最终落地状态见 `docs/operations/PRODUCTION_V11.md`。

## 1. 生产拓扑（§48）

```text
Home macOS 12 Server
  │
  ├── SELF_TOOLS_HOME（默认 ~/Library/Application Support/self-tools）
  │     ├── config/    settings.json（含 LLM/Jev key；gitignored；0600）
  │     ├── data/      memory.db documents.db files.db server_actions.db
  │     │              history.db geography.db language.db travel.db dashboard.db
  │     ├── cache/     可重建索引 / 派生 enrichment
  │     ├── logs/      结构化日志（JSONL）
  │     ├── backup/    备份目标（manifest.json + 快照）
  │     └── runtime/   instance.lock / unclean-shutdown 标记
  │
  ├── self-tools backend（release binary，launchd 托管）
  ├── web frontend（static bundle + PWA assets）
  ├── local MCP（STDIO；仅本机）
  └── optional remote MCP（默认 OFF；需身份提供者）
```

`AppPaths`（`crates/core/src/operations/mod.rs`）统一 config/data/cache/logs/
backup/runtime，`SELF_TOOLS_HOME` 可覆盖；不硬编码任何个人目录。

## 2. 配置与 Secret（§50-§53）

- 统一入口：`AppSettings`（core）→ `config/settings.json`。
- 校验：`validate_startup`（`crates/core/src/operations/config.rs`）——
  非法端口 / 非 loopback 生产绑定 / 不可写目录 / 零或越界上限 / 越界阈值 /
  远程 MCP 无身份 / 缺失文件根 → **拒绝启动**（fail-closed）。
- Secret：LLM key、Jev key、OIDC 凭据、MCP 凭据只进 `settings.json`；
  前端响应、日志、git、debug dump 一律不出现（`redact_log_value` 双保险）。
- Safe Defaults：Remote MCP OFF、SYSTEM 确认 ON、Decision/Jev 失败回落开、
  Multi-Agent 有界（max_workers ≤ 32）、文件根显式配置。

## 3. 服务生命周期（§54-§57）

- launchd plist 生成（`scripts/launchd/`）：绝对可执行路径、显式
  `SELF_TOOLS_HOME`、`EnvironmentVariables` 引用env文件、KeepAlive 安全重启策略。
- 优雅退出：停止接受请求 → 取消有界 agent → 过期待确认工作 →
  flush audit → 关闭 DB → 关闭 MCP → 关闭 HTTP → 释放锁。
- 崩溃恢复：启动时检测 `runtime/unclean-shutdown`：
  未完成 AgentRun → `interrupted`；过期 confirmation ticket → `expired`；
  stale lock → 清理；partial index → 标记 rebuildable。

## 4. 数据存储盘点（§64-§70）

见 `docs/operations/BACKUP_RESTORE.md` 的 inventory 表（path/owner/purpose/
sensitive/backup-required/rebuildable/schema version）。

## 5. 韧性矩阵（§71-§75）

| 故障 | 期望行为 | 实现 |
| --- | --- | --- |
| LLM down | Documents/Files/Memory/Server/Global Search 仍工作 | provider fail → 受控错误；非 AI 路径不依赖 provider |
| Jev down | Rule fallback | `DecisionEngine` 强制回落（fallback 不可关闭） |
| Search down | degrade | per-source 失败进 `degraded_sources` |
| OIDC down | remote 拒绝（fail-closed） | identity provider 不可用 → 远程拒绝 |
| MCP failure | PersonalAgent 内部仍工作 | MCP 是 adapter， hub 不依赖它 |
| DB lock | 受控错误，不 panic | busy_timeout + 受控错误映射 |
| worker timeout | bounded | V9 gate6/gate9 已有 |
| bad document / bad file | 受控错误 | V6 已有 |
| backup unavailable | 报告失败，不影响服务 | BackupService 独立 |

## 6. 备份 / 恢复（§66-§70）

- 覆盖：Personal Memory、Documents 数据、Conversation、Study Boards、
  settings（脱敏导出说明）、其他不可重建用户数据。
- REBUILDABLE 标记：cache、index、derived enrichment。
- Manifest：timestamp / app version / schema versions / files / checksums。
- SQLite：禁止 `cp` 运行中的库；用安全 snapshot（VACUUM INTO / Backup API）。
- Restore drill：fixture → backup → 改 fixture → 恢复到隔离目标 → 校验。

## 7. 验收

- `docs/acceptance/V11_MORNING_ACCEPTANCE.md`：10 分钟验收路线。
- `docs/operations/V11_RELEASE_CHECKLIST.md`：发布前逐项确认。
