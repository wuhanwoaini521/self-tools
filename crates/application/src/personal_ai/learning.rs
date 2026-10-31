//! Read-only Learning OS adapter for PersonalAgent.
//!
//! All scheduling, filtering, statistics and card state come from `LearningService`;
//! this module only projects a privacy-minimized view for the model.

use std::sync::Arc;

use devtoolbox_core::learning::{ReviewQueueItem, ReviewQueueStats};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{Action, AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::learning::LearningService;
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_DUE_REVIEWS: &str = "learning.due_reviews";

/// The existing LearningService's read surface, exposed as a narrow tool dependency.
pub trait LearningReviewQuery: Send + Sync {
    fn due_reviews(&self, now: i64, limit: usize) -> Result<Vec<ReviewQueueItem>, String>;
    fn review_stats(&self, now: i64) -> Result<ReviewQueueStats, String>;
}

impl LearningReviewQuery for LearningService {
    fn due_reviews(&self, now: i64, limit: usize) -> Result<Vec<ReviewQueueItem>, String> {
        self.get_review_queue(None, now, limit)
            .map_err(|error| error.to_string())
    }

    fn review_stats(&self, now: i64) -> Result<ReviewQueueStats, String> {
        self.get_review_stats(now)
            .map_err(|error| error.to_string())
    }
}

pub struct LearningTools {
    query: Arc<dyn LearningReviewQuery>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl LearningTools {
    #[must_use]
    pub fn new(
        query: Arc<dyn LearningReviewQuery>,
        clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self { query, clock }
    }

    fn spec() -> ToolSpec {
        ToolSpec {
            name: TOOL_DUE_REVIEWS.to_string(),
            description: "从现有 Learning OS / SRS 调度器读取当前到期复习数量与题目；不返回答案、个人笔记或修改进度。需要当前队列时必须调用。".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "additionalProperties": false
            }),
            risk: ToolRisk::Read,
            module: "learning".to_string(),
        }
    }

    fn due_reviews(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let limit = arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 50) as usize;
        let now = (self.clock)();
        let stats = self.query.review_stats(now).map_err(|error| {
            AgentError::tool_execution_failed(format!("learning stats: {error}"))
        })?;
        let queue = self.query.due_reviews(now, limit).map_err(|error| {
            AgentError::tool_execution_failed(format!("learning queue: {error}"))
        })?;
        let items: Vec<_> = queue.iter().map(review_item_projection).collect();
        let mut result = ToolResult::ok_with_metadata(
            serde_json::json!({
                "as_of": now,
                "total_due": stats.total_due,
                "due_count": stats.due_count,
                "overdue_count": stats.overdue_count,
                "upcoming_count": stats.upcoming_count,
                "returned_count": items.len(),
                "truncated": stats.total_due as usize > items.len(),
                "items": items,
            }),
            serde_json::json!({"source": "learning_service", "limit": limit}),
        );
        result.actions.push(Action::navigate(
            "learning",
            serde_json::json!({"page": "review"}),
        ));
        Ok(result)
    }
}

fn review_item_projection(item: &ReviewQueueItem) -> serde_json::Value {
    let card = &item.card;
    serde_json::json!({
        "id": card.id,
        "module": card.module,
        "entity_type": card.entity_type,
        "entity_id": card.entity_id,
        "source_title": card.source_title,
        "prompt": card.prompt,
        "due_at": card.due_at,
        "is_overdue": item.is_overdue,
        "urgency_score": item.urgency_score,
        "repetition_count": card.repetition_count,
        "mastery_score": card.mastery_score,
    })
}

struct DueReviewsTool {
    tools: Arc<LearningTools>,
    spec: ToolSpec,
}

#[async_trait::async_trait]
impl ToolExecutor for DueReviewsTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        self.tools.due_reviews(&arguments)
    }
}

