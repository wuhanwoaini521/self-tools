//! Language 学习用例：把「语言内容」与「平台学习系统」编排成完整闭环。
//!
//! 分工（关键约束）：
//!
//! | 能力 | 归属 |
//! |---|---|
//! | 掌握度 / 状态判定 | 平台 `MasteryCalculator` |
//! | 复习排期 | 平台 `SpacedRepetitionScheduler` |
//! | 事件流 / 进度 / 复习卡 / 合集 | 平台 `LearningService` |
//! | 条目 / 释义 / 句子 / Lesson / 错题 | 本模块 `LanguageStorePort` |
//!
//! 本服务**不持有**任何 `interval_days` / `ease` / 掌握阶段的副本。它只负责：
//! 把一次用户动作翻译成平台事件 + 平台复习卡，并补上平台不记的那部分（错题原文）。

use std::sync::Arc;

use devtoolbox_core::language::{
    Difficulty, LanguageCode, LanguageLearningItem, LearningItemType, Lesson, LessonPosition,
    LessonStep, Mistake, SentenceStudy,
};
use devtoolbox_core::learning::{
    LearningAction, LearningEvent, LearningProgress, LearningStatus, ReviewCardType, ReviewRating,
    ReviewScheduleOutcome, UniversalReviewCard,
};
use serde::Serialize;

use crate::error::ApplicationError;
use crate::learning::ports::LearningPortError;
use crate::learning::service::LearningService;

use super::ports::LanguageStorePort;

/// Language 事件落库的 `module` 值。与平台 `learning_progress.module` 一致。
const MODULE: &str = "language";

fn language_error(message: String) -> ApplicationError {
    ApplicationError::Language { message }
}

fn learning_error(error: LearningPortError) -> ApplicationError {
    language_error(error.to_string())
}

/// Language 学习服务。
pub struct LanguageLearningService {
    content: Arc<dyn LanguageStorePort>,
    platform: Arc<LearningService>,
}

impl LanguageLearningService {
    #[must_use]
    pub fn new(content: Arc<dyn LanguageStorePort>, platform: Arc<LearningService>) -> Self {
        Self { content, platform }
    }

    // ========================================================================
    // 学习行为
    // ========================================================================

    /// 记录一次学习行为（看 / 学 / 读完）。
    ///
    /// 同时保证平台掌握度被更新。`entity_type` 用学习条目类型（`word` / `sentence`…），
    /// 与 `LanguageLearningItem::entity_key` 保持一致，因此跨入口可对照同一条进度。
    pub fn record_study(
        &self,
        item: &LanguageLearningItem,
        action: StudyAction,
        now: i64,
    ) -> Result<LearningProgress, ApplicationError> {
        self.platform
            .record_event(
                &LearningEvent {
                    id: String::new(),
                    module: MODULE.to_string(),
                    entity_type: item.item_type.as_str().to_string(),
                    entity_id: item.id.clone(),
                    entity_title: Some(item.content.clone()),
                    action: action.into(),
                    timestamp: now,
                    duration_ms: None,
                    metadata: serde_json::json!({
                        "language": item.language.code(),
                        "difficulty": item.difficulty,
                    }),
                    source: Some("language".to_string()),
                },
                now,
            )
            .map_err(learning_error)
    }

    /// 把一个条目加入复习：生成平台复习卡。
    ///
    /// 卡片内容来自**真实词典数据**（释义 / 读音）。缺字段时该字段留空而不是编造。
    pub fn add_to_review(
        &self,
        item: &LanguageLearningItem,
        now: i64,
    ) -> Result<(), ApplicationError> {
        // 进度行只在**尚不存在**时补一条 `Study`：加入复习本身不是「学习」。
        // 无条件记一次会让已学过的条目 `study_count` 重复累加，掌握度随之虚高。
        if self
            .platform
            .get_progress(&item.entity_key())
            .map_err(learning_error)?
            .is_none()
        {
            self.record_study(item, StudyAction::Study, now)?;
        }

        let prompt = review_prompt(item);
        let card = UniversalReviewCard {
            id: review_card_id(item),
            module: MODULE.to_string(),
            entity_id: item.id.clone(),
            entity_type: item.item_type.as_str().to_string(),
            card_type: review_card_type(item),
            prompt,
            answer: item
                .translation
                .clone()
                .unwrap_or_else(|| item.content.clone()),
            options: None,
            hint: item.pronunciation.clone(),
            context: Some(item.source.clone()),
            // 立即到期：用户刚决定要学它，下次进入复习就该见到。
            due_at: now,
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        };
        self.platform
            .upsert_review_card(&card)
            .map_err(learning_error)
    }

