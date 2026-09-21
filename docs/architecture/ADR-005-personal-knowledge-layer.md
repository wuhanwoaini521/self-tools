# ADR-005 · Personal Knowledge Layer：三域独立索引 + 候选确认写入 + 只读文件访问

- 状态：**Accepted**（2026-09-21，V6 Gates 0-10 PASS）
- 领域：`crates/core` / `crates/application` / `crates/infrastructure` / `apps/desktop`
- 关联：[ADR-003-personal-ai-hub.md](ADR-003-personal-ai-hub.md)（V4）、
  [ADR-004-personal-ai-module-expansion.md](ADR-004-personal-ai-module-expansion.md)（V5）、
  [V6 计划](../personal-ai/PERSONAL_KNOWLEDGE_V6_PLAN.md)、
  [V6 状态](../personal-ai/V6_OVERNIGHT_STATUS.md)

## 背景

V5 之后 Personal AI 已可挂载多模块，但**没有任何长期个人知识**：对话结束即遗忘；
用户本机的文档与文件对 AI 完全不可见。V6 要补三层能力（Personal Memory / Documents
索引 / Files 检索）和一个统一检索入口，且不能松动 V4 立下的两条平台不变式：
PersonalAgent 核心零业务分支、AI 只读。

约束清单（计划 §1.3/§7）：不引入外部依赖；每域一个 SQLite；检索必须确定性可复现；
用户数据不出本机（无网络、无远端模型调用发生在检索路径上）。

## 决策

### 1. 一个域 = 一个 SQLite 库 = 一组端口，共享数据形状上移 core

`config/memory.db` / `config/documents.db` / `config/files.db` 三库独立（删一个不
影响其它域）；`core::{memory,documents,files,knowledge}` 只放纯契约（`MemoryItem` /
`DocumentChunk` / `FileMetadata` / `KnowledgeResult`+`Provenance`），`infrastructure`
实现 SQLite 与文件系统，`application` 只依赖 core（`grep devtoolbox_infrastructure
crates/application/src` = 0 保持）。理由：与 `history_enrichment`（V5）同一形态，
审查时分层一眼可验；共享形状放 core 让 infra 无需反向依赖 application。

### 2. Memory 写入走「候选 → 用户确认 → ACTIVE」双门

模型工具 `memory.save` 只写 `MemoryStatus::Candidate` 并回传 `Action::confirm_memory`
（前端渲染确认卡片）；只有 UI 确认命令 `memory_confirm` / `memory_save`（用户显式动作）
能产出 ACTIVE。secret 正则（key/token/password/私钥形态）在 service 写入前拦截，
返回受控错误而非落库。理由：对话内容永不被静默持久化（计划 §2.1 第一条边界）；
敏感度 `Private/Sensitive` 在模型检索路径过滤，只在用户显式查看时可见。

### 3. Documents = 分块索引，Files = 元数据索引 + 按需安全读取

文档抽取正文切 chunk（`chunk_text`，目标 1200 字符 / overlap 200 / 标题感知），
正文命中可定位到章节与字符区间，带 provenance；PDF / 超大 / 解析失败降级为**仅元数据**
（不报错、不阻塞其它文件）。文件只索引元数据（mtime/size/kind/restricted），正文读取
必须经 `FileService::authorize`（允许根 + canonicalize + traversal + deny + 大小上限），
二进制 / NUL / 超大只回元数据与原因，**字节不进 ToolResult**。`files.open` 只返回
`Action::open_file`，backend 无任何 shell/进程执行入口。
理由：文档要「引用得到位置」，文件要「找得到、打开交给用户」；把文件正文默认挡在
模型之外是隐私默认值，也是上下文预算 default `max_files = 0` 的原因。

### 4. 检索是确定性编排，不是魔法

`KnowledgeRetrievalService` 做四件事且只做四件事：各源候选 → 规则打分排序 →
同路径去重（Document 优先于 File）→ 预算截断（`max_results` / `max_per_source` /
`max_chars`）。单源失败只记 `KnowledgeDiagnostics.errors`，不阻塞其它源；空查询
不触发任何源；无命中时工具如实返回「没有找到」（prompt 规则 + 测试双保）。
自动注入经**平台通用** `RetrievalAugmenter`（`PersonalHub.retrieval: Option`），
只注入相关度 ≥ 0.5 的记忆类结果；不装配即完全关闭（回滚开关）。
理由：P0 不引入向量库/Embedding（计划 §49）；确定性排序可单测、可复现、可解释。

### 5. 模块接入沿用 V4/V5 模块机制，`documents.*` 工具名全新

四个新模块（memory/documents/files/knowledge）全部是
`descriptor + tools + ContextProvider + register_*()`，与 geography 同构；
V5 的 `documents.*` 命令名（Markdown 编辑器）全部保留原名不变，新工具集使用
`documents.search/get/read/get_context/list_recent` 语义但经注册表隔离，
`PersonalAgent` 零改动（仅 `agent.rs` 插入通用 retrieval stage）。
理由：复用已验证的注册/风险门禁/能力过滤；避免与既有模块名冲突。

## 备选方案（拒绝）

| 方案 | 拒绝理由 |
| --- | --- |
| 单一 `knowledge.db` 存三域 | 删/备份粒度变粗；文档索引膨胀会拖慢记忆查询 |
| 记忆直接 ACTIVE（无确认） | 违反「对话永不静默持久化」；误记无法撤回 |
| 文件正文默认进上下文 | 隐私默认值错误 + 上下文预算失控 |
| 检索路径调 LLM 做相关性 | 非确定性、慢、且让本地检索依赖远端密钥 |
| 扩展现有 `DocumentStorePort`（Markdown 编辑器） | 那是编辑器读写端口，语义与知识索引相反（写 vs 只读） |

## 影响

- 新增 3 个 SQLite 库与 4 个 AI 模块（19 个工具：memory 5 / documents 5 / files 4 /
  knowledge 1），`ActionKind` +3（OpenDocument/OpenFile/ConfirmMemory）、`UiBlockKind` +5（MemoryList/DocumentList/DocumentCard/DocumentReference/FileList）；前端新增 Knowledge 页与设置区。
- `cargo test --workspace`：333 → 499（+166）；零既有测试改动（除 4 个模块测试
  literal 补 `retrieval: None` 与 1 个章节读取断言按修复后语义更新）。
- 平台唯一改动：`PersonalHub.retrieval: Option<Arc<dyn RetrievalAugmenter>>`。
