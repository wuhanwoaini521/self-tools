//! Language 模块适配器（V5 §49-§53，Track D）。
//!
//! Language 是 V5 标准模块：注册 descriptor + 4 个 Read 工具 + ContextProvider。
//! **不重做 Language 域**：数据经 `LanguageService`/`LanguageStorePort`（既有 use case）
//! 复用；`language.explain` 在有模型插槽时用统一 `ChatModelProvider` 作可选增强，
//! 无模型时仍返回权威词典数据（绝不编造，§59/§63）。
//!
//! - `language.get_context` → 词条/语言紧凑上下文
//! - `language.explain(item_id, question?)` → 词典释义 + (可选) AI 解读
//! - `language.generate_examples(item_id, count?)` → 已收录例句（只读，不编造）
//! - `language.practice(language?, limit?)` → 待复习队列（只读）

use std::sync::Arc;

use async_trait::async_trait;
use devtoolbox_core::language::{LanguageCode, LanguageItem};
use devtoolbox_core::personal_ai::{AppContext, ChatMessage, ChatModelProvider, ChatRequest};
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::language::{LanguageDetailRows, LanguageStorePort};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_GET_CONTEXT: &str = "language.get_context";
const TOOL_EXPLAIN: &str = "language.explain";
const TOOL_GENERATE_EXAMPLES: &str = "language.generate_examples";
const TOOL_PRACTICE: &str = "language.practice";

/// Language 工具执行器（一个结构体、四个身份；dispatch 属模块内部细节）。
pub struct LanguageTools {
    store: Arc<dyn LanguageStorePort>,
    llm: Option<Arc<dyn ChatModelProvider>>,
}

impl LanguageTools {
    #[must_use]
    pub fn new(store: Arc<dyn LanguageStorePort>, llm: Option<Arc<dyn ChatModelProvider>>) -> Self {
        Self { store, llm }
    }

