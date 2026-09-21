# SELF-TOOLS V6 · PERSONAL KNOWLEDGE LAYER — 终版架构

> 状态：✅ 已实施（Gates 0-10 PASS，测试 499/499，见
> [`V6_OVERNIGHT_STATUS.md`](V6_OVERNIGHT_STATUS.md)；计划见
> [`PERSONAL_KNOWLEDGE_V6_PLAN.md`](PERSONAL_KNOWLEDGE_V6_PLAN.md)；决策记录见
> [`ADR-005-personal-knowledge-layer.md`](../architecture/ADR-005-personal-knowledge-layer.md)）。
> 基线 HEAD `3b1a0aa`；全部改动未 commit（保留工作树）。

---

## 0. 一句话

给 Personal AI 加**三层本地个人知识**（长期记忆 / 文档索引 / 文件检索）+ **一个确定性
统一检索入口**；AI 对用户数据**只读**，记忆写入必须**用户确认**，`PersonalAgent`
核心仍然零业务分支。

```text
BEFORE: 对话结束即遗忘；文档与文件对 AI 不可见
AFTER : memory.db / documents.db / files.db 三库 + knowledge.search 一次查询四类源
        （Memory / Documents / Files / Module），每条结果带 provenance
```

---

## 1. 分层与依赖（不变式，实测保持）

```text
core            memory/documents/files/knowledge 纯契约（数据形状 + 纯函数）
application  →  core only（域服务 + 端口 + personal_ai 模块适配器）
infrastructure →  core（SQLite store / LocalDocumentSource / LocalFileSystem）
apps/desktop  =  唯一组合根（knowledge.rs 适配器 + 22 条 Tauri 命令 + build_hub 装配）
```

实测：`grep devtoolbox_infrastructure crates/application/src` = 0；共享数据形状
（`DocumentChunk` / `FileMetadata` / `KnowledgeResult` / `Provenance` …）全部在 core，
infra 与 application 各自 `pub use` 消费（同 `history_enrichment` 形态）。

| 域 | core 契约 | application | infrastructure | 库 |
| --- | --- | --- | --- | --- |
| Memory | `memory/{model,gate}.rs` | `memory/{ports,service}.rs` + `personal_ai/memory.rs` | `memory/store.rs` | `config/memory.db` |
| Documents | `documents/{model,chunk}.rs` | `documents/{ports,service}.rs` + `personal_ai/documents.rs` | `documents/{store,extract}.rs` | `config/documents.db` |
| Files | `files/model.rs` | `files/{ports,service}.rs` + `personal_ai/files.rs` | `files/{fs,store}.rs` | `config/files.db` |
| Retrieval | `knowledge/model.rs` | `knowledge/{ports,service,retrievers,context,observability}.rs` + `personal_ai/knowledge.rs` | —（纯编排） | — |

---

## 2. Memory（Track A）

模型：`MemoryItem{id, category, content, status, source_type, source_reference,
sensitivity, confidence, created_at, updated_at, last_used_at, expires_at, metadata}`。

生命周期（唯一写入路径）：

```text
模型 memory.save ──► CANDIDATE + Action::confirm_memory（UI 确认卡片）
                          │
用户点「记住」──► memory_confirm ──► ACTIVE ──► 进入模型检索路径
用户点「忽略」──► memory_reject   ──► REJECTED（不再检索）
用户归档    ──► memory_archive  ──► ARCHIVED（不再检索）
用户在设置页主动保存 memory_save ──► ACTIVE（唯一另一入口，显式用户动作）
```

- 敏感度：`Normal` 模型可见；`Private/Sensitive` **只在用户显式查看时可见**
  （`memory.list/get` 的 UI 路径 `include_sensitive=true`，模型路径恒 false）。
- secret 门覆盖 `content` **与 `metadata`**（`flatten_json` 展平后检测），正则覆盖
  主流云厂前缀、连接串、named token 与 ≥32 位 base62 通用形态。
- secret 正则（key/token/password/私钥形态）在 service 写入前拦截 → 受控错误，不落库。
- 检索：`crate::text::keywords`（ASCII 词 + CJK bigram）→ tokens **任意命中**；
  排序 = 相关度 + `updated_at` + id（确定性）。

