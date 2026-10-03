//! 英语课程学习服务（NCE 主课程的应用层编排）。
//!
//! 分工：
//! - 课程内容（Course/Book/Lesson/Sentence/Vocab）→ `CourseStorePort`（本文件端口）
//! - 单词 SRS / 掌握度 / 复习排期 → 平台 `LearningService`（不在此复制）
//! - 词典查询 → `DictionaryService`（业务页不直接碰 ECDICT 存储）
//!
//! 所有 id 稳定：`nce:{book}:{lesson}` / `language:word:en:{lemma}`，重导入不丢进度。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use devtoolbox_core::language::{
    BookSummary, Course, CourseBook, CourseLesson, LearningPlan, LessonListEntry, LessonProgress,
    LessonSentence, LessonStage, LessonVocab, QuizAnswer, QuizItem, QuizResult, WordEntry,
    WordMark, WordOccurrence,
};
use devtoolbox_core::learning::{
    LearningAction, LearningEvent, LearningProgress, LearningStatus, ReviewCardType, ReviewRating,
    UniversalReviewCard,
};

use crate::error::ApplicationError;
use crate::learning::ports::LearningPortError;
use crate::learning::service::LearningService;

/// Language 模块在平台学习库里的 module 名（与 learning.rs 保持一致）。
const MODULE: &str = "language";

fn err(message: String) -> ApplicationError {
    ApplicationError::Language { message }
}

fn platform(error: LearningPortError) -> ApplicationError {
    err(error.to_string())
}

// ============================================================================
// 端口
// ============================================================================

/// 课程存储端口（实现 = infrastructure `LanguageStore` 的 course 部分）。
pub trait CourseStorePort: Send + Sync {
    fn courses(&self) -> Result<Vec<Course>, String>;
    fn course_books(&self, course_id: &str) -> Result<Vec<CourseBook>, String>;
    fn book(&self, book_id: &str) -> Result<Option<CourseBook>, String>;
    fn book_lessons(&self, book_id: &str) -> Result<Vec<LessonListEntry>, String>;
    fn course_lesson(&self, lesson_id: &str) -> Result<Option<CourseLesson>, String>;
    fn lesson_sentences(&self, lesson_id: &str) -> Result<Vec<LessonSentence>, String>;
    fn lesson_vocab(&self, lesson_id: &str) -> Result<Vec<LessonVocab>, String>;
    fn book_summary(&self, book_id: &str) -> Result<BookSummary, String>;
    fn lesson_progress(&self, lesson_id: &str) -> Result<Option<LessonProgress>, String>;
    fn save_lesson_progress(&self, progress: &LessonProgress) -> Result<(), String>;
    fn latest_learning_lesson(&self) -> Result<Option<LessonListEntry>, String>;
    fn recent_lesson_entries(&self, limit: usize) -> Result<Vec<LessonListEntry>, String>;
    fn study_seconds_since(&self, day_start: i64) -> Result<i64, String>;
    fn study_streak_days(&self, now: i64) -> Result<u32, String>;
    fn learning_plan(&self, language: &str) -> Result<Option<LearningPlan>, String>;
    fn save_learning_plan(&self, plan: &LearningPlan) -> Result<(), String>;
    fn record_word_occurrence(&self, occurrence: &WordOccurrence) -> Result<(), String>;
    fn word_occurrences(&self, word: &str, limit: usize) -> Result<Vec<WordOccurrence>, String>;
    fn word_occurrence_count(&self, word: &str) -> Result<i64, String>;
    fn dict_lookup(&self, word: &str) -> Result<Option<WordEntry>, String>;
    fn dict_search(&self, query: &str, limit: usize) -> Result<Vec<WordEntry>, String>;
    fn dict_count(&self) -> Result<i64, String>;
    fn lesson_title_search(&self, query: &str, limit: usize) -> Result<Vec<CourseLesson>, String>;
}

// ============================================================================
// DictionaryService
// ============================================================================

/// 统一词典服务：业务页面只调它，不直接查询 ECDICT 存储。
pub struct DictionaryService {
    store: Arc<dyn CourseStorePort>,
}

impl DictionaryService {
    #[must_use]
    pub fn new(store: Arc<dyn CourseStorePort>) -> Self {
        Self { store }
    }

