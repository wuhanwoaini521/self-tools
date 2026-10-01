//! Personal Knowledge & Learning OS (V11) 领域模型与核心算法实现。

use serde::{Deserialize, Serialize};

// ============================================================================
// 1. Learning Event（统一学习行为事件）
// ============================================================================

/// 学习行为动作。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningAction {
    View,
    Study,
    Complete,
    Review,
    Answer,
    Correct,
    Incorrect,
    Bookmark,
    Note,
    AskAi,
    #[serde(untagged)]
    Custom(String),
}

impl LearningAction {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::View => "view",
            Self::Study => "study",
            Self::Complete => "complete",
            Self::Review => "review",
            Self::Answer => "answer",
            Self::Correct => "correct",
            Self::Incorrect => "incorrect",
            Self::Bookmark => "bookmark",
            Self::Note => "note",
            Self::AskAi => "ask_ai",
            Self::Custom(val) => val.as_str(),
        }
    }

    #[must_use]
    pub fn parse(val: &str) -> Self {
        match val.trim().to_lowercase().as_str() {
            "view" => Self::View,
            "study" => Self::Study,
            "complete" => Self::Complete,
            "review" => Self::Review,
            "answer" => Self::Answer,
            "correct" => Self::Correct,
            "incorrect" => Self::Incorrect,
            "bookmark" => Self::Bookmark,
            "note" => Self::Note,
            "ask_ai" => Self::AskAi,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// 学习事件。跨模块记录用户在任何模块发生的可沉淀行为。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LearningEvent {
    pub id: String,
    pub module: String,
    pub entity_type: String,
    pub entity_id: String,
    pub entity_title: Option<String>,
    pub action: LearningAction,
    pub timestamp: i64,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub source: Option<String>,
}

// ============================================================================
// 2. Learning Progress & Mastery
// ============================================================================

/// 学习掌握阶段。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningStatus {
    NotStarted,
    Learning,
    Familiar,
    Mastered,
}

impl LearningStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotStarted => "未开始",
            Self::Learning => "学习中",
            Self::Familiar => "较熟悉",
            Self::Mastered => "已掌握",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Learning => "learning",
            Self::Familiar => "familiar",
            Self::Mastered => "mastered",
        }
    }

    #[must_use]
    pub fn parse(val: &str) -> Self {
        match val.trim().to_lowercase().as_str() {
            "mastered" | "已掌握" => Self::Mastered,
            "familiar" | "熟悉" | "较熟悉" => Self::Familiar,
            "learning" | "学习中" => Self::Learning,
            _ => Self::NotStarted,
        }
    }
}

/// 跨模块实体学习进度与掌握度。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LearningProgress {
    /// 唯一主键：`${module}:${entity_type}:${entity_id}`
    pub entity_key: String,
    pub module: String,
    pub entity_type: String,
    pub entity_id: String,
    pub entity_title: String,
    pub status: LearningStatus,
    pub study_count: u32,
    pub review_count: u32,
    pub correct_count: u32,
    pub incorrect_count: u32,
    /// 掌握度得分：0.0 ~ 100.0（清晰可解释加权算法）
    pub mastery_score: f64,
    pub last_studied_at: i64,
    pub next_review_at: Option<i64>,
    pub interval_days: f64,
    pub ease: f64,
    #[serde(default)]
    pub custom_tags: Vec<String>,
}

impl LearningProgress {
    #[must_use]
    pub fn new(
        module: impl Into<String>,
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        entity_title: impl Into<String>,
        now: i64,
    ) -> Self {
        let module = module.into();
        let entity_type = entity_type.into();
        let entity_id = entity_id.into();
        let entity_key = format!("{module}:{entity_type}:{entity_id}");
        Self {
            entity_key,
            module,
            entity_type,
            entity_id,
            entity_title: entity_title.into(),
            status: LearningStatus::NotStarted,
            study_count: 0,
            review_count: 0,
            correct_count: 0,
            incorrect_count: 0,
            mastery_score: 0.0,
            last_studied_at: now,
            next_review_at: None,
            interval_days: 0.0,
            ease: 2.5,
            custom_tags: Vec::new(),
        }
    }
}

/// 确定性掌握度计算器。
#[derive(Clone, Copy, Debug, Default)]
pub struct MasteryCalculator;

