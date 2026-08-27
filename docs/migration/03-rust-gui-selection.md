# Rust GUI 技术选型

## 结论

**第一推荐：Tauri 2 + React/TypeScript（UI）+ Rust workspace（核心与桌面命令）。**

当前产品的难点不是普通表单，而是 Markdown 源码编辑、语法高亮、富预览、命令面板和文件树。Tauri 提供 Rust 命令和事件边界，同时允许采用成熟 Web 编辑器生态实现这些高交互部件；官方文档明确其前端无关，并通过 `command` 支持带错误值的同步或 async Rust 调用。[Tauri 概览](https://v2.tauri.app/start/) [调用 Rust](https://v2.tauri.app/develop/calling-rust/)

**第二推荐：Slint + Rust。** 若“所有应用代码必须为 Rust”是不可让步约束，选 Slint；它是原生 Rust host + 声明式 UI，并对跨线程回主事件循环和定时器有明确模型。[Slint Rust API](https://docs.slint.dev/latest/docs/rust/slint/) [桌面平台](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/) 但当前编辑器与富预览将需要较多自定义开发和原型验证。

## 比较矩阵

评分：5 最适合当前项目，1 风险/成本最高。评分是基于本项目已实现功能的工程判断，而非框架普适排名。

| 维度 | Slint | Tauri | egui | iced | Qt Rust / CXX-Qt |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust 原生程度 | 5 | 4（核心为 Rust，UI 为 Web） | 5 | 5 | 3（Rust+C++/Qt） |
| UI 基础能力 | 3 | 5 | 3 | 3 | 5 |
| 当前项目适配度 | 3 | 5 | 2 | 2 | 4 |
| Markdown | 2 | 5 | 2 | 2 | 5 |
| 富文本/源码编辑 | 2 | 5 | 2 | 2 | 5 |
| 自定义组件 | 4 | 5 | 4 | 4 | 5 |
| Windows/macOS/Linux | 5 | 5 | 5 | 5 | 5 |
| 状态管理清晰度 | 4 | 5 | 3 | 4 | 3 |
| Async 与后台任务 | 4 | 5 | 3 | 4 | 3 |
| 性能/内存 | 5 | 4 | 5 | 4 | 3 |
| 打包、权限、更新 | 4 | 5 | 3 | 3 | 3 |
| 学习与迁移成本 | 3 | 4 | 3 | 3 | 2 |
| 长期维护风险 | 4 | 5 | 3 | 3 | 3 |

## 各方案判断

### Tauri（推荐）

适合将现有 PySide6 UI 一次重塑为可维护的 React 页面，而不是模拟 Qt 对象树。当前首个可运行版本采用原生 `textarea` 完成基础输入、快捷键和任务行操作；CodeMirror 6 仍是补齐语法高亮、编辑历史与复杂输入体验的候选。Markdown 渲染交给前端的严格消毒渲染链，状态转换、文件访问、配置和工作区扫描由 Rust 提供。Tauri 的命令/事件边界天然对应本项目的 UI Event → Command → Service 模型。

代价是 UI 不再是纯 Rust，需维护 Node/TypeScript 构建链和 WebView 差异；Tauri 前端本质由 HTML/CSS/JavaScript 资产提供，不能宣称是“零 JavaScript”的方案。[Tauri 前端配置](https://v2.tauri.app/start/frontend/)

### Slint（备选）

Slint 的 `.slint` 文件可编译为 Rust 组件，具备声明式属性/回调和跨平台桌面支持；其官方模型要求事件循环在主线程，后台线程通过 `invoke_from_event_loop` 通信，符合本项目避免共享可变状态的方向。[Slint 线程与事件循环](https://docs.slint.dev/latest/docs/rust/slint/)

但需要先证明其文本编辑、选择、剪贴板、markdown 富预览、滚动同步、语法着色和命令面板能达到现有体验。若这些部件靠大量自绘实现，成本远高于业务迁移，故不作为默认选择。

### egui（不推荐）

egui 是纯 Rust、跨平台的立即模式 GUI；官方也指出立即模式更易用但能力更有限，并且 crate 仍处于快速演进/破坏性变更阶段。[egui 文档](https://docs.rs/egui/latest/egui/) 对当前源代码编辑与富文本预览要求，它会迫使团队自行补齐编辑器体验；不符合“功能一致优先”。

### iced（不推荐）

iced 的 State/Message/Update/View 与目标事件模型相近，但官方文档把它描述为实验性软件，且目前富文本与成熟源码编辑能力不足以低风险复刻该项目。[iced 文档](https://docs.rs/iced/latest/iced/) 因此其架构收益不足以抵消 UI 迁移成本。

### Qt Rust Bindings / CXX-Qt（不推荐为默认）

CXX-Qt 能让正常 Qt 与正常 Rust 代码通过桥接共存，且文档说明其 CI 覆盖 Windows、macOS、Linux。[CXX-Qt 文档](https://kdab.github.io/cxx-qt/book/) 它最利于保留原生 Qt 富文本能力，却延续 Qt/C++ 工具链、部署依赖和 QObject 生命周期模型；也会使“从 PySide 重新设计”退化为“继续绑定 Qt”。仅当验收要求与 Qt 文本行为像素级一致时再重新评估。

## 必须在实施前确认的决定

1. 接受 Tauri 的 TypeScript UI 层；Rust 承担全部业务、存储和桌面能力。
2. 以 CodeMirror 6 为编辑器候选、以 React Markdown 渲染链为预览候选，Phase 7 前先做可丢弃的技术验证。
3. 以 Tauri 2 为目标版本；具体 crate/npm 版本只在架构确认后按当时兼容矩阵锁定。