    /// 查词（大小写不敏感 + 词形还原）。
    pub fn lookup(&self, word: &str) -> Result<Option<WordEntry>, ApplicationError> {
        self.store.dict_lookup(word).map_err(err)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<WordEntry>, ApplicationError> {
        self.store.dict_search(query, limit).map_err(err)
    }

    /// 词典是否已导入（UI 决定要不要显示「先导入词典」提示）。
    pub fn is_ready(&self) -> Result<bool, ApplicationError> {
        self.store.dict_count().map(|count| count > 0).map_err(err)
    }
}

// ============================================================================
// CourseService DTO
// ============================================================================

/// English 首页（学习驾驶舱）所需的全部数据。
#[derive(Clone, Debug, Serialize)]
pub struct TodayDashboard {
    /// 是否已导入课程。
    pub imported: bool,
    /// 词典是否就绪。
    pub dict_ready: bool,
    pub plan: Option<LearningPlan>,
    /// 继续学习（最近有进度的课）。
    pub continue_lesson: Option<LessonListEntry>,
    /// 当前书中下一课（第一个未开始）。
    pub next_lesson: Option<LessonListEntry>,
    pub current_book: Option<CourseBook>,
    pub book_summary: Option<BookSummary>,
    /// 今日到期复习卡数（平台 module=language）。
    pub due_reviews: u32,
    /// 今日已学秒数。
    pub study_seconds_today: i64,
    pub streak_days: u32,
    /// 已学单词总数（平台进度 module=language + word）。
    pub words_learned: u32,
    pub recent_lessons: Vec<LessonListEntry>,
}

/// Book 页。
#[derive(Clone, Debug, Serialize)]
pub struct BookView {
    pub book: CourseBook,
    pub summary: BookSummary,
    pub lessons: Vec<LessonListEntry>,
}

/// 课时生词 + 用户掌握状态。
#[derive(Clone, Debug, Serialize)]
pub struct VocabWithState {
    #[serde(flatten)]
    pub vocab: LessonVocab,
    /// `new` / `learning` / `known`（平台进度推导）。
    pub state: String,
    /// 用户在其它地方见过它几次（occurrences 特色能力）。
    pub seen_count: i64,
}

/// Lesson 工作台的完整数据包。
#[derive(Clone, Debug, Serialize)]
pub struct LessonDetail {
    pub lesson: CourseLesson,
    pub book: Option<CourseBook>,
    pub sentences: Vec<LessonSentence>,
    pub vocab: Vec<VocabWithState>,
    pub progress: LessonProgress,
}

/// 查词浮层的完整数据包。
#[derive(Clone, Debug, Serialize)]
pub struct WordLookup {
    pub entry: Option<WordEntry>,
    pub seen_count: i64,
    pub occurrences: Vec<WordOccurrence>,
    /// 平台学习状态（没学过 = None）。
    pub learning: Option<LearningProgress>,
}

/// English 统计页。
#[derive(Clone, Debug, Serialize)]
pub struct EnglishProgress {
    pub lessons_completed: u32,
    pub lessons_learning: u32,
    pub study_seconds_total: i64,
    pub words_learned: u32,
    pub words_mastered: u32,
    /// 复习掌握度（平台平均分 0–100；诚实命名为 mastery 而非 retention）。
    pub review_mastery: f64,
    pub due_reviews: u32,
    pub streak_days: u32,
    pub books: Vec<BookProgressView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BookProgressView {
    pub book: CourseBook,
    pub summary: BookSummary,
}

/// 英语学习搜索（词典 + 课时）。
#[derive(Clone, Debug, Serialize)]
pub struct EnglishSearchResult {
    pub words: Vec<WordEntry>,
    pub lessons: Vec<CourseLesson>,
}

/// 进度更新补丁（前端工作台心跳）。
#[derive(Clone, Debug, Deserialize)]
pub struct ProgressPatch {
    pub stage: Option<LessonStage>,
    pub position_ms: Option<i64>,
    pub sentence_seq: Option<u32>,
    pub vocab_index: Option<u32>,
    pub shadow_seq: Option<u32>,
    /// 本次累加的学习秒数（心跳差值）。
    pub study_seconds_delta: Option<i64>,
}

// ============================================================================
// CourseService
// ============================================================================

pub struct CourseService {
    store: Arc<dyn CourseStorePort>,
    platform: Arc<LearningService>,
}

/// 单词在平台学习库里的实体 id（`en:{lemma}`）。
fn word_entity_id(lemma: &str) -> String {
    format!("en:{}", lemma.to_ascii_lowercase())
}

fn word_entity_key(lemma: &str) -> String {
    format!("{MODULE}:word:{}", word_entity_id(lemma))
}

fn word_card_id(lemma: &str) -> String {
    format!("language:nce-word:{}", lemma.to_ascii_lowercase())
}

impl CourseService {
    #[must_use]
    pub fn new(store: Arc<dyn CourseStorePort>, platform: Arc<LearningService>) -> Self {
        Self { store, platform }
    }