impl MasteryCalculator {
    /// 计算掌握度得分 (0.0 ..= 100.0) 并推导掌握阶段。
    #[must_use]
    pub fn calculate(
        study_count: u32,
        correct_count: u32,
        incorrect_count: u32,
        interval_days: f64,
        last_studied_at: i64,
        now: i64,
    ) -> (f64, LearningStatus) {
        if study_count == 0 && correct_count == 0 && incorrect_count == 0 {
            return (0.0, LearningStatus::NotStarted);
        }

        // 1. 正确率因子 (0.0 ~ 40.0)
        let total_reviews = correct_count + incorrect_count;
        let accuracy = if total_reviews == 0 {
            0.6
        } else {
            f64::from(correct_count) / f64::from(total_reviews)
        };
        let accuracy_score = accuracy * 40.0;

        // 2. 学习深度因子 (0.0 ~ 25.0, 达到 5 次学习/复习得满分)
        let total_touches = study_count + total_reviews;
        let depth_score = (f64::from(total_touches) / 5.0).min(1.0) * 25.0;

        // 3. 记忆间隔周期因子 (0.0 ~ 20.0, 达到 21 天间隔得满分)
        let interval_score = (interval_days / 21.0).min(1.0) * 20.0;

        // 4. 时间新鲜度/记忆衰减因子 (0.0 ~ 15.0)
        let days_since_last = ((now.saturating_sub(last_studied_at)) as f64 / 86400.0).max(0.0);
        let recency_score = (1.0 / (1.0 + days_since_last * 0.04)) * 15.0;

        let total_score = (accuracy_score + depth_score + interval_score + recency_score)
            .clamp(0.0, 100.0);
        let rounded_score = (total_score * 10.0).round() / 10.0;

        // 状态推导
        let status = if rounded_score >= 85.0 && interval_days >= 14.0 {
            LearningStatus::Mastered
        } else if rounded_score >= 55.0 {
            LearningStatus::Familiar
        } else if total_touches > 0 {
            LearningStatus::Learning
        } else {
            LearningStatus::NotStarted
        };

        (rounded_score, status)
    }
}

// ============================================================================
// 3. Review Center & Spaced Repetition (SRS)
// ============================================================================

/// 复习卡片类型。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCardType {
    Recall,
    MultipleChoice,
    Qa,
    MapLocate,
    FillBlank,
}

impl ReviewCardType {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Recall => "概念回忆",
            Self::MultipleChoice => "单项选择",
            Self::Qa => "问答题",
            Self::MapLocate => "地图定位",
            Self::FillBlank => "填空题",
        }
    }
}

/// 统一复习评分。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRating {
    /// 忘记 / 重学 (Quality = 0)
    Again = 0,
    /// 困难 (Quality = 2.5)
    Hard = 1,
    /// 记得 / 良好 (Quality = 4.0)
    Good = 2,
    /// 轻松掌握 (Quality = 5.0)
    Easy = 3,
}

impl ReviewRating {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Again => "忘记",
            Self::Hard => "困难",
            Self::Good => "良好",
            Self::Easy => "轻松",
        }
    }

    #[must_use]
    pub const fn quality(self) -> f64 {
        match self {
            Self::Again => 0.0,
            Self::Hard => 2.5,
            Self::Good => 4.0,
            Self::Easy => 5.0,
        }
    }
}

/// 通用复习卡片（支持 Language, History, Geography, Study Board 等）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UniversalReviewCard {
    pub id: String,
    pub module: String,
    pub entity_id: String,
    pub entity_type: String,
    pub card_type: ReviewCardType,
    pub prompt: String,
    pub answer: String,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    pub hint: Option<String>,
    pub context: Option<String>,
    pub due_at: i64,
    pub interval_days: f64,
    pub ease: f64,
    pub mastery_score: f64,
    pub repetition_count: u32,
    pub lapses: u32,
    pub last_reviewed_at: Option<i64>,
    pub created_at: i64,
}

/// SRS 调度结果。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReviewScheduleOutcome {
    pub interval_days: f64,
    pub ease: f64,
    pub due_at: i64,
    pub repetition_count: u32,
    pub lapses: u32,
    pub is_correct: bool,
}

/// 间隔复习调度算法。
#[derive(Clone, Copy, Debug, Default)]
pub struct SpacedRepetitionScheduler;

