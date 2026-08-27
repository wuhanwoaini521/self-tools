# 渐进迁移计划

当前状态：**Phase 1 — Complete**。Phase 2 必须获得下一条明确指令后才能开始。

| Phase | Goal | 修改内容/涉及文件（确认后） | 前置依赖 | 风险 | 测试与 Validation | Done Definition |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | Architecture Approval | 审核 `docs/migration/01` 至 `08` | 当前分析 | 技术选型偏离验收 | 审阅与决策记录 | 用户明确确认架构 |
| 1 | Rust Skeleton | 新建最小 workspace、CI、fmt/clippy/test 门禁 | Phase 0 | 工具链/锁定版本 | 空 crate smoke | `fmt/check/clippy/test` 全绿 |
| 2 | Domain Parity | `core`：task state、parser、共享 fixtures | Phase 1 | Markdown 边界差异 | 单元 + Python/Rust fixture diff | 全部 parser 契约一致 |
| 3 | Infrastructure | UTF-8 文件、工作区扫描、配置 schema 与迁移策略 | Phase 2 | Windows 路径/数据损失 | 临时目录集成测试 | 原子保存与扫描规则验证 |
| 4 | Application Workflows | DocumentState、open/new/save/discard、错误 DTO | Phase 3 | dirty 状态歧义 | 用例集成测试 | 无 UI 依赖的流程通过 |
| 5 | UI Spike | 在独立原型验证 CodeMirror、预览、任务点击、快捷键、文件对话框 | Phase 4 | 编辑体验/安全不足 | 手工体验清单 + E2E spike | 通过或更新架构后停止 |
| 6 | Basic Desktop UI | Tauri shell、导航、Markdown 单页、文件打开/保存、主题 | Phase 5 | 状态双写 | 前端组件/E2E + Rust 测试 | 基础文档闭环可用 |
| 7 | Advanced Markdown UI | 分屏、预览、语法高亮、任务交互、undo、滚动/字数 | Phase 6 | 行为/性能差异 | parity fixtures + E2E | 核心编辑行为 Done |
| 8 | Settings & Commands | 设置、命令面板、快捷键、工作区树、窗口状态 | Phase 7 | 平台快捷键差异 | E2E + 手工三平台检查 | 对应矩阵 Done |
| 9 | Background & Hardening | 受控任务、取消、错误提示、性能、退出 | Phase 8 | 生命周期/阻塞 | 压力、取消、性能测试 | 无后台泄漏/可观卡顿 |
| 10 | Regression & Packaging | 发布构建、三平台验收、Python/Rust 行为对比 | Phase 9 | 打包差异 | CI、安装 smoke、回归 | 核心功能 100% Done |
| 11 | Python Removal | 仅删除已 100% 对等且完成回归的 Python 模块 | Phase 10 | 过早删除 | clean checkout 回归 | 删除有 parity 证据与可回滚提交 |

## 每阶段执行规则

1. 只执行一个 Phase；先阅读本计划和 feature parity。
2. 只修改该 Phase 的文件，先为目标行为增加/迁移测试。
3. 执行：`cargo fmt --check`、`cargo check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test`。
4. 若任何检查失败，修复或回到设计，不能推进下一阶段。
5. 更新本文件、`06-feature-parity.md`、`07-risks.md`，并记录准确命令和结果。

## Phase 1 拟议最小边界

- 仅创建 Rust workspace、`core` 空库、desktop 空壳和质量门禁。
- 不接入 Python、不实现 UI、不迁移业务、不删除旧文件。
- Rust/MSVC 必须在本阶段确认并锁定；Node/pnpm 与 Tauri 系统依赖不属于无 UI skeleton 的范围，推迟到 Phase 5 UI Spike 前按目标平台安装并验证。

## Phase 1 执行记录（2026-08-27）

**状态：Complete**

- 新建 `rust-app/` workspace，包含无业务实现的 `devtoolbox-core` 库和 `devtoolbox-desktop` 占位二进制。
- 新建 `rust-app/Cargo.lock`、`rust-toolchain.toml`、workspace lint 配置和跨 Windows/macOS/Linux 的 GitHub Actions 质量工作流。
- 本地确认 Rust 1.95.0、MSVC host；本机起初缺少 Clippy/rustfmt，已安装后完成验证。CI 固定 Rust 1.95.0；本地 `rust-toolchain.toml` 使用 stable，并通过 workspace `rust-version = "1.95"` 声明最低版本。
- 未引入 Tauri、Node 前端依赖、Markdown 逻辑或 Python 互操作；未修改或删除任何 Python 代码。

验证命令与结果：

```text
cargo fmt --check                                      PASS
cargo check                                            PASS
cargo clippy --all-targets --all-features -- -D warnings  PASS
cargo test                                             PASS (1 passed, 0 failed)
```

下一阶段候选为 **Phase 2 — Domain Parity**，但本阶段到此停止。

## 连续迁移执行记录（2026-08-27）

用户已授权连续完成后续阶段，实际结果如下：

| Phase | 状态 | 实际交付 |
| --- | --- | --- |
| 2 | Complete | `core` 迁入三态 registry、任务行解析、转换、切换和 Unicode 列位置测试；`tests/fixtures/task_rules.json` 由 Python 与 Rust 同时执行。 |
| 3 | Complete | `infrastructure` 实现 UTF-8 原子写、工作区递归扫描、版本化 JSON 设置。 |
| 4 | Complete | `application` 实现打开/保存/扫描与行级任务 command 用例。 |
| 5 | Complete | Tauri/React 技术验证：对话框、invoke、文件树、预览、命令面板、快捷键均可编译。 |
| 6 | Complete | 基础桌面页面、主题、设置、文档打开/保存/另存为、工作区已实现；随后按 Project Canvas 设计重构为深色工作区导航、文档画布和右侧任务时间线。 |
| 7 | Testing | 三态转换、快捷键、点击切换、列表续写、预览已实现；CodeMirror 6 提供 Markdown 高亮、行号、原子编辑历史、格式工具栏与编辑区→预览滚动同步，仍待人工体验验收。 |
| 8 | Testing | 设置、命令面板、导航和工作区树已实现；跨平台快捷键需人工验收。 |
| 9 | N/A | 当前没有网络或长任务；原 Python 预览线程模型未照搬，前端渲染为同步且输入规模未完成性能基准。 |
| 10 | Testing | `npm run build`、Rust 全量门禁和 Tauri debug executable 已验证；MSI/三平台安装包尚未完成。 |
| 11 | Pending | Python 保留，等待 Feature Parity 所有核心项 Done 和跨平台人工回归。 |

连续迁移的最终本地验证：

```text
npm run build                                      PASS
python -m pytest tests\\test_parser.py tests\\test_shared_task_fixtures.py -q -p no:cacheprovider  PASS (19 passed)
cargo fmt --check                                  PASS
cargo check --workspace                            PASS
cargo clippy --workspace --all-targets --all-features -- -D warnings  PASS
cargo test --workspace                             PASS (12 passed, 0 failed；含 shared fixture)
Tauri debug executable                             PASS
```

## Architecture Gate

进入 Phase 1 的唯一授权语句是用户明确表示“架构确认，可以开始迁移”或同等含义。未经该确认，当前交付只包含分析文档。