    // ========================================================================
    // Today 驾驶舱
    // ========================================================================

    pub fn today(&self, now: i64) -> Result<TodayDashboard, ApplicationError> {
        let plan = self.store.learning_plan("eng").map_err(err)?;
        let courses = self.store.courses().map_err(err)?;
        let imported = courses.iter().any(|course| course.code == "nce");
        let dict_ready = self.store.dict_count().map_err(err)? > 0;

        let continue_lesson = self.store.latest_learning_lesson().map_err(err)?;
        // 还没设置计划时，**自动选第一本有课的册**——首页必须回答「我今天该学什么」，
        // 而不是显示「未设置」让用户自己做决定（任务书 §26）。
        let fallback_book = self
            .store
            .course_books("nce")
            .map_err(err)?
            .into_iter()
            .next();
        let current_book = plan
            .as_ref()
            .and_then(|plan| plan.book_id.clone())
            .and_then(|book_id| self.store.book(&book_id).ok().flatten())
            .or(fallback_book);
        let (book_summary, next_lesson) = match &current_book {
            Some(book) => {
                let lessons = self.store.book_lessons(&book.id).map_err(err)?;
                let next = lessons
                    .iter()
                    .find(|entry| {
                        entry.status == devtoolbox_core::language::LessonStatus::NotStarted
                    })
                    .cloned();
                (Some(self.store.book_summary(&book.id).map_err(err)?), next)
            }
            None => (None, None),
        };

        let stats = self.platform.get_review_stats(now).map_err(platform)?;
        let due_reviews = stats.by_module.get(MODULE).copied().unwrap_or(0);
        let day_start = now - now.rem_euclid(86_400);
        let words = self
            .platform
            .list_progress(Some(MODULE), None, 10_000)
            .map_err(platform)?;
        let words_learned = words
            .iter()
            .filter(|progress| progress.entity_type == "word")
            .count() as u32;

        Ok(TodayDashboard {
            imported,
            dict_ready,
            plan,
            continue_lesson,
            next_lesson,
            current_book,
            book_summary,
            due_reviews,
            study_seconds_today: self.store.study_seconds_since(day_start).map_err(err)?,
            streak_days: self.store.study_streak_days(now).map_err(err)?,
            words_learned,
            recent_lessons: self.store.recent_lesson_entries(6).map_err(err)?,
        })
    }

    // ========================================================================
    // 课程浏览
    // ========================================================================

    pub fn library(&self) -> Result<Vec<CourseBook>, ApplicationError> {
        self.store.course_books("nce").map_err(err)
    }

    pub fn book_view(&self, book_id: &str) -> Result<Option<BookView>, ApplicationError> {
        let Some(book) = self.store.book(book_id).map_err(err)? else {
            return Ok(None);
        };
        Ok(Some(BookView {
            summary: self.store.book_summary(book_id).map_err(err)?,
            lessons: self.store.book_lessons(book_id).map_err(err)?,
            book,
        }))
    }

    // ========================================================================
    // Lesson 工作台
    // ========================================================================

    pub fn lesson_detail(
        &self,
        lesson_id: &str,
        now: i64,
    ) -> Result<Option<LessonDetail>, ApplicationError> {
        let Some(lesson) = self.store.course_lesson(lesson_id).map_err(err)? else {
            return Ok(None);
        };
        let book = self.store.book(&lesson.book_id).map_err(err)?;
        let sentences = self.store.lesson_sentences(lesson_id).map_err(err)?;
        let vocab = self.store.lesson_vocab(lesson_id).map_err(err)?;
        let progress = self
            .store
            .lesson_progress(lesson_id)
            .map_err(err)?
            .unwrap_or_else(|| LessonProgress::new(lesson_id, now));
        let vocab = vocab
            .into_iter()
            .map(|word| {
                let state = self.word_state(&word.word);
                let seen_count = self.store.word_occurrence_count(&word.word).unwrap_or(0);
                VocabWithState {
                    vocab: word,
                    state,
                    seen_count,
                }
            })
            .collect();
        Ok(Some(LessonDetail {
            lesson,
            book,
            sentences,
            vocab,
            progress,
        }))
    }

