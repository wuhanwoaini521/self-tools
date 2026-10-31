# 备份与恢复（BACKUP_RESTORE V11）

> 对应 Goal §64-§70 / §160。实现：`crates/core/src/backup/`（契约）、
> `crates/application/src/backup/`（快照编排）、`crates/infrastructure/src/backup/`（安全快照 + 演练）、
> `crates/runtime/src/backup.rs`（应用备份管理、进程锁与启动恢复）。

## 1. 数据存储盘点

| 存储 | 路径（相对 `SELF_TOOLS_HOME`） | 内容 | sensitive | backup required | rebuildable |
| --- | --- | --- | --- | --- | --- |
| `memory.db` | `config_dir/` | Personal Memory | **是** | **是** | 否 |
| `documents.db` | `config_dir/` | 文档索引 + 分块 | **是** | **是** | 部分（可从源重建） |
| `files.db` | `config_dir/` | 文件索引 | 否（仅元数据） | 是 | **是** |
| `server_actions.db` | `config_dir/` | SafeAction + audit | **是** | **是** | 否 |
| `study_boards.db` | `config_dir/` | 学习板笔迹 + 快照 | **是** | **是** | 否 |
| `conversations.db` | `config_dir/` | 会话历史（**不是** Memory） | **是** | **是** | 否 |
| `dashboard.db` | `config_dir/` | RSS feeds / articles | **是** | **是** | 否 |
| `news.db` | `config_dir/` | News sources / articles | **是** | **是** | 否 |
| `travel.db` / `language.db` / `geography.db` / `learning.db` | `config_dir/` | 模块业务库 | **是** | **是** | 否 |
| `history_enrichment.db` | `config_dir/` | 历史资料富化状态 | **是** | **是** | 否 |
| `history-data-pipeline/dist/history.duckdb` | pipeline 资源目录 | History 只读库 | 否 | 否 | **是**（pipeline 重建） |
| `settings.json`、用户选择的文件根目录、下载语料、`cache/`、`index/` | 多位置 | 设置/外部数据/派生数据 | 可能含秘密 | **不纳入** | 视来源而定 |

> Conversation ≠ Memory：会话历史是**会话**唯一 Source of Truth；
> 记忆服务 MUST NOT 读取它（契约层 doc + 测试双重保证）。

## 2. 备份

```rust
let service = BackupService::new()
    .with_source(Arc::new(SqliteBackupSource::new("memory.db", memory_db_path, sensitive = true)))
    // ... 其它源
    ;
let manifest = service.backup(dest_dir, app_version, note)?;
```

规则：

- **禁止 `cp` 运行中的 SQLite**：使用 `VACUUM INTO`（SQLite）或
  `ATTACH + COPY FROM DATABASE`（DuckDB）。裸 copy 可能复制到写事务中间态，
  产生损坏副本 + 错误的安全感。
- Manifest（`config_dir/backup/<UUID>/manifest.json`）：`format_version` / `complete` /
  `created_at` / `app_version` / `schema_versions`（每库版本）/ `entries[]`（path,
  kind, sha256, bytes, sensitive, rebuildable）/ `note`。只有 `format_version=1`
  且 `complete=true` 的清单可以用于应用级恢复。
- 单源失败不中止整轮：失败原因写入本地 manifest；备份列表只返回用户备注，不回显源路径。
- 大库分块读摘要（64KB），不进内存。
- 目标目录幂等创建；同名旧快照先清理。

## 3. 恢复

```rust
let report = service.restore(backup_dir, isolated_dest)?;
assert!(report.integrity_ok);
```

规则：

- 只替换 manifest 列出的条目；数据库目录中的其它文件忽略。allowlist 中未列入 manifest 的数据库保持原样，因为该备份只记录拍摄快照时存在的库。
- 每条恢复前校验 **sha256 + 字节数**（双重复核）；不匹配 → 拒绝且不落盘。
- 路径封闭：拒绝 `../`、绝对路径、逃逸 dest 的条目（`is_safe_backup_path`）。
- staging + 原子 rename；写盘失败清理残留。
- SQLite 恢复后跑 `PRAGMA integrity_check` + 摘要复核。
- **不覆盖真实用户数据做测试**：通用 restore 的目标永远是隔离目录。

