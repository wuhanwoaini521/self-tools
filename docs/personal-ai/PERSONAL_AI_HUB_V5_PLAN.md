# SELF-TOOLS V5 · PERSONAL AI MODULE EXPANSION — PLAN

> 层次：确认以当前仓库代码为准（HEAD `903327f…`，工作区干净）。V4 已验收入库；
> 本轮目标二：**验证 V4 平台承载多业务模块** + **补齐 History V3.1 按需 AI 富化**。
> 实施状态见 [`V5_OVERNIGHT_STATUS.md`](V5_OVERNIGHT_STATUS.md)；最终架构见
> [`PERSONAL_AI_HUB_V5.md`](PERSONAL_AI_HUB_V5.md)（Gate 10 产出）。

---

## 1. Current State（审计结论，V4 基线）

| 面 | 现状 |
| --- | --- |
| PersonalAgent | `crates/application/src/personal_ai/agent.rs` 工具循环（max_tool_rounds=4）；零业务分支 |
| 注册表 | `ModuleRegistry`（history）+ `ToolRegistry`（4 工具，风险门禁注册期强制） |
| ChatModelProvider | `core/personal_ai/provider.rs`（多消息+工具+usage）；infra `OpenAiCompatibleChatModelProvider`；`AiSettings` 配置 |
| **Travel LlmProvider** | `core/travel/provider.rs`（`complete(system,user)` 单轮）；infra `travel/llm.rs`；`TravelSettings.llm_*` 配置；错误 Display 前缀 `travel llm request failed`（冻结，`is_llm_transport_error` 依赖） |
| History adapter | `personal_ai/history.rs` 4 工具（read-only）；V3 = Canonical+Evidence+批量富化，**无 on-demand**（V4 审计确认） |
| Geography/Language | 已有 application services（GeographyService `home/search/detail/favorite`；LanguageService 14 方法），无 AI 模块 |
| Search 基础设施 | `core/travel/provider.rs` `SearchProvider`/`WebFetcher` + infra Bing/Baidu/SearXNG/HttpWebFetcher —— **可复用**（含域名/权威字段不需要；SearchResult 含 title/url/snippet/provider/published_at） |
| 前端 | V4：App.tsx 上下文桥（History 上报）；TravelPage 无意图上报；GeographyPage/LanguagePage 有意图 |

## 2. Provider Consolidation（Track A，Gate 1）

目标：`ChatModelProvider` 成为唯一模型抽象；**删除 travel LlmProvider**。

- travel service `llm: Option<Box<dyn LlmProvider>>` → `Option<Arc<dyn ChatModelProvider>>`；
  薄辅助 `travel_complete(llm, system, user) -> Result<String, String>`（内部
  `chat(messages=[system,user]).content`；**错误 Display 保持 `travel llm request failed: …` 前缀**，
  使 `is_llm_transport_error`、`GuideGenError::Llm`、命令 code `travel_llm_failed` 全部不变）。
- 配置：删除 `LlmConfig`，统一用 infra `AiModelConfig`（base_url/api_key/model/timeout），
  travel 侧从 `TravelSettings.llm_*` 映射构造（**不改 Settings schema，不破坏用户现有配置**）。
- 删除：`core::travel::LlmProvider` + `ProviderErrorKind::Llm`；`infra/travel/llm.rs`；
  `MockLlmProvider` → `MockChatProvider`（队列语义不变）。
- `test_travel_llm` 命令迁移到 `chat()`（返回契约不变）。
- 提供方名：travel 侧统一 `OpenAiCompatibleChatModelProvider`（name `"openai-compatible"`）。

Gate 1 PASS：`grep LlmProvider` 全仓 = 0（除文档）；travel 行为测试原样通过。

## 3. History V3.1 On-demand Enrichment（Track B，Gate 2-4，P0）

### 域定义（application/history/enrichment/）

- 状态机：`MISSING → GENERATING → READY / FAILED`；派生 `STALE`（TTL / canonical_revision 变 /
  schema_version 变 / 手动 refresh）；`REVIEWED`（人工定稿，**禁止自动覆盖**，重生成=新版本候选）。
