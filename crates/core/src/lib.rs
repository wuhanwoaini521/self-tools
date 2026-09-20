//! `DevToolbox` 的纯领域核心。
//!
//! 此 crate 不依赖 Tauri、文件系统或 UI 框架，因此 Markdown 任务规则可被
//! 桌面应用和回归测试共同复用。

pub mod geography;
pub mod personal_ai;
pub mod rss;
pub mod history_records;
pub mod language;
pub mod parser;
pub mod settings;
pub mod task_state;
pub mod travel;
pub mod workspace;

pub use personal_ai::{
    Action, ActionKind, AgentError, AgentErrorKind, AgentMessage, AgentRequest, AgentResponse,
    AgentUsage, AppContext, ChatMessage, ChatModelProvider, ChatRequest, ChatResponse, ChatRole,
    ChatToolCall, ChatToolSpec, ChatUsage, EntityRef, ModuleDescriptor, ProviderError,
    ProviderErrorKind, SelectionRef, ToolCallRequest, ToolResult, ToolRisk, ToolSpec,
    ToolTraceEntry, UiBlock, UiBlockKind,
};

pub use settings::{
    AiSettings, AppSettings, GeographySettings, MarkdownView, ThemeMode, TravelSearchBackend,
    TravelSettings,
};
pub use workspace::WorkspaceFile;
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