## 3. Documents（Track B）

- 索引：`LocalDocumentSource::scan_root`（跳过隐藏文件与噪声目录）→
  `extract`（文本 / PDF→仅元数据 / 体积门 / 编码失败记录原因）→
  `chunk_text`（目标 1200 字符、overlap 200、Markdown 标题感知、`max_chunks` 封顶）→
  `documents` + `document_chunks` 两表；增量靠 `DocumentFingerprint`
  （size+mtime）比对，消失文件只清索引。
- 失败隔离：单文件失败写元数据 + `index_error`，其它文件照常索引（§91）。
- 模型工具（5 个，全 Read）：`search`（标题命中优先，带位置与片段）/ `get`（卡片）/
  `read`（chunk / section / offset+max_chars 三选一）/ `get_context`（紧凑：元数据 +
  章节标题 + 文首，**不整份进 prompt**）/ `list_recent`。
- 索引写入**不是模型工具**（`documents.scan` 不存在）；只有桌面命令 `documents_scan`
  与启动同步能写索引。
- 章节读取语义（实施期修复）：从标题在原文中的位置起读，`location.section` 取读取
  区间实际落入的章节，避免 chunk overlap 把上一章尾巴带进来。

## 4. Files（Track C）

- 索引：元数据 only（`file_id` / root / 相对路径 / size / mtime / content_kind /
  restricted / index_error）；正文读取一律实时、受权限约束。
- 授权链 `FileService::authorize`：允许根（`KnowledgeRoot`）→ canonicalize →
  traversal（`..`）拒绝 → deny 列表（凭据/密钥类标注 `restricted`）→ 大小上限。
- 降级矩阵：二进制 / NUL / 超大 → **只回元数据 + 原因**，字节不进 ToolResult；
  根外 / 不存在 / 目录目标 → 受控错误（错误文本不回显内容）。
- 模型工具（4 个，全 Read）：`search` / `get_metadata` / `read_text` / `open`。
  `open` 只返回 `Action::open_file`，**backend 无 shell/进程执行入口**。
- `FileQuery`：query / extension / root_id / modified_after / limit；
  `include_restricted` 模型路径恒 false（§71）。

## 5. Retrieval（Track D）

`KnowledgeRetrievalService`（纯编排，无 IO 自身）：

```text
各源候选（MemoryRetriever / DocumentRetriever / FileRetriever，单源失败只记 errors）
  → 规则打分排序（分数降序；同分按源优先级 + id，确定性）
  → 同路径去重（Document 优先于 File；Memory 最优先）
  → 预算截断（max_results / max_per_source / max_chars，超出记 dropped_by_budget）
```

- 统一结果 `KnowledgeResult{source_type, source_id, title, snippet, score, location,
  metadata, provenance}`；provenance 必填（`source_kind/source_id/title/path/location/module`）。
- 观测 `KnowledgeDiagnostics`：只计数与耗时，**不含正文**，可安全展示/记录。
- 预算默认：`max_results 8 / max_chars 4000 / max_per_source 4 / memories 5 /
  document_chunks 3 / files 0`（文件定位默认走工具，§22）。
- 自动注入（`RetrievalAugmenter`，平台通用 stage）：查询 ≥2 字符且记忆类结果
  分数 ≥0.5 才注入 prompt 段；无命中/低相关/失败 → `None`（绝不编造）。
- `knowledge.search` 工具（facade）：`sources` 限定源（未知值 → 参数错误）、
  `limit` 上限；空命中返回「没有找到相关个人资料」+ diagnostics，**不产出 ui_hint**。

## 6. PersonalAgent 集成（Gate 6）

唯一平台改动：`PersonalHub.retrieval: Option<Arc<dyn RetrievalAugmenter>>`。
`agent.rs` 在 `assemble_system` 与工具结果之间插入一个**无业务知识**的 stage：

```text
if let Some(augmenter) = &self.hub.retrieval { augmenter.augment(query, &app_context).await }
```

不装配即完全关闭（回滚开关）。四个新模块全部走
`ModuleDescriptor + tools + ContextProvider + register_*()`，与 geography 同构；
`ToolRegistry` 注册期校验 module/risk（`Read | SafeWrite`）。

