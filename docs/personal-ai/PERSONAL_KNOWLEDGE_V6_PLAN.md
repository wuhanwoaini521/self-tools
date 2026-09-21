# SELF-TOOLS V6 · PERSONAL KNOWLEDGE LAYER — PLAN

> Gate 0 审计结论 + 四 Track 实施计划。基线：HEAD `3b1a0aa`（V5 Gate 10 收口）+
> 工作区 27 个已修改文件（V5 收尾/格式/局部行为调整，全部保留）。
> 基线验证（本次真实执行）：`cargo test --workspace` = **333 passed / 0 failed**
> （core 77 / application 146 / infrastructure 103 / server 7），工作区干净可编译。
> 实施状态见 [`V6_OVERNIGHT_STATUS.md`](V6_OVERNIGHT_STATUS.md)；最终架构见
> [`PERSONAL_KNOWLEDGE_V6.md`](PERSONAL_KNOWLEDGE_V6.md)（Gate 10 产出）。

---

## 1. Current architecture（Gate 0 审计，逐项核实）

### 1.1 V4/V5 平台现状（真实调用链）

| 面 | 位置 | 事实 |
| --- | --- | --- |
| `PersonalAgent` | `crates/application/src/personal_ai/agent.rs`（554 行） | 工具循环 `max_tool_rounds=4`；**零业务分支**（无 `if module == …`）；V5 起 `ToolExecutor` 为 async |
| `ModuleRegistry` / `ToolRegistry` | `personal_ai/registry.rs`（529 行） | 注册即校验 `module.action` + 风险门禁 `allowed_risk = Read \| SafeWrite`；SensitiveWrite/System 注册期拒绝 |
| `AppContext` | `core/src/personal_ai/types.rs` | `module/page/entity/selection/view_state`（`selection` 已有字段，V4 预留） |
| `ChatModelProvider` | `core/src/personal_ai/provider.rs` + `infra/src/personal_ai/llm.rs` | **唯一**模型抽象（V5 Gate 1 已融合 travel） |
| `ActionProtocol` | `core/src/personal_ai/types.rs` | `ActionKind = Navigate / OpenEntity / RefreshView / ShowPanel`（4 种） |
| `UiBlock` | 同上 | `kind = entity_list / entity_card / source_list / key_value / timeline_preview`（5 种） |
| Context 装配 | `personal_ai/context.rs` + `prompt.rs` | `ContextBudget{max_items:30, max_chars:6000}`；`assemble_system` 只注入**当前模块**上下文 |
| 模块清单 | `memory?` 无；`history` / `travel` / `geography` / `language` 四个 | 均为 `descriptor + tools + ContextProvider + register_*()` 同构形态（`geography.rs` 为样板） |
| 组合根 | `apps/desktop/src/personal_ai.rs::build_hub` | 单点装配；`AppState.ai: Arc<PersonalHub>`；命令 `personal_ai_status` / `personal_ai_chat` |
| 前端 | `ui/src/features/ai/{aiClient,aiTypes,AIPanel}` | 8 个 feature client；裸 `invoke` = 0；`AppContext` 由页面 `onContextChange` 上报 |

### 1.2 已有 Documents / Files / Storage 资产（决定「复用 vs 新建」）