    /// 单词掌握状态：平台进度 → new/learning/known。
    fn word_state(&self, lemma: &str) -> String {
        let key = word_entity_key(lemma);
        match self.platform.get_progress(&key) {
            Ok(Some(progress)) => match progress.status {
                LearningStatus::Familiar | LearningStatus::Mastered => "known".to_string(),
                _ => "learning".to_string(),
            },
            _ => "new".to_string(),
        }
    }

    /// 更新课时进度（工作台心跳；合并补丁，不动未提供的字段）。
    pub fn update_lesson_progress(
        &self,
        lesson_id: &str,
        patch: ProgressPatch,
        now: i64,
    ) -> Result<LessonProgress, ApplicationError> {
        if self.store.course_lesson(lesson_id).map_err(err)?.is_none() {
            return Err(err(format!("lesson not found: {lesson_id}")));
        }
        let mut progress = self
            .store
            .lesson_progress(lesson_id)
            .map_err(err)?
            .unwrap_or_else(|| LessonProgress::new(lesson_id, now));
        if let Some(stage) = patch.stage {
            progress.stage = stage;
        }
        if let Some(position_ms) = patch.position_ms {
            progress.position_ms = position_ms.max(0);
        }
        if let Some(seq) = patch.sentence_seq {
            progress.sentence_seq = seq;
        }
        if let Some(index) = patch.vocab_index {
            progress.vocab_index = index;
        }
        if let Some(seq) = patch.shadow_seq {
            progress.shadow_seq = seq;
        }
        if let Some(delta) = patch.study_seconds_delta {
            progress.study_seconds += delta.max(0);
        }
        progress.updated_at = now;
        self.store.save_lesson_progress(&progress).map_err(err)?;
        Ok(progress)
    }

    /// 完成一课：进度标记 + 平台事件 + 生成「明天复习这一课」的平台复习卡。
    pub fn complete_lesson(
        &self,
        lesson_id: &str,
        quiz_score: Option<u32>,
        now: i64,
    ) -> Result<LessonProgress, ApplicationError> {
        let lesson = self
            .store
            .course_lesson(lesson_id)
            .map_err(err)?
            .ok_or_else(|| err(format!("lesson not found: {lesson_id}")))?;
        let mut progress = self
            .store
            .lesson_progress(lesson_id)
            .map_err(err)?
            .unwrap_or_else(|| LessonProgress::new(lesson_id, now));
        progress.stage = LessonStage::Done;
        progress.completed_at = progress.completed_at.or(Some(now));
        progress.quiz_score = quiz_score.or(progress.quiz_score);
        progress.updated_at = now;
        self.store.save_lesson_progress(&progress).map_err(err)?;

        self.platform
            .record_event(
                &LearningEvent {
                    id: String::new(),
                    module: MODULE.to_string(),
                    entity_type: "lesson".to_string(),
                    entity_id: lesson_id.to_string(),
                    entity_title: Some(format!(
                        "NCE{} Lesson {} {}",
                        lesson.book_id.rsplit(':').next().unwrap_or(""),
                        lesson.lesson_no,
                        lesson.title
                    )),
                    action: LearningAction::Complete,
                    timestamp: now,
                    duration_ms: None,
                    metadata: serde_json::json!({ "quiz_score": progress.quiz_score }),
                    source: Some("language-course".to_string()),
                },
                now,
            )
            .map_err(platform)?;

        // 课时复习卡：明天复习（让「完成」进入闭环，不是终点）。
        let card = UniversalReviewCard {
            id: format!("language:nce-lesson:{lesson_id}"),
            module: MODULE.to_string(),
            entity_id: lesson_id.to_string(),
            entity_type: "lesson".to_string(),
            card_type: ReviewCardType::Recall,
            prompt: format!("复习 Lesson {} {}", lesson.lesson_no, lesson.title),
            answer: "重新听一遍 + 复述要点".to_string(),
            options: None,
            hint: None,
            context: Some(lesson.book_id.clone()),
            due_at: now + 86_400,
            interval_days: 1.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        };
        self.platform.upsert_review_card(&card).map_err(platform)?;
        Ok(progress)
    }