- Cache key：`(entity_type, entity_id, section, locale, schema_version)`。
- Cache metadata：`generated_at, refreshed_at, model, provider, prompt_version, schema_version,
  canonical_revision, source_ids`。
- 存储：SQLite `config/history_enrichment.db`（整库 read-only Canonical 不动；
  富化是 derived 用户数据 —— 每域一个 SoT 惯例）。infra `history_enrichment/store.rs`。

### 管线（Gate 3）

```
ensure(entity, section, locale)
  → 状态检查（READY→直接返回；REVIEWED→直接返回；单飞去重）
  → SearchProvider（复用 core::travel::SearchProvider + infra 实现，desktop 组合根按设置装配）
  → 来源排序（去重/域名归一/来源类型分类/权威加权：official>museum>archive>university>academic>reference>general web）
  → 证据包（元数据 + title/url/snippet/published_at + top-K 网页短 excerpt；不存整页）
  → ChatModelProvider 结构化生成（schema: section/content/claims[text,source_ids]/uncertainties/controversies）
  → 校验门（entity 存在 / source_ids 解析 / content 非空 / 引用解析 / payload 形状）→ FAILED 不入 READY
  → 持久化 READY
```

- 单飞：`(entity, section, locale)` 并发只 1 次 search + 1 次 generation（application hub 层
  Mutex 去重，AppState 持有）。
- 首次渲染不阻塞：detail 只读状态；搜索/Provider/网络全挂时 Canonical/Person/Timeline/Search 正常（§28）。
- STALE 时旧内容立即显示 + 后台 refresh（stale-while-revalidate）。

### ensure_enrichment 工具（Gate 4）

- `history.ensure_enrichment` 注册 ToolRegistry，Risk = **SafeWrite**（写 derived cache，不写 Canonical）。
- 平台门禁升级：`ToolRegistry::register` 允许 `Read + SafeWrite`，`SensitiveWrite/System` 仍禁
  （V4 只 Read；V5 首次启用 SafeWrite —— registry 测试更新 + 文档记录）。
- Agent 集成：History context 含 `enrichment_state`；缺失 overview 时允许模型调
  `history.ensure_enrichment(entity={event,id}, section="overview")` 后继续回答（§32）。

## 4. Travel Integration（Track C，Gate 5）

- 注册 `travel` ModuleDescriptor（capabilities: search/destination/planning/itinerary）。
- `TravelContextProvider`：AppContext{module=travel, entity=destination, view_state{days, preferences,
  current_tab, selected_day}} → 经 `TravelAiPort`（application 端口：search_destination /
  get_trip_context / plan_preview）返回紧凑上下文与行程摘要。
- 工具（P0）：`travel.search_destination` / `travel.get_destination` / `travel.get_trip_context` /
  `travel.plan_trip`；（P1 `travel.modify_itinerary` 不做）。
- 规则：工具不返回巨型对象；规划默认 **preview**（不落库），保存走既有 travel 流程（§43）。
- 依赖边界：Travel 模块经端口（SearchProvider/ChatModelProvider 由组合根按设置装配），
  应用层零 infra；PersonalAgent core 零改动。

## 5. Geography Integration（Track D，Gate 6）

- 注册 `geography`；`GeographyContextProvider`（location/landform/route/node/knowledge）经
  `GeographyService`（既有端口 GeographyQueryPort）。
- 工具：`geography.search` / `geography.get_location` / `geography.get_context` /
  `geography.get_exploration`（全部 Read）。
- 前端：GeographyPage 上报当前 node/entity。

## 6. Language Integration（Track D，Gate 7）

- 注册 `language`；`LanguageContextProvider`（language/lesson/word|sentence/selected text/mode）。
- 工具（读取/生成优先）：`language.get_context`（Read）、`language.explain`（LLM，
  经 ChatModelProvider 注入）、`language.generate_examples`（LLM）、`language.practice`（读
  现有 review 数据）。
- 前端：LanguagePage 上报选中句/词。

## 7. Migration Risks（真实约束清单）