    fn spec_for(&self, name: &str) -> ToolSpec {
        let input_schema = match name {
            TOOL_GET_CONTEXT => serde_json::json!({
                "type": "object",
                "properties": {
                    "language": {"type": "string", "description": "语言代码（如 jpn / eng / cmn / yue）"},
                    "id": {"type": "string", "description": "词条 id（如 jmdict:1002990）"}
                }
            }),
            TOOL_EXPLAIN => serde_json::json!({
                "type": "object",
                "required": ["item_id"],
                "properties": {
                    "item_id": {"type": "string"},
                    "question": {"type": "string", "description": "用户的具体问题（如 为什么用 は）"}
                }
            }),
            TOOL_GENERATE_EXAMPLES => serde_json::json!({
                "type": "object",
                "required": ["item_id"],
                "properties": {
                    "item_id": {"type": "string"},
                    "count": {"type": "integer", "minimum": 1, "maximum": 10}
                }
            }),
            _ => serde_json::json!({
                "type": "object",
                "properties": {
                    "language": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                }
            }),
        };
        let description = match name {
            TOOL_GET_CONTEXT => "获取词条或语言的紧凑学习上下文（供指代解析）",
            TOOL_EXPLAIN => "解释词条（权威词典释义 + 可选 AI 解读），只使用本地已收录数据",
            TOOL_GENERATE_EXAMPLES => "返回词条已收录的例句（不编造新句子）",
            _ => "查看某语言待复习词条队列与今日统计（只读）",
        };
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            risk: ToolRisk::Read,
            module: "language".to_string(),
        }
    }

    fn rows(&self, item_id: &str) -> Result<Option<LanguageDetailRows>, AgentError> {
        let rows = self
            .store
            .item_detail(item_id)
            .map_err(AgentError::tool_execution_failed)?;
        Ok((rows.item.is_some()).then_some(rows))
    }

    /// language.get_context：词条或语言层紧凑上下文。
    pub fn get_context(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = arguments.get("id").and_then(serde_json::Value::as_str);
        if let Some(id) = id.filter(|id| !id.is_empty()) {
            return self.item_context(id);
        }
        let language = match arguments
            .get("language")
            .and_then(serde_json::Value::as_str)
        {
            Some(code) => LanguageCode::from_code(code).ok_or_else(|| {
                AgentError::tool_invalid_argument(format!("未知语言代码: {code}"))
            })?,
            None => LanguageCode::Jap,
        };
        self.language_context(language)
    }

    fn item_context(&self, id: &str) -> Result<ToolResult, AgentError> {
        let Some(rows) = self.rows(id)? else {
            return Ok(ToolResult::fail(format!("词条 `{id}` 不存在")));
        };
        let item = rows.item.as_ref().expect("rows has item");
        let data = serde_json::json!({
            "entity": {"kind": "word", "id": item.id, "label": item.text},
            "item": {
                "id": item.id,
                "text": item.text,
                "reading": item.reading,
                "romanization": item.romanization,
                "language": item.language.code(),
                "item_type": item.item_type.label(),
            },
            "language": item.language.code(),
            "meanings_count": rows.meanings.len(),
            "meanings_head": cap_list(&rows.meanings, 3, |meaning| serde_json::json!({
                "pos": meaning.pos,
                "gloss": meaning.gloss,
                "raw": cap_chars(meaning.raw.as_deref(), 160),
            })),
            "examples_count": rows.examples.len(),
            "examples_head": cap_list(&rows.examples, 3, |example| serde_json::json!({
                "text": example.text,
                "translation": example.translation,
            })),
        });
        Ok(ToolResult::ok(data))
    }

    fn language_context(&self, language: LanguageCode) -> Result<ToolResult, AgentError> {
        let now = now_unix();
        let today = self
            .store
            .today_plan(language, now)
            .map_err(AgentError::tool_execution_failed)?;
        let sentences = self
            .store
            .sentences_by_language(language, 3)
            .map_err(AgentError::tool_execution_failed)?;
        let data = serde_json::json!({
            "language": language.code(),
            "label": language.native_label(),
            "today": {
                "due_reviews": today.due_reviews,
                "new_words": today.new_words,
                "sentences": today.sentences,
                "total": today.total,
            },
            "sentences_head": cap_list(&sentences, 3, |sentence| serde_json::json!({
                "text": sentence.text,
            })),
        });
        Ok(ToolResult::ok(data))
    }

    /// language.explain：词典数据先行；有模型插槽时追加 AI 解读（失败不阻塞）。
    pub async fn explain(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let item_id = arguments
            .get("item_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if item_id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "language.explain: item_id required",
            ));
        }
        let question = arguments
            .get("question")
            .and_then(serde_json::Value::as_str)
            .filter(|q| !q.is_empty());
        let Some(rows) = self.rows(item_id)? else {
            return Ok(ToolResult::fail(format!("词条 `{item_id}` 不存在")));
        };
        let item = rows.item.as_ref().expect("rows has item");

        let mut sections: Vec<serde_json::Value> = Vec::new();
        sections.push(serde_json::json!({
            "title": "权威词典释义",
            "kind": "dictionary",
            "items": cap_list(&rows.meanings, 30, |meaning| serde_json::json!({
                "pos": meaning.pos,
                "gloss": meaning.gloss,
                "raw": cap_chars(meaning.raw.as_deref(), 300),
            })),
        }));
        if !rows.examples.is_empty() {
            sections.push(serde_json::json!({
                "title": "已收录例句",
                "kind": "dictionary",
                "items": cap_list(&rows.examples, 30, |example| serde_json::json!({
                    "text": example.text,
                    "translation": example.translation,
                    "source": example.source,
                })),
            }));
        }

        // 可选 AI 解读：只用上面的词典数据回答问题，禁止编造。
        let mut llm_used = false;
        if let (Some(llm), Some(question)) = (&self.llm, question) {
            let system = "你是语言学习助手。只依据下方提供的本地词典释义与例句回答，\
                          不得编造新的释义或来源；不确定时明确说没有收录。";
            let user = format!(
                "词条: {}（{}）\n问题: {}\n\n=== 词典释义 ===\n{json}\n\n=== 例句 ===\n{examples}",
                item.text,
                item.language.native_label(),
                question,
                json = serde_json::to_string(&sections).unwrap_or_default(),
                examples = serde_json::to_string(&rows.examples).unwrap_or_default(),
            );
            let request = ChatRequest {
                messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
                tools: Vec::new(),
                temperature: Some(0.2),
                max_tokens: None,
            };
            if let Ok(response) = llm.chat(request).await
                && let Some(content) = response.content
            {
                llm_used = true;
                sections.push(serde_json::json!({
                    "title": "AI 辅助解读",
                    "kind": "ai",
                    "text": cap_chars(Some(&content), 2000),
                }));
            }
        }

        Ok(ToolResult::ok(serde_json::json!({
            "entity": {"kind": "word", "id": item.id, "label": item.text},
            "item": {
                "id": item.id,
                "text": item.text,
                "reading": item.reading,
                "language": item.language.code(),
            },
            "explanation_sections": sections,
            "llm_used": llm_used,
        })))
    }

    /// language.generate_examples：只读已收录例句/句子，不编造新文本。
    pub fn generate_examples(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let item_id = arguments
            .get("item_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if item_id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "language.generate_examples: item_id required",
            ));
        }
        let count = arguments
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(3)
            .clamp(1, 10);
        let Some(rows) = self.rows(item_id)? else {
            return Ok(ToolResult::fail(format!("词条 `{item_id}` 不存在")));
        };
        let mut examples: Vec<serde_json::Value> = Vec::new();
        for example in &rows.examples {
            if examples.len() >= count {
                break;
            }
            examples.push(serde_json::json!({
                "text": example.text,
                "translation": example.translation,
                "source": example.source,
            }));
        }
        for sentence in &rows.sentences {
            if examples.len() >= count {
                break;
            }
            examples.push(serde_json::json!({
                "text": sentence.text,
                "translation": null,
                "source": format!("sentence:{}", sentence.source),
            }));
        }
        if examples.is_empty() {
            return Ok(ToolResult::fail("该词没有已收录例句"));
        }
        Ok(ToolResult::ok(serde_json::json!({
            "examples": examples,
            "count": examples.len(),
        })))
    }

    /// language.practice：待复习队列 + 今日统计（只读，不改变学习状态）。
    pub fn practice(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let language = match arguments
            .get("language")
            .and_then(serde_json::Value::as_str)
        {
            Some(code) => LanguageCode::from_code(code).ok_or_else(|| {
                AgentError::tool_invalid_argument(format!("未知语言代码: {code}"))
            })?,
            None => LanguageCode::Jap,
        };
        let limit = arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(5)
            .clamp(1, 20);
        let now = now_unix();
        let today = self
            .store
            .today_plan(language, now)
            .map_err(AgentError::tool_execution_failed)?;
        let mut due: Vec<LanguageItem> = Vec::new();
        while due.len() < limit {
            match self
                .store
                .review_next(language, now)
                .map_err(AgentError::tool_execution_failed)?
            {
                Some(item) => due.push(item),
                None => break,
            }
        }
        Ok(ToolResult::ok(serde_json::json!({
            "language": language.code(),
            "today": {
                "due_reviews": today.due_reviews,
                "new_words": today.new_words,
                "total": today.total,
            },
            "due": cap_list(&due, limit, |item| serde_json::json!({
                "id": item.id,
                "text": item.text,
                "reading": item.reading,
            })),
            "note": if due.is_empty() { "暂无待复习词条" } else { "" },
        })))
    }
}

