# P0 实测报告：self-tools 在本机能否跑起来、好不好用

> ⚠️ **已过期（2026-11-18）**：本报告的基线是 `93fe71e`。`git pull` 到 `b2b88c6` 后，
> 第 3 节的启动崩溃已在上游修复（`semantic_history_path` 改指 `dist/history.duckdb`，
> 缺失时 `attach_without_probe` 降级而不再 panic），第 5/6 节的多数结论也随之失效。
> Windows 基线的新问题与修复见 `docs/p0-windows-fixes.md`；本文件仅保留当时的环境数据作为历史记录。

- 日期：2026-11-17 22:00–23:00
- 目的：验证 `docs/unified-hub-plan.md` 的 P0 硬门（工具链 → 构建 → 测试 → 真实运行）
- 结论：**能跑，但不是"克隆即可用"**。首次启动会崩溃；绕过一处数据路径后，5 个模块真实可用、2 个受输入授权限制未能实测。

---

## 1. 环境准备

| 步骤 | 结果 | 备注 |
|---|---|---|
| rustup | ✅ 1.29.3 | `winget install Rustlang.Rustup` 报 exit 1，但工具链实际装好了 |
| Rust | ✅ 1.100.0（stable-x86_64-pc-windows-msvc） | 满足 `rust-version = 1.95` |
| **rustup 默认工具链** | ⚠️ 需要手动 `rustup default stable` | winget 安装后没有默认工具链，`rustc` 在非项目目录会报"no default is configured" |
| VS Build Tools 2022 | ✅ 17.14.45（C++ 工作负载 + Windows SDK 10.0.26100） | 前两次 winget 失败：一次 `stdin is not a tty`，一次 exit 32；第三次用 pty 才拉起安装器 |
| MSVC 链接器 | ✅ `MSVC/14.44.35207/bin/Hostx64/x64/{cl,link}.exe` | `rustc` 编译 hello world 通过（`link-ok`） |
| 安装耗时 | 约 40 分钟（22:05 起，SDK 下载阶段最慢，约 0.8 MB/s） | 下载量数 GB，全部在 `ProgramData/Microsoft/VisualStudio/Packages` |

> 注意：本机账号**非管理员**，VS Build Tools 需要提权；本次是通过 UAC 授权完成的。换机重装时这一步无法省略。

---

## 2. 自动化测试与构建

```
cargo test --workspace
  → 33 passed / 0 failed   (crates/core)
  → 70 passed / 0 failed   (crates/application?)
  → 99 passed / 0 failed   (crates/infrastructure?)
  → 合计 202 passed / 0 failed / 0 ignored
```

- 实测 **202** 项通过（此前按 `#[test]` 计数得 184，差额来自参数化/生成用例；以实测为准）
- `cargo clippy --workspace --all-targets -- -D warnings` ✅ **0 error**（3m13s，含 duckdb/tauri 全量检查）—— 官方质量门是真绿的
- `cargo build -p devtoolbox-desktop` ✅ → `target/debug/devtoolbox-desktop.exe`（92,230,144 字节，增量 38s）
- 前端 `tsc --noEmit && vite build` ✅ 6.85s，产出 `ui/dist`（主 chunk 2.23 MB，Vite 提示可再分包）

---

## 3. 首次启动：崩溃（最重要发现）

```
thread 'main' panicked at tauri-2.11.5/src/app.rs:1425:11:
Failed to setup app: error encountered during setup hook:
History semantic database not found from current directory or executable path:
E:\Code\self-github\self-tools
```

- 定位：`apps/desktop/src/lib.rs:176 semantic_history_path()` 只找
  `<root>/history-data-pipeline/data/normalized/history.duckdb`
- 实际仓库里：`history-data-pipeline/data/normalized/` 是**空目录**，真实库在
  `history-data-pipeline/dist/history.duckdb`（12 MB）
- 即：**submodule 已初始化为 `32d5d61`，但管线的输出路径与桌面端的期望路径已经不一致**，新克隆必崩
- 影响：`main` 上没有任何"缺少历史库时降级启动"的分支，崩溃发生在 Tauri setup hook，窗口都不会出现

### 复现与绕过（本次测试用，未改仓库）

```bash
# 在 scratch 里搭等价环境，仓库零改动
mkdir -p $SCRATCH/hubtest/history-data-pipeline/data/normalized
cp history-data-pipeline/dist/history.duckdb \
   $SCRATCH/hubtest/history-data-pipeline/data/normalized/
cd $SCRATCH/hubtest && "…/target/debug/devtoolbox-desktop.exe"
```

绕过之后应用正常启动；`config/` 自动生成 `dashboard.db / travel.db / history.db / geography.db`，
安装语料后追加 `language.db`。

**建议的正式修法（P1 之前）**：`semantic_history_path` 按
`data/normalized/ → dist/ → 空库降级` 三级回退，并在 history 模块显示"语义库未安装"而不是 panic。

---

## 4. 模块走查（桌面端，真实数据）