| 资产 | 位置 | 结论 |
| --- | --- | --- |
| `DocumentStorePort` | `application/src/workflows/ports.rs`（`read` / `write` / `scan_markdown`） | **是 Markdown 编辑器的文件读写端口，不是知识索引**。V6 不复用它承载知识语义；其底层 `infra::read_utf8` 由新 Documents 内容端口复用 |
| `scan_markdown_files` | `infra/src/workspace_scanner.rs` | 递归扫描 Markdown、跳过 `.git`/`node_modules` 等；V6 Files 扫描器参考其 skip 规则但**独立实现**（需 mtime/size/二进制判定 + 允许根约束） |
| `write_utf8_atomic` / `read_utf8` | `infra/src/document_store.rs` | 只读复用 `read_utf8`；**V6 不使用任何写文件能力**（Principle 6） |
| Workspace | `core::workspace::WorkspaceFile` + `settings.workspace_path` | V6 将其作为 Documents 索引的**候选根之一**（用户可另行配置），不写入、不移动 |
| Settings | `core/src/settings.rs`（`AppSettings`，`serde(default)` 向后兼容） | V6 新增 `knowledge: KnowledgeSettings`（`serde(default)`，旧 settings.json 零迁移） |
| database/storage | 每域一个 SQLite：`config/{dashboard,travel,language,geography}.db`；V5 新增 `config/history_enrichment.db` | V6 沿用：`config/memory.db`、`config/documents.db`、`config/files.db` |
| session store | `application/src/personal_ai/session.rs`（内存，上限 60 条） | **Conversation ≠ Memory**：本文件已显式声明「绝不进入 PersonalMemoryStore」，V6 保持 |
| 现有搜索基础设施 | `core::travel::SearchProvider`（Web 搜索）+ SQLite LIKE | V6 P0 用**词法/元数据检索**（§49 不引入向量库）；Web 搜索仅 V5 富化使用，V6 不调用 |
| Embedding / PDF | 全仓 grep：**无** | PDF = 建立接口边界 + 「仅元数据」降级（§35）；Embedding = P1 保留抽象位 |

### 1.3 依赖方向（不变式，V6 必须保持）

```text
core        无内部依赖（新增 memory/documents/files/knowledge 纯契约）
application → core only（grep devtoolbox_infrastructure crates/application/src = 0，V6 必须保持 0）
infrastructure → core（只做实现：SQLite / 文件系统 / 抽取）
apps/desktop = 唯一组合根（端口适配器 + 命令 + Provider 装配）
apps/server  与 desktop 互不依赖（V6 不新增 server 面）
```

---

## 2. Memory model（Track A）

### 2.1 四条边界（不可破坏）

```text
Conversation ≠ Memory     会话消息永不自动成为 Memory
Business Knowledge ≠ Memory  History/Travel/Geography 数据不是 Memory
Document ≠ Memory         整份文档不进 Memory
AI ≠ 自动写入者            模型只能 propose；ACTIVE 必须由用户确认
```

### 2.2 数据模型（`core::memory::MemoryItem`）

```rust
MemoryCategory  = Preference | PersonalFact | ProjectFact | Environment | Routine | Instruction
MemoryStatus    = Candidate | Active | Rejected | Archived | Expired
MemorySourceType= ExplicitUser | ConversationCandidate | Import | System
MemorySensitivity = Normal | Private | Sensitive

MemoryItem {
  id, category, content, status, source_type, source_reference: Option<String>,
  created_at, updated_at, last_used_at: Option<i64>, expires_at: Option<i64>,
  confidence: f32, sensitivity, metadata: Value,
}
```

### 2.3 生命周期与写入 gate

```text
propose(draft)                        → Candidate（永不 Active）
confirm(id, ConfirmationSource::Ui)   → Active（记录 confirmed 来源与时间）
archive(id)                           → Archived（可恢复；排除默认检索）
reject(id)                            → Rejected（用户拒绝）
expire  ：expires_at 到期 → 派生 Expired（不参与检索）
```

硬性 gate（`MemoryWriteGate`，纯函数 + 单测）：

| 输入 | 结果 |
| --- | --- |
| 模型工具调用（任何工具） | **最多 Candidate**，不可能是 Active |
| 用户 UI 点击确认（`memory_confirm` 命令） | Active（`source_type = ExplicitUser`，若非则保留原来源 + `confirmed_by=ui`） |
| 用户显式保存意图（`记住…` 触发的前端 SaveMemory 动作，§26） | Active |
| 普通聊天 | 无任何持久化 |

`detect_secret`（core 纯函数，regex，确定性）：密码/口令/私钥/API key/token/JWT/`sk-…`/`AKIA…`/SSH key 体 → **拒绝写入**，返回受控错误，提示使用 credential store（§70）。

