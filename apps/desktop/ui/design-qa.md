# DevToolbox Focus Mode Command Center 设计 QA

## 对比对象

- Source visual truth：`C:\Users\admin\AppData\Local\Temp\codex-clipboard-0d9a1458-2c0a-4801-bc2d-f4f4f3eb598a.png`
- Implementation screenshot：`D:\code\self-github\self-tools\rust-app\apps\desktop\ui\.playwright-cli\page-2026-08-28T01-42-28-351Z.png`
- 组合对比：`D:\code\self-github\self-tools\output\playwright\focus-mode-command-center-final-comparison.png`
- Viewport：1487 × 1058 CSS px，device scale factor 1。
- Source / implementation pixels：1487 × 1058 / 1487 × 1058；无需密度归一化。
- State：深色 Focus Mode，左右面板展开，任务筛选为 All。

## 证据与交互

- 全视图：源图与实现图使用相同窗口尺寸，并拼接在一张图中逐项检查。
- 聚焦区域：顶部命令栏、312px 左侧项目树、中央编辑器、373px 任务大纲及底部状态栏均在全视图中清晰可读。
- 主交互：New / Open / Save、工作区选择、CodeMirror 编辑、任务状态循环、All/Todo/In Progress/Done 筛选、Find 命令面板、Focus / Zen / Split 均保留或实现。
- 浏览器视觉验证：任务筛选与 Zen 模式已实际点击验证；控制台为 0 errors、0 warnings。
- 原生验证：Tauri 桌面进程启动并响应；读写文档、工作区扫描与设置持久化仍通过 Rust command 层处理。

## 必要保真面检查

| Surface | 结果 |
| --- | --- |
| 布局与层级 | 59px 顶栏、312px 左栏、中央标签/元信息/编辑区、373px 右栏，以及固定底栏均与源图层级对齐。 |
| 字体与排版 | UI 使用 Manrope；编辑器使用系统可用的 Cascadia Mono / Consolas 等宽字体，14px、约 1.48 行高。 |
| 颜色与 tokens | 近黑背景、深色面板、蓝色选中与标题、绿色完成状态、低对比边框均整理为集中 token。 |
| 边框、圆角和图标 | 采用 1px 深灰分隔线、4px 紧凑圆角；所有图标均来自 Phosphor，同一线性风格。 |
| 滚动条 | 项目树、Outline、任务列表和 CodeMirror 编辑器统一为 8px 深色细窄滚动条；Firefox 使用同色 `thin` 回退。 |
| 状态与信息密度 | 已覆盖 hover、active、selected、任务完成/进行中/待办、无任务提示及保存状态。 |

## Findings

- 第一轮发现 P1：默认编辑器主题出现红绿语法色，且左栏内容撑出窗口。已改为截图中的蓝灰 Markdown 色，并将工作台锁定为视口高度和内部滚动。
- 第二轮发现 P1：左侧 Outline 与底部快捷栏垂直位置错误。已改为固定 290px Outline 和 46px 底栏。
- 第三轮同尺寸复核：未发现可行动的 P0/P1/P2 差异。

## Remaining Differences

- P3：源图所用等宽字体文件不可得，当前使用系统 Cascadia Mono / Consolas，因此个别字形的笔画与抗锯齿存在轻微差异；布局、字号和行高已匹配。

## Comparison History

1. 2026-08-28：搭建深色三栏 Command Center，按 1487 × 1058 捕获首版。
2. 2026-08-28：修正编辑器语法色和左栏超出视口的问题，再次截图。
3. 2026-08-28：修正左侧 Outline/底栏高度，以同尺寸最终截图和组合图复核；结果通过。

final result: passed
