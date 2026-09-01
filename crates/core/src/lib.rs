//! `DevToolbox` 的纯领域核心。
//!
//! 此 crate 不依赖 Tauri、文件系统或 UI 框架，因此 Markdown 任务规则可被
//! 桌面应用和回归测试共同复用。

pub mod geography;
pub mod history;
pub mod language;
pub mod parser;
pub mod task_state;
pub mod travel;

pub use language::{
    LanguageCode, LanguageItem, LanguageItemType, LanguageMetadata, LearningState,
    LearningStateKind, ReviewRating, ReviewScheduler, SourceLicense, kana_to_romaji,
    normalize_roman, score as speaking_score, tones_from_syllables,
};
pub use parser::{
    TaskLineInfo, cycle_task_mark, is_task_line, iter_task_lines, make_task_line, match_task,
    set_task_mark,
};
pub use task_state::{TaskState, TaskStateRegistry, default_registry};