### 2.4 存储

`config/memory.db`（SQLite，gitignored），单表 `memory_items` + `meta(schema_version)`；
`INSERT … ON CONFLICT(id) DO UPDATE`；**无物理删除 API**（§20）。

### 2.5 Memory 工具（§17 的 P0 面）

| 工具 | 风险 | 语义 |
| --- | --- | --- |
| `memory.search` | Read | 关键词 + category + limit；**永不返回 Sensitive**（§69） |
| `memory.list` | Read | 按 status/category 列出；**永不返回 Sensitive** |
| `memory.get` | Read | 单条；Sensitive → 受控拒绝 |
| `memory.save` | SafeWrite | **只产生 Candidate** + `confirm_memory` Action（§15/§25）；绝不 Active |
| `memory.archive` | SafeWrite | 可恢复归档（不删除） |

`ACTIVE` 的唯一通道是 UI 确认命令 `memory_confirm` / `memory_save`（前端驱动 = 真实用户意图）。

---

## 3. Document integration（Track B）

### 3.1 与现有资产的关系

现有 `DocumentStorePort` 是**编辑器文件 I/O**；V6 Documents 是**知识索引**：
新增 `DocumentIndexPort`（SQLite `config/documents.db`）+ `DocumentContentPort`（抽取，infra 实现）。
旧 `DocumentStorePort` 与其消费方（Markdown 编辑器）**完全不动**；V6 只复用 infra `read_utf8` 的编码处理思想。

### 3.2 契约

```rust
DocumentType = Markdown | Text | Json | Pdf | Other
DocumentMeta { document_id, root_id, title, document_type, path, relative_path,
               size_bytes, modified_at, indexed_at, chunk_count,
               content_available: bool, visibility: DocumentVisibility, index_error: Option<String> }
DocumentChunk { document_id, chunk_id, ordinal, text, location: DocumentLocation, metadata }
DocumentLocation { section: Option<String>, page: Option<u32>, char_start, char_end }
ChunkConfig { target_chars: 1200, overlap_chars: 200, max_document_bytes: 2_000_000 }
```

`chunk_text` 为 core 纯函数（确定性、可单测、按段落/行边界切分 + overlap），配置集中在 `ChunkConfig`（§33 不散落）。

### 3.3 索引触发与增量

```text
触发：手动 scan（前端命令 documents_index_scan）+ 启动轻量同步（setup）
增量：按 (path, mtime, size) 指纹；未变 → 跳过（§90）
失败隔离：单文件抽取/解析失败 → 记录 index_error 继续（§91）
体积门：> max_document_bytes → 仅元数据（§92）
```

### 3.4 工具（全 Read）

`documents.search` / `documents.get` / `documents.read`（chunk/section/range，默认不整份读）/ `documents.get_context` / `documents.list_recent`。
**索引写入不是模型工具**（模型不能触发全盘扫描）；由用户命令与启动同步驱动。

### 3.5 PDF

`DocumentContentPort` 接口先立：`ExtractedContent::Text | MetadataOnly(reason)`。
`.pdf` 目前 → `MetadataOnly("pdf extraction not available")`，索引保留 title/type/size/mtime（§35，不引入重型转换平台）。
`files.search` 的「韩国旅行 PDF 在哪」场景因此仍可回答（文件名/元数据命中）。

### 3.6 Provenance

每个检索结果携带 `Provenance { source_kind, source_id, title, location, path, module }`；
`documents.read` 返回 `document_id + chunk_id + location`，回答可回溯（§37）。

---

## 4. File integration（Track C）

### 4.1 与 Documents 的边界（§40）

```text
Documents = 已进入 self-tools 知识系统的文档（抽取 + 分块 + 内容检索）
Files     = 允许根内的文件实体（元数据检索 + 安全读取 + 打开）
```

物理隔离：不同 SQLite 文件、不同表、不同端口、不同模块。同一路径可同时出现在两侧，`knowledge` 层做跨源去重（§5.3）。

