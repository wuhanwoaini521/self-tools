# FEATURE_MATRIX.md

> 16 个页面全部经真实浏览器加载验证（1440×900）。
> 「数据」列区分**真实数据**与**空/假数据**——空状态本身不算功能可用。

## 页面总览

| # | 页面 | Route | 主要功能 | 数据来源 | Web 端 | 测试 | 问题 |
|---|---|---|---|---|---|---|---|
| 1 | Home | `#home` | 今日概览 / 快捷入口 / Continue | platform today | ⚠️ 部分 | — | 依赖 learning_*（已修） |
| 2 | Review Center | `#review` | 跨模块 SRS 复习 | platform review_cards | ✅ 已修 | — | 无卡片时仅空态 |
| 3 | History | `#history` | 时间轴 / 时期 / 人物 / 事件 | duckdb | ✅ | 7 个 Rust | — |
| 4 | Geography | `#geography` | 地图 / 地形 / 3D | geography.db | ❌ | 无 | 数据仅 14 条示例；Web 端搜索走 12 条 fallback |
| 5 | Language | `#language` | 学习卡片 / 课程 / 复习 / 错题 | language.db + learning.db | ✅ | 8 个 E2E | — |
| 6 | Study Board | `#study-board` | 手写板 / 笔刷 | `study_boards.db` | ✅ | 5 个 server + 5 前端 | — |
| 7 | News | `#news` | 今日 / 分类 / 搜索 | news.db | ✅ | 14 个 server | 推荐源恒空（11 条用尽） |
| 8 | RSS | `#rss` | 订阅 / 阅读 | rss | ❌ | 无 | `rss_*` 无 HTTP 映射 |
| 9 | Travel | `#travel` | 城市探索 / AI 行程 | travel.db | ❌ | 无 | `travel_*` 无映射；AI 未配 |
| 10 | Markdown | `#markdown` | 编辑 / 工作区 / 大纲 | 本地文件系统 | ❌ | 无 | 对话框已修但 Web 无法实现 |
| 11 | Collections | `#collections` | 专题合集 | platform collections | ✅ 已修 | 3 个 server | — |
| 12 | Knowledge | `#knowledge` | Memory / Docs / Files | 多个 db | ❌ | 无 | 21 个命令无映射 |
| 13 | Graph | `#graph` | 跨模块知识图谱 | platform graph | ✅ 已修 | — | 无 root 时节点少 |
| 14 | Search | `#search` | 全局检索 | platform + memory/docs/files | ⚠️ | 无 | `global_search` 已接；无结果 |
| 15 | Server | `#server` | 家庭服务器 | server_actions.db | ❌ | 无 | Web 端硬阻断 |
| 16 | System | `#system` | 就绪状态 / 诊断 | **无** | ❌ | 无 | **假报告（无后端）** |

## 能力覆盖

| 能力 | 状态 |
|---|---|
| LearningEvent | ✅ 平台统一，Language 已接入 |
| LearningProgress / 掌握度 | ✅ 平台统一（`MasteryCalculator`） |
| 复习 SRS | ✅ 平台统一（Review Center + Language 共用） |
| 全局搜索 | ⚠️ 已接 History/Language/News，Memory/Docs/Files 未接 |
| 合集 Collections | ✅ 已接 |
| 全局检索 Graph | ✅ 已接 |
| AI Provider | ⚠️ **未配置**，仅验证「诚实拒绝」 |
| PWA / 移动端 | ✅ 有 Service Worker 与响应式布局 |

## 已修复

| 模块 | 问题 | 状态 |
|---|---|---|
| Language | 自建第二套复习/进度；写入路径全断；Mastered 不可达 | ✅ |
| Language | 跟读无反馈（`speaking::score` 写好却从未被调用） | ✅ |
| Language | 复习只做「认词」，不练「用句子」 | ✅ |
| History / News / AI / Settings | Web 端 `isTauriRuntime` 早退 + 内部错误串泄漏 | ✅ |
| Learning OS | 8 个 client 未迁移 + 无 HTTP 端点 | ✅ |
| Markdown | 原生对话框失败静默 | ✅ |
| Study Board | 笔迹只存 localStorage，网页端与 AI 各存各的 | ✅ |
| Study Board | 列表笔画数恒为 0（SQLite 层硬编码） | ✅ |
| Travel | 主按钮失色 / 图标被撑成巨幅 | ✅ |
| 设计系统 | Pixel-Lift 硬阴影 + 结构 token 分散在 4 个主题 | ✅ |

## 未修复（按影响）

1. **~50 个命令无 HTTP 映射** → Geography(4) / RSS / Server / Conversation(6) /
   History enrichment(5) 在 Web 端是空壳（Learning / Language / News / Travel /
   Knowledge / Study Board 已接）
2. `SystemReadinessPage` 无后端、假报告
3. Geography 数据仅 14 条示例（Web 端搜索走 fallback，非真实库）
4. 未在真实 Tauri 宿主中验证
