//! 桌面端组合根（Composition Root）：把基础设施实现绑定到 application 的端口。
//!
//! 本模块只做装配（创建适配器、把 Store/Port 接成 Service），不含业务规则、
//! SQL 或 UI；端口 trait 全部定义在 `devtoolbox_application`，基础设施类型
//! 全部来自 `devtoolbox_infrastructure`，二者在这里汇合。
//!
//! 每个领域一个轻量适配器（文档 + 设置 + History + Geography），
//! 避免出现中心化的 God Object。

use std::path::{Path, PathBuf};

use devtoolbox_application::workflows::{
    DocumentStoreError, DocumentStorePort, SettingsStoreError, SettingsStorePort,
};
use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::workspace::WorkspaceFile;
use devtoolbox_infrastructure::{SettingsStore, read_utf8, scan_markdown_files, write_utf8_atomic};

// ---------- 文档 / 工作区（文件系统适配器） ----------

/// 无状态文件系统文档存储：每次调用直接读 / 写 / 扫描，不持有任何状态。
#[derive(Clone, Copy, Default)]
pub struct DocumentStoreAdapter;

impl DocumentStorePort for DocumentStoreAdapter {
    fn read(&self, path: &Path) -> Result<String, DocumentStoreError> {
        read_utf8(path).map_err(|error| DocumentStoreError(error.to_string()))
    }
    fn write(&self, path: &Path, text: &str) -> Result<(), DocumentStoreError> {
        write_utf8_atomic(path, text).map_err(|error| DocumentStoreError(error.to_string()))
    }
    fn scan_markdown(&self, root: &Path) -> Result<Vec<WorkspaceFile>, DocumentStoreError> {
        scan_markdown_files(root).map_err(|error| DocumentStoreError(error.to_string()))
    }
}

// ---------- 设置（SettingsStore 适配器） ----------

/// 把 `devtoolbox_infrastructure::SettingsStore` 包装成 application 的设置端口。
pub struct SettingsStoreAdapter {
    store: SettingsStore,
}

impl SettingsStoreAdapter {
    #[must_use]
    pub fn new(store: SettingsStore) -> Self {
        Self { store }
    }
}

impl SettingsStorePort for SettingsStoreAdapter {
    fn path(&self) -> PathBuf {
        self.store.path().to_owned()
    }
    fn load(&self) -> Result<AppSettings, SettingsStoreError> {
        self.store
            .load()
            .map_err(|error| SettingsStoreError(error.to_string()))
    }
    fn save(&self, settings: &AppSettings) -> Result<(), SettingsStoreError> {
        self.store
            .save(settings)
            .map_err(|error| SettingsStoreError(error.to_string()))
    }
}

// ---------- Language（SQLite 适配器，Gate 7.5） ----------

use std::sync::{Arc, Mutex};

use devtoolbox_application::language::{
    LanguageCount, LanguageDetailRows, LanguageExample, LanguageStorePort, SearchHitModel,
};
use devtoolbox_core::language::{
    DatasetManifest, LanguageCode, LanguageItem, LanguageSource, LearningState, LearningStateKind,
    ReviewOutcome, ReviewRating, SentenceRecord, TodayPlan,
};
use devtoolbox_infrastructure::language::LanguageStore;

/// 把 `devtoolbox_infrastructure::language::LanguageStore`（SQLite）包装成
/// application 的语言存储端口，存储语义与升级前一致（短锁、不跨 await）。
pub struct LanguageStoreAdapter {
    store: Arc<Mutex<LanguageStore>>,
}

impl LanguageStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<LanguageStore>>) -> Self {
        Self { store }
    }
}

impl LanguageStorePort for LanguageStoreAdapter {
    fn language_counts(&self) -> Result<Vec<LanguageCount>, String> {
        let store = self.store.lock().expect("language store poisoned");
        store
            .language_counts()
            .map(|counts| counts.into_iter().map(map_count).collect())
            .map_err(err_text)
    }

    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHitModel>, String> {
        let store = self.store.lock().expect("language store poisoned");
        store
            .search(language, query, limit)
            .map(|hits| {
                hits.into_iter()
                    .map(|hit| SearchHitModel {
                        item: hit.item,
                        matched: hit.matched,
                    })
                    .collect()
            })
            .map_err(err_text)
    }

    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String> {
        let store = self.store.lock().expect("language store poisoned");
        store
            .item_detail(id)
            .map(|rows| LanguageDetailRows {
                item: rows.item,
                meanings: rows.meanings,
                pronunciations: rows.pronunciations,
                relations: rows.relations,
                related_items: rows.related_items,
                examples: rows
                    .examples
                    .into_iter()
                    .map(|example| LanguageExample {
                        text: example.text,
                        translation: example.translation,
                        source: example.source,
                    })
                    .collect(),
                sentences: rows.sentences,
                state: rows.state,
                favorite: rows.favorite,
                extra: rows.extra,
            })
            .map_err(err_text)
    }

    fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .source_by_id(id)
            .map_err(err_text)
    }

    fn today_plan(&self, language: LanguageCode, now: i64) -> Result<TodayPlan, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .today_plan(language, now)
            .map_err(err_text)
    }

    fn review_next(
        &self,
        language: LanguageCode,
        now: i64,
    ) -> Result<Option<LanguageItem>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .review_next(language, now)
            .map_err(err_text)
    }

    fn learning_state(&self, item_id: &str) -> Result<Option<LearningState>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .learning_state(item_id)
            .map_err(err_text)
    }

    fn rate_review(
        &self,
        item_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewOutcome, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .rate_review(item_id, rating, now)
            .map_err(err_text)
    }

    fn toggle_favorite(&self, item_id: &str, now: i64) -> Result<bool, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .toggle_favorite(item_id, now)
            .map_err(err_text)
    }

    fn favorites(&self, limit: usize) -> Result<Vec<LanguageItem>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .favorites(limit)
            .map_err(err_text)
    }

    fn set_learning_state(
        &self,
        item_id: &str,
        state: LearningStateKind,
        now: i64,
    ) -> Result<(), String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .set_learning_state(item_id, state, now)
            .map_err(err_text)
    }

    fn progress(&self) -> Result<serde_json::Value, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .progress()
            .map_err(err_text)
    }

    fn favorites_count(&self) -> Result<i64, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .favorites_count()
            .map_err(err_text)
    }

    fn sources(&self) -> Result<Vec<LanguageSource>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .sources()
            .map_err(err_text)
    }

    fn manifests(&self) -> Result<Vec<DatasetManifest>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .manifests()
            .map_err(err_text)
    }

    fn count_by_source(&self, source_id: &str) -> Result<i64, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .count_by_source(source_id)
            .map_err(err_text)
    }

    fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, String> {
        self.store
            .lock()
            .expect("language store poisoned")
            .sentences_by_language(language, limit)
            .map_err(err_text)
    }
}

fn map_count(count: devtoolbox_core::language::LanguageCount) -> LanguageCount {
    LanguageCount {
        language: count.language,
        words: count.words,
        phrases: count.phrases,
        sentences: count.sentences,
        total: count.total,
    }
}

fn err_text(error: devtoolbox_infrastructure::InfrastructureError) -> String {
    error.to_string()
}
// ---------- Study Board（SQLite 适配器 → application 端口，V11 §112） ----------

use devtoolbox_application::study_board::ports::{StudyBoardStoreError, StudyBoardStorePort};
use devtoolbox_core::study_board::{StudyBoard, StudyBoardSnapshot, StudyBoardSummary};

/// 把 `devtoolbox_infrastructure::StudyBoardSqliteStore` 包装成 application 端口。
///
/// 只做错误类型转换：端口语义（upsert 幂等、列表不含笔迹）由 store 本身保证。
pub struct StudyBoardStoreAdapter {
    store: Arc<devtoolbox_infrastructure::StudyBoardSqliteStore>,
}

impl StudyBoardStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<devtoolbox_infrastructure::StudyBoardSqliteStore>) -> Self {
        Self { store }
    }
}

impl StudyBoardStorePort for StudyBoardStoreAdapter {
    fn upsert_board(&self, board: &StudyBoard) -> Result<(), StudyBoardStoreError> {
        self.store.upsert_board(board).map_err(|error| StudyBoardStoreError(error.to_string()))
    }

    fn get_board(&self, id: &str) -> Result<Option<StudyBoard>, StudyBoardStoreError> {
        self.store.get_board(id).map_err(|error| StudyBoardStoreError(error.to_string()))
    }

    fn list_boards(&self, limit: usize) -> Result<Vec<StudyBoardSummary>, StudyBoardStoreError> {
        self.store.list_boards(limit).map_err(|error| StudyBoardStoreError(error.to_string()))
    }

    fn upsert_snapshot(&self, snapshot: &StudyBoardSnapshot) -> Result<(), StudyBoardStoreError> {
        self.store.upsert_snapshot(snapshot).map_err(|error| StudyBoardStoreError(error.to_string()))
    }