### 4.2 允许根（§43）

```rust
KnowledgeRoot { id, label, path, enabled }
KnowledgeSettings { file_roots: Vec<KnowledgeRoot>, document_roots: Vec<KnowledgeRoot>,
                    max_file_bytes, max_read_chars, … }
```

未配置 → Files/Documents 模块如实报告「未配置允许目录」，工具返回受控错误，**其余功能零影响**（Principle 7）。
不允许 AI 读任意路径：任何路径参数都必须先解析再校验。

### 4.3 路径安全（§44，infra 实现 + core 策略）

```text
1. canonicalize(root) 与 canonicalize(target)（解析 symlink）
2. target 必须 starts_with root（拒绝 traversal `..`，拒绝 symlink escape）
3. deny 规则（大小写不敏感）：.env / *.pem / *.key / id_rsa / id_ed25519 / .ssh / .aws /
   .gnupg / credentials / keychain / *.pfx / *.p12 / secrets*
4. 非文本（NUL/UTF-8 失败）→ 仅元数据（§47）
5. 读取上限：max_read_chars / max_file_bytes
```

`FileAccessPolicy` 判定为 core 纯函数（roots + deny patterns + 相对路径），canonicalize 由 infra 端口提供 → 可单测且不依赖真实文件系统。

### 4.4 工具

| 工具 | 风险 | 语义 |
| --- | --- | --- |
| `files.search` | Read | filename / extension / modified range / root / query（元数据 + 词法） |
| `files.get_metadata` | Read | 单文件元数据（含 content_kind） |
| `files.read_text` | Read | 安全读取（限长、限字符）；拒绝规则命中 → 受控拒绝 |
| `files.open` | Read | 返回 `OpenFile` Action（**不执行 shell**，§46） |

索引触发同 Documents（手动 + 启动同步）。

---

## 5. Retrieval architecture（Track D）

### 5.1 服务

`application::knowledge::KnowledgeRetrievalService`，内含三个检索器（memory / documents / files）+
确定性 `RetrievalPlanner`（规则式，**不引入 KnowledgeAgent / RetrievalAgent**，§56）。

```text
User Query
   ↓ RetrievalPlanner（词法意图判定：personal / document / file / business；规则式）
┌────────┬───────────┬────────┐
│ Memory │ Documents │ Files  │        （各自 fetch 过量候选）
└────────┴───────────┴────────┘
   ↓ merge → dedupe（identity + 跨源 path 优先 Document）→ rank → budget 截断
KnowledgeRetrievalOutcome { results, diagnostics }
```

### 5.2 统一结果

```rust
KnowledgeSourceKind = Memory | Document | File | Module
KnowledgeResult { source_type, source_id, title, snippet, score: f32,
                  location: Option<String>, metadata: Value, provenance: Provenance }
```

### 5.3 排序与去重（确定性，可单测）

- Memory：词法命中权重 + 类别权重 + 近期使用/创建衰减；`Archived/Rejected/Expired/Sensitive` 不入结果。
- Documents：store 内 SQL LIKE 候选 + Rust 端片段打分（标题命中 > 正文命中；chunk 级）。
- Files：文件名命中权重 > 路径命中；extension/root 过滤先行。
- 跨源：同一 canonical path 的 Document 与 File → 保留 Document（信息更丰富），File 标记 `merged_into`。

### 5.4 Context budget（§64/§65）

```rust
KnowledgeBudget { max_results: 8, max_chars: 4000, max_per_source: 4, max_memories: 5,
                  max_document_chunks: 3, max_files: 0 }
```

- 自动注入（agent 前置 stage）：memory ≤ 5、document chunks ≤ 3、files = 0（位置类问题走工具）；
- 工具调用（`knowledge.search`）允许 files，但同样受 `max_results/max_chars` 硬截断；
- 预算与 `ContextBudget` 并存：前者约束知识层，后者约束模块上下文。

### 5.5 PersonalAgent 集成（§22/§55/§117）

`PersonalHub` 新增**一个可选通用 stage**：