// ---------------------------------------------------------------------------
// 每个工具一个薄执行器（spec 存于构造时，无需静态缓存）
// ---------------------------------------------------------------------------

struct ToolImpl {
    name: &'static str,
    spec: ToolSpec,
    tools: Arc<LanguageTools>,
}

#[async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_GET_CONTEXT => self.tools.get_context(&arguments),
            TOOL_EXPLAIN => self.tools.explain(&arguments).await,
            TOOL_GENERATE_EXAMPLES => self.tools.generate_examples(&arguments),
            _ => self.tools.practice(&arguments),
        }
    }
}

// ---------------------------------------------------------------------------
// ContextProvider（V4 §25 模式）
// ---------------------------------------------------------------------------

/// Language 模块上下文提供方：选中词条 → 紧凑学习上下文。
pub struct LanguageContextProvider<'a> {
    tools: &'a LanguageTools,
}

impl ModuleContextProvider for LanguageContextProvider<'_> {
    fn module_id(&self) -> &str {
        "language"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        if let Some(entity) = &app_context.entity
            && entity.kind == "word"
        {
            let rows = self.tools.rows(&entity.id)?;
            if let Some(rows) = rows
                && let Some(item) = &rows.item
            {
                let headline = format!("Language · {}", item.text);
                let summary = serde_json::json!({
                    "module": "language",
                    "entity": {"kind": "word", "id": item.id, "label": item.text},
                    "language": item.language.code(),
                    "reading": item.reading,
                    "romanization": item.romanization,
                    "meanings_head": cap_list(&rows.meanings, budget.max_items / 4, |meaning| serde_json::json!({
                        "gloss": meaning.gloss,
                        "raw": cap_chars(meaning.raw.as_deref(), 200),
                    })),
                    "examples_count": rows.examples.len(),
                    "examples_head": cap_list(&rows.examples, budget.max_items / 6, |example| serde_json::json!({
                        "text": example.text,
                        "translation": example.translation,
                    })),
                });
                return Ok(ContextBundle {
                    module: "language".to_string(),
                    headline,
                    summary,
                });
            }
        }
        // 无选中词条：语言级上下文（view_state.language 或默认 jpn）。
        let language = app_context
            .view_state
            .get("language")
            .and_then(serde_json::Value::as_str)
            .and_then(LanguageCode::from_code)
            .unwrap_or(LanguageCode::Jap);
        Ok(ContextBundle {
            module: "language".to_string(),
            headline: format!("Language · {}", language.native_label()),
            summary: serde_json::json!({
                "module": "language",
                "language": language.code(),
                "note": "当前无选中词条（语言学习总览）",
            }),
        })
    }
}

