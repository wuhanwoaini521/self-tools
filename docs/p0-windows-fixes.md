# P0 / L0 修复记录（Windows 基线）

- 日期：2026-11-18
- 起点：`git pull` 到 `b2b88c6`（落后 186 个提交）+ submodule `history-data-pipeline` → `e4518b7`
- 目标：把"新克隆在本机能不能编译、能不能启动、数据是不是同一份"这三件最底层的事修到绿
- 结论：**4 个 L0 问题，3 个已改代码，1 个是构建产物缺失（按文档重建即可）**

---

## L0-1 `CXXFLAGS` 用 GCC 语法喂给 MSVC，Windows 上整个 workspace 编译不了

- **现象**
  ```
  error: failed to run custom build command for `libduckdb-sys v1.10505.0`
    error occurred in cc-rs: command did not execute successfully (status code exit code: 2):
    "…\cl.exe" … "-std=c++17" …
    cl: warning D9002: 忽略未知选项"-std=c++17"
  ```
- **根因**：`.cargo/config.toml` 写了全局 `CXXFLAGS = "-std=c++17"`。`cl.exe` 只认 `/std:c++17`；
  duckdb（bundled）是唯一使用 C++ 的依赖，于是所有 Windows 构建都卡在这一步。
- **为什么 Linux/macOS 发现不了**：那边 `-std=c++17` 正好是 GCC/Clang 的写法。
- **关键细节**：cc-rs 的查找顺序是 `CXXFLAGS_<target>` → `CXXFLAGS_<target_u>` → `HOST_CXXFLAGS` → `CXXFLAGS`，
  并且 `envflags()` 会把**命中的多个变量追加**在一起。所以不能"加一个 MSVC 变量、保留通用变量"，
  必须把通用 `CXXFLAGS` 删掉，按目标三元组分别声明。
- **修复**：`.cargo/config.toml` 改为按目标声明（MSVC 用 `/std:c++17`，其余用 `-std=c++17`）。
- **验证**：`cargo build -p devtoolbox-infrastructure` → `EXIT=0`，4m06s（含 duckdb 全量 C++ 编译）。

```toml
[env]
CXXFLAGS_x86_64_pc_windows_msvc = "/std:c++17"
CXXFLAGS_aarch64_pc_windows_msvc = "/std:c++17"
CXXFLAGS_x86_64_unknown_linux_gnu = "-std=c++17"
CXXFLAGS_aarch64_unknown_linux_gnu = "-std=c++17"
CXXFLAGS_x86_64_apple_darwin = "-std=c++17"
CXXFLAGS_aarch64_apple_darwin = "-std=c++17"
```

---

## L0-2 网页端与桌面端读的是两套数据目录

- **现象**：`scripts/self-tools-web.sh` 末尾打印「数据目录：$REPO_ROOT/config（与桌面开发模式共用）」，
  但它启动服务时既没传 `--data-dir`、也没设 `SELF_TOOLS_DATA_DIR`；
  而 `apps/server/src/main.rs` 的默认值是 `apps/desktop/config`。
  桌面端在 dev 构建下用 `<仓库根>/config`（`project_config_directory`）。
  → **网页端配好的 AI provider、学习进度，桌面端看不到，反之亦然。**
- **为什么这是"设计不一致"而不是"猜的"**：`docs/operations/BACKUP_RESTORE.md` 明确写
  「desktop 与 server 都先获取**同一数据目录**的进程锁」，说明共享同一个目录是既定设计。
- **修复**
  1. `apps/server/src/main.rs`：`DEFAULT_DATA_DIR` 由 `apps/desktop/config` 改为 `config`；
     并让服务端也认 `SELF_TOOLS_HOME`（优先级：`SELF_TOOLS_DATA_DIR` > `SELF_TOOLS_HOME/config` > `config`），
     与桌面端 `select_config_directory` 的语义对齐（含 V11 的 home 布局）。
  2. `scripts/self-tools-web.sh`：显式 `export SELF_TOOLS_DATA_DIR="${SELF_TOOLS_DATA_DIR:-$REPO_ROOT/config}"`，
     不再依赖默认值。
- **验证**：见下节「实测」——服务端启动后 `config/` 里能看到桌面端写入的库，而不是新建 `apps/desktop/config`。

---

## L0-3 迁移报告里的路径在 Windows 上是反斜杠