impl SpacedRepetitionScheduler {
    #[must_use]
    pub fn schedule(
        current_interval_days: f64,
        current_ease: f64,
        current_repetition: u32,
        current_lapses: u32,
        rating: ReviewRating,
        now: i64,
    ) -> ReviewScheduleOutcome {
        let quality = rating.quality();
        let ease =
            (current_ease + (0.1 - (5.0 - quality) * (0.08 + (5.0 - quality) * 0.02))).max(1.3);

        let (interval_days, lapses, reps, is_correct) = match rating {
            ReviewRating::Again => {
                let lapses = current_lapses + 1;
                (0.0, lapses, 0, false)
            }
            ReviewRating::Hard => {
                let interval = if current_repetition == 0 {
                    1.0
                } else {
                    (current_interval_days * ease * 0.8).max(1.0)
                };
                (interval, current_lapses, current_repetition + 1, true)
            }
            ReviewRating::Good => {
                let interval = if current_repetition == 0 {
                    1.0
                } else if current_repetition == 1 {
                    3.0
                } else {
                    (current_interval_days * ease).max(1.0)
                };
                (interval, current_lapses, current_repetition + 1, true)
            }
            ReviewRating::Easy => {
                let interval = if current_repetition == 0 {
                    3.0
                } else if current_repetition == 1 {
                    6.0
                } else {
                    (current_interval_days * ease * 1.35).max(1.0)
                };
                (interval, current_lapses, current_repetition + 1, true)
            }
        };

        let due_at = now + (interval_days * 86_400.0).round() as i64;

        ReviewScheduleOutcome {
            interval_days,
            ease,
            due_at,
            repetition_count: reps,
            lapses,
            is_correct,
        }
    }
}

/// 复习队列项。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReviewQueueItem {
    pub card: UniversalReviewCard,
    pub is_overdue: bool,
    pub urgency_score: f64,
}

/// 复习统计。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ReviewQueueStats {
    pub total_due: u32,
    pub due_count: u32,
    pub overdue_count: u32,
    pub upcoming_count: u32,
    pub by_module: std::collections::HashMap<String, u32>,
    pub mastered_count: u32,
    pub learning_count: u32,
    pub total_cards: u32,
}

// ============================================================================
// 4. Knowledge Graph（跨模块知识图谱）
// ============================================================================

/// 实体类别。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Place,
    Event,
    Time,
    Concept,
    Article,
    Document,
    LanguageItem,
    StudyItem,
    Topic,
    Destination,
    #[serde(untagged)]
    Other(String),
}

impl EntityType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Person => "person",
            Self::Place => "place",
            Self::Event => "event",
            Self::Time => "time",
            Self::Concept => "concept",
            Self::Article => "article",
            Self::Document => "document",
            Self::LanguageItem => "language_item",
            Self::StudyItem => "study_item",
            Self::Topic => "topic",
            Self::Destination => "destination",
            Self::Other(val) => val.as_str(),
        }
    }

    #[must_use]
    pub fn parse(val: &str) -> Self {
        match val.trim().to_lowercase().as_str() {
            "person" | "人物" => Self::Person,
            "place" | "地点" | "地理" => Self::Place,
            "event" | "事件" => Self::Event,
            "time" | "时代" | "朝代" | "时期" => Self::Time,
            "concept" | "概念" => Self::Concept,
            "article" | "文章" | "新闻" => Self::Article,
            "document" | "文档" | "笔记" => Self::Document,
            "language_item" | "word" | "单词" | "语言" => Self::LanguageItem,
            "study_item" | "study" | "学习板" => Self::StudyItem,
            "topic" | "专题" | "合集" => Self::Topic,
            "destination" | "城市" | "旅游" => Self::Destination,
            other => Self::Other(other.to_string()),
        }
    }
}

/// 关系类别。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    OccurredAt,
    RelatedTo,
    PartOf,
    CausedBy,
    ResultedIn,
    MentionedIn,
    LocatedIn,
    StudiedWith,
    SavedFrom,
    BornIn,
    AssociatedWith,
    #[serde(untagged)]
    Other(String),
}

impl RelationKind {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::OccurredAt => "occurred_at",
            Self::RelatedTo => "related_to",
            Self::PartOf => "part_of",
            Self::CausedBy => "caused_by",
            Self::ResultedIn => "resulted_in",
            Self::MentionedIn => "mentioned_in",
            Self::LocatedIn => "located_in",
            Self::StudiedWith => "studied_with",
            Self::SavedFrom => "saved_from",
            Self::BornIn => "born_in",
            Self::AssociatedWith => "associated_with",
            Self::Other(val) => val.as_str(),
        }
    }
}