/// 组合根可见的 owned ContextProvider。
pub struct LanguageProviderOwned {
    tools: LanguageTools,
}
impl LanguageProviderOwned {
    #[must_use]
    pub fn new(store: Arc<dyn LanguageStorePort>, llm: Option<Arc<dyn ChatModelProvider>>) -> Self {
        Self {
            tools: LanguageTools::new(store, llm),
        }
    }
}
impl ModuleContextProvider for LanguageProviderOwned {
    fn module_id(&self) -> &str {
        "language"
    }
    fn build_context(
        &self,
        ctx: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        LanguageContextProvider { tools: &self.tools }.build_context(ctx, budget)
    }
}

/// 注册 language 模块（descriptor + 4 工具 + context provider）。
pub fn register_language(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    store: Arc<dyn LanguageStorePort>,
    llm: Option<Arc<dyn ChatModelProvider>>,
) -> Result<(), AgentError> {
    let language = Arc::new(LanguageTools::new(Arc::clone(&store), llm));
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "language".to_string(),
            display_name: "Language".to_string(),
            description: "语言学习：词条、例句、复习与发音".to_string(),
            capabilities: vec![
                "context".to_string(),
                "explain".to_string(),
                "examples".to_string(),
                "practice".to_string(),
            ],
            tools: vec![
                TOOL_GET_CONTEXT.to_string(),
                TOOL_EXPLAIN.to_string(),
                TOOL_GENERATE_EXAMPLES.to_string(),
                TOOL_PRACTICE.to_string(),
            ],
        },
        context_provider: Some(Arc::new(LanguageProviderOwned::new(
            Arc::clone(&store),
            None,
        ))),
    })?;
    for name in [
        TOOL_GET_CONTEXT,
        TOOL_EXPLAIN,
        TOOL_GENERATE_EXAMPLES,
        TOOL_PRACTICE,
    ] {
        let spec = language.spec_for(name);
        tools.register(Arc::new(ToolImpl {
            name,
            spec,
            tools: Arc::clone(&language),
        }))?;
    }
    Ok(())
}

/// 导出工具常量（外部测试 / 组合根引用）。
#[must_use]
pub fn language_tool_names() -> [&'static str; 4] {
    [
        TOOL_GET_CONTEXT,
        TOOL_EXPLAIN,
        TOOL_GENERATE_EXAMPLES,
        TOOL_PRACTICE,
    ]
}

// ---------------------------------------------------------------------------
// 预算辅助 & 时钟
// ---------------------------------------------------------------------------

fn cap_chars(text: Option<&str>, max: usize) -> Option<String> {
    text.map(|t| {
        if t.chars().count() <= max {
            t.to_string()
        } else {
            let head: String = t.chars().take(max).collect();
            format!("{head}…[截断]")
        }
    })
}

fn cap_list<T>(
    items: &[T],
    max: usize,
    map: impl Fn(&T) -> serde_json::Value,
) -> Vec<serde_json::Value> {
    items.iter().take(max).map(map).collect()
}

pub(crate) fn now_unix() -> i64 {
    crate::time::now_unix()
}

#[cfg(test)]
mod language_tests;
