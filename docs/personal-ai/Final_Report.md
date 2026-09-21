# V6 · Personal Knowledge Layer — Final Report

- 日期：2026-09-21
- 范围：`crates/core` / `crates/application` / `crates/infrastructure` / `apps/desktop`（lib + ui）
- 工作树：全部改动**未 commit**（用户要求保留）。基线 HEAD `3b1a0aa`。

## 1. 交付物

| 类别 | 内容 |
| --- | --- |
| 核心契约 | `core/src/{memory,documents,files,knowledge}/`（纯数据 + 纯函数：`chunk_text`、`keywords`/`normalize`、provenance、budget、diagnostics） |
| 应用服务 | `application/src/{memory,documents,files,knowledge}/`（域服务 + 端口 + 19 工具 / 4 ContextProvider / 3 检索器） |
| 基础设施 | `infrastructure/src/{memory,documents,files}/`（三个 SQLite store + `LocalDocumentSource` + `LocalFileSystem`） |
| 组合根 | `apps/desktop/src/knowledge.rs`（`KnowledgeRuntime::build` + `startup_sync` + 适配器 + 4 测试） |
| 命令层 | `apps/desktop/src/lib.rs`：22 条 V6 命令 + `AppState.knowledge` + 错误码 4 个 |
| Agent 集成 | `personal_ai/build_hub` 注册 4 模块 + `hub.retrieval` 通用 stage；`agent.rs` 零业务分支 |
| 前端 | `ui/src/features/knowledge/**`（7 文件）、`aiTypes.ts`、`AIPanel.tsx`、`App.tsx`、`SettingsDialog.tsx`、`styles.css` |
| 文档 | `docs/personal-ai/PERSONAL_KNOWLEDGE_V6.md`、`docs/architecture/ADR-005-personal-knowledge-layer.md`、`docs/personal-ai/V6_OVERNIGHT_STATUS.md`、本文件 |

## 2. 验证证据（全部真实执行）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 全量测试 | `cargo test --workspace` | **508 passed / 0 failed**（基线 333，+175） |
| core | `cargo test -p devtoolbox-core` | 117 passed |
| infrastructure | `cargo test -p devtoolbox-infrastructure` | 124 passed |
| application | `cargo test -p devtoolbox-application` | 256 passed |
| desktop | `cargo test -p devtoolbox-desktop --lib` | 4 passed |
| 桌面编译 | `cargo check -p devtoolbox-desktop --tests` | 0 error / 0 warning |
| 前端类型 | `npx tsc --noEmit -p apps/desktop/ui/tsconfig.json` | 0 error |
| 分层不变式 | `grep devtoolbox_infrastructure crates/application/src` | 0 命中 |
| 无 shell | `grep -rn "std::process\|Command::new" <V6 文件>` | 0 命中（仅注释提及） |
| 无裸 invoke | `grep -c "invoke(" ui/src/features/knowledge/knowledgeClient.ts` | 0 |

新增测试分布（application lib 实测）：documents 域 15 + documents 模块 9、
files 域 13 + files 模块 12、knowledge 域 18（tests 13 + context 4 + observability 1）
+ knowledge 模块 7、memory 域 13 + memory 模块 8；另有组合根 4（desktop）、
infrastructure 三域 store/fs/extract 测试、core 契约与 `chunk_text`/`keywords` 测试。

## 3. 关键设计落地（对应 ADR-005）

1. **三域三库**：`config/{memory,documents,files}.db`；共享形状在 core；分层 grep 验证为 0。
2. **候选确认写入**：`memory.save` 只写 CANDIDATE + `confirm_memory` Action；ACTIVE 只能由
   `memory_confirm` / `memory_save`（用户显式动作）产生；secret 正则写入前拦截。
3. **文档分块 / 文件元数据**：文档正文切 chunk（1200/200、标题感知、章节+字符区间定位）；
   PDF/超大/失败降级仅元数据；文件只索引元数据，正文读取必须过 `authorize`
   （允许根 + canonicalize + traversal + deny + 大小）。
4. **确定性检索**：合并 → 规则打分 → 同路径去重（Document > File）→ 预算截断；
   单源失败隔离；无命中如实「没有找到」；`max_files=0` 默认（文件定位走工具）。
5. **平台通用增强**：`PersonalHub.retrieval: Option<Arc<dyn RetrievalAugmenter>>`；
   不装配即关闭。`agent.rs` 只加 3 行 stage，零业务分支。

## 4. 实施期修复的真实缺陷

1. memory store 搜索语义：整串 LIKE → tokens OR（中文多词查询原本永不命中）。
2. documents 章节读取定位错章（chunk overlap 带入上一章尾巴）→ 按标题原文位置起读，
   `location.section` 取读取区间实际落入章节。
3. 子代理中断导致的 4 个测试文件缺失 / 1 个占位 → 补齐（约 2400 行测试）。
4. 11 个非 ASCII 测试函数名 → ASCII 重命名（Rust 不允许）。
5. 组合根编译问题：`DocumentStoreError` 未重导出、infra 方法私有、`FileMetadata` 导入缺失。
6. `App.tsx` 默认设置缺 `knowledge` 字段 → 补齐（与 core 默认一致）。
7. `KnowledgePage` 未挂载 + 三个 V6 Action 无执行入口 → 补挂载、`knowledgeIntent`
   state 与 `onConfirmMemory`/`onOpenFile`/`onOpenDocument` handlers。
8. Knowledge 面板样式缺失（`.knowledge-*` 等类 0 条定义）→ 补 Design Tokens 样式。
9. Gate 8 架构审查 5 项发现全部修复：`Module` 源从工具枚举移除（无 retriever）、
   Explicit `limit` 不再被每源预算钳制、删除死端口 `DocumentIndexPort::chunk`、
   删除恒等函数 `retrievers()`、四模块重复助手抽为 `personal_ai/args.rs`。
10. Gate 8 隐私审查 8 项发现全部修复（含 2 个 high）：
    - secret 正则绕过（未收录厂商 / 中文无标点 / **metadata 字段旁路**）→
      `flatten_json(metadata)` 递归检测 + 正则扩充（`github_pat_`/`xoxb-`/
      `dashscope-`/连接串/named token/≥32 位 base62）；
    - Documents 索引缺 deny 规则（凭据正文入库）→ `index_file`/`search` 复用
      `FileAccessPolicy::is_denied`，deny 文件只写元数据且不进检索；
    - `files.open` 旁路 deny → authorize 后补 restricted 检查；
    - `files.recent` 不过滤 restricted → service 层 retain；
    - `is_denied` 裸词后缀误命中 → 模式分两类（`.` 前缀后缀匹配 / 裸词全等）；
    - 根 canonicalize 失败回退原文 → fail-closed 丢弃该根；
    - 超长错误回显字符数（侧信道）→ 只保留上限；
    - `outline` 用 `usize::MAX` 读全文 → 有界 `OUTLINE_SCAN_CHARS = 4_800`。

## 5. 已知限制（P1，范围外）

Embedding/语义检索、PDF 正文抽取、文件 watcher、后台增量索引、会话持久化、
记忆物理删除 UI、`apps/server` 面。回滚路径：删三个 `.db` + 不装配 retrieval stage。

## 6. 给审查者的入口

- 隐私：`memory/service.rs`（secret 正则 + 敏感度过滤）、`files/service.rs::authorize`、
  `knowledge/observability.rs`（diagnostics 不含正文）、`personal_ai/knowledge.rs`（ui_hint）。
- 架构：`personal_ai/agent.rs`（stage）、`registry.rs`（风险门禁）、`apps/desktop/src/knowledge.rs`（组合根）。