- **现象**：`cargo test -p devtoolbox-infrastructure` 中
  `legacy_config_migration::tests::media_import_is_file_by_file_and_never_overwrites` 失败：
  `assertion failed: report.skipped.contains("language/nce/lesson-1/audio.mp3")`
- **根因**：`copy_media_tree` 用 `relative.to_string_lossy()` 记报告，Windows 得到 `language\nce\lesson-1\audio.mp3`。
  同一份报告会被写日志、给前端展示，分隔符随平台变化会让下游比较/展示不一致。
- **项目内既有约定**：`crates/application/src/files/service.rs:164`、`crates/infrastructure/src/workspace_scanner.rs:87`
  都已经是 `.replace('\\', "/")`——`legacy_config_migration.rs` 是唯一的例外。
- **修复**：新增 `report_relative(&Path) -> String`（统一 `/`），`copied` / `skipped` 三处写入改用它。
- **验证**：该测试转绿（见下节）。

---

## L0-4 `history.duckdb` 是旧产物，5 个 History 语义层测试失败

- **现象**
  ```
  history::duckdb::semantic_tests::semantic_repository_reads_dataset_totals
    panicked: 章节级证据至少 120，当前 26
  …::semantic_repository_reads_curated_story_flow_and_calibration_evidence
    left: 0, right: 7
  ```
- **根因**：`history-data-pipeline/dist/` **未被 git 跟踪**，本机那份还是 9 月 5 日（submodule `32d5d61`）时构建的；
  submodule 已更新到 `e4518b7`，数据契约（章节证据、curated story）变了，旧库不满足新断言。
- **修复（不是改代码）**：用管线重建
  ```bash
  cd history-data-pipeline
  .venv/Scripts/python.exe -m src.history_data_pipeline backbone build
  ```
  产物 `dist/history.duckdb`（12.07 MB，2026-11-18 重建）。
- **对使用者的影响**：`git pull` 或换机后**必须**跑一次这条命令，否则桌面端 History 会降级为
  "数据未生成"卡片、这 5 个测试会红。README 里已提到该命令，但**没有和"拉取 submodule"绑在一起**——
  建议在 P1 里把这条写进 `mise run setup` / README 的"拉取后必做"。
- **验证**：重建后测试转绿（见下节）。

---

## L0-5 备份/恢复在 Windows 上整体不可用（目录 fsync + 文件 fsync 都失败）

- **现象**：`cargo test -p devtoolbox-runtime --lib` 里 4 个 backup 用例失败，两种错误交替出现：
  ```
  create backup: "sync directory C:\...\backup\<uuid>: 拒绝访问。 (os error 5)"
  create backup: "sync backup file: 拒绝访问。 (os error 5)"
  ```
- **根因 A（目录）**：`sync_directory` 用 `File::open(dir).sync_all()` 做目录 fsync。
  Windows 上普通 `File::open` 打开目录直接返回 `ERROR_ACCESS_DENIED(5)`，`FlushFileBuffers` 对目录句柄也不支持。
- **根因 B（文件）**：`restrict_tree_files`、恢复数据库两处用 `File::open(file).sync_all()`。
  Windows 的 `FlushFileBuffers` **要求句柄带写权限**，只读句柄返回 `ERROR_ACCESS_DENIED(5)`；
  Unix 允许对只读 fd 做 fsync，所以 Linux CI 完全看不出来。
- **影响**：备份、恢复、启动清理三条路径全部经过这两个函数 → **Windows 上备份功能等于没有**。
- **修复**
  - `sync_directory`：unix 保持原实现；Windows 退化为无操作（目录项持久化交给 `CreateDirectory`/`Rename`），并注明原因。
  - 新增 `sync_file(path)`：用 `OpenOptions::new().write(true)` 打开再 `sync_all()`，替换两处只读句柄同步。
- **旁证**：同文件的 `replace_file` 早就写了 `#[cfg(windows)] remove_file + rename`——说明 Windows 差异是已知的，
  只是 fsync 这两处漏了。
- **验证**：`cargo test -p devtoolbox-runtime --lib`（见下节）。

## L0-6 `cargo clippy -D warnings` 在 Windows 上必然失败（只在 unix 分支用到的 `mut` / 参数）

