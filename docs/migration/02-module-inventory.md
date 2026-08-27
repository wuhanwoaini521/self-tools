# 模块清单

耦合度以现役运行路径为准；“保留”指行为/规格保留，不代表按文件翻译。

| Python 模块 | 功能 | 类型 | 直接依赖 | 耦合 | Rust 迁移难度 | 推荐处理 | 优先级 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `src/document/task_state.py` | 三态定义、注册、循环 | Domain | stdlib | Low | Low | 不可变 `TaskState` + 已验证 registry | P0 |
| `src/document/parser.py` | 任务行解析、转换、切换 | Domain | regex、task_state | Low | Low | 纯 Rust 模块，属性/fixture 测试 | P0 |
| `src/document/document.py` | 当前文档路径元数据 | Application state | Qt QObject | Medium | Low | 合并为 `DocumentState`，移除 Qt Signal | P0 |
| `src/services/document_service.py` | 文本 IO、目录扫描、文件对话框、最近文档 | Infrastructure + UI | pathlib、Qt、settings | High | Medium | 拆为 `DocumentRepository`、`WorkspaceScanner`、UI dialog adapter | P0 |
| `src/config/settings.py` | 偏好与窗口几何 | Infrastructure | Qt QSettings | Medium | Medium | 版本化 JSON/TOML 配置；迁移兼容读取策略 | P0 |
| `src/modules/markdown/editor.py` | 编辑器交互、任务转换/点击/列表续写 | UI | Qt、parser、registry、highlighter、theme | High | High | 重新实现为编辑器 UI adapter；纯规则移入 core | P0 |
| `src/modules/markdown/preview.py` | Markdown→HTML、状态展示、后台渲染 | UI + Application | mistune、Qt、threading、parser | High | High | Rust renderer 服务 + WebView 前端预览 | P0 |
| `src/modules/markdown/page.py` | 页面布局、文件用例、定时器、同步滚动 | UI + Application | 上述全部 | Critical | High | 拆成 commands、state、React/TS 页面组件 | P0 |
| `src/app/main_window.py` | 页面装配、导航、主题、命令注册、关闭保护 | UI composition | Qt、qfluentwidgets、全部页面 | Critical | High | Tauri shell + command registry；不迁移 Qt 类型 | P1 |
| `src/shortcuts/shortcut_manager.py` | 快捷键规格与安装 | UI application | Qt | Medium | Medium | 保留 action ID；前端快捷键映射 | P1 |
| `src/ui/dialogs/command_palette.py` | 命令搜索与执行 | UI | Qt、qfluentwidgets | Medium | Medium | 前端命令注册表 + 模态组件 | P1 |
| `src/ui/widgets/file_tree.py` | 文件树显示与点击 | UI | Qt、DocumentService、theme | Medium | Medium | 前端树；扫描结果由 Rust 返回 | P1 |
| `src/ui/theme/__init__.py` | 主题模式单例与持久化 | Application state | Qt、darkdetect、settings | High | Medium | `ThemeMode` 值对象 + 前端 CSS variables | P1 |
| `src/ui/theme/tokens.py` | 色彩、间距、字体 token | UI | 无 | Low | Low | 转为 CSS variables/design tokens | P1 |
| `src/ui/editor/highlighter.py` | Markdown/代码语法高亮 | UI | Qt、parser、code_highlight、theme | High | High | 优先采用 CodeMirror/成熟 Markdown language package | P2 |
| `src/ui/editor/code_highlight.py` | 轻量 fenced-code 分词 | UI utility | regex、theme | Medium | Medium | 不原样迁移；由编辑器语言扩展替代 | P2 |
| `src/modules/settings/page.py` | 设置 UI 与持久化 | UI | Qt、theme、settings | Medium | Medium | 前端表单 + settings commands | P2 |
| 四个 `src/modules/*/page.py` 占位页 | 未来工具页面 | UI placeholder | Qt、qfluentwidgets | Low | Low | 仅保留导航占位；实际需求再设计 | P2 |
| `src/main.py` / `run.py` | Python 入口 | Bootstrap | PySide6 | Low | N/A | 用 Rust binary 入口替换，最后阶段删除 | P2 |
| `src/ui/main_window.py` | 已失效旧 Qt 窗口 | Dead code | 已删除模块 | N/A | N/A | 不迁移；功能对等后删除 | P2 |

## 分类

### A：第一批迁移

- `src/document/task_state.py`
- `src/document/parser.py`
- `src/services/document_service.py` 中不依赖 QFileDialog 的 UTF-8 读写与扫描规则
- `src/config/settings.py` 中配置键/默认值的规格

这些内容无运行时 Qt Widget 依赖，且已有稳定的输入输出测试。

### B：需要重新设计

- `src/document/document.py`
- `src/modules/markdown/page.py`
- `src/modules/markdown/preview.py`
- `src/app/main_window.py`
- 主题、命令、快捷键和工作区状态

它们需要从 QObject 关系和私有 Widget 调用，改为显式 command/state/reducer 边界。

### C：最后迁移

- Markdown 源码编辑器与语法着色
- 命令面板、文件树、系统级快捷键
- 窗口几何、文件对话框、打包与自动更新
- 未实现工具页的实际功能
