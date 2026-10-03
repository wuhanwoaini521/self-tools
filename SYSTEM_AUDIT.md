# SYSTEM_AUDIT.md

> 过程产物。每条结论都来自实际代码阅读或真实浏览器验证；未验证的一律标注「未验证」。
> **记录不等于完成** —— 每条问题都在 `FIXED` / `OPEN` 列跟踪。

## 1. 技术栈与结构

| 层 | 技术 | 规模 |
|---|---|---|
| 桌面壳 | Tauri 2（Rust） | 128 个 `#[tauri::command]` |
| 后端 | Rust workspace：core / application / infrastructure | 103k LOC |
| 前端 | React + TypeScript + Vite | 32k LOC |
| 样式 | 单文件 `styles.css` | 18.7k LOC |
| 数据库 | SQLite（每模块一个库） | 14 个 `.db` |
| HTTP 服务 | axum 0.8 | `apps/server` |
| AI | `personal_ai` + `ChatModelProvider` + Tool Registry | — |

**依赖方向**：`infrastructure → application → core`（无环）。
**端口**：port trait 定义在 `application`，store 在 `infrastructure`，适配器原在
`apps/desktop`（现已下沉到 `crates/infrastructure::ports`，两端共用）。

## 2. 数据存储

| 库 | 归属模块 | 数据 |
|---|---|---|
| `history.duckdb` | History | 31 时期 / 3 故事（外部 submodule 产物） |
| `language.db` | Language | 1054 词条（starter pack） |
| `news.db` | News | 11 订阅源 / 文章 |
| `geography.db` | Geography | **14 条硬编码示例实体** |
| `learning.db` | Learning OS | 进度 / 复习卡 / 合集（平台统一） |
| `files/documents/memory/travel/…` | Knowledge 等 | — |

## 3. 核心发现

### 3.1 「网页端大面积失效」— 最严重

- 前端调用 **118** 个命令，HTTP 只映射 **38** 个 → **80 个在网页端静默失败**。
- **12 个 `*Client.ts` 中有 8 个** 仍绑定 `tauriTransport`：即使端点齐全也不走 HTTP。

**影响**：Collections / Graph / Review Center / Home 今日面板在浏览器里永远是空壳，
且**无任何错误提示**。**状态：已修 Learning OS（14 端点 + 8 client 迁移），其余 OPEN。**

### 3.2 平台能力被前端绕过

`transport.ts` 的注释写着「未来增加 HTTP 实现时 feature Client 不需要改动」——
架构预留了，但 8 个 client 从未迁移。属于**设计正确、执行未完成**。

### 3.3 半成品功能

| 功能 | 症状 | 状态 |
|---|---|---|
| `SystemReadinessPage` | **完全没有后端命令**，永远用本地推导的假报告，还宣称「Backend 就绪」 | OPEN |
| `StudyBoardPage` | 后端 `study_boards.db` 存在，前端却用 **localStorage** | OPEN |
| News 推荐源 | 目录硬编码 **11 条**，用户已全部订阅 → 推荐区恒为空 | OPEN（设计问题） |

### 3.4 静默失败模式（本次已系统性修复）

多个页面在能力缺失时**无提示地不工作**：
- Markdown 对话框：原生 API 失败被吞，点按钮毫无反应 → 已修
- AI 未配置：显示内部串 `Not in Tauri runtime` → 已修
- History/Language/News：非桌面端直接 return → 已修

### 3.5 数据可靠性

- **未发现** UI 更新而数据库未更新的情况（已验证合集 CRUD 往返）。
- 迁移为幂等执行，**不要求 reset database**（已用测试锁定）。

### 3.6 设计语言不统一

我上一轮引入全局按钮基线时，特异度 (0,4,1) 压过了页面级 `.travel-submit-btn` (0,1,0)，
导致主按钮退回白底 —— **这是我引入的回归**，已修。说明「页面级样式只允许覆盖间距与排版」
的约定需要靠机制保证，不能只靠自觉。

## 4. 剩余 OPEN 项（按影响排序）

1. **~50 个命令仍无 HTTP 映射**：memory(8) / files(7) / documents(6) /
   conversation(6) / history_enrichment(5) / geography(4) / travel(4) /
   server(4) / language(3) / news(2) / knowledge(1)
2. `SystemReadinessPage` 假报告
3. `StudyBoardPage` 用 localStorage 而非后端
4. Geography 仅 14 条示例数据；搜索结果在网页端不渲染（**已定位，未修**）
5. News 推荐源目录仅 11 条且已用尽

## 5. 未验证 / 已知盲区

- **未在真实 Tauri 桌面壳中运行过**（本环境只能起 Web 预览 + HTTP 服务）。
  因此 IPC 路径的正确性仅由 Rust 单测与类型检查保证，未做运行时验证。
- **未配置任何 LLM provider**，AI 的真实调用链（模型返回、tool 调用、超时）
  未经端到端验证；只验证了「未配置时诚实拒绝」。
- WebDriver E2E 套件依赖真实桌面宿主，本次未运行。
