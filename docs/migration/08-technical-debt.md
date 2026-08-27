# 当前技术债务（只记录，不修复）

| 严重度 | 项目 | 证据 | 影响 | 迁移处理 |
| --- | --- | --- | --- |
| P0 | 失效 UI 实现残留 | `src/ui/main_window.py` 仍导入已删除的 `markdown_editor`、`preview`、`sidebar` | 阅读者可能误判启动路径；该文件不可直接运行 | 不迁移；功能对等后删除，并更新 README |
| P1 | MarkdownPage 是 God Object | 478 行同时管理布局、文件用例、状态、对话框、计时器、预览、滚动与提示 | 测试难、迁移边界模糊、业务难复用 | 拆为前端组件 + application use cases |
| P1 | Service 混合 UI 与基础设施 | `DocumentService` 同时执行文件读写、扫描和 `QFileDialog` | 无法脱离 Qt 测试/复用 | 拆分 repository/scanner/dialog adapter |
| P1 | 隐式全局主题 | `ui.theme._theme` 是模块级单例，所有组件自行读取 | 生命周期和测试隔离脆弱 | 显式配置 + 前端 theme provider |
| P1 | 预览线程无界且不可取消 | 每次渲染 `threading.Thread(... daemon=True)`，generation 仅忽略旧结果 | 高频编辑/退出时资源与生命周期风险 | 有界 latest-wins 任务模型 |
| P1 | 同步 I/O 在 UI 线程 | 文件读取、写入、递归扫描直接由页面/树调用 | 大文件和大目录会卡界面 | 后台 blocking worker + UI 消息 |
| P1 | 私有字段跨层调用 | MainWindow 命令直接操作 `MarkdownPage._editor_widget`、`_stackedWidget` | 封装名义化，替换 UI 困难 | 以 public command/action 边界替代 |
| P2 | 设置只部分生效 | `editor_font_size`、`auto_save`、`markdown_default_view` 能持久化，但当前编辑器仍硬编码 13pt，页面初始为 split，未见自动保存订阅 | UI 暗示功能与实际不一致 | 先将其列为验收决策；不盲目复制未兑现行为 |
| P2 | 文档状态信号不完整 | `Document.pathChanged` 未连接到现役窗口；页面只在文本变化时发 `documentChanged` | 标题/状态更新依赖隐式时机 | 用 reducer/command 返回新 state |
| P2 | README 与运行实现漂移 | README 写 OxNote、旧 `src/ui/sidebar.py` 和旧编辑器路径 | 开发/使用说明不可信 | 迁移后以实际架构重写 |
| P2 | 预览与业务规则混合 | 预览模块在替换任务标记、渲染、CSS、线程之间耦合 | 规则无法独立验证，安全策略不清 | Rust 规则与前端展示分离 |
| P2 | 直接覆盖保存 | `Path.write_text` 无原子写入/恢复路径 | 崩溃/磁盘异常时可能损坏文件 | 基础设施阶段引入安全写入 |
| P2 | 高亮器采用全局可变语言 ID | `_LANG_IDS.setdefault` 在高亮过程中增长 | 长期状态隐式且难测试 | 交由成熟编辑器语言服务或固定映射 |
| P2 | 纯规则包曾在导入时依赖 Qt | `src.document.__init__` 曾无条件导入 `Document(QObject)` | 无 PySide6 的规则测试、脚本和迁移对照无法运行 | 已改为惰性导出 `Document`；保留兼容导入路径 |

以上项目均不是本阶段的修复授权；它们仅用于防止迁移时将同类结构带入 Rust。
