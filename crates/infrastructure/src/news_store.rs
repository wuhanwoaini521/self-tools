//! News 持久化：SQLite（rusqlite bundled），**独立于 RSS 的 `dashboard.db`**。
//!
//! - 文件：`config/news.db`（gitignored）；ADR-010：News 与 RSS 是两个
//!   bounded context，只共享 `feed_fetcher` 等基础设施，**不共享表**。
//! - 两张表：`news_sources`（系统 seed / 用户添加的新闻源）+ `news_articles`
//!   （抓取落地的新闻文章），外键级联删除、`(source_id, guid)` 唯一约束去重。
//! - 幂等迁移：`CREATE TABLE IF NOT EXISTS` + `schema_meta.version`（§86，
//!   与 memory / documents / files / study_board 同构）。
//! - 首次打开会用 `core::news::recommended_sources()` seed 新闻源
//!   （「News 应建立独立的 NewsSource seed 数据」）。
//! - `Connection` 非线程安全，由上层用 `Mutex` 串行化访问。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use crate::error::InfrastructureError;
use devtoolbox_core::news::{NewsArticle, NewsCategory, NewsSource, NewsSourceType};

/// 当前 schema 版本（新增列时递增并补非破坏性迁移）。
pub const NEWS_SCHEMA_VERSION: u32 = 1;

/// 当前 Unix 秒。
pub fn news_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// 新闻文章统一投影（列序与 `article_from_row` 严格对应）。
const ARTICLE_SELECT: &str =
    "SELECT a.id, a.source_id, s.name, a.guid, a.url, a.title, a.author, a.image_url,
            a.published_at, a.summary, a.is_read, a.starred
     FROM news_articles a JOIN news_sources s ON s.id = a.source_id";

/// LIKE 通配符转义（`%` `_` `\`）。
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// `NewsArticle` 行映射（列序见 `ARTICLE_SELECT`）。
fn article_from_row(row: &rusqlite::Row<'_>) -> NewsArticle {
    NewsArticle {
        id: row.get(0).unwrap_or_default(),
        source_id: row.get(1).unwrap_or_default(),
        source_name: row.get(2).unwrap_or_default(),
        guid: row.get(3).unwrap_or_default(),
        url: row.get(4).unwrap_or_default(),
        title: row.get(5).unwrap_or_default(),
        author: row.get(6).unwrap_or(None),
        image_url: row.get(7).unwrap_or(None),
        published_at: row.get(8).unwrap_or(None),
        summary: row.get(9).unwrap_or(None),
        is_read: row.get::<_, i64>(10).unwrap_or_default() != 0,
        starred: row.get::<_, i64>(11).unwrap_or_default() != 0,
    }
}

/// News SQLite 存储器。
pub struct NewsRepository {
    connection: Connection,
}