    fn get_snapshot(&self, id: &str) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError> {
        self.store.get_snapshot(id).map_err(|error| StudyBoardStoreError(error.to_string()))
    }

    fn latest_snapshot(
        &self,
        board_id: &str,
    ) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError> {
        self.store.latest_snapshot(board_id).map_err(|error| StudyBoardStoreError(error.to_string()))
    }
}

// ---------- Conversation（SQLite 适配器 → application 端口，V11 §96） ----------

use devtoolbox_core::personal_ai::conversation::{
    Conversation, ConversationMessage, ConversationSummary,
};

/// 把 `devtoolbox_infrastructure::ConversationSqliteStore` 包装成 application 端口。
/// 只做错误类型转换；时间戳由 store 的 `now` 参数决定（调用方给 `now_unix`）。
pub struct ConversationStoreAdapter {
    store: Arc<devtoolbox_infrastructure::ConversationSqliteStore>,
}

impl ConversationStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<devtoolbox_infrastructure::ConversationSqliteStore>) -> Self {
        Self { store }
    }
}

fn conversation_error(
    error: devtoolbox_infrastructure::InfrastructureError,
) -> devtoolbox_application::personal_ai::ConversationStoreError {
    devtoolbox_application::personal_ai::ConversationStoreError(error.to_string())
}

impl devtoolbox_application::personal_ai::ConversationStore for ConversationStoreAdapter {
    fn list(
        &self,
        limit: usize,
    ) -> Result<Vec<ConversationSummary>, devtoolbox_application::personal_ai::ConversationStoreError>
    {
        self.store.list(limit, false).map_err(conversation_error)
    }

    fn list_all(
        &self,
        limit: usize,
    ) -> Result<Vec<ConversationSummary>, devtoolbox_application::personal_ai::ConversationStoreError>
    {
        self.store.list(limit, true).map_err(conversation_error)
    }

    fn load(
        &self,
        conversation_id: &str,
    ) -> Result<Option<Conversation>, devtoolbox_application::personal_ai::ConversationStoreError>
    {
        self.store.load(conversation_id).map_err(conversation_error)
    }

    fn create(
        &self,
        title: &str,
        module_origin: Option<&str>,
    ) -> Result<Conversation, devtoolbox_application::personal_ai::ConversationStoreError> {
        self.store
            .create(title, module_origin, devtoolbox_infrastructure::now_unix())
            .map_err(conversation_error)
    }

    fn append_message(
        &self,
        conversation_id: &str,
        message: &ConversationMessage,
    ) -> Result<(), devtoolbox_application::personal_ai::ConversationStoreError> {
        self.store
            .append_message(conversation_id, message, devtoolbox_infrastructure::now_unix())
            .map_err(conversation_error)
    }

    fn rename(
        &self,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), devtoolbox_application::personal_ai::ConversationStoreError> {
        self.store
            .rename(conversation_id, title)
            .map_err(conversation_error)
    }

    fn set_archived(
        &self,
        conversation_id: &str,
        archived: bool,
    ) -> Result<(), devtoolbox_application::personal_ai::ConversationStoreError> {
        self.store
            .set_archived(conversation_id, archived)
            .map_err(conversation_error)
    }

    fn delete(
        &self,
        conversation_id: &str,
    ) -> Result<(), devtoolbox_application::personal_ai::ConversationStoreError> {
        self.store
            .delete(conversation_id)
            .map_err(conversation_error)
    }
}

// ---------- RSS（SQLite 仓储 + HTTP 抓取适配器，Gate 7.6） ----------

use devtoolbox_application::rss::{
    FeedFetchError, FeedFetchErrorKind, FeedFetcherPort, RssRepositoryPort,
};
use devtoolbox_core::rss::{ArticleRow, FeedRow, FetchedEntry, FetchedFeed};
use devtoolbox_infrastructure::FeedRepository;

/// 把 `FeedRepository`（SQLite）包装成 application 的 RSS 持久化端口。
pub struct RssRepositoryAdapter {
    store: Arc<Mutex<FeedRepository>>,
}

impl RssRepositoryAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<FeedRepository>>) -> Self {
        Self { store }
    }
}