## 7. 前端（Gate 7）

- `features/knowledge/`：`KnowledgePage`（Memory / Documents / Files 三面板）+
  `knowledgeClient`（21 个命令的薄封装，裸 `invoke` = 0）+ `knowledgeTypes`。
- `aiTypes.ts`：`ActionKind` +3（`ConfirmMemory`/`OpenDocument`/`OpenFile`）；
  `UiBlockKind` +5（`MemoryList`/`DocumentList`/`DocumentCard`/`DocumentReference`/`FileList`）。
- `AIPanel.tsx`：渲染 `metadata.ui_hint`（模型承诺原样回传 action/ui_blocks）；
  `MemoryConfirmCard` 提供「记住 / 忽略」。
- `SettingsDialog.tsx`：允许根管理（文件根 + 文档根、启用开关、体积/字符/条数上限、
  启动同步开关）；空 = 未配置，页面如实提示而非报错。
- `App.tsx`：导航注册 `Knowledge` + `defaultSettings.knowledge` 默认值。

## 8. 隐私边界（Gate 8，逐条核验）

| # | 声明 | 核验 |
| --- | --- | --- |
| 1 | 对话内容永不自动成为记忆 | `memory.save` 只写 CANDIDATE；confirm 只来自用户动作；测试锁定 |
| 2 | 敏感记忆不进模型路径 | `include_sensitive` 模型路径恒 false；`MemoryRetriever` 过滤 |
| 3 | 凭据/密钥文件不进检索 | `restricted` 标注 + 模型路径 `include_restricted=false` |
| 4 | 文件字节不默认进模型上下文 | 正文读取必须授权 + 预算默认 `max_files=0` |
| 5 | backend 不执行 shell | `files.open` 只回 Action；`grep std::process` 在 V6 代码 = 0 |
| 6 | 观测不含正文 | `KnowledgeDiagnostics` 仅计数/耗时（字段审查） |
| 7 | 三库独立、无网络 | 检索路径无 reqwest/无 LLM 调用；库文件各自独立 |
| 8 | deny 规则对「打开 / 检索 / 索引」一致生效 | `files.open`/`files.recent` 均查 restricted；Documents `index_file`/`search` 复用同一 `is_denied` |
| 9 | 拒绝不回显输入特征 | 超长错误只报上限；secret 检测只报类别；拒绝错误不回显文件内容 |
| 10 | 根路径校验 fail-closed | 根 canonicalize 失败即丢弃该根（不回退原文） |

## 9. 测试矩阵（Gate 9）

| 面 | 位置 | 覆盖 |
| --- | --- | --- |
| Memory 域 + 模块 | `memory/tests.rs`、`personal_ai/memory/memory_tests.rs` | 生命周期、secret 拒绝、candidate gate、工具面、context provider |
| Documents 域 + 模块 | `documents/tests.rs`、`personal_ai/documents/documents_tests.rs` | 增量索引、体积门、失败隔离、精排、读取边界、provenance、模块工具面 |
| Files 域 + 模块 | `files/tests.rs`、`personal_ai/files/files_tests.rs` | 授权（根内/根外/traversal/symlink）、降级、open action、只读工具面 |
| Retrieval 域 + 模块 | `knowledge/tests.rs`、`personal_ai/knowledge/knowledge_tests.rs` | 合并、确定性排序、去重、预算、单源失败隔离、augment 门槛、facade |
| Infra | `infrastructure/src/{memory,documents,files}/` 内测试 | SQLite 语义、扫描 skip、NUL/TooLarge、多 token OR 搜索 |
| 组合根 | `apps/desktop/src/knowledge.rs` 内测试 | tempdir 装配、startup_sync、缺根降级 |
| 平台 | `personal_ai/agent.rs` + 4 个既有模块测试 | retrieval stage 插入、零业务分支、既有模块不退化 |

## 10. 已知限制（P1，明确范围外）

Embedding / 语义检索、PDF 正文抽取、文件 watcher、后台增量索引、会话持久化、
记忆物理删除 UI、`apps/server` 面。回滚：删三个 `.db` + 不装配 `retrieval` stage。