    // ========================================================================
    // 单词学习
    // ========================================================================

    /// 课前单词三态标记：Know / Fuzzy / Unknown。
    ///
    /// 编排：occurrence（哪里学的）+ 平台复习卡（立即排一次对应评分）+
    /// 平台事件（掌握度单点真相）。
    pub fn mark_word(
        &self,
        lesson_id: &str,
        word: &str,
        mark: WordMark,
        now: i64,
    ) -> Result<LearningProgress, ApplicationError> {
        let lemma = word.trim().to_ascii_lowercase();
        if lemma.is_empty() {
            return Err(err("empty word".to_string()));
        }
        let entry = self.store.dict_lookup(&lemma).map_err(err)?;
        let translation = entry
            .as_ref()
            .and_then(|e| e.translation_zh.clone())
            .or_else(|| entry.as_ref().and_then(|e| e.definition_en.clone()))
            .unwrap_or_else(|| lemma.clone());

        // 1) 遇见记录（幂等键：word+lesson）。
        let _ = self.store.record_word_occurrence(&WordOccurrence {
            word: lemma.clone(),
            source_type: "lesson".to_string(),
            source_id: lesson_id.to_string(),
            sentence: None,
            occurred_at: now,
        });

        // 2) 平台复习卡（卡片内容来自真实词典数据）。
        let card = UniversalReviewCard {
            id: word_card_id(&lemma),
            module: MODULE.to_string(),
            entity_id: word_entity_id(&lemma),
            entity_type: "word".to_string(),
            card_type: ReviewCardType::Recall,
            prompt: lemma.clone(),
            answer: translation,
            options: None,
            hint: entry.as_ref().and_then(|e| e.phonetic.clone()),
            context: Some(lesson_id.to_string()),
            due_at: now,
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        };
        self.platform.upsert_review_card(&card).map_err(platform)?;

        // 3) 立即按自评排期：Know=Easy / Fuzzy=Hard / Unknown=Again（今天再见）。
        let rating = match mark {
            WordMark::Know => ReviewRating::Easy,
            WordMark::Fuzzy => ReviewRating::Hard,
            WordMark::Unknown => ReviewRating::Again,
        };
        self.platform
            .submit_review(&card.id, rating, now)
            .map_err(platform)?;

        // 4) 平台进度（get_progress 的 key 与事件 entity 对应）。
        self.platform
            .get_progress(&word_entity_key(&lemma))
            .map_err(platform)?
            .ok_or_else(|| err("progress missing after review".to_string()))
    }

    /// 查词浮层：词典 + 遇见历史 + 学习状态；同时记一次 lookup occurrence。
    pub fn lookup_word(
        &self,
        word: &str,
        context_sentence: Option<&str>,
        source_lesson: Option<&str>,
        now: i64,
    ) -> Result<WordLookup, ApplicationError> {
        let normalized = word.trim().to_ascii_lowercase();
        let entry = self.store.dict_lookup(&normalized).map_err(err)?;
        let lemma = entry
            .as_ref()
            .map(|e| e.lemma.clone())
            .unwrap_or_else(|| normalized.clone());
        if !lemma.is_empty() {
            let _ = self.store.record_word_occurrence(&WordOccurrence {
                word: lemma.clone(),
                source_type: "lookup".to_string(),
                source_id: format!("{}:{now}", source_lesson.unwrap_or("global")),
                sentence: context_sentence.map(str::to_string),
                occurred_at: now,
            });
        }
        let learning = self
            .platform
            .get_progress(&word_entity_key(&lemma))
            .map_err(platform)?;
        Ok(WordLookup {
            entry,
            seen_count: self.store.word_occurrence_count(&lemma).map_err(err)?,
            occurrences: self.store.word_occurrences(&lemma, 8).map_err(err)?,
            learning,
        })
    }

    // ========================================================================
    // Quiz
    // ========================================================================

