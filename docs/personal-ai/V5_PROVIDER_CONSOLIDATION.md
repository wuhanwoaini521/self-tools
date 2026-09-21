# V5 · Provider Consolidation（已执行，记录归档）

> 状态：**✅ 已于 V5 Gate 1 执行**（见 [PERSONAL_AI_HUB_V5.md](PERSONAL_AI_HUB_V5.md) 与
> [ADR-004](../architecture/ADR-004-personal-ai-module-expansion.md)）。
> `LlmProvider` / `infra/travel/llm.rs` 已删除；`ChatModelProvider` 为唯一模型抽象；
> travel 经 `travel_complete` 薄适配，行为冻结（26 测试原样过）。下文「V5 应如何合并」
> 即已落地路径，保留作为决策记录。

---

## 1. 当前两个 Provider 的职责

### Personal AI `ChatModelProvider`（V4 新增）

- **位置**：契约 `crates/core/src/personal_ai/provider.rs`；实现 `crates/infrastructure/src/personal_ai/llm.rs`
  （`OpenAiCompatibleChatModelProvider`）。
- **能力**：多消息（system/user/assistant/tool 四角色）、工具调用（tools + tool_calls）、
  usage 回传（input/output/total tokens + duration）、timeout 配置。
- **消费方**：`PersonalAgent`（工具循环）。
- **配置**：`AiSettings{provider, model, base_url, api_key, timeout_secs}`；
  未配置 → `UnconfiguredModelProvider` 降级桩 → `personal_ai_model_unavailable` 受控错误。
- **错误模型**：`ProviderError{kind: Unavailable|Timeout|Transport|InvalidResponse}` →
  application 映射 `AgentError` 9 类 code。

### Travel `LlmProvider`（V3/Gate 7 既有）

- **位置**：契约 `crates/core/src/travel/provider.rs`；实现 `crates/infrastructure/src/travel/llm.rs`
  （`OpenAiCompatibleLlmProvider`）。
- **能力**：单轮 `complete(system, user) -> String`；temperature 0.2；120s timeout。
- **消费方**：`TravelResearchService`（搜索主题归纳 / 日程生成）。
- **配置**：`TravelSettings{llm_base_url, llm_api_key, llm_model, ...}`；
  未配置 → `None` → travel 降级「来源列表」模式。
- **错误模型**：`ProviderError(kind: Llm)`，Display 前缀 `travel llm request failed`，
  命令错误 code `travel_llm_failed`（冻结契约，不得改）。

## 2. 重复点

| 维度 | 重复内容 |
| --- | --- |
| 传输 | 同一个 OpenAI-Compatible `/chat/completions` `POST`（reqwest + bearer auth + ACCEPT_ENCODING identity + 非 2xx 处理） |
| 配置 | `LlmConfig` 与 `AiModelConfig` 字段几乎同构（base_url / api_key / model / timeout） |
| 超时 | 各自 120s（travel 硬编码；AI 可配置） |
| 解析 | 各自解析 `choices[0].message.content`（travel 仅 content；AI 含 tool_calls + usage） |
| 错误 | 各自一套 `ProviderError`（travel 的 `Llm` kind 与 AI 的 `Transport/InvalidResponse` 语义重叠） |

## 3. V5 应如何合并

1. **统一契约**：以 `ChatModelProvider` 为唯一 provider 抽象（它涵盖 travel 的单轮场景：
   `chat(messages=[system, user]).content` 等价于 `complete(system, user)`）。
2. **travel 迁移**：`TravelResearchService` 的依赖从 `Option<Box<dyn LlmProvider>>` 换为
   `Option<Arc<dyn ChatModelProvider>>`（经 application 端口注入），对 `complete` 调用点做
   薄适配（一条发送两条消息 + 取 content 的辅助函数）。
3. **错误契约保持**：application/travel 层把一句话错误映射为既有 `travel_llm_failed` code，
   用户可见消息不变；`extract_chat_content` 与 usage 解析合并进统一 parse 函数
   （`parse_chat_response` 已具备，travel 解析调用它即可）。
4. **配置收敛**：`TravelSettings.llm_*` 标记 deprecated 并从 `ai.model/base_url/api_key`
   读取（迁移期二选一读，写侧仍写 travel 字段或迁移到 ai 字段——取决于 V5 决定）。
5. **删除**：`core::travel::LlmProvider` 与 `infra::travel::OpenAiCompatibleLlmProvider`
   在迁移完成、零引用后删除（保留 `ProviderError` 的 travel 命名映射直至 travel 错误
   链全部转统一模型）。

## 4. 最终统一入口

**`ChatModelProvider`（core/personal_ai/provider.rs）** —— 多消息 + 工具调用 + usage 的
超集抽象，作为 self-tools 唯一的模型接入点；travel 是它的第一个非 Agent 消费者。

## 5. V5 注意事项

- travel 的合约冻结（命令 code `travel_llm_failed`、LLM 未配置降级语义）迁移时优先，
  不改变用户可见行为；
- `travel_llm_test` 测试命令（test_travel_llm）需同步迁移到统一 provider；
- 统一配置后 settings.json 迁移：`serde(default)` 已保证旧文件可读，迁移只增不减。
