//! 共享端口适配器：News 持久化（SQLite）→ `NewsRepositoryPort`。
//!
//! 与 [`super::language`] 同理：原先只在 `apps/desktop`，服务端无法复用，
//! 于是网页端的 News 拿不到数据。

use std::sync::Arc;

use devtoolbox_application::news::ports::NewsRepositoryPort;
use parking_lot::Mutex;

use crate::news_store::NewsRepository;
use devtoolbox_core::news::{NewsArticle, NewsCategory, NewsSource};

/// 把 `NewsRepository`（`config/news.db`）包装成 application 的 News 持久化端口。
///
/// **与 `RssRepositoryAdapter` 无关** —— 两个 bounded context 各自持有各自的
/// store（ADR-010：共享基础设施，不共享 repository）。
pub struct NewsRepositoryAdapter {
    store: Arc<Mutex<NewsRepository>>,
}

impl NewsRepositoryAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<NewsRepository>>) -> Self {
        Self { store }
    }

    fn lock(&self) -> parking_lot::MutexGuard<'_, NewsRepository> {
        self.store.lock()
    }
}

impl NewsRepositoryPort for NewsRepositoryAdapter {
    fn list_sources(&self) -> Result<Vec<NewsSource>, String> {
        self.lock()
            .list_sources()
            .map_err(|error| error.to_string())
    }
    fn source_by_id(&self, source_id: i64) -> Result<Option<NewsSource>, String> {
        self.lock()
            .source_by_id(source_id)
            .map_err(|error| error.to_string())
    }
    fn find_source_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        self.lock()
            .find_source_id_by_url(url)
            .map_err(|error| error.to_string())
    }
    fn insert_source(
        &self,
        name: &str,
        url: &str,
        category: NewsCategory,
        site_url: Option<&str>,
    ) -> Result<i64, String> {
        self.lock()
            .insert_source(name, url, category, site_url)
            .map_err(|error| error.to_string())
    }
    fn set_source_category(&self, source_id: i64, category: NewsCategory) -> Result<(), String> {
        self.lock()
            .set_source_category(source_id, category)
            .map_err(|error| error.to_string())
    }
    fn set_source_health(&self, source_id: i64, error: Option<&str>) -> Result<(), String> {
        self.lock()
            .set_source_health(source_id, error)
            .map_err(|error| error.to_string())
    }
    fn delete_source(&self, source_id: i64) -> Result<(), String> {
        self.lock()
            .delete_source(source_id)
            .map_err(|error| error.to_string())
    }
    fn has_failed_source(&self) -> Result<bool, String> {
        self.lock()
            .has_failed_source()
            .map_err(|error| error.to_string())
    }
    fn insert_articles(
        &self,
        source_id: i64,
        entries: &[devtoolbox_core::feed::FetchedEntry],
    ) -> Result<usize, String> {
        self.lock()
            .insert_articles(source_id, entries)
            .map_err(|error| error.to_string())
    }
    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.lock().latest(limit).map_err(|error| error.to_string())
    }
    fn latest_by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        self.lock()
            .latest_by_category(category, limit)
            .map_err(|error| error.to_string())
    }
    fn latest_by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.lock()
            .latest_by_source(source_id, limit)
            .map_err(|error| error.to_string())
    }
    fn article_by_id(&self, article_id: i64) -> Result<Option<NewsArticle>, String> {
        self.lock()
            .article_by_id(article_id)
            .map_err(|error| error.to_string())
    }
    fn query_articles(
        &self,
        keyword: Option<&str>,
        source_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        self.lock()
            .query_articles(keyword, source_id, limit)
            .map_err(|error| error.to_string())
    }
    fn starred_articles(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.lock()
            .starred_articles(limit)
            .map_err(|error| error.to_string())
    }
    fn toggle_star(&self, article_id: i64) -> Result<bool, String> {
        self.lock()
            .toggle_star(article_id)
            .map_err(|error| error.to_string())
    }
    fn mark_read(&self, article_id: i64) -> Result<(), String> {
        self.lock()
            .mark_read(article_id)
            .map_err(|error| error.to_string())
    }
    fn unread_total(&self) -> Result<i64, String> {
        self.lock()
            .unread_total()
            .map_err(|error| error.to_string())
    }
}