    /// 标记需要复习（不改变掌握度，仅产出事件 + 卡片）。
    pub fn mark_needs_review(
        &self,
        item: &LanguageLearningItem,
        now: i64,
    ) -> Result<(), ApplicationError> {
        self.add_to_review(item, now)
    }

    /// 学习卡片队列：进 Language 就该立刻有东西可学。
    ///
    /// 顺序 = **先到期复习，再新内容**。复习队列只装「已加进复习的条目」，新装的库里
    /// 它是空的，光靠它进不去；只看新内容又会漏掉该复习的。两者合起来才是「今天学什么」。
    ///
    /// 「学过没有」以平台 `learning_progress` 为准——语言库不持有进度副本。
    pub fn study_queue(
        &self,
        language: LanguageCode,
        limit: usize,
        now: i64,
    ) -> Result<Vec<StudyCard>, ApplicationError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut cards: Vec<StudyCard> = Vec::new();

        // 1) 到期复习优先
        let due = self
            .platform
            .get_review_queue(Some(MODULE), now, limit)
            .map_err(learning_error)?;
        for row in due {
            let Some(entity) = self
                .content
                .learning_item(&row.card.entity_id)
                .ok()
                .flatten()
            else {
                continue;
            };
            if cards.iter().any(|card| card.item.id == entity.id) {
                continue;
            }
            cards.push(StudyCard {
                from_review: true,
                card_id: Some(row.card.id),
                item: entity,
            });
        }

        // 2) 不足则补新内容（排除已经学过的）
        if cards.len() < limit {
            let learned: Vec<String> = self
                .platform
                .list_progress(Some(MODULE), None, 5000)
                .map_err(learning_error)?
                .into_iter()
                .map(|entry| entry.entity_id)
                .collect();
            let fresh = self
                .content
                .next_new_items(language, &learned, (limit - cards.len()) * 3)
                .map_err(language_error)?;
            for entity in fresh {
                if cards.iter().any(|card| card.item.id == entity.id) {
                    continue;
                }
                cards.push(StudyCard {
                    from_review: false,
                    card_id: None,
                    item: entity,
                });
                if cards.len() >= limit {
                    break;
                }
            }
        }