```rust
pub trait RetrievalAugmenter: Send + Sync {          // application 端口，无语义
    async fn augment(&self, query: &str, app_context: &AppContext, budget: &KnowledgeBudget)
        -> Result<Option<String>, AgentError>;        // None = 无相关内容（不注入）
}
```

`agent.rs` 只增加「若注册了 augmenter，则 await 并把返回文本追加到 system prompt」——**平台能力，不是业务分支**（与 V5 的 async ToolExecutor 同性质）。
`KnowledgeRetrievalService` 实现该端口（desktop 组合根装配）。检索为空 → `None` → prompt 不出现知识段（§67 不猜测、§68 不无条件发送个人数据）。

### 5.6 Facility 工具

`knowledge.search`（Read）作为 facade（§57/§58）：`query + sources? + limit?` → 统一结果 + provenance。

---

## 6. Context aggregation（§63）

```rust
PersonalKnowledgeContext {   // application::knowledge
  app_context: AppContext,
  module_context: Option<ContextBundle>,
  memories: Vec<KnowledgeResult>,
  document_refs: Vec<KnowledgeResult>,
  file_refs: Vec<KnowledgeResult>,
  omitted: OmittedSummary { memories: usize, documents: usize, files: usize },
}
```

不做字符串拼接：`render_for_prompt(&self) -> String` 单一渲染入口（供 augmenter 使用），上限 `KnowledgeBudget.max_chars`。

---

## 7. Privacy boundaries（§68-§72）

| 边界 | 机制 |
| --- | --- |
| 任意文件读 | 允许根 + canonicalize + denylist + 读取上限；无「绝对路径直读」通道 |
| 任意文件写 | V6 **不存在**任何文件写/移动/删除 API（`write_utf8_atomic` 不被知识层引用） |
| 聊天自动存记忆 | 工具最多产生 Candidate；Active 只能用户确认 |
| Secret 入 Memory | `detect_secret` 拒绝 + 单测 |
| Sensitive 无条件注入 | Sensitive 永不进入工具结果与自动注入；仅 UI 管理页可见 |
| 全量个人数据入 prompt | 仅 query 驱动的有界检索（budget 硬截断） |
| 日志泄漏 | 观测只计数（id/type/size/count/duration），无正文（§94） |

---

## 8. Storage choice（§84-§86）

```text
SQLite（rusqlite，workspace 既有依赖，无新依赖）
config/memory.db       memory_items(schema_version/meta)
config/documents.db    documents + document_chunks (+ meta)
config/files.db        file_entries (+ meta)
```

迁移：`CREATE TABLE IF NOT EXISTS` + `meta.schema_version` 单调递增 + `ALTER TABLE ADD COLUMN` 形式的非破坏增量；幂等（重复启动无副作用）；`ensure_schema` 单测覆盖「空库 → v1」「已存在 → 不重建」。
不引入 Mongo/Postgres/Redis/向量库（§84/§136）。

---

## 9. Migration（既有数据）

- `AppSettings` 新增 `knowledge` 字段：`serde(default)` → 旧 `config/settings.json` 直接可读（无需迁移脚本）；
- 既有 SQLite（dashboard/travel/language/geography/history_enrichment）：**零改动**；
- 既有 Documents（Markdown 编辑器链路、`WorkspaceFile`）：**零改动**，ID/路径/元数据不迁移、不丢失（§87）；
- 新增索引库首次启动为空 → 需要一次手动 scan 或启动同步（不静默全盘扫描）。

---

## 10. Frontend design（§73-§79）

```text
ui/src/features/knowledge/
  knowledgeClient.ts     Feature Client（沿用 transport 模式，无裸 invoke）
  KnowledgePage.tsx      Knowledge 入口：Memory / Documents / Files 三区
  MemoryPanel.tsx        Active / Candidates / Archived 分组；搜索/分类/状态/来源/编辑/归档
  DocumentsPanel.tsx     Documents / Search / Recent / References
  FilesPanel.tsx         Allowed Roots 内 Search / Recent / Metadata / Open（不做 Finder clone）
  MemoryConfirmCard.tsx  「是否记住这条信息？[记住] [不要]」
```