## 3.1 应用级恢复与 HTTP 合约

应用级备份只覆盖上表中的 app-owned SQLite allowlist。`settings.json`（可能含 provider secrets）、
用户选择的文件根目录、下载语料、cache/index 和随应用分发的只读 History DuckDB 均不进入备份。
每个数据库通过 SQLite `VACUUM INTO` 快照；备份文件保存在 `config_dir/backup/<UUID>/`，目录权限为
`0700`、文件权限为 `0600`。

本机 HTTP server 提供稳定的 `/api/v1` 合约：

| 方法与路径 | 请求 / 响应 | 行为 |
| --- | --- | --- |
| `GET /api/v1/backups` | `{ "backups": [...] }` | 列出 manifest 摘要 |
| `POST /api/v1/backups` | `{ "note": "..." }` → `201` | 立即创建快照 |
| `GET /api/v1/backups/restore-status` | 状态 DTO | 查询待执行或最近一次恢复 |
| `POST /api/v1/backups/{id}/restore` | `{ "confirmation": "apply_on_next_clean_startup" }` → `202` | 校验并暂存恢复 |

这些 API 沿用 server 的 loopback bind、`Host` 校验和 unsafe request `Origin` 校验；服务没有用户认证，也不启用 CORS，
因此不得暴露给 LAN/WAN 或由远程代理转发。恢复请求必须显式确认。成功暂存只返回 `pending_restart`，不会改写当前运行中的数据库。

启动前，desktop 与 server 都先获取同一数据目录的进程锁。若所有使用该目录的进程已退出，下一次干净启动持有排他锁，
重新验证备份到隔离目录，创建完整的自动安全快照，再安装数据库。启动锁在每个 app 实例退出前保持；若其它实例仍在使用目录，
带待恢复标记的新启动会拒绝打开数据库，并提示先关闭所有实例。安装期间的 journal 会区分 `applying` 与 `committed`：
启动中断时按安全快照恢复原有数据库集合及内容；常规恢复只替换备份中列出的数据库，未列出的 allowlist 数据库保持现状。提交完成后保留安全快照供人工回滚。损坏或不支持的请求不会被应用。

## 4. 恢复演练（强制）

`crates/infrastructure/src/backup/drill.rs`（真实 SQLite/DuckDB + tempfile）：

```bash
cargo +1.95.0 test -p devtoolbox-infrastructure --lib backup::drill --locked
cargo +1.95.0 test -p devtoolbox-runtime --lib backup::tests --locked
cargo +1.95.0 test -p devtoolbox-server --bin devtoolbox-server backup_api::tests --locked
```

覆盖：

| 演练 | 断言 |
| --- | --- |
| SQLite backup → 改数据 → 恢复 | 数据 + schema 完整；`integrity_check` ok |
| 快照不污染活动库 | 活动库仍可写；第二次快照读到新行 |
| 缺失源 | 受控失败（不是 panic） |
| 篡改快照 | 拒绝恢复且不写入 |
| 路径逃逸 | 拒绝（`../` / 绝对路径） |
| DuckDB 快照 | 可打开、可查询 |

## 5. 建议节奏

| 频率 | 动作 |
| --- | --- |
| 每日 | `service.backup()` 到 `backup/daily/`（保留 7 份滚动） |
| 每周 | 演练：备份 → 恢复到隔离目录 → `integrity_check` |
| 升级前 | 手动全量备份 + 记录 manifest 的 `app_version` |
| 灾难后 | 从备份恢复 → 启动 → 检查 readiness 页 |

## 6. Rebuildable

`cache/`、`index/`、`history.duckdb`（可由 `history-data-pipeline` 重建）、
`files.db`（可由文件系统重新扫描）标记 REBUILDABLE——丢失不致命，
但恢复后需触发重建（启动时 `rebuildable_artifacts` 会列出）。