    /// 从本课真实数据生成 Quiz（词汇选择 / 填空 / 听写 / 翻译）。
    pub fn generate_quiz(&self, lesson_id: &str) -> Result<Vec<QuizItem>, ApplicationError> {
        let sentences = self.store.lesson_sentences(lesson_id).map_err(err)?;
        let vocab = self.store.lesson_vocab(lesson_id).map_err(err)?;
        if sentences.is_empty() {
            return Err(err("lesson has no sentences".to_string()));
        }
        let mut items = Vec::new();

        // 1) 词汇选择（最多 5 题）：正确答案 + 同课其它词的中文做干扰项。
        let with_translation: Vec<&LessonVocab> = vocab
            .iter()
            .filter(|word| word.translation_zh.is_some())
            .take(8)
            .collect();
        let pool: Vec<String> = with_translation
            .iter()
            .filter_map(|word| first_meaning(word.translation_zh.as_deref()))
            .collect();
        for (index, word) in with_translation.iter().take(5).enumerate() {
            let Some(correct) = first_meaning(word.translation_zh.as_deref()) else {
                continue;
            };
            let mut options: Vec<String> = pool
                .iter()
                .filter(|candidate| **candidate != correct)
                .take(3)
                .cloned()
                .collect();
            // 不编造干扰项：本课候选不足时，选项可以少于 4 个。
            let answer = if options.is_empty() {
                options.push(correct.clone());
                0
            } else {
                index % (options.len() + 1)
            };
            options.insert(answer, correct);
            items.push(QuizItem::Vocabulary {
                word: word.word.clone(),
                phonetic: word.phonetic.clone(),
                options,
                answer,
            });
        }

        // 2) 填空（最多 3 题）：课文原句挖掉本课生词（用词形 surface 替换）。
        let mut fill_count = 0;
        'outer: for word in vocab.iter().filter(|w| w.translation_zh.is_some()) {
            for sentence in &sentences {
                if let Some(blanked) = blank_word(&sentence.english, word) {
                    items.push(QuizItem::FillBlank {
                        sentence: blanked,
                        chinese: sentence.chinese.clone(),
                        answer: word.surface.clone().unwrap_or_else(|| word.word.clone()),
                    });
                    fill_count += 1;
                    if fill_count >= 3 {
                        break 'outer;
                    }
                    continue 'outer;
                }
            }
        }

        // 3) 听写（最多 2 句）：挑 4–12 词的中等句。
        let dictation: Vec<&LessonSentence> = sentences
            .iter()
            .filter(|sentence| {
                let words = sentence.english.split_whitespace().count();
                (4..=12).contains(&words)
            })
            .take(2)
            .collect();
        for sentence in dictation {
            items.push(QuizItem::Dictation {
                lesson_id: lesson_id.to_string(),
                sentence_seq: sentence.sequence,
                start_ms: sentence.start_ms,
                end_ms: sentence.end_ms,
                answer: sentence.english.clone(),
            });
        }

        // 4) 翻译（最多 2 句）：有中文的句子，自评对错。
        for sentence in sentences
            .iter()
            .filter(|sentence| sentence.chinese.is_some())
            .take(2)
        {
            items.push(QuizItem::Translate {
                chinese: sentence.chinese.clone().unwrap_or_default(),
                reference: sentence.english.clone(),
            });
        }

        Ok(items)
    }

    /// 提交 Quiz：计分 + 错词进 SRS（今天再见）+ 课时进度。
    pub fn submit_quiz(
        &self,
        lesson_id: &str,
        answers: &[QuizAnswer],
        now: i64,
    ) -> Result<QuizResult, ApplicationError> {
        if answers.is_empty() {
            return Err(err("empty quiz submission".to_string()));
        }
        let vocab = self.store.lesson_vocab(lesson_id).map_err(err)?;
        let correct = answers.iter().filter(|answer| answer.correct).count() as u32;
        let total = answers.len() as u32;
        let score = correct * 100 / total;

        // 错题对应词（Vocabulary/FillBlank 题序与 generate_quiz 的 vocab 序对齐的
        // 部分由前端把 word 放进 user_answer；这里兜底：把答错题相关的本课生词标记 Again）。
        let mut wrong_words = Vec::new();
        for answer in answers.iter().filter(|answer| !answer.correct) {
            if let Some(word) = answer
                .user_answer
                .as_deref()
                .and_then(|raw| raw.split('|').next())
                .map(str::trim)
                .filter(|raw| !raw.is_empty())
            {
                let lemma = word.to_ascii_lowercase();
                if vocab.iter().any(|v| v.word == lemma) {
                    wrong_words.push(lemma.clone());
                    let _ = self.mark_word(lesson_id, &lemma, WordMark::Unknown, now);
                }
            }
        }

        // 课时进度：记录分数；>=60 视为完成本课。
        let progress = if score >= 60 {
            self.complete_lesson(lesson_id, Some(score), now)?
        } else {
            let mut progress = self
                .store
                .lesson_progress(lesson_id)
                .map_err(err)?
                .unwrap_or_else(|| LessonProgress::new(lesson_id, now));
            progress.stage = LessonStage::Quiz;
            progress.quiz_score = Some(score);
            progress.updated_at = now;
            self.store.save_lesson_progress(&progress).map_err(err)?;
            progress
        };
        let _ = progress;

        Ok(QuizResult {
            lesson_id: lesson_id.to_string(),
            total,
            correct,
            score,
            wrong_words,
            finished_at: now,
        })
    }

