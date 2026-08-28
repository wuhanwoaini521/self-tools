# DevToolbox

跨平台（Windows / Linux / macOS）Markdown 笔记与任务管理工具，
核心特色是**多状态 Checkbox**（Pending / In Progress / Done，可扩展）与键盘优先的专注写作流。

技术栈：Rust + Tauri 2 + React 19 + CodeMirror 6。

![Focus Mode 工作区](docs/screenshots/focus-mode.png)

## 界面一览

| Focus Mode 工作区 | Zen Mode | 命令面板 |
|---|---|---|
| 项目树、编辑器、任务大纲三栏联动 | 隐藏一切干扰，纯写作面 | `Ctrl+F` 快速执行命令 |

<table>
  <tr>
    <td><img src="docs/screenshots/zen-mode.png" alt="Zen Mode"></td>
    <td><img src="docs/screenshots/command-palette.png" alt="命令面板"></td>
  </tr>
</table>

## 开发环境

前置要求：Rust 1.95+（`rust-toolchain.toml` 自动固定）、Node.js 20+。

```bash
# 前端依赖（首次）
npm --prefix apps/desktop/ui install

# 桌面应用开发调试
npm --prefix apps/desktop/ui exec -- tauri dev

# 仅前端开发（浏览器预览，无需 Rust 编译）
npm --prefix apps/desktop/ui run dev
```

发布构建：

```bash
npm --prefix apps/desktop/ui exec -- tauri build
```

## 核心体验

```
打开应用 → 写 Markdown → 写下 US / JP / CA
→ 转换为任务 → Ctrl+Enter 切换状态（Pending → In Progress → Done → Pending）
→ 保存 → 下次打开状态仍在
```

## 多状态 Checkbox 语法

底层保持纯 Markdown 任务列表，其他编辑器打开完全可读：

| 状态 | Markdown | 符号 | 颜色 |
|------|----------|------|------|
| Pending | `- [ ] US` | ○ | 灰 |
| In Progress | `- [~] JP` | ◐ | 蓝 |
| Done | `- [x] CA` | ● | 绿 |

* 点击编辑器中的 `[ ]` 标记即可切换状态；
* 状态定义集中在 `crates/core/src/task_state.rs` 的注册表中，
  新增 Blocked / Failed 等状态只需 `register()` 一条；
* 解析规则由 `tests/fixtures/task_rules.json` 固化，Rust 单元测试直接消费该文件。

## 快捷键

| 动作 | Windows / Linux | macOS |
|------|-----------------|-------|
| 打开命令面板 | Ctrl+F | Cmd+F |
| 切换 Focus Mode | F11 | F11 |
| 切换侧栏 | Ctrl+B | Cmd+B |
| 切换任务大纲 | Ctrl+\ | Cmd+\ |
| 切换任务状态（当前行） | Ctrl+Enter | Cmd+Enter |

## 工作区

选择一个文件夹后，侧栏以**目录树**形式列出其中所有 Markdown 文件
（自动跳过 `.git`、`node_modules` 等噪音目录），点击即打开；应用会记住上次的工作区。

## 项目结构

仓库根即 Rust workspace，按领域分层：

```
├── Cargo.toml               # workspace 定义（lint、共享依赖）
├── rust-toolchain.toml      # 固定 Rust 版本
├── crates/
│   ├── core/                # 纯领域规则：任务行解析、状态注册表（无 UI / 无 I/O）
│   ├── application/         # 用例编排（workflows）
│   └── infrastructure/      # 文件 IO、配置存储、工作区扫描
├── apps/
│   └── desktop/             # Tauri 桌面适配器
│       ├── src/             # Tauri 命令 / 事件边界
│       └── ui/              # React 19 + CodeMirror 6 前端（Vite 构建）
├── tests/fixtures/          # 跨实现共享的 Markdown 行为样例
└── docs/
    ├── migration/           # PySide6 → Rust 迁移历史档案
    └── screenshots/
```

分层原则：`UI ≠ Markdown 解析 ≠ 文件读写 ≠ 任务状态逻辑`。

## 质量门禁

在仓库根运行（CI 于 Windows / macOS / Linux 三平台执行同一组命令）：

```bash
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

覆盖：解析器单元测试（含共享 fixture）、状态注册表、文档存取与工作区扫描。

## 迁移历史

本项目由 PySide6 (Python) 迁移至 Rust，原 Python 版已移除。
迁移过程的设计决策与阶段记录保留在 `docs/migration/`，仅作历史档案。