| 模块 | 状态 | 证据 |
|---|---|---|
| Home | ✅ | 渲染正常；历史推荐卡显示「楚汉争霸 · 来自本地语义资料库」，说明 DuckDB 语义层真的被读到；「最近笔记」空态文案正常 |
| Markdown | ✅（未做文件读写） | 编辑器 UI、Outline、Focus/Zen 控件齐全；需系统文件对话框，本次未点 |
| RSS | ✅ **端到端通过** | 从 SQLite 预置 1 个 Feed（Solidot）后点刷新：真实抓取 **20 篇**，全部含摘要；侧栏徽标变 20，右下 toast「刷新完成，20 篇新文章」 |
| Travel | ⚠️ 仅渲染 | 表单（城市/天数/日期/偏好）正常；研究流程需输入城市名，受输入授权限制未触发 |
| Geography | ✅ | MapLibre GL + Mapterhorn DEM 瓦片渲染出地形晕渲；"14 个本地实体 / 0 条关系"；Explore / 知识 两个 tab 正常 |
| History | ✅ | 31 个时期时间轴、"3 条可读 Story"、当前时期卡；未收录时期显示「内容正在完善」而不是空白 |
| Language | ✅ **端到端通过** | 点「安装数据包」→「Starter Pack 安装完成：+1113 条」；库内 `languages=4`、`language_items=1054`、`examples=309`、`sources=10`；Today 页出现日语例句（Tatoeba，附 CC BY 2.0 FR 署名） |
| Settings | 未测 | 需要点击按钮/输入，受输入授权限制 |

### 浏览器模式（同一份 UI，无后端）

`npm run dev` → `127.0.0.1:1420`：

- 9 个导航项与主题、字体全部正常
- Home / Markdown / RSS / Travel / Geography 能渲染（Geography 用的是内置种子数据）
- **History 显示「需要通过桌面应用启动」**（明确的降级提示）
- **Language 卡在「加载语言数据…」**，并且此后点 Tools/Settings 也停在同一屏 —— 说明缺后端时有一个渲染态把导航阻塞住了
- 原因：`isTauriRuntime()` 为假 → `get_settings`/`latest_rss_articles`/`geography_home`/`language_*` 全部跳过

这条正好量化了 P1 的必要性：现状不是"Web 少几个功能"，而是**整站空壳 + 一个卡死态**。

---

## 5. 未测项与原因

| 项 | 原因 |
|---|---|
| Travel 真实研究（搜索→抓取→事实核验→攻略） | 需要输入城市名；桌面输入操作触发了授权确认，未获响应，按规则不重试 |
| Markdown 打开/保存文件 | 需要系统文件对话框交互 |
| Settings 里 LLM / 高德 / 和风 的"测试连接" | 同上（需输入 + 点击） |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 已完成：0 error（3m13s） |

---

## 6. 本次暴露的问题清单（按优先级）

1. **P0 阻塞**：语义库路径不一致 → 新克隆必崩（`lib.rs:176`）。修法见第 3 节。
2. **P1 必改**：Web 模式缺后端时 `Language` 页卡死并阻塞导航（不是优雅降级）。
3. **P1 必改**：`settings.json` 明文保存 `llm_api_key` / `amap_api_key` / `qweather_api_key` / `baidu_map_api_key`（`settings_store.rs:21` 起）。接 Ollama 时可以留空，但一旦填云模型 key 就是明文落盘；方案里的"密钥只走 env"要在 P2 一并落地。
4. **P1 建议**：`cargo test` 有 202 项但**没有一项覆盖 `apps/desktop/src/lib.rs` 的命令层**（184→202 的测试全在 core/application/infrastructure）。抽出 `crates/app-api` 时应补一层命令级契约测试，否则两个壳的行为一致性无保障。
5. **P2 注意**：前端主 chunk 2.23 MB、`maplibre-gl` 962 KB，Web 模式首屏在手机端会明显偏慢，届时要分包。
6. **记录**：侧面证实 `config/` 里的 5 个库都是按需创建的（geography/language 只在首次进入对应模块时出现），P3/P4 新增 `hub.db` 时保持同样策略即可。

---

## 7. 本次产生的本地数据（可删）

全部在 `E:\Code\self-github\self-tools\config\`（已在 `.gitignore`）：

| 文件 | 来源 |
|---|---|
| `dashboard.db` | 预置 1 个 Feed + 刷新出 20 篇文章 |
| `language.db` | Starter Pack（1054 条词条 / 309 例句） |
| `history.db` / `geography.db` / `travel.db` | 应用首次初始化自建（空） |

临时验证环境在 `%LOCALAPPDATA%\hermes\cache\scratch\hubtest\`（含复制的 12 MB DuckDB），可随时删除。

---

## 8. 对 P1 的影响（方案修订）

原计划 P1 是"抽 `crates/app-api` + 加 axum 壳"。基于实测补充两条前置：

- **P1.0（半天）**：修 `semantic_history_path` 三级回退 + History 模块的"未安装"空态。否则任何新环境（包括将来的服务器壳）都起不来。
- **P1.1**：抽命令层时**同时补命令级契约测试**（至少覆盖本次实测用到的 12 个命令），这也是方案里 P1 验收 ⑤ 的落地方式。