/// 知识图谱节点。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GraphNode {
    /// 统一全局标识，如 `history:person:102` 或 `geography:city:hangzhou`
    pub id: String,
    pub module: String,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub name: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub learning_status: Option<LearningStatus>,
    pub mastery_score: Option<f64>,
}

/// 知识图谱边（关系）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GraphEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_kind: RelationKind,
    pub label: String,
    pub weight: f64,
    pub source_module: String,
}

/// 知识图谱邻域（N-hop 查询返回结果）。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GraphNeighborhood {
    pub root_id: Option<String>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub hops: u32,
}

// ============================================================================
// 5. Collections（跨模块知识合集）
// ============================================================================

/// 跨模块合集。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub item_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 合集内条目引用（不复制底层数据）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CollectionItem {
    pub id: String,
    pub collection_id: String,
    pub module: String,
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub note: Option<String>,
    pub added_at: i64,
}

// ============================================================================
// 6. Today & Explore Dashboard 聚合模型
// ============================================================================

/// Continue 继续学习入口项。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContinueItem {
    pub module: String,
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub progress_percent: Option<f64>,
    pub last_studied_at: i64,
    pub action_target: String,
}

/// Explore 知识探索推荐项。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExploreRecommendation {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub module: String,
    pub entity_type: String,
    pub entity_id: String,
    /// 推荐理由（如 "关联自您正在学习的明治维新" 或 "巩固薄弱知识点"）
    pub reason: String,
    pub connected_entity_title: Option<String>,
    pub tags: Vec<String>,
}

/// Today 统一个人首页聚合载荷。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TodayDashboardData {
    pub date_str: String,
    pub greeting: String,
    pub studied_topics_today: u32,
    pub pending_reviews_count: u32,
    pub average_mastery: f64,
    pub recent_streak_days: u32,
    pub continue_items: Vec<ContinueItem>,
    pub review_stats: ReviewQueueStats,
    pub explore_recommendations: Vec<ExploreRecommendation>,
    pub today_news_summary: Option<String>,
    pub recent_collections: Vec<Collection>,
    pub recent_bookmarks: Vec<ContinueItem>,
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mastery_calculator_gradual_improvement() {
        let now = 1_700_000_000;
        // 初始状态
        let (score0, status0) = MasteryCalculator::calculate(0, 0, 0, 0.0, now, now);
        assert_eq!(score0, 0.0);
        assert_eq!(status0, LearningStatus::NotStarted);

        // 首次学习
        let (score1, status1) = MasteryCalculator::calculate(1, 0, 0, 0.0, now, now);
        assert!(score1 > 40.0 && score1 < 55.0);
        assert_eq!(status1, LearningStatus::Learning);

        // 多次正确复习，达到 Familiar
        let (score2, status2) = MasteryCalculator::calculate(3, 4, 0, 7.0, now, now);
        assert!(score2 >= 55.0);
        assert_eq!(status2, LearningStatus::Familiar);

        // 长期掌握，间隔达到 21 天且正确率高
        let (score3, status3) = MasteryCalculator::calculate(8, 10, 0, 21.0, now, now);
        assert!(score3 >= 85.0);
        assert_eq!(status3, LearningStatus::Mastered);
    }

    #[test]
    fn srs_scheduler_transitions() {
        let now = 1_700_000_000;
        // Good 评分：新卡片 -> 1 天
        let o1 = SpacedRepetitionScheduler::schedule(0.0, 2.5, 0, 0, ReviewRating::Good, now);
        assert_eq!(o1.interval_days, 1.0);
        assert_eq!(o1.due_at, now + 86400);
        assert_eq!(o1.repetition_count, 1);
        assert_eq!(o1.lapses, 0);

        // 再评分 Good：1 天 -> 3 天
        let o2 = SpacedRepetitionScheduler::schedule(o1.interval_days, o1.ease, o1.repetition_count, o1.lapses, ReviewRating::Good, now);
        assert_eq!(o2.interval_days, 3.0);
        assert_eq!(o2.repetition_count, 2);

        // Again 评分：重置
        let o3 = SpacedRepetitionScheduler::schedule(o2.interval_days, o2.ease, o2.repetition_count, o2.lapses, ReviewRating::Again, now);
        assert_eq!(o3.interval_days, 0.0);
        assert_eq!(o3.repetition_count, 0);
        assert_eq!(o3.lapses, 1);
        assert!(!o3.is_correct);
    }
}
