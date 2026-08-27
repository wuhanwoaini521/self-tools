# 迁移风险

| 优先级 | 风险 | 事实/失败条件 | 缓解与验证 | 当前状态 |
| --- | --- | --- | --- | --- |
| P0 | 编辑器体验退化 | Qt `QPlainTextEdit` 的选择、Undo、鼠标点击、输入法、快捷键并非自动等价 | Phase 5 独立验证 CodeMirror；以 editor/gui smoke 用例重写 E2E | Open |
| P0 | Markdown 行为不兼容 | parser 对缩进、列表符、空行、未知 mark 有精确规则 | 先迁移 fixtures 与属性测试；Rust 结果逐字节对比 | Open |
| P0 | 文档数据丢失 | 现有保存直接覆盖 UTF-8 文件，迁移中格式/编码差异可能改写内容 | 原子保存、备份/恢复策略、golden 文档回归 | Open |
| P0 | 前端 Markdown XSS | Python QTextBrowser 与 WebView 的 HTML 安全模型不同 | 预览渲染消毒、禁止不受控 HTML，增加恶意 Markdown 测试 | Open |
| P0 | GUI 框架选择错误 | Slint/egui/iced 对本项目富编辑器有额外实现成本 | 架构确认后先做 Tauri UI spike；未通过即停止重评估 | Open |
| P1 | Windows 路径与文件替换 | 工作区/最近文件保存绝对路径；原子替换、锁文件和 Unicode 路径存在平台差异 | Windows tempdir/unicode/只读文件集成测试 | Open |
| P1 | 配置迁移丢失 | 现有 QSettings 是平台存储，迁移后位置/类型不同 | 记录键映射、只读导入、备份旧配置 | Open |
| P1 | UI 主线程阻塞 | 当前文件读写和递归扫描同步；错误地原样迁移会复现卡顿 | 明确 blocking worker 边界与大目录性能预算 | Open |
| P1 | 后台任务不可取消 | 当前预览每次防抖后新建 daemon thread，仅丢弃旧结果 | revision + cancel token + 有界 worker；退出测试 | Open |
| P1 | `Arc<Mutex>` 泛化 | Tauri state 易被误用为全局可变 AppState | 将编辑 state 保留在前端，业务用值/消息传递；代码审查禁例 | Open |
| P1 | 快捷键与命令面板差异 | Qt 自动 Ctrl→Cmd，Web 快捷键和浏览器默认行为不同 | Windows/macOS/Linux 手工矩阵；按 action ID 验收 | Open |
| P1 | 打包和 WebView 差异 | 已生成 Windows debug executable，但 MSI 仍依赖 WiX 与三平台环境 | 目标 OS 的 clean-machine 安装 smoke、CI 构建 | Partially mitigated |
| P1 | Rust 质量工具缺失或漂移 | 本机初始 stable 工具链缺少 Clippy/rustfmt；未来本地与 CI 可能采用不同编译器 | `rust-toolchain.toml` 声明所需组件；CI 固定 Rust 1.95.0；Phase 1 已本地验证 | Partially mitigated |
| P2 | 高亮视觉差异 | 当前为自定义轻量规则、不是完整语法服务 | 对关键 Markdown/代码 fixture 做截图/交互验收，不追求逐像素 | Open |
| P2 | 主题系统差异 | darkdetect/QSS 与 CSS media query、token 行为不同 | Light/Dark/System 三态截图和重启持久化验证 | Open |
| P2 | README/代码漂移 | README 仍为 OxNote/旧目录，源码有失效旧窗口 | 迁移完成后单独更新文档，避免以 README 作为实现依据 | Accepted |
| P2 | 测试耦合私有字段 | 现有多个 GUI 测试访问 `_editor_widget` 等内部成员 | 抽取用户可见行为；保留 domain fixture；逐步减少实现耦合 | Open |
| P2 | 未实现功能范围蔓延 | API/JSON/String/Converter 仅占位 | 本次仅对等占位；任何工具实现需新需求和单独 phase | Accepted |
| P2 | 编辑器高级体验差异 | 当前 Rust UI 使用原生 textarea，不是 CodeMirror；语法高亮、IME/Undo 的行为需实测 | 将 Python GUI 测试重写为 Tauri E2E，并在 Windows/macOS/Linux 人工验收 | Open |

## 风险门槛

- 任一 P0 未通过验证，不进入正式 UI 迁移。
- 任一数据写入/安全风险未被测试覆盖，不发布可安装包。
- 发现目标架构需要广泛共享可变状态或无限后台任务时，暂停实现并更新 `04-target-architecture.md`。
