# OxNote

跨平台（Windows / Linux / macOS）Markdown 笔记工具，核心特色是
**多状态 Checkbox**（Pending / Ing / Done，可扩展）。

技术栈：Rust + Tauri 2 + React 19 + CodeMirror 6。

![screenshot](docs/screenshot.png)

## 开发环境

```powershell
cd rust-app/apps/desktop
npm --prefix ui install
npm --prefix ui exec -- tauri dev
```

发布构建：

```powershell
npm --prefix ui exec -- tauri build
```

## 核心体验

```
打开应用 → 写 Markdown → 写下 US / JP / CA
→ 选中 → Ctrl+L 转换为任务
→ Ctrl+Enter 切换状态（Pending → Ing → Done → Pending）
→ 保存 → 下次打开状态仍在
```

## 多状态 Checkbox 语法

底层保持纯 Markdown 任务列表，其他编辑器打开完全可读：

| 状态 | Markdown | 符号 | 颜色 |
|------|----------|------|------|
| Pending | `- [ ] US` | ○ | 灰 |
| Ing | `- [~] JP` | ◐ | 蓝 |
| Done | `- [x] CA` | ● | 绿 |

* 点击编辑器中的 `[ ]` 标记即可切换状态；
* 状态定义集中在 `rust-app/crates/core/src/task_state.rs` 的注册表中，
  新增 Blocked / Failed 等状态只需 `register()` 一条。

## 快捷键

| 动作 | Windows / Linux | macOS |
|------|-----------------|-------|
| 选中行转换为任务 | Ctrl+L | Cmd+L |
| 切换到下一状态 | Ctrl+Enter | Cmd+Enter |
| 切换到上一状态 | Ctrl+Shift+Enter | Cmd+Shift+Enter |

编辑器快捷键在 `rust-app/apps/desktop/ui/src/App.tsx` 中集中处理。

## 工作区

通过「文件 → 打开文件夹」选择一个文件夹后，侧栏会以**目录树**形式列出其中所有
Markdown 文件（自动跳过 `.git`、`node_modules` 等噪音目录），点击即打开；
应用会记住上次的工作区。

## 项目结构

Rust workspace 位于 `rust-app/`，按领域分层：

```
rust-app/
├── apps/desktop/            # Tauri 桌面适配器
│   ├── src-tauri/           # Tauri 命令 / 事件边界
│   └── ui/                  # React 19 + CodeMirror 6 前端（Vite 构建）
├── crates/
│   ├── core/                # 纯领域规则：任务行解析、状态注册表（无 UI / 无 I/O）
│   ├── application/         # 用例编排（workflows）
│   └── infrastructure/      # 文件 IO、配置存储、工作区扫描
└── tests/fixtures/          # 共享 Markdown 行为样例（含 task_rules.json）
```

分层原则：`UI ≠ Markdown 解析 ≠ 文件读写 ≠ 任务状态逻辑`。

## 测试

在 `rust-app/` 运行：

```powershell
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

覆盖：解析器单元测试（含 `tests/fixtures/task_rules.json` 共享样例）、
状态注册表、编辑器转换/切换行为等。

## 迁移历史

本项目由 PySide6 (Python) 迁移至 Rust，原 Python 版已移除。
迁移过程的设计决策与阶段记录保留在 `docs/migration/`，仅作历史档案。
