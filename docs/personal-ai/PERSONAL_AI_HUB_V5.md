# SELF-TOOLS V5 · PERSONAL AI MODULE EXPANSION — 终版架构

> 状态：✅ 已实施（Gates 0-10 PASS，见 [`V5_OVERNIGHT_STATUS.md`](V5_OVERNIGHT_STATUS.md)；
> 计划见 [`PERSONAL_AI_HUB_V5_PLAN.md`](PERSONAL_AI_HUB_V5_PLAN.md)；
> Provider 融合记录见 [`V5_PROVIDER_CONSOLIDATION.md`](V5_PROVIDER_CONSOLIDATION.md)；
> 决策记录见 [`docs/architecture/ADR-004-personal-ai-module-expansion.md`](../architecture/ADR-004-personal-ai-module-expansion.md)）。

---

## 1. Track A · 统一 Provider（Gate 1）

`ChatModelProvider`（core/personal_ai/provider.rs）成为 **self-tools 唯一模型抽象**：

- `core::travel::LlmProvider` 已删除；`infra/travel/llm.rs`（OpenAiCompatibleLlmProvider / LlmConfig）已删除。
- travel 消费迁移：`TravelResearchService.llm = Option<Arc<dyn ChatModelProvider>>`；
  `travel_complete` 薄适配（system+user → content），错误 Display 保持
  `travel llm request failed: …` 前缀（`is_llm_transport_error` / `travel_llm_failed` code /
  fallback「来源列表」行为全部冻结，26 个 travel 测试原样通过）。
- 配置统一到 `AiModelConfig`（构造层从 `TravelSettings.llm_*` / `AiSettings` 映射，settings schema 未动）。
- `test_travel_llm` 迁移到 chat；`MockLlmProvider` → `MockChatProvider`。

```text
BEFORE: Personal AI → ChatModelProvider；Travel → LlmProvider（两套 chat/completions 客户端）
AFTER : Personal AI → ChatModelProvider；Travel → ChatModelProvider（travel_complete 薄适配）
```

## 2. Track B · History V3.1 On-demand Enrichment（Gates 2-4，P0）

- **Canonical = Truth Layer**：AI 只 read canonical/evidence → search → **write enrichment（独立 SQLite `config/history_enrichment.db`）**；canonical 永不写入（`AccessMode::ReadOnly` 不变）。
- 域：`core/history_enrichment`（Key/Section/State/Payload/Metadata/Record/View 纯数据）；
  `application/history/enrichment`（ports：search/llm/entity/store + runner；ranking：域名归一/去重/来源分类/权威加权；generation：结构化 JSON envelope `section/content/claims[text,source_ids]/uncertainties/controversies`；validation 门：entity 存在 / content 非空 / 引用解析 / section 一致 → 失败 FAILED 不存 READY）。
- 状态机：`MISSING/GENERATING/READY/STALE/FAILED/REVIEWED`（STALE 派生：TTL 30 天 / canonical_revision 指纹变 / schema_version 变 / 手动 refresh）。
- **Reviewed 保护**：REVIEWED 行 automatic refresh 跳过；手动重生成 = 新 revision 候选（并列存储，不覆盖审定内容）。
- **单飞**：`(entity, section, locale)` 并发只 1 次 search + 1 次生成（runner 层跨调用 guard）。
- **Stale-while-revalidate**：STALE 旧内容立即返回，后台/手动 refresh。
- **ensure_enrichment 工具**：`history.ensure_enrichment`（Risk=SafeWrite，V5 首次启用
  SafeWrite 门禁；只写派生缓存）——只读 4 工具 + 本工具 = History 标准面 5 工具，
  PersonalAgent 可调用后继续回答（agent 路径测试覆盖）。
- 未配置搜索/模型 → FAILED(unavailable)，Canonical/Person/Timeline/Search 全照常（Fake 驱动 8 Case + 校验/排序/生成单测）。

## 3. Track C · Travel 模块（Gate 5）

- 注册 `travel`（capabilities: search/destination/planning/itinerary）；`TravelContextProvider`
  （destination/days/preferences/page → 已缓存行程紧凑上下文；无缓存 → 如实提示）。
- 工具（全部 Read）：`travel.search_destination` / `travel.get_destination` /
  `travel.get_trip_context` / `travel.plan_trip`（preview 不落库，保存走既有 Travel 流程 §43）。
- 接入面：`TravelAiPort`（application 端口；desktop `TravelAiAdapter` 复用搜索 Provider +
  TravelStore 缓存）；PersonalAgent core 零改动（agent 路由测试证明）。

## 4. Track D · Geography + Language（Gates 6-7）

- `geography`：descriptor + `GeographyContextProvider`（location/relations/sources）+ 4 工具
  （search/get_location/get_context/get_exploration），全部 Read；经既有 `GeographyQueryPort`。
- `language`：descriptor + `LanguageContextProvider`（选中词条/句子 → 学习上下文）+ 4 工具
  （get_context / explain（词典先行 + 可选 LLM 解读，无模型确定性降级）/ generate_examples
  （只读已收录）/ practice（只读队列））；经既有 `LanguageStorePort`。
- 前端：TravelPage / GeographyPage / LanguagePage 上报 AppContext（页码实体 + view_state），
  App 页面切换清理上下文；LanguagePage 上报选中句（Demo 5「为什么用 は」的指代基础）。

## 5. PersonalAgent Core Diff

```text
为实现 Travel/Geography/Language 是否增加业务 if/else？ → NO
```

- agent.rs 仅一处平台演进：ToolExecutor 由同步改为 async（`#[async_trait]`，V5 需要慢/IO 工具），
  循环在注册表层 await —— 非业务分支。
- 模块接入 = ModuleDescriptor + tools + ContextProvider + composition root registration（§55 PASS）。

## 6. 测试与回归（Gate 9，全部本次执行）

| 面 | 结果 |
| --- | --- |
| Rust | `cargo test --workspace`：**333 passed / 0 failed**（新：travel 8 / geography 6 / language 9 / enrichment 25+3 / provider 迁移 2 …） |
| Check | `cargo check --workspace --all-targets`：0 error / 0 warning |
| 前端 | `npm run build`（tsc + vite）：PASS（仅既有 chunk 体积警告） |
| V3 回归 | `history-data-pipeline && uv run pytest`：**265 passed**；history 4 旧工具不退化（测试覆盖） |

## 7. 已知限制（P1/P2，非 Mandatory）

- language.explain 的 LLM 插槽在 setup 时读取一次配置（改设置需重启生效；主 agent 路径仍每次读取）。
- Enrichment sources 只保存元数据 + excerpt（不存整页，§24 设计使然）；检索仅用 snippet+短摘要。
- 前端无自动测试 runner（仓库既有事实；tsc+vite + reviewer 校验）。
- Session 持久化 / streaming / 快捷键仍 P1（V5 范围外）。