| 风险 | 处置 |
| --- | --- |
| travel 错误契约冻结（`travel_llm_failed`、`is_llm_transport_error` 前缀匹配） | `travel_complete` 包装消息保持前缀；迁移后跑既有 travel 测试全绿 |
| settings schema 冻结（`TravelSettings.llm_*`、旧 json 兼容） | 配置统一只发生在**构造层**（映射到 AiModelConfig），不移动 settings 字段 |
| HistoryService 视图模型冻结 | 富化只读 canonical（复用 port），输出是独立派生数据 |
| PersonalAgent core 冻结 | 新模块只注册 descriptor/tools/context，agent.rs 零改动（reviewer 核验） |
| ToolRegistry 门禁收紧后新模块误配风险 | 组合根注册函数显式声明 risk；新增 SafeWrite 仅 `history.ensure_enrichment` |
| 富化 LLM 输出不可信 | 校验门强制（content 非空/source 解析/形状）；FAILED 不入 READY；UI 标注生成时间与来源 |
| Search/LLM 未配置 | 富化返回 Unavailable 状态；Canonical 照常（Fake 测试覆盖） |

## 8. Dependency Boundaries（不变式）

```text
core/personal_ai (ChatModelProvider)          ← travel/history/enrichment 消费（core 允许）
core/travel (SearchProvider 复用)              ← enrichment 搜索复用（同一 abstraction）
application/…：enrichment 域只依赖 core + history 端口；travel/geo/lang 模块只依赖
               自身 application services + core 契约
infrastructure：只做实现（SQLite store / provider impl）
apps/desktop：组合根装配所有端口/Provider；lib.rs 新增命令
禁止：application→infra；模块适配器内 if module == …；agent 内业务分支
```

## 9. Test Matrix（Gate 9）

| 面 | 必须覆盖 |
| --- | --- |
| Provider 迁移（§67） | travel 配置/未配置/timeout/错误/有效响应 —— **迁移前后行为同** |
| Enrichment（§66） | 8 Case：missing→READY / cache hit / stale-refresh / reviewed 不覆盖 / 单飞一次生成 / search 失败 canonical 可用 / 非法输出→FAILED / canonical 变→stale |
| Travel 模块（§68） | 注册/工具发现/context/destination 搜索/trip context/agent 工具调用 |
| Geography（§69） | 注册/context/get_location/agent 路由 |
| Language（§70） | 注册/选中文本 context/explain/agent 路由 |
| 回归 | cargo test --workspace、cargo check --all-targets、npm build、pipeline tests、V4 4 工具不退化 |
| 前端 | tsc + vite（无 runner，记录事实）+ reviewer 人工核验 |

## 10. Rollback Strategy

- 每 Track 独立 commit（§103 git checkpoints）；任一 Track 失败可单独 revert，不影响其余。
- Provider 融合回滚 = revert Gate 1 commit（travel 测试即回归门）。
- 富化数据独立 DB（config/history_enrichment.db，gitignored），删除即清，不影响 Canonical。
- 工具门禁回滚：`allowed_risk` 一处函数判定。

## 11. Gate 顺序与判定

| Gate | 内容 | PASS 判据 |
| --- | --- | --- |
| 0 | 审计 + 本计划 | 落盘 + 基线核实 |
| 1 | Track A 统一 Provider | 全仓 `LlmProvider`=0；travel 行为冻结测试全绿 |
| 2 | 富化域 + 仓储 | 存储///状态/元数据/REVIEWED 保护/单飞 有测试 |
| 3 | 搜索 + 生成管线 | 8 Case（§66，Fake 驱动） |
| 4 | ensure_enrichment 工具 | agent 可调用；只写 cache；Canonical 只读 |
| 5 | Travel 模块 | 注册/context/工具/无 hardcode |
| 6 | Geography 模块 | 同上 |
| 7 | Language 模块 | 同上 |
| 8 | 前端 | 各页 context chip 正确；History 富化 UI；action 执行 |
| 9 | 测试 + 回归 | 全部套件绿 + V4 无退化 |
| 10 | 文档 + 审核 | V5 文档/ADR/status + reviewer 通过 |