        cards.truncate(limit);
        Ok(cards)
    }

    // ========================================================================
    // 复习
    // ========================================================================

    /// Language 复习队列（走平台 Review Center，只过滤 module = language）。
    pub fn review_queue(
        &self,
        limit: usize,
        now: i64,
    ) -> Result<Vec<devtoolbox_core::learning::ReviewQueueItem>, ApplicationError> {
        self.platform
            .get_review_queue(Some(MODULE), now, limit)
            .map_err(learning_error)
    }

    /// 提交复习评分。
    ///
    /// 答错 → 记错题（平台只记 `Incorrect` 事件，这里补「用户答成了什么」）。
    /// 答对且此前错过 → 移除错题（「答对了就掌握了」）。
    pub fn submit_review(
        &self,
        card: &UniversalReviewCard,
        user_answer: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, ApplicationError> {
        let outcome = self
            .platform
            .submit_review(&card.id, rating, now)
            .map_err(learning_error)?;

        // 卡片上的 entity_id 可能已被删除；缺内容时只记 id，不编造展示文本。
        let item = self.content.learning_item(&card.entity_id).ok().flatten();
        // 卡片自带 `entity_type`（写入时由 LearningItemType 决定），它才是权威来源；
        // 条目可能已被数据包更新删除，那时 `item` 为 `None`。
        let item_type =
            LearningItemType::parse(&card.entity_type).unwrap_or(LearningItemType::Word);
        let language = item
            .as_ref()
            .map_or(LanguageCode::Eng, |item| item.language);

        if outcome.is_correct {
            self.content
                .resolve_mistake(&card.entity_id, &card.id)
                .map_err(language_error)?;
        } else {
            let mistake = Mistake {
                id: card.id.clone(),
                item_id: card.entity_id.clone(),
                item_type,
                language,
                content: item
                    .as_ref()
                    .map_or_else(|| card.answer.clone(), |item| item.content.clone()),
                question: card.prompt.clone(),
                user_answer: user_answer.to_string(),
                correct_answer: card.answer.clone(),
                error_count: 1,
                last_missed_at: now,
            };
            self.content
                .record_mistake(&mistake, &card.id)
                .map_err(language_error)?;
        }

        Ok(outcome)
    }

    /// 取单张平台复习卡（提交评分前需要卡片原文作为正确答案）。
    pub fn review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, ApplicationError> {
        self.platform
            .get_review_card(card_id)
            .map_err(learning_error)
    }

    // ========================================================================
    // Lesson
    // ========================================================================

    /// 新建 Lesson：步骤**引用**条目 id，不复制正文。
    ///
    /// 引用了不存在的条目会被静默跳过（数据可能被数据包更新替换），
    /// 因此步骤数可能少于入参——这是诚实的行为，好过让前端跳到空卡片。
    pub fn create_lesson(
        &self,
        language: LanguageCode,
        title: impl Into<String>,
        item_ids: &[String],
        now: i64,
    ) -> Result<Lesson, ApplicationError> {
        let items = self
            .content
            .learning_items(item_ids)
            .map_err(language_error)?;
        let steps: Vec<LessonStep> = items
            .into_iter()
            .map(|item| LessonStep {
                item_id: item.id,
                item_type: item.item_type,
                content: item.content,
                translation: item.translation,
            })
            .collect();

        let lesson = Lesson {
            id: format!("lesson_{}", now),
            title: title.into(),
            language,
            description: None,
            steps,
            created_at: now,
            updated_at: now,
        };
        self.content
            .upsert_lesson(&lesson)
            .map_err(language_error)?;
        Ok(lesson)
    }

    pub fn lessons(
        &self,
        language: Option<LanguageCode>,
        limit: usize,
    ) -> Result<Vec<Lesson>, ApplicationError> {
        self.content
            .lessons(language, limit)
            .map_err(language_error)
    }

    pub fn lesson(&self, lesson_id: &str) -> Result<Option<Lesson>, ApplicationError> {
        self.content.lesson(lesson_id).map_err(language_error)
    }

    pub fn delete_lesson(&self, lesson_id: &str) -> Result<(), ApplicationError> {
        self.content
            .delete_lesson(lesson_id)
            .map_err(language_error)
    }

    /// 上报学习位置（每步结束时调用）。位置越界会被读取时收敛。
    pub fn save_position(
        &self,
        lesson_id: &str,
        step_index: usize,
        now: i64,
    ) -> Result<(), ApplicationError> {
        self.content
            .save_lesson_position(&LessonPosition {
                lesson_id: lesson_id.to_string(),
                step_index,
                updated_at: now,
            })
            .map_err(language_error)
    }

    /// Lesson 当前步骤：已保存位置经 `Lesson::steps` 长度收敛，越界即视为已完成。
    pub fn resume_step(&self, lesson: &Lesson) -> usize {
        self.content
            .lesson_position(&lesson.id)
            .ok()
            .flatten()
            .map_or(0, |position| position.clamped(lesson.steps.len()))
    }

    /// 最近学过的 Lesson（Continue 学习入口）。
    pub fn continue_lessons(&self, limit: usize) -> Result<Vec<ContinueLesson>, ApplicationError> {
        let positions = self
            .content
            .recent_lesson_positions(limit)
            .map_err(language_error)?;
        let mut result = Vec::with_capacity(positions.len());
        for position in positions {
            let Some(lesson) = self.content.lesson(&position.lesson_id).ok().flatten() else {
                continue;
            };
            let step_index = position.clamped(lesson.steps.len());
            let total_steps = lesson.steps.len();
            result.push(ContinueLesson {
                step_index,
                completed_steps: (step_index + 1).min(total_steps),
                total_steps,
                last_studied_at: position.updated_at,
                lesson,
            });
        }
        result.sort_by_key(|entry| std::cmp::Reverse(entry.last_studied_at));
        Ok(result)
    }

    /// Lesson 的完整内容（含每个步骤的学习条目，供专注模式渲染）。
    pub fn lesson_steps(
        &self,
        lesson: &Lesson,
    ) -> Result<Vec<LanguageLearningItem>, ApplicationError> {
        let ids: Vec<String> = lesson
            .steps
            .iter()
            .map(|step| step.item_id.clone())
            .collect();
        self.content.learning_items(&ids).map_err(language_error)
    }

    // ========================================================================
    // 句子 / 错题 / 进度
    // ========================================================================

    pub fn sentence_study(
        &self,
        sentence_id: &str,
    ) -> Result<Option<SentenceStudy>, ApplicationError> {
        self.content
            .sentence_study(sentence_id)
            .map_err(language_error)
    }

    pub fn mistakes(&self, limit: usize) -> Result<Vec<Mistake>, ApplicationError> {
        self.content.mistakes(limit).map_err(language_error)
    }

    pub fn mistake_count(&self) -> Result<i64, ApplicationError> {
        self.content.mistake_count().map_err(language_error)
    }

    /// Language 的学习进度（来自平台 `learning_progress`，不另算）。
    pub fn progress(&self, limit: usize) -> Result<Vec<LearningProgress>, ApplicationError> {
        self.platform
            .list_progress(Some(MODULE), None, limit)
            .map_err(learning_error)
    }

    /// 薄弱项：掌握度低且最近学过。
    ///
    /// 排序确定：先按掌握度升序，再按最近学习时间倒序。空进度返回空列表，
    /// 前端据此显示空态而不是伪造「最弱的词」。
    pub fn weak_items(&self, limit: usize) -> Result<Vec<WeakItem>, ApplicationError> {
        let progress = self.progress(200)?;
        let mut weak: Vec<WeakItem> = progress
            .into_iter()
            .filter(|entry| entry.status != LearningStatus::Mastered)
            .map(|entry| {
                let item = self.content.learning_item(&entry.entity_id).ok().flatten();
                let difficulty = Difficulty::derive(entry.incorrect_count);
                WeakItem {
                    entity_id: entry.entity_id.clone(),
                    entity_type: entry.entity_type.clone(),
                    content: item
                        .as_ref()
                        .map_or_else(|| entry.entity_title.clone(), |item| item.content.clone()),
                    translation: item.as_ref().and_then(|item| item.translation.clone()),
                    mastery_score: entry.mastery_score,
                    incorrect_count: entry.incorrect_count,
                    status: entry.status,
                    difficulty,
                    last_studied_at: entry.last_studied_at,
                }
            })
            .collect();
        weak.sort_by(|left, right| {
            left.mastery_score
                .partial_cmp(&right.mastery_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(right.last_studied_at.cmp(&left.last_studied_at))
        });
        weak.truncate(limit);
        Ok(weak)
    }
}

// ============================================================================
// 视图模型
// ============================================================================

/// 学习行为分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudyAction {
    /// 浏览（打开详情）。
    View,
    /// 认真学习（进入学习流程）。
    Study,
    /// 完成（读完一课 / 一篇）。
    Complete,
}