- `App.tsx` 顶栏增加 Knowledge 入口；`AIPanel` 渲染 `memory_list/document_list/file_list/document_reference` UI Block 与 `confirm_memory` / `open_file` / `open_document` Action；
- `AppContextPayload` 支持 `documents` / `files` 模块 + `selection`；
- SettingsDialog 增加 Knowledge 区（允许根增删 + 扫描触发 + 索引状态）；
- 模型不可用（无 key）时 Memory/Documents/Files 页面与搜索照常（§134）。

### 10.1 Action / UI Block 扩展（最小集，§80）

```text
ActionKind  += OpenDocument | OpenFile | ConfirmMemory      （4 → 7）
UiBlockKind += MemoryList | DocumentList | DocumentCard | DocumentReference | FileList  （5 → 10）
```

---

## 11. Test matrix（Gate 2-5 + Gate 9）

| 面 | 用例 | 备注 |
| --- | --- | --- |
| Memory（§95） | explicit save→ACTIVE；普通对话→无 Active；search 命中；archive 后不检索；secret 拒绝；candidate→confirm→active | Fake store + 真 SQLite（infra 侧） |
| Documents（§96） | search / get / read chunk / 无效 id / 大文档边界 / provenance / 不整份注入 | 真 SQLite + 临时目录 |
| Files（§97） | 允许根内 / 根外拒绝 / `..` traversal 拒绝 / symlink escape 拒绝 / 文本读取 / 二进制仅元数据 / 不存在 / open action | `tempfile::tempdir` |
| Retrieval（§98） | A 个人偏好→memory；B 已知文档→documents；C 文件位置→files；D History 问题→history 优先；E 无结果→不编造 | 内存 Fake 三源 |
| 平台（Gate 6） | agent 路由（capabilities 过滤）、无业务 hardcode（grep + 路由测试）、budget 截断 | |
| 回归 | `cargo test --workspace` / `cargo check --workspace --all-targets` / `npm run build` / pipeline `uv run pytest` | |

---

## 12. Gate 顺序与 PASS 判据

| Gate | 内容 | PASS 判据 |
| --- | --- | --- |
| 0 | 审计 + 本计划 | 落盘 + 基线 333 全绿 |
| 1 | Knowledge contracts | `MemoryItem/KnowledgeResult/DocumentChunk/FileMetadata/Provenance` + 风险契约编译通过 |
| 2 | Personal Memory | save/search/list/archive/confirm + 无静默持久化（§113） |
| 3 | Documents module | 注册 + context provider + search + read + provenance（§114） |
| 4 | Files module | 注册 + allowed roots + search + metadata + safe read + open action + 安全测试（§115） |
| 5 | Knowledge Retrieval | 三源合并 + budget + provenance + facade（§116） |
| 6 | PersonalAgent integration | 零业务 if/else（§117） |
| 7 | Frontend | 管理页 + 检索 + AI 使用知识上下文（§118） |
| 8 | Privacy review | 5 条「impossible」逐条核验（§119） |
| 9 | Regression | 全量套件 + V5 无退化（§120/§121） |
| 10 | Docs | V6 文档 + ADR-005 + 架构文档更新 + Final Report |

## 13. Rollback

每 Track 独立 commit；三个新 SQLite 库独立（删除即清，不影响既有域）；`AppSettings.knowledge` 走 `serde(default)`，回滚无需数据迁移；
唯一平台改动（`PersonalHub.retrieval` 可选 stage）可通过不装配 augmenter 完全关闭。

## 14. Future extension（P1，Mandatory 全绿后才考虑）

```text
EmbeddingProvider + SemanticIndex（独立 abstraction，ChatModelProvider 仍唯一生成入口）
hybrid ranking / 增量后台索引 / 文件 watcher / 会话持久化 / PDF 抽取实现 / 物理删除管理页
```