    // ========================================================================
    // 计划 / 统计 / 搜索
    // ========================================================================

    pub fn get_plan(&self) -> Result<Option<LearningPlan>, ApplicationError> {
        self.store.learning_plan("eng").map_err(err)
    }

    pub fn save_plan(&self, plan: &LearningPlan) -> Result<(), ApplicationError> {
        self.store.save_learning_plan(plan).map_err(err)
    }

    pub fn english_progress(&self, now: i64) -> Result<EnglishProgress, ApplicationError> {
        let books = self.store.course_books("nce").map_err(err)?;
        let mut book_views = Vec::new();
        let mut lessons_completed = 0;
        let mut lessons_learning = 0;
        let mut study_total = 0;
        for book in &books {
            let summary = self.store.book_summary(&book.id).map_err(err)?;
            lessons_completed += summary.completed_lessons;
            lessons_learning += summary.learning_lessons;
            study_total += summary.study_seconds;
            book_views.push(BookProgressView {
                book: book.clone(),
                summary,
            });
        }
        let progress_rows = self
            .platform
            .list_progress(Some(MODULE), None, 20_000)
            .map_err(platform)?;
        let words: Vec<&LearningProgress> = progress_rows
            .iter()
            .filter(|progress| progress.entity_type == "word")
            .collect();
        let stats = self.platform.get_review_stats(now).map_err(platform)?;
        // 掌握度均值：平台 `MasteryCalculator` 写在每行 learning_progress 上，这里
        // 只做平均（不重算规则）。
        let review_mastery = if words.is_empty() {
            0.0
        } else {
            let sum: f64 = words.iter().map(|progress| progress.mastery_score).sum();
            (sum / words.len() as f64 * 10.0).round() / 10.0
        };
        Ok(EnglishProgress {
            lessons_completed,
            lessons_learning,
            study_seconds_total: study_total,
            words_learned: words.len() as u32,
            words_mastered: words
                .iter()
                .filter(|progress| progress.status == LearningStatus::Mastered)
                .count() as u32,
            review_mastery,
            due_reviews: stats.by_module.get(MODULE).copied().unwrap_or(0),
            streak_days: self.store.study_streak_days(now).map_err(err)?,
            books: book_views,
        })
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<EnglishSearchResult, ApplicationError> {
        Ok(EnglishSearchResult {
            words: self.store.dict_search(query, limit).map_err(err)?,
            lessons: self.store.lesson_title_search(query, limit).map_err(err)?,
        })
    }
}

/// 中文释义的第一条（`v. 犹豫；迟疑；踌躇` → 保留原样；多行取首行）。
fn first_meaning(translation: Option<&str>) -> Option<String> {
    translation?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// 把句子里的目标词挖空（大小写不敏感、整词匹配、保留原句其它部分）。
fn blank_word(sentence: &str, vocab: &LessonVocab) -> Option<String> {
    let targets = [vocab.surface.clone(), Some(vocab.word.clone())];
    for target in targets.into_iter().flatten() {
        let lower_sentence = sentence.to_ascii_lowercase();
        let lower_target = target.to_ascii_lowercase();
        let mut search_from = 0;
        while let Some(found) = lower_sentence[search_from..].find(&lower_target) {
            let start = search_from + found;
            let end = start + lower_target.len();
            let before_ok = start == 0
                || !lower_sentence[..start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
            let after_ok = end >= lower_sentence.len()
                || !lower_sentence[end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric);
            if before_ok && after_ok {
                return Some(format!("{}_____{}", &sentence[..start], &sentence[end..]));
            }
            search_from = end;
        }
    }
    None
}

#[cfg(test)]
mod tests;
