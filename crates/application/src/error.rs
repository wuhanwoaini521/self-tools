use std::path::PathBuf;

use thiserror::Error;

use crate::history::ports::HistoryPortError;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("document path is empty")]
    EmptyDocumentPath,
    #[error("workspace path is empty")]
    EmptyWorkspacePath,
    #[error("feed url is invalid (expect http/https): {0}")]
    InvalidFeedUrl(String),
    #[error("feed already subscribed: {0}")]
    DuplicateFeed(String),
    #[error("feed not found: {0}")]
    FeedNotFound(i64),
    #[error("city name is empty")]
    EmptyCity,
    #[error("travel research failed: {0}")]
    TravelFailed(String),
    #[error("geography error: {message}")]
    Geography { message: String },
    #[error("geography data error: {0}")]
    GeographyData(String),
    /// History 只读查询失败。消息由适配层在端口错误中提供，保持此前桌面端
    /// 透传的 Infrastructure 错误文本不变（用户可见的 message 与 code 均不改变）。
    /// 注意：不能写 `#[from]`（Rss 已占用 From<InfrastructureError>），
    /// 由 HistoryService 用 `map_err(ApplicationError::History)` 显式构造。
    #[error(transparent)]
    History(HistoryPortError),
    #[error("operation failed for {path}: {message}")]
    Infrastructure {
        path: PathBuf,
        /// 可显示的错误文本（适配层已把基础设施错误转换为字符串）。
        message: String,
    },
    #[error("language error: {message}")]
    Language { message: String },
    #[error("language license gate: {0}")]
    License(String),
    /// RSS 失败分类（Gate 7.6：消息由应用层端口提供，不再携带基础设施类型；
    /// 桌面端错误 code 映射保持不变：Fetch→rss_fetch_failed / Parse→rss_parse_failed /
    /// Repository→infrastructure_error）。
    #[error("rss error: {message}")]
    Rss { kind: RssErrorKind, message: String },
    /// Travel 失败分类（Gate 8：不再携带 `InfrastructureError`；`TravelFailure`
    /// 提供 kind + 文本。桌面端错误 code 映射保持不变：Search→travel_search_failed /
    /// Fetch→travel_fetch_failed / Llm→travel_llm_failed / Data→travel_data_failed /
    /// Store→travel_error）。
    #[error("travel error: {0}")]
    Travel(TravelFailure),
}

/// RSS 失败分类（与端口 `FeedFetchErrorKind` 对齐，存储错误归为 `Repository`）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RssErrorKind {
    Fetch,
    Parse,
    Repository,
}

/// Travel 失败分类。`Store` 为本地缓存存取错误（消息不带前缀，与旧
/// `InfrastructureError` 直传文本一致）；其余类别对应 Provider 失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TravelErrorKind {
    Search,
    Fetch,
    Llm,
    Data,
    Store,
}

/// Travel 失败载荷：分类 + 人类可读文本（不携带任何基础设施类型）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TravelFailure {
    pub kind: TravelErrorKind,
    pub message: String,
}

impl TravelFailure {
    #[must_use]
    pub fn new(kind: TravelErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// 与既有 `ApplicationError::Travel { source }` 的 Display 文本保持一致：
/// Search/Fetch/Llm/Data 补 "travel … failed: " 前缀，Store 直通存储层文本。
impl std::fmt::Display for TravelFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            TravelErrorKind::Store => f.write_str(&self.message),
            TravelErrorKind::Search => write!(f, "travel search failed: {}", self.message),
            TravelErrorKind::Fetch => write!(f, "travel page fetch failed: {}", self.message),
            TravelErrorKind::Llm => write!(f, "travel llm request failed: {}", self.message),
            TravelErrorKind::Data => write!(f, "travel data provider failed: {}", self.message),
        }
    }
}

pub(crate) fn infrastructure(path: PathBuf, message: String) -> ApplicationError {
    ApplicationError::Infrastructure { path, message }
}