impl RssRepositoryPort for RssRepositoryAdapter {
    fn list_feeds(&self) -> Result<Vec<FeedRow>, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .list_feeds()
            .map_err(|e| e.to_string())
    }
    fn find_feed_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .find_feed_id_by_url(url)
            .map_err(|e| e.to_string())
    }
    fn insert_feed(&self, title: &str, url: &str, site_url: Option<&str>) -> Result<i64, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .insert_feed(title, url, site_url)
            .map_err(|e| e.to_string())
    }
    fn insert_articles(&self, feed_id: i64, entries: &[FetchedEntry]) -> Result<usize, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .insert_articles(feed_id, entries)
            .map_err(|e| e.to_string())
    }
    fn set_feed_success(&self, feed_id: i64) -> Result<(), String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .set_feed_success(feed_id)
            .map_err(|e| e.to_string())
    }
    fn set_feed_error(&self, feed_id: i64, message: &str) -> Result<(), String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .set_feed_error(feed_id, message)
            .map_err(|e| e.to_string())
    }
    fn feed_title(&self, feed_id: i64) -> Result<Option<String>, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .feed_title(feed_id)
            .map_err(|e| e.to_string())
    }
    fn list_articles(&self, feed_id: i64, limit: i64) -> Result<Vec<ArticleRow>, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .list_articles(feed_id, limit)
            .map_err(|e| e.to_string())
    }
    fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .latest_articles(limit)
            .map_err(|e| e.to_string())
    }
    fn mark_article_read(&self, article_id: i64) -> Result<(), String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .mark_article_read(article_id)
            .map_err(|e| e.to_string())
    }
    fn delete_feed(&self, feed_id: i64) -> Result<(), String> {
        self.store
            .lock()
            .expect("rss store poisoned")
            .delete_feed(feed_id)
            .map_err(|e| e.to_string())
    }
}

/// 把共享 `reqwest::Client` + `fetch_feed` 包装成 application 的抓取端口。
pub struct FeedFetcherAdapter {
    client: reqwest::Client,
}

impl FeedFetcherAdapter {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl FeedFetcherPort for FeedFetcherAdapter {
    async fn fetch_feed(&self, url: &str) -> Result<FetchedFeed, FeedFetchError> {
        devtoolbox_infrastructure::fetch_feed(url, &self.client)
            .await
            .map_err(|error| {
                let (kind, message) = match &error {
                    devtoolbox_infrastructure::InfrastructureError::FeedFetch(message) => {
                        (FeedFetchErrorKind::Fetch, message.clone())
                    }
                    devtoolbox_infrastructure::InfrastructureError::FeedParse(message) => {
                        (FeedFetchErrorKind::Parse, message.clone())
                    }
                    other => (FeedFetchErrorKind::Fetch, other.to_string()),
                };
                FeedFetchError { kind, message }
            })
    }
}

// ---------- 旅行（缓存存储适配器） ----------

use devtoolbox_application::travel::TravelStorePort;
use devtoolbox_core::travel::{CityGuide, SearchResult, TravelDocument};
use devtoolbox_infrastructure::TravelStore;

/// 把 `Arc<Mutex<TravelStore>>`（SQLite 旅行缓存）包装成 application 的旅行存储端口；
/// 语义与 Gate 6 前 `TravelResearchService` 直接持锁读缓存一致（短锁、不跨 await）。
pub struct TravelStoreAdapter {
    store: Arc<Mutex<TravelStore>>,
}

impl TravelStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<TravelStore>>) -> Self {
        Self { store }
    }
}

impl TravelStorePort for TravelStoreAdapter {
    fn get_guide(&self, city: &str, days: u8, now: i64) -> Result<Option<CityGuide>, String> {
        self.store
            .lock()
            .expect("travel store poisoned")
            .get_guide(city, days, now)
            .map_err(err_text)
    }

    fn upsert_guide(&self, guide: &CityGuide, now: i64) -> Result<CityGuide, String> {
        self.store
            .lock()
            .expect("travel store poisoned")
            .upsert_guide(guide, now)
            .map_err(err_text)
    }

    fn get_search_results(
        &self,
        query: &str,
        now: i64,
    ) -> Result<Option<Vec<SearchResult>>, String> {
        self.store
            .lock()
            .expect("travel store poisoned")
            .get_search_results(query, now)
            .map_err(err_text)
    }

    fn put_search_results(
        &self,
        query: &str,
        results: &[SearchResult],
        now: i64,
    ) -> Result<(), String> {
        self.store
            .lock()
            .expect("travel store poisoned")
            .put_search_results(query, results, now)
            .map_err(err_text)
    }

    fn get_document(&self, url: &str, now: i64) -> Result<Option<TravelDocument>, String> {
        self.store
            .lock()
            .expect("travel store poisoned")
            .get_document(url, now)
            .map_err(err_text)
    }

    fn put_document(&self, document: &TravelDocument, now: i64) -> Result<(), String> {
        self.store
            .lock()
            .expect("travel store poisoned")
            .put_document(document, now)
            .map_err(err_text)
    }
}
