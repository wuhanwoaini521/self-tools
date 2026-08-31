//! Language Learning Hub 的纯领域层（无 I/O / 无 UI / 无网络）。
//!
//! 分层约定与 `crates/core/src/travel/` 一致：领域模型与纯规则在此，
//! 解析/存储/网络在 infrastructure，用例编排在 application。

pub mod license;
pub mod metadata;
pub mod model;
pub mod review;
pub mod romaji;
pub mod speaking;

pub use license::{DatasetManifest, LanguageSource, LicenseKind, SourceLicense};
pub use metadata::{
    CantoneseMetadata, EnglishMetadata, JapaneseMetadata, LanguageMetadata, MandarinMetadata,
};
pub use model::{
    AudioAsset, AudioType, LanguageCode, LanguageCount, LanguageItem, LanguageItemType,
    LanguageRelation, LanguageRelationKind, Meaning, Pronunciation, PronunciationScheme,
    SentenceRecord,
};
pub use review::{
    LearningState, LearningStateKind, ReviewOutcome, ReviewRating, ReviewScheduler, TodayPlan,
};
pub use romaji::{kana_to_romaji, normalize_roman, tones_from_syllables};
pub use speaking::{SpeakingScore, WordDiff, compare_words, score, tokenize};
