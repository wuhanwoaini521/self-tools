//! Language Learning Hub 的纯领域层（无 I/O / 无 UI / 无网络）。
//!
//! 分层约定与 `crates/core/src/travel/` 一致：领域模型与纯规则在此，
//! 解析/存储/网络在 infrastructure，用例编排在 application。

pub mod course;
pub mod learning;
pub mod license;
pub mod lrc;
pub mod metadata;
pub mod mining;
pub mod model;
pub mod roadmap;
pub mod romaji;
pub mod scenario;
pub mod speaking;
pub mod speaking_session;

pub use course::{
    BookSummary, Course, CourseBook, CourseLesson, LearningPlan, LessonListEntry, LessonProgress,
    LessonSentence, LessonStage, LessonStatus, LessonVocab, QuizAnswer, QuizItem, QuizResult,
    ShadowAttempt, ShadowStats, WordEntry, WordMark, WordOccurrence, importance_from_frequency,
    is_stopword, summarize_shadow_attempts, tokenize_english,
};
pub use learning::{
    Difficulty, LanguageLearningItem, LearningItemType, Lesson, LessonPosition, LessonStep,
    Mistake, SentenceChunk, SentenceStudy,
};
pub use license::{DatasetManifest, LanguageSource, LicenseKind, SourceLicense};
pub use lrc::{LrcParse, TimedLine, parse_lrc};
pub use metadata::{
    CantoneseMetadata, EnglishMetadata, JapaneseMetadata, LanguageMetadata, MandarinMetadata,
};
pub use mining::{
    BlankReason, MinedCard, MinedCardKind, blank_score, mine_cloze, mine_dictation, mine_lesson,
    mine_sentence, mine_translate, mined_card_id, split_words,
};
pub use model::{
    LanguageCode, LanguageCount, LanguageItem, LanguageItemType, LanguageRelation,
    LanguageRelationKind, Meaning, Pronunciation, PronunciationScheme, SentenceRecord,
};
pub use roadmap::{
    CheckKind, Checkpoint, ROADMAP, RoadmapMetrics, RoadmapWeek, current_week, latest_checkpoint,
    week_complete, week_plan,
};
pub use romaji::{kana_to_romaji, normalize_roman, tones_from_syllables};
pub use scenario::{
    CefrLevel, ScenarioGoal, ScriptedReply, SpeakingScenario, TargetWord, all_goals_complete,
    builtin_scenarios, completed_goals, next_open_goal, scenario_by_id, scripted_reply_for,
    utterance_hits_goal,
};
pub use speaking::{SpeakingScore, WordDiff, compare_words, score, tokenize};
pub use speaking_session::{
    InvalidSessionTransition, SpeakingCorrection, SpeakingDictMatch, SpeakingDrillItem,
    SpeakingFeedback, SpeakingSession, SpeakingSessionEvent, SpeakingSessionStatus, SpeakingTurn,
    SpeakingTurnRole, SpeakingTurnSource, apply_event,
};
