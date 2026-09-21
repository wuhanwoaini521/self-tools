# V6 · Personal Knowledge Layer — 实施状态

> 实时状态（Gate 0–10）。基线：HEAD `3b1a0aa` + 未提交工作区。
> 全部工作未 commit（用户明确要求保留工作树，见交付说明）。

## 测试总览（真实执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test -p devtoolbox-core` | 116 passed / 0 failed |
| `cargo test -p devtoolbox-infrastructure` | 124 passed / 0 failed |
| `cargo test -p devtoolbox-application` | 248 passed / 0 failed |
| `cargo test -p devtoolbox-desktop --lib` | 4 passed / 0 failed |
| `cargo test --workspace` | **499 passed / 0 failed**（基线 333，新增 166） |
| `cargo check -p devtoolbox-desktop` | 通过（1 条非阻塞 warning） |
| `npx tsc --noEmit -p apps/desktop/ui/tsconfig.json` | 通过（0 error） |

## Gate 状态

| Gate | 内容 | 状态 | 证据 |
| --- | --- | --- | --- |
| 0 | 审计 + 计划 | ✅ | `PERSONAL_KNOWLEDGE_V6_PLAN.md`；基线 333 全绿 |
| 1 | Knowledge contracts | ✅ | `core/src/{memory,documents,files,knowledge}/`；`ActionKind` +6 / `UiBlockKind` +8；`settings.knowledge`（serde default） |
| 2 | Personal Memory | ✅ | `memory/service.rs` 16 方法 + `personal_ai/memory.rs` 5 工具；secret 正则写入前拦截；save 只写 CANDIDATE（测试 `save_tool_only_creates_candidate_and_returns_confirm_action`） |
| 3 | Documents module | ✅ | `personal_ai/documents.rs` 5 Read 工具 + ContextProvider；`get_context` 走 `chunk_text`（同索引函数）只回标题+文首；provenance 测试通过 |
| 4 | Files module | ✅ | `personal_ai/files.rs` 4 Read 工具；`files.open` 只回 Action；traversal/symlink/根外/二进制降级测试通过 |
| 5 | Knowledge Retrieval | ✅ | `knowledge/service.rs`（合并/排序/同路径去重 Document 优先/预算截断/单源失败隔离）+ `retrievers.rs` + facade `knowledge.search` |
| 6 | PersonalAgent integration | ✅ | `agent.rs` 只在 `assemble_system` 与工具结果之间插入**通用** `hub.retrieval` stage；零业务 if/else；4 个既有模块测试迁移补 `retrieval: None` |
| 7 | Frontend | ✅ | `features/knowledge/` 7 文件（Memory/Documents/Files 面板 + KnowledgePage + client + types）；`aiTypes.ts` 新 Action/UiBlock；`AIPanel.tsx` 渲染分支；`App.tsx` 导航 + 默认设置；`SettingsDialog.tsx` 允许根配置 + CSS |
| 8 | Privacy review | ✅ | 隐私审查 agent 报告见下；5 条 impossible 逐条核验 |
| 9 | Regression | ✅ | workspace 499 全绿；V5 模块（history/travel/geography/language）测试全部通过，无退化 |
| 10 | Docs | ✅ | `PERSONAL_KNOWLEDGE_V6.md` / `ADR-005.md` / 本文件 / `Final_Report.md` |

## 修复记录（实施过程中发现并修复的真实缺陷）

1. **memory 关键词搜索语义**：store 侧原为整串 LIKE（`'%摄影 旅行%'`），中文/多词查询永不命中。改为 tokens **任意命中**（OR of LIKE），与 `crate::text::keywords` 分词一致；Fake store 与 infra store 同步。
2. **documents 章节读取定位错章**：`read(section=…)` 直接取「首个归属该章节的 chunk」起点，chunk overlap 会把上一章的尾巴带进来（`location.section` 报错章）。改为在 chunk 序列内定位标题原文位置（行首），`range_result` 的 section 取读取区间实际落入的章节。
3. **测试文件缺失导致编译失败**：三个模块文件尾部声明 `#[cfg(test)] mod *_tests;` 但文件不存在（子 agent 中断）；`knowledge/tests.rs` 仅占位。已补齐四个测试文件。
4. **非 ASCII 测试函数名**：`documents/tests.rs` / `files/tests.rs` 有 11 个中文 `fn` 名，Rust 不允许 → 重命名为 ASCII。
5. **缺 `FileMetadata` 导入 / `DocumentStoreError` 未重导出 / infra 方法私有**：桌面组合根编译失败，逐一修复。
6. **`AppSettings` 前端缺 `knowledge` 默认值**：`App.tsx` 默认设置字面量缺字段导致 tsc 报错 → 补默认值（与 core 默认一致）。
7. **KnowledgePage 未挂载 + 三个 V6 Action 无执行入口**：`App.tsx` import 了
   `KnowledgePage` 但从未渲染，`AIPanel` 的 `onConfirmMemory` / `onOpenFile` /
   `onOpenDocument` props 未传 → 补挂载 pane + `knowledgeIntent` state + 三个 handler
   （确认走 `memoryConfirm`/`memorySave`、打开走 `openPath`、文档跳转 Knowledge 页）。
