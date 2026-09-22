# 备份与恢复（BACKUP_RESTORE V11）

> 对应 Goal §64-§70 / §160。实现：`crates/core/src/backup/`（契约）、
> `crates/application/src/backup/`（编排）、`crates/infrastructure/src/backup/`（安全快照 + 演练）。

## 1. 数据存储盘点

| 存储 | 路径（相对 `SELF_TOOLS_HOME`） | 内容 | sensitive | backup required | rebuildable |
| --- | --- | --- | --- | --- | --- |
| `memory.db` | `data/` 或 `config/`（桌面组合根） | Personal Memory | **是** | **是** | 否 |
| `documents.db` | 同上 | 文档索引 + 分块 | **是** | **是** | 部分（可从源重建） |
| `files.db` | 同上 | 文件索引 | 否（仅元数据） | 否 | **是** |
| `server_actions.db` | 同上 | SafeAction + audit | **是** | **是** | 否 |
| `study_boards.db` | `config/` | 学习板笔迹 + 快照 | **是** | **是** | 否 |
| `conversations.db` | `data/` | 会话历史（**不是** Memory） | **是** | **是** | 否 |
| `history.db` / `geography.db` / `language.db` / `travel.db` / `dashboard.db` | `config/` | 模块业务库 | 部分 | **是** | 否 |
| `history.duckdb` | `history-data-pipeline/dist/` | History 只读库 | 否 | 否 | **是**（pipeline 重建） |
| `cache/` `index/` | `cache/` | 派生缓存 / 索引 | 否 | 否 | **是** |

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
- Manifest（`backup/manifest.json`）：`created_at` / `app_version` /
  `schema_versions`（每库版本）/ `entries[]`（path, kind, sha256, bytes,
  sensitive, rebuildable）/ `note`。
- 单源失败不中止整轮：失败原因进 `manifest.note` 与失败项，其余源继续。
- 大库分块读摘要（64KB），不进内存。
- 目标目录幂等创建；同名旧快照先清理。

## 3. 恢复

```rust
let report = service.restore(backup_dir, isolated_dest)?;
assert!(report.integrity_ok);
```

规则：

- 只恢复 manifest 列出的条目；忽略目录里的额外文件。
- 每条恢复前校验 **sha256 + 字节数**（双重复核）；不匹配 → 拒绝且不落盘。
- 路径封闭：拒绝 `../`、绝对路径、逃逸 dest 的条目（`is_safe_backup_path`）。
- staging + 原子 rename；写盘失败清理残留。
- SQLite 恢复后跑 `PRAGMA integrity_check` + 摘要复核。
- **不覆盖真实用户数据做测试**：恢复目标永远是隔离目录。

## 4. 恢复演练（强制）

`crates/infrastructure/src/backup/drill.rs`（真实 SQLite/DuckDB + tempfile）：

```bash
cargo test -p devtoolbox-infrastructure --lib backup::drill
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