impl From<StudyAction> for LearningAction {
    fn from(action: StudyAction) -> Self {
        match action {
            StudyAction::View => Self::View,
            StudyAction::Study => Self::Study,
            StudyAction::Complete => Self::Complete,
        }
    }
}

/// 学习卡片：一件今天该学的东西 + 它是不是来自复习队列。
#[derive(Clone, Debug, Serialize)]
pub struct StudyCard {
    pub item: LanguageLearningItem,
    /// 来自平台复习队列（已学过，到期该复习）。
    pub from_review: bool,
    /// 复习卡 id；新内容没有卡（第一次作答后才会建卡）。
    pub card_id: Option<String>,
}

/// Continue 入口项。
#[derive(Clone, Debug, Serialize)]
pub struct ContinueLesson {
    pub lesson: Lesson,
    pub step_index: usize,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub last_studied_at: i64,
}

/// 薄弱项。
#[derive(Clone, Debug, Serialize)]
pub struct WeakItem {
    pub entity_id: String,
    pub entity_type: String,
    pub content: String,
    pub translation: Option<String>,
    pub mastery_score: f64,
    pub incorrect_count: u32,
    pub status: LearningStatus,
    pub difficulty: Difficulty,
    pub last_studied_at: i64,
}

/// 平台复习卡 id。
///
/// **不能**与 `LearningService::record_event` 自动建卡的命名相同：那条路径用
/// `format!("card_{module}_{entity_type}_{entity_id}")` 建卡，并写一套自己的初始
/// SRS（`due_at = now + 86400`、`interval_days = 1.0`）。同名会让
/// `upsert_review_card` 的 `ON CONFLICT DO UPDATE` 把对方的排期覆盖掉，
/// 两个写入方对同一张卡各写各的。用 `langcard_` 前缀把「用户主动加入复习」
/// 与「平台按首次学习自动建卡」明确分开。
fn review_card_id(item: &LanguageLearningItem) -> String {
    format!("langcard_{MODULE}_{}_{}", item.item_type.as_str(), item.id)
}