struct LearningContextProvider {
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl ModuleContextProvider for LearningContextProvider {
    fn module_id(&self) -> &str {
        "learning"
    }

    fn build_context(
        &self,
        _app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        Ok(ContextBundle {
            module: "learning".to_string(),
            headline: "Learning OS · review queue".to_string(),
            summary: serde_json::json!({
                "as_of": (self.clock)(),
                "note": "Use learning.due_reviews for the live queue and counts."
            }),
        })
    }
}

pub fn register_learning(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    query: Arc<dyn LearningReviewQuery>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
) -> Result<(), AgentError> {
    let descriptor = ModuleDescriptor {
        id: "learning".to_string(),
        display_name: "Learning OS".to_string(),
        description: "现有统一复习队列、间隔重复进度与已保存学习来源".to_string(),
        capabilities: vec!["due_reviews".to_string(), "review_progress".to_string()],
        tools: vec![TOOL_DUE_REVIEWS.to_string()],
    };
    modules.register(ModuleRegistration {
        descriptor,
        context_provider: Some(Arc::new(LearningContextProvider {
            clock: Arc::clone(&clock),
        })),
    })?;
    let adapter = Arc::new(LearningTools::new(query, clock));
    tools.register(Arc::new(DueReviewsTool {
        spec: LearningTools::spec(),
        tools: adapter,
    }))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_ai::registry::ToolRegistry;
    use devtoolbox_core::ToolCallRequest;
    use devtoolbox_core::learning::{
        ReviewCardType, ReviewQueueItem, ReviewQueueStats, UniversalReviewCard,
    };

    struct FakeLearning {
        queue: Vec<ReviewQueueItem>,
        stats: ReviewQueueStats,
    }

    impl LearningReviewQuery for FakeLearning {
        fn due_reviews(&self, _now: i64, limit: usize) -> Result<Vec<ReviewQueueItem>, String> {
            Ok(self.queue.iter().take(limit).cloned().collect())
        }
        fn review_stats(&self, _now: i64) -> Result<ReviewQueueStats, String> {
            Ok(self.stats.clone())
        }
    }

    fn card() -> ReviewQueueItem {
        ReviewQueueItem {
            card: UniversalReviewCard {
                id: "knowledge:history:event:e1".into(),
                module: "history".into(),
                entity_id: "e1".into(),
                entity_type: "event".into(),
                card_type: ReviewCardType::Qa,
                prompt: "What happened?".into(),
                answer: "This answer must never be sent to the provider.".into(),
                options: None,
                hint: None,
                context: Some("private user note must not be sent".into()),
                source_title: Some("Reviewed event".into()),
                user_note: Some("private note".into()),
                due_at: 100,
                interval_days: 1.0,
                ease: 2.5,
                mastery_score: 0.5,
                repetition_count: 1,
                lapses: 0,
                last_reviewed_at: None,
                created_at: 1,
            },
            is_overdue: true,
            urgency_score: 1.0,
        }
    }

    #[test]
    fn learning_tool_is_read_only_uses_scheduler_counts_and_omits_answers_and_notes() {
        let fake = Arc::new(FakeLearning {
            queue: vec![card()],
            stats: ReviewQueueStats {
                total_due: 3,
                due_count: 2,
                overdue_count: 1,
                total_cards: 8,
                ..ReviewQueueStats::default()
            },
        });
        let query: Arc<dyn LearningReviewQuery> = fake;
        let clock: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(|| 1234);
        let tools = LearningTools::new(query, clock);
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(DueReviewsTool {
                spec: LearningTools::spec(),
                tools: Arc::new(tools),
            }))
            .unwrap();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(registry.execute(&ToolCallRequest {
                id: "call-1".into(),
                name: TOOL_DUE_REVIEWS.into(),
                arguments: serde_json::json!({"limit": 10}),
            }))
            .unwrap();
        assert!(result.ok);
        assert_eq!(result.data["as_of"], 1234);
        assert_eq!(result.data["total_due"], 3);
        assert_eq!(result.data["items"][0]["prompt"], "What happened?");
        assert!(result.data["items"][0].get("answer").is_none());
        assert!(result.data["items"][0].get("user_note").is_none());
        assert_eq!(result.actions[0].target["page"], "review");
        let serialized = result.data.to_string();
        assert!(!serialized.contains("must never be sent"));
        assert!(!serialized.contains("private"));
    }

    #[test]
    fn learning_context_reports_that_live_counts_must_use_the_tool() {
        let provider = LearningContextProvider {
            clock: Arc::new(|| 42),
        };
        let bundle = provider
            .build_context(&AppContext::default(), &ContextBudget::default())
            .unwrap();
        assert_eq!(bundle.summary["as_of"], 42);
        assert!(
            bundle.summary["note"]
                .as_str()
                .unwrap()
                .contains(TOOL_DUE_REVIEWS)
        );
    }
}