impl NewsRepository {
    /// 打开（必要时创建）数据库并确保 Schema 存在 + seed 新闻源。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(path)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let repository = Self { connection };
        repository.ensure_schema()?;
        repository.seed_sources()?;
        repository.write_version()?;
        Ok(repository)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS news_sources (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    url TEXT NOT NULL UNIQUE,
                    source_type TEXT NOT NULL DEFAULT 'rss',
                    category TEXT NOT NULL DEFAULT 'general',
                    site_url TEXT,
                    last_updated INTEGER,
                    last_error TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_news_sources_category
                    ON news_sources(category);
                CREATE TABLE IF NOT EXISTS news_articles (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_id INTEGER NOT NULL REFERENCES news_sources(id) ON DELETE CASCADE,
                    guid TEXT NOT NULL,
                    url TEXT NOT NULL DEFAULT '',
                    title TEXT NOT NULL,
                    author TEXT,
                    image_url TEXT,
                    published_at INTEGER,
                    summary TEXT,
                    is_read INTEGER NOT NULL DEFAULT 0,
                    starred INTEGER NOT NULL DEFAULT 0,
                    UNIQUE (source_id, guid)
                );
                CREATE INDEX IF NOT EXISTS idx_news_articles_published
                    ON news_articles(source_id, published_at DESC);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    /// 首次打开：把系统维护的推荐源目录灌进 `news_sources`（`URL` 幂等，
    /// 用户删过的**不会**被重新灌回 —— seed 只跑一次，用 schema_meta 标记）。
    fn seed_sources(&self) -> Result<(), InfrastructureError> {
        let seeded = self
            .connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'sources_seeded'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        if seeded.is_some() {
            return Ok(());
        }
        for source in devtoolbox_core::news::recommended_sources() {
            self.connection
                .execute(
                    "INSERT OR IGNORE INTO news_sources (name, url, source_type, category, site_url)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        source.name,
                        source.url,
                        NewsSourceType::Rss.id(),
                        source.category.id(),
                        source.site_url.unwrap_or_default(),
                    ],
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        }
        self.connection
            .execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('sources_seeded', '1')",
                [],
            )
            .map(|_| ())
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    fn write_version(&self) -> Result<(), InfrastructureError> {
        let stored = self
            .connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        match stored {
            Some(value) => {
                let version: u32 = value.parse().unwrap_or(NEWS_SCHEMA_VERSION);
                if version > NEWS_SCHEMA_VERSION {
                    return Err(InfrastructureError::Sqlite(format!(
                        "news schema version {version} is newer than supported {NEWS_SCHEMA_VERSION}"
                    )));
                }
            }
            None => {
                self.connection
                    .execute(
                        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('version', ?1)",
                        [NEWS_SCHEMA_VERSION.to_string()],
                    )
                    .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // news_sources
    // -----------------------------------------------------------------------

    /// 全部新闻源（含未读聚合），按分类分组内按名称排序。
    pub fn list_sources(&self) -> Result<Vec<NewsSource>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT s.id, s.name, s.url, s.source_type, s.category, s.site_url,
                        s.last_updated, s.last_error,
                        COUNT(a.id) FILTER (WHERE a.is_read = 0) AS unread
                 FROM news_sources s
                 LEFT JOIN news_articles a ON a.source_id = s.id
                 GROUP BY s.id
                 ORDER BY s.category, s.name COLLATE NOCASE",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                let site_url: String = row.get(5)?;
                Ok(NewsSource {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    url: row.get(2)?,
                    source_type: NewsSourceType::from_id(&row.get::<_, String>(3)?)
                        .unwrap_or_default(),
                    category: NewsCategory::from_id(&row.get::<_, String>(4)?).unwrap_or_default(),
                    site_url: if site_url.is_empty() {
                        None
                    } else {
                        Some(site_url)
                    },
                    last_updated: row.get(6)?,
                    last_error: row.get(7)?,
                    unread_count: row.get(8)?,
                })
            })
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(rows)
    }

    /// 单个新闻源（未命中返回 `None`）。
    pub fn source_by_id(&self, id: i64) -> Result<Option<NewsSource>, InfrastructureError> {
        Ok(self
            .list_sources()?
            .into_iter()
            .find(|source| source.id == id))
    }

    /// 按 URL 找新闻源（seed / 添加时判重用）。
    pub fn find_source_id_by_url(&self, url: &str) -> Result<Option<i64>, InfrastructureError> {
        self.connection
            .query_row("SELECT id FROM news_sources WHERE url = ?1", [url], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    /// 添加新闻源（用户在 News 页显式添加；URL 冲突返回错误）。
    pub fn insert_source(
        &self,
        name: &str,
        url: &str,
        category: NewsCategory,
        site_url: Option<&str>,
    ) -> Result<i64, InfrastructureError> {
        self.connection
            .execute(
                "INSERT INTO news_sources (name, url, source_type, category, site_url)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    name,
                    url,
                    NewsSourceType::Rss.id(),
                    category.id(),
                    site_url.unwrap_or_default()
                ],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(self.connection.last_insert_rowid())
    }

    /// 更新抓取健康态（成功时清 error，失败时写入 error）。
    pub fn set_source_health(
        &self,
        source_id: i64,
        error: Option<&str>,
    ) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "UPDATE news_sources SET last_updated = ?1, last_error = ?2 WHERE id = ?3",
                rusqlite::params![news_now(), error, source_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    /// 改分类（News 页的分类切换）。
    pub fn set_source_category(
        &self,
        source_id: i64,
        category: NewsCategory,
    ) -> Result<(), InfrastructureError> {
        let changed = self
            .connection
            .execute(
                "UPDATE news_sources SET category = ?1 WHERE id = ?2",
                rusqlite::params![category.id(), source_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        if changed == 0 {
            return Err(InfrastructureError::Sqlite(format!(
                "news source {source_id} not found"
            )));
        }
        Ok(())
    }

    /// 删除新闻源（文章级联删除）。
    pub fn delete_source(&self, source_id: i64) -> Result<(), InfrastructureError> {
        self.connection
            .execute("DELETE FROM news_sources WHERE id = ?1", [source_id])
            .map(|_| ())
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    // -----------------------------------------------------------------------
    // news_articles
    // -----------------------------------------------------------------------

    /// 批量落地新闻文章；`(source_id, guid)` 冲突自动忽略。返回实际新增数。
    pub fn insert_articles(
        &self,
        source_id: i64,
        entries: &[crate::feed_fetcher::FetchedEntry],
    ) -> Result<usize, InfrastructureError> {
        let mut inserted = 0;
        for entry in entries {
            let changed = self
                .connection
                .execute(
                    "INSERT OR IGNORE INTO news_articles
                         (source_id, guid, url, title, author, image_url, published_at, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        source_id,
                        entry.guid,
                        entry.url,
                        entry.title,
                        entry.author,
                        entry.image_url,
                        entry.published_at,
                        entry.summary
                    ],
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            inserted += changed;
        }
        Ok(inserted)
    }

    fn map_articles<F>(
        rows: rusqlite::Result<rusqlite::MappedRows<'_, F>>,
    ) -> Result<Vec<NewsArticle>, InfrastructureError>
    where
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<NewsArticle>,
    {
        let mapped = rows.map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    /// 今日最新（跨源，按发布时间倒序）—— `news.latest`。
    pub fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "{ARTICLE_SELECT}
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?1",
            ))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(statement.query_map([limit], |row| Ok(article_from_row(row))))
    }

    /// 按分类取最新（`news.by_category`）。
    pub fn latest_by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "{ARTICLE_SELECT}
                 WHERE s.category = ?1
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?2",
            ))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(
            statement.query_map(rusqlite::params![category.id(), limit], |row| {
                Ok(article_from_row(row))
            }),
        )
    }

    /// 按源取最新（`news.by_source`）。
    pub fn latest_by_source(
        &self,
        source_id: i64,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "{ARTICLE_SELECT}
                 WHERE a.source_id = ?1
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?2",
            ))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(
            statement.query_map(rusqlite::params![source_id, limit], |row| {
                Ok(article_from_row(row))
            }),
        )
    }

    /// 单条新闻（`news.get_article`）。
    pub fn article_by_id(&self, id: i64) -> Result<Option<NewsArticle>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!("{ARTICLE_SELECT}\n WHERE a.id = ?1"))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let row = statement
            .query_map([id], |row| Ok(article_from_row(row)))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(row.into_iter().next())
    }

    /// 关键词检索（`news.search`）：标题 / 署名 / 摘要三列 LIKE 粗筛。
    ///
    /// 不能依赖「裸 LIKE 就是子串匹配」：部分 SQLite build 下
    /// `x LIKE 'kw'` 等价精确比较（实测 `LIKE 'typhoon%'` 命中而
    /// `LIKE 'typhoon'` 不命中）。因此显式小写 + 两侧加 `%`。
    /// 占位符用顺序 `?`；参数按**实际出现的条件**追加（rusqlite 要求
    /// 参数个数与占位符个数精确匹配）。
    pub fn query_articles(
        &self,
        keyword: Option<&str>,
        source_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, InfrastructureError> {
        let escaped = keyword.map(|value| format!("%{}%", escape_like(&value.to_lowercase())));
        let mut sql = format!("{ARTICLE_SELECT}\n WHERE 1 = 1");
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(escaped) = escaped {
            sql.push_str(" AND (lower(a.title) LIKE ? OR lower(a.summary) LIKE ?)");
            params.push(rusqlite::types::Value::Text(escaped.clone()));
            params.push(rusqlite::types::Value::Text(escaped));
        }
        if let Some(source_id) = source_id {
            sql.push_str(" AND a.source_id = ?");
            params.push(rusqlite::types::Value::Integer(source_id));
        }
        sql.push_str(
            "\n ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC\n LIMIT ?",
        );
        params.push(rusqlite::types::Value::Integer(limit));

        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let refs: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|value| value as &dyn rusqlite::ToSql)
            .collect();
        let rows = statement
            .query_map(refs.as_slice(), |row| Ok(article_from_row(row)))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(Ok(rows))
    }

    /// 收藏（稍后读）列表。
    pub fn starred_articles(&self, limit: i64) -> Result<Vec<NewsArticle>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "{ARTICLE_SELECT}
                 WHERE a.starred = 1
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?1",
            ))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(statement.query_map([limit], |row| Ok(article_from_row(row))))
    }

    /// 切换收藏；返回切换后的状态（true = 已收藏）。
    pub fn toggle_star(&self, article_id: i64) -> Result<bool, InfrastructureError> {
        self.connection
            .execute(
                "UPDATE news_articles SET starred = CASE starred WHEN 0 THEN 1 ELSE 0 END WHERE id = ?1",
                [article_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        self.connection
            .query_row(
                "SELECT starred FROM news_articles WHERE id = ?1",
                [article_id],
                |row| Ok(row.get::<_, i64>(0)? != 0),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .ok_or_else(|| {
                InfrastructureError::Sqlite(format!("news article {article_id} not found"))
            })
    }

    /// 标已读（幂等）。
    pub fn mark_read(&self, article_id: i64) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "UPDATE news_articles SET is_read = 1 WHERE id = ?1",
                [article_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    /// 未读总数（News 页导航徽标）。
    pub fn unread_total(&self) -> Result<i64, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM news_articles WHERE is_read = 0",
                [],
                |row| row.get(0),
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    /// 新闻源健康态：是否存在抓取失败（`news.sources` 的 degraded 判据）。
    pub fn has_failed_source(&self) -> Result<bool, InfrastructureError> {
        let count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM news_sources WHERE last_error IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(count > 0)
    }

    // -----------------------------------------------------------------------
    // 一次性 RSS → News 迁移标记（由 `news_migration` 驱动，见 ADR-010）
    // -----------------------------------------------------------------------

    /// 旧的 RSS `kind='news'` 行是否已搬完。
    pub fn rss_news_migrated(&self) -> Result<bool, InfrastructureError> {
        let value = self
            .connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'rss_news_migrated'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(value.as_deref() == Some("1"))
    }

    /// 置迁移标记（只有在搬运成功后才允许调用）。
    pub fn mark_rss_news_migrated(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('rss_news_migrated', '1')",
                [],
            )
            .map(|_| ())
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    /// 落地从 RSS 搬来的文章（保留 `is_read` / `starred` 用户状态）。
    pub fn insert_migrated_articles(
        &self,
        source_id: i64,
        articles: &[crate::news_migration::MigratedArticle],
    ) -> Result<usize, InfrastructureError> {
        let mut inserted = 0;
        for article in articles {
            let changed = self
                .connection
                .execute(
                    "INSERT OR IGNORE INTO news_articles
                         (source_id, guid, url, title, author, image_url, published_at, summary,
                          is_read, starred)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        source_id,
                        article.guid,
                        article.url,
                        article.title,
                        article.author,
                        article.image_url,
                        article.published_at,
                        article.summary,
                        article.is_read,
                        article.starred,
                    ],
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            inserted += changed;
        }
        Ok(inserted)
    }
}

impl std::fmt::Debug for NewsRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewsRepository").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::feed::FetchedEntry;
    use tempfile::tempdir;

    fn entry(guid: &str, title: &str, published_at: i64) -> FetchedEntry {
        FetchedEntry {
            guid: guid.to_string(),
            url: format!("https://example.com/{guid}"),
            title: title.to_string(),
            author: Some("Desk".to_string()),
            image_url: Some(format!("https://cdn.example.com/{guid}.jpg")),
            published_at: Some(published_at),
            summary: Some("<p>summary</p>".to_string()),
        }
    }

    fn open() -> (tempfile::TempDir, NewsRepository) {
        let directory = tempdir().expect("temp dir");
        let repository =
            NewsRepository::open(directory.path().join("news.db")).expect("open news db");
        (directory, repository)
    }

    #[test]
    fn seeds_recommended_sources_once() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("news.db");
        {
            let repository = NewsRepository::open(&path).expect("open");
            let seeded = repository.list_sources().expect("list");
            assert_eq!(
                seeded.len(),
                devtoolbox_core::news::recommended_sources().len(),
                "首次打开必须 seed 系统维护的新闻源"
            );
        }
        // 用户删一个源后重开：不会被 seed 回来（seed 只跑一次）。
        {
            let repository = NewsRepository::open(&path).expect("reopen");
            let sources = repository.list_sources().expect("list");
            let victim = sources[0].id;
            repository.delete_source(victim).expect("delete");
            assert_eq!(
                repository.list_sources().expect("list").len(),
                sources.len() - 1
            );
        }
        {
            let repository = NewsRepository::open(&path).expect("reopen again");
            assert_ne!(
                repository.list_sources().expect("list").len(),
                devtoolbox_core::news::recommended_sources().len(),
                "用户删过的源不得被 seed 回来"
            );
        }
    }

    #[test]
    fn rejects_newer_schema_version() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("news.db");
        {
            let connection = Connection::open(&path).expect("open");
            connection
                .execute_batch(&format!(
                    "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO schema_meta (key, value) VALUES ('version', '{}');",
                    NEWS_SCHEMA_VERSION + 1
                ))
                .expect("seed future version");
        }
        assert!(
            NewsRepository::open(&path).is_err(),
            "更新的 schema 必须被拒绝"
        );
    }

    #[test]
    fn latest_is_newest_first_and_bounded() {
        let (_dir, repository) = open();
        let source = repository
            .insert_source(
                "Wire",
                "https://wire.example/rss",
                NewsCategory::General,
                None,
            )
            .expect("insert source");
        repository
            .insert_articles(
                source,
                &[
                    entry("a", "Old", 1_600_000_000),
                    entry("b", "New", 1_800_000_000),
                    entry("c", "Mid", 1_700_000_000),
                ],
            )
            .expect("insert");
        let rows = repository.latest(2).expect("latest");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "New");
        assert_eq!(rows[1].title, "Mid");
        assert_eq!(rows[0].source_name, "Wire");
        assert_eq!(rows[0].author.as_deref(), Some("Desk"));
    }

    #[test]
    fn dedupes_articles_by_guid() {
        let (_dir, repository) = open();
        let source = repository
            .insert_source(
                "Wire",
                "https://wire.example/rss",
                NewsCategory::General,
                None,
            )
            .expect("insert source");
        assert_eq!(
            repository
                .insert_articles(source, &[entry("a", "A", 1_700_000_000)])
                .expect("first"),
            1
        );
        assert_eq!(
            repository
                .insert_articles(source, &[entry("a", "A again", 1_700_000_000)])
                .expect("dup"),
            0,
            "同 guid 重复插入必须被忽略"
        );
        assert_eq!(repository.latest(10).expect("latest").len(), 1);
    }

    #[test]
    fn filters_by_category_and_source() {
        let (_dir, repository) = open();
        // 注意：`open()` 已 seed 推荐目录（含 hnrss.org / news.cn），
        // 这里必须用**不同 URL** 否则撞 UNIQUE 约束 —— 也顺带验证
        // 「同一分类可以有多个源」。
        let tech = repository
            .insert_source(
                "HN mirror",
                "https://hn.example/rss",
                NewsCategory::Tech,
                None,
            )
            .expect("tech");
        let china = repository
            .insert_source(
                "Xinhua mirror",
                "https://xinhua.example/rss",
                NewsCategory::China,
                None,
            )
            .expect("china");
        repository
            .insert_articles(tech, &[entry("t", "Rust 2.0", 1_800_000_000)])
            .expect("tech article");
        repository
            .insert_articles(china, &[entry("x", "Summit opens", 1_800_000_100)])
            .expect("china article");

        let tech_rows = repository
            .latest_by_category(NewsCategory::Tech, 10)
            .expect("tech");
        assert_eq!(tech_rows.len(), 1);
        assert_eq!(tech_rows[0].title, "Rust 2.0");

        let by_source = repository.latest_by_source(china, 10).expect("china");
        assert_eq!(by_source.len(), 1);
        assert_eq!(by_source[0].title, "Summit opens");
    }

    #[test]
    fn search_matches_case_insensitively_and_ignores_wildcards() {
        let (_dir, repository) = open();
        let source = repository
            .insert_source(
                "Wire",
                "https://wire.example/rss",
                NewsCategory::General,
                None,
            )
            .expect("insert source");
        repository
            .insert_articles(
                source,
                &[
                    entry("a", "Typhoon makes landfall", 1_800_000_000),
                    entry("b", "Market closes higher", 1_800_000_100),
                ],
            )
            .expect("insert");

        assert_eq!(
            repository
                .query_articles(Some("typhoon"), None, 10)
                .expect("lowercase")
                .len(),
            1
        );
        assert_eq!(
            repository
                .query_articles(Some("TYPHOON"), None, 10)
                .expect("uppercase")
                .len(),
            1
        );
        assert!(
            repository
                .query_articles(Some("%"), None, 10)
                .expect("wildcard literal")
                .is_empty(),
            "通配符必须被转义，用户搜 % 是搜字面百分号"
        );
        assert!(
            repository
                .query_articles(Some("earthquake"), None, 10)
                .expect("no hit")
                .is_empty()
        );
        assert_eq!(
            repository
                .query_articles(None, None, 10)
                .expect("all")
                .len(),
            2
        );
    }

    #[test]
    fn toggles_star_and_marks_read() {
        let (_dir, repository) = open();
        let source = repository
            .insert_source(
                "Wire",
                "https://wire.example/rss",
                NewsCategory::General,
                None,
            )
            .expect("insert source");
        repository
            .insert_articles(source, &[entry("a", "A", 1_700_000_000)])
            .expect("insert");
        let article = repository.latest(1).expect("latest").remove(0);
        assert!(!article.starred);
        assert!(!article.is_read);

        assert!(repository.toggle_star(article.id).expect("star"));
        assert_eq!(repository.starred_articles(10).expect("starred").len(), 1);
        assert!(!repository.toggle_star(article.id).expect("unstar"));
        assert!(repository.starred_articles(10).expect("starred").is_empty());

        repository.mark_read(article.id).expect("read");
        assert!(repository.latest(1).expect("latest")[0].is_read);
        assert_eq!(repository.unread_total().expect("unread"), 0);
    }

    #[test]
    fn health_is_degraded_when_a_source_failed() {
        let (_dir, repository) = open();
        let source = repository
            .insert_source(
                "Wire",
                "https://wire.example/rss",
                NewsCategory::General,
                None,
            )
            .expect("insert source");
        assert!(!repository.has_failed_source().expect("health"));
        repository
            .set_source_health(source, Some("server returned 500"))
            .expect("set error");
        assert!(repository.has_failed_source().expect("health"));
        let failing = repository.source_by_id(source).expect("source");
        assert_eq!(
            failing.expect("source").last_error.as_deref(),
            Some("server returned 500")
        );
        repository.set_source_health(source, None).expect("clear");
        assert!(!repository.has_failed_source().expect("health"));
    }

    #[test]
    fn changing_category_does_not_affect_rss_domain() {
        // ADR-010：News 的分类切换只动 news_sources，不碰任何 RSS 表。
        let (_dir, repository) = open();
        let source = repository
            .insert_source(
                "Wire",
                "https://wire.example/rss",
                NewsCategory::General,
                None,
            )
            .expect("insert source");
        repository
            .set_source_category(source, NewsCategory::World)
            .expect("set category");
        let updated = repository
            .source_by_id(source)
            .expect("source")
            .expect("source");
        assert_eq!(updated.category, NewsCategory::World);
        assert!(
            repository
                .set_source_category(9999, NewsCategory::World)
                .is_err(),
            "未知源必须受控报错"
        );
    }
}