/// 复习卡题型：句子适合填空，词适合回忆。
fn review_card_type(item: &LanguageLearningItem) -> ReviewCardType {
    match item.item_type {
        LearningItemType::Sentence => ReviewCardType::FillBlank,
        _ => ReviewCardType::Recall,
    }
}

/// 复习卡题干。只用真实字段，缺失即不提。
fn review_prompt(item: &LanguageLearningItem) -> String {
    match item.item_type {
        LearningItemType::Sentence => format!("补全这句话：{}", item.content),
        _ => match item.translation.as_deref() {
            Some(translation) if !translation.trim().is_empty() => {
                format!("「{translation}」对应哪个词？")
            }
            _ => format!("回忆：{}", item.content),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::language::{LanguageItem, LanguageItemType};

    fn word() -> LanguageLearningItem {
        LanguageLearningItem::from_item(
            &LanguageItem::plain(
                LanguageCode::Jap,
                LanguageItemType::Word,
                "jmdict:1".into(),
                "駅".into(),
                "jmdict".into(),
            ),
            Some("车站".into()),
            Some("エキ".into()),
            Difficulty::Unknown,
        )
        .expect("word")
    }

    #[test]
    fn review_prompt_uses_translation_when_present_and_falls_back_honestly() {
        assert_eq!(
            review_prompt(&word()),
            "「车站」对应哪个词？",
            "有译意时应考「译意 → 词」"
        );

        let mut bare = word();
        bare.translation = None;
        assert_eq!(review_prompt(&bare), "回忆：駅");
    }

    #[test]
    fn blank_translation_is_treated_as_absent() {
        let mut blank = word();
        blank.translation = Some("   ".to_string());
        assert_eq!(
            review_prompt(&blank),
            "回忆：駅",
            "空白译意不应生成「「  」对应哪个词？」这种无意义题干"
        );
    }

    #[test]
    fn sentence_gets_fill_blank_card_and_word_gets_recall() {
        let mut sentence = word();
        sentence.item_type = LearningItemType::Sentence;
        assert_eq!(review_card_type(&sentence), ReviewCardType::FillBlank);
        assert_eq!(
            review_prompt(&sentence),
            "补全这句话：駅",
            "句子卡题干应可作答"
        );
        assert_eq!(review_card_type(&word()), ReviewCardType::Recall);
    }

    #[test]
    fn card_id_is_stable_per_item_and_type_sensitive() {
        assert_eq!(review_card_id(&word()), "langcard_language_word_jmdict:1");
        assert_eq!(review_card_id(&word()), review_card_id(&word()));

        let mut sentence = word();
        sentence.item_type = LearningItemType::Sentence;
        assert_ne!(
            review_card_id(&word()),
            review_card_id(&sentence),
            "同一 id 的不同类型应是不同的卡，否则会互相覆盖"
        );
    }

    #[test]
    fn card_id_cannot_collide_with_platform_auto_created_cards() {
        // 平台 `LearningService::record_event` 在首次 Study/Bookmark/Complete 时
        // 自动建卡，id 形如 `card_{module}_{entity_type}_{entity_id}`，并写入它自己
        // 的初始排期（`due_at = now + 86400`、`interval_days = 1.0`）。若 Language
        // 用同名 id，两条写入路径会通过 `ON CONFLICT DO UPDATE` 互相覆盖排期。
        let item = word();
        let platform_id = format!("card_{MODULE}_{}_{}", item.item_type.as_str(), item.id);
        assert_ne!(
            review_card_id(&item),
            platform_id,
            "语言卡 id 必须与平台自动建卡区分开"
        );
    }
}