- **现象**：`cargo clippy --workspace --all-targets --all-features -- -D warnings` → 3 个 error：
  - `backup.rs:1140 variable does not need to be mutable`（`DirBuilder` 的 `mut` 只在 unix 的 `mode()` 用）
  - `backup.rs:1227 / 1236 unused variable: path`（`restrict_directory` / `restrict_file` 只在 unix 分支用参数）
- **影响**：仓库自带的 `mise run check`（= fmt + check + clippy -D warnings）**在 Windows 上无法通过**，
  也就是说 Windows 开发者的本地门禁形同虚设。
- **修复**：`#[cfg_attr(windows, allow(unused_mut))]`；两个 `restrict_*` 加 `#[cfg(windows)] let _ = path;`。
- **验证**：同上。

---

## 实测（本节数字为最终验证）

| 项 | 结果 |
|---|---|
| `cargo build -p devtoolbox-infrastructure`（修 CXXFLAGS 后） | ✅ EXIT=0，4m06s（含 duckdb C++ 全量） |
| `cargo test --workspace`（拉取后、修复前） | ❌ 1225 通过 / 7 失败：5 个 History（旧库缺列）+ 1 个迁移分隔符 + 1 个 personal_ai 401 |
| `cargo test --workspace`（修复后） | ✅ **1356 通过 / 0 失败**，21 个测试套件 |
| `cargo test -p devtoolbox-runtime --lib` | ✅ 17 通过 / 0 失败（修复前 13/4） |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | ✅ 0 error（40.84s） |
| `rustfmt --check`（本次改动的 4 个 .rs） | ✅ 全部合规 |
| `cargo fmt --all -- --check` | ⚠️ 仓库既有漂移（如 `crates/infrastructure/src/learning/store.rs:1229`），与本次改动无关，未擅自全仓重排 |
| 前端 `tsc --noEmit && vite build` | 见运行实测节 |
| 服务端启动 + HTTP 冒烟 | 见运行实测节 |

### 运行实测（服务端 + 网页端，真实数据）

| 步骤 | 结果 |
|---|---|
| `target/debug/devtoolbox-server.exe`（无参数，仓库根） | ✅ 监听 `127.0.0.1:8080`；数据写进 `config/`（新增 `study_boards.db` / `server_actions.db`），**没有**创建 `apps/desktop/config` → L0-2 生效 |
| `GET /health` | ✅ `{"status":"ok"}` |
| `GET /api/v1/history/home` | ✅ 返回真实时期（夏/商…，含 `event_count`），读的是重建后的 DuckDB |
| `GET /api/v1/news/sources` | ✅ `health: healthy`，源状态 `ok`，`last_updated` 有值 |
| `GET /api/v1/news/headlines` | ✅ 自动抓取后返回真实条目（例：IT之家「微软启动 AI 透明度计划…」） |
| 网页端 `127.0.0.1:1420/#history` | ✅ 渲染 31 个时期时间轴（浏览器 → Vite 代理 → axum → DuckDB 全链路） |
| 网页端 `#news` | ✅ 先显示「正在抓取最新新闻…」，随后出列表；空态/加载态是诚实文案，不是白屏 |

> 注：`mise` 未安装在本机，所以 `mise run data` 这个任务本身没被 mise 执行过；
> 我验证的是它调用的底层命令 `uv run history-data backbone build`（可正常打印帮助并完成构建）。

### 各 crate 分布（修复后）

| crate | 测试数 |
|---|---|
| devtoolbox-core | 325 |
| devtoolbox-application | 616 |
| devtoolbox-infrastructure | 278 |
| devtoolbox-runtime | 17 |
| devtoolbox-desktop | 12 |
| devtoolbox-server / mcp + 文档测试 | 其余 |

> 说明：本轮首次跑出的 1225 是"到 runtime 就失败"的截断值；全绿后是 1356。

---

## 待办（L0 之后的下一层）

1. 把「拉取 submodule → 重建 history.duckdb」写进 `mise run setup` / README 的必做步骤（现在只写在管线 README 里）。
2. 数据目录共享在**桌面端 + 服务端同时运行**时会撞进程锁（`AppDataDirectoryLock`）；
   目前的行为需要实测确认是"拒绝启动"还是"排队"，并把提示写清楚（这是用户最容易踩的一步）。
3. `apps/server` 的 `--help` 里 `SELF_TOOLS_DATA_DIR` 默认值说明要跟着 L0-2 一起更新（已改常量，帮助文本引用常量，无需再改）。
