# 功能一致性矩阵

状态只能使用 `Pending`、`Designing`、`Migrating`、`Testing`、`Done`、`Blocked`。当前没有 Rust 实现，所有实际功能均未完成迁移。

| 功能 | Python 状态 | Rust 状态 | Python 测试 | Rust 测试 | 行为一致 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| Rust workspace 质量骨架 | 不适用 | `core` crate、desktop 占位 binary、CI 已建立 | — | `desktop` 单元 smoke | 不适用 | Done |
| 三态定义与循环 | 已实现 | `core::TaskStateRegistry` | `test_parser.py` | `task_state::cycles_in_both_directions` | 共享 fixture 覆盖默认循环 | Done |
| 任务行识别/未知 mark 保留 | 已实现 | `core::match_task/is_task_line` | `test_shared_task_fixtures.py` | `parser::conforms_to_shared_python_rust_fixtures` | 同一 JSON fixture 已双实现执行 | Done |
| 普通/列表/嵌套行转任务 | 已实现 | `core::make_task_line` | `test_shared_task_fixtures.py` | `parser::conforms_to_shared_python_rust_fixtures` | 同一 JSON fixture 已双实现执行 | Done |
| 前后切换状态 | 已实现 | `core::cycle_task_mark` | `test_shared_task_fixtures.py` | `parser::conforms_to_shared_python_rust_fixtures` | 同一 JSON fixture 已双实现执行 | Done |
| Enter 延续任务/列表 | 已实现 | React textarea handler | `test_editor_behavior.py` | TypeScript build | 待人工 UI 回归 | Testing |
| 点击 Checkbox 标记切换 | 已实现 | React textarea handler → Rust command | `test_gui_smoke.py` | TypeScript build | 待人工 UI 回归 | Testing |
| Undo/Redo 原子编辑 | 已实现 | CodeMirror history；任务/格式操作以单次 transaction 写入 | test_editor_behavior.py | TypeScript build | 待人工 UI 回归 | Testing |
| Markdown 高亮 | 已实现 | CodeMirror Markdown language | test_highlighter.py | TypeScript build | 待人工 UI 回归 | Testing |
| 围栏代码轻量着色 | 已实现 | CodeMirror Markdown fenced-code support | test_highlighter.py | TypeScript build | 待人工 UI 回归 | Testing |
| Markdown 预览 | 已实现 | marked + DOMPurify | `test_preview.py` | TypeScript build | 待人工 UI 回归 | Testing |
| 预览任务符号/颜色 | 已实现 | React preview 状态符号 | `test_preview.py` | TypeScript build | 待人工 UI 回归 | Testing |
| 预览 latest-wins 与不阻塞 | 已实现 | 未开始 | `test_preview.py` | — | 未验证 | Pending |
| 新建/打开/保存/另存为 | 已实现 | React dialog + Rust application | app startup、GUI smoke | application workflow tests | 待人工 UI 回归 | Testing |
| 未保存修改确认 | 已实现 | 未开始 | app startup、GUI smoke | — | 未验证 | Pending |
| UTF-8 文档往返 | 已实现 | 原子 UTF-8 store | app startup、GUI smoke | `document_store::replaces_existing_utf8_document` | 核心规则已对照 | Done |
| 工作区扫描与跳过规则 | 已实现 | Rust scanner | `test_workspace.py` | `workspace_scanner::skips_noise_directories_and_sorts_paths` | 核心规则已对照 | Done |
| 文件树与打开文件 | 已实现 | 未开始 | `test_app_startup.py` | — | 未验证 | Pending |
| 任务时间线 | 无独立页面 | 右侧按 Pending/In Progress/Done 实时分组，可点击切换状态 | — | Playwright 本地浏览器快照 | 待 Tauri 人工回归 | Testing |
| 最近文件 | 已实现但 UI 未展示 | 未开始 | 间接覆盖 | — | 未验证 | Pending |
| 主题（Light/Dark/System） | 已实现 | 未开始 | app startup、new features | — | 未验证 | Pending |
| 设置持久化 | 已实现 | JSON SettingsStore | `test_new_features.py` | `settings_store::defaults_then_round_trips` | 新格式已验证；旧 QSettings 导入未做 | Testing |
| 视图 Editor/Split/Preview | 已实现 | 未开始 | `test_app_startup.py` | — | 未验证 | Pending |
| 命令面板 | 已实现 | 未开始 | app startup、new features | — | 未验证 | Pending |
| 快捷键 action id | 已实现 | 未开始 | 间接覆盖 | — | 未验证 | Pending |
| API Test 工具 | 仅占位 | 不计划实现 | 页面存在性 | — | 不适用 | Pending |
| JSON/String/Converter 工具 | 仅占位 | 不计划实现 | 页面存在性 | — | 不适用 | Pending |
| 窗口几何恢复 | 已实现 | 未开始 | 无专门测试 | — | 未验证 | Pending |
| 跨平台安装包/升级 | 未实现 | 未开始 | — | — | 不适用 | Pending |

删除 Python 的前提：所有标注为“已实现”的核心功能达到 `Done`，Python/Rust fixtures 与端到端回归均通过，并有独立人工验收记录。占位页不能成为阻止删除核心编辑器的伪功能，但在产品仍需保留导航时必须提供等价占位或获批移除。