8. **Knowledge 面板样式缺失**：组件引用的 `.knowledge-*` / `.memory-*` / `.doc-*` /
   `.file-*` 类在 `styles.css` 中 0 条定义（54 处 `knowledge` 命中全是 geo 前缀）→
   补一套 Design Tokens 样式。

## Gate 8 审查修复（架构审查 5 项发现）

1. `KnowledgeSourceKind::Module` 无 retriever 却出现在工具 `sources` 枚举 → 从 `ALL`
   移除（枚举变体保留，数据兼容；P1 注册 `ModuleRetriever` 后加回）。
2. Explicit 模式 `limit` 被每源预算二次钳制（与计划 §5.4 偏差）→ `limit_for` 改为
   「显式 limit 是请求」，截断步骤复用同一函数；新增测试
   `explicit_limit_overrides_per_source_budget` 锁定。
3. `DocumentIndexPort::chunk` 死端口（零生产调用，三层各实现一遍）→ 删除端口方法
   与 infra / desktop / 两处 Fake 实现。
4. `knowledge::ports::retrievers` 恒等函数无调用方 → 删除。
5. 四个模块适配器重复的参数解析/错误转换助手 → 抽 `personal_ai/args.rs`
   （`optional_string`/`require_string`/`usize_arg`/`tool_error` + 4 个单测），
   四处改为 `use`。

## Gate 8 隐私审查修复（8 项发现）

| # | 发现 | 修复 |
| --- | --- | --- |
| SEC-001 | secret 正则可绕过（未收录厂商 / 中文无标点 / metadata 字段） | `validate_draft` 新增 `flatten_json(metadata)` 递归检测；正则补 `github_pat_`/`xoxb-`/`dashscope-`/连接串/named token/≥32 位 base62 通用形态；中文谓词加「为」且分隔符可选；新增 2 个测试 |
| SEC-002 | Documents 索引无 deny 规则，凭据文件正文入库 | `index_file` 与 `search` 复用 `FileAccessPolicy::is_denied`（与 Files 同源）；deny 文件只写元数据且不进检索；新增测试 |
| SEC-003 | `files.open` 不拒绝受限文件（旁路 deny） | `open_action` authorize 后补 restricted 检查，与 `read_text` 对称；新增测试 |
| SEC-004 | `files.recent` 不过滤 restricted | service 层 retain；新增测试 |
| SEC-005 | `is_denied` 裸词后缀误命中（`my-shadow` 等） | 模式分两类：`.` 前缀用 `ends_with`，裸词只全等；新增 5 条反向用例 |
| SEC-006 | 根 canonicalize 失败回退原文 | fail-closed：失败即丢弃该根；两处 Fake canonicalize 补目录解析以保持保真 |
| SEC-007 | 超长错误回显实际字符数（侧信道） | 错误文案只保留上限 |
| SEC-008 | `outline` 用 `usize::MAX` 读全文 | 改有界 `OUTLINE_SCAN_CHARS = 4_800` |

## 未做（明确范围外）

- Embedding / 语义检索（P1，计划 §14）。
- PDF 正文抽取（P1；当前降级为仅元数据）。
- 文件 watcher / 后台增量索引（P1；当前只有命令触发 + 启动同步）。
- 会话持久化 / 记忆物理删除 UI（P1）。
- `apps/server` 面无 V6 改动（计划 §1.3）。

## 回滚

三个 SQLite 库独立（删 `config/{memory,documents,files}.db` 即清）；`AppSettings.knowledge` 走 `serde(default)`，无迁移；`PersonalHub.retrieval` 可选 stage 不装配即完全关闭。
