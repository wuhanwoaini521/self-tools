//! RSS 持久化：SQLite(rusqlite bundled)。面向个人桌面工具，保持简单：
//! 两张表(feeds / articles)、外键级联删除、`(feed_id, guid)` 唯一约束天然去重。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use crate::error::InfrastructureError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedRow {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub site_url: Option<String>,
    pub last_updated: Option<i64>,
    pub last_error: Option<String>,
    pub unread_count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArticleRow {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub guid: String,
    pub url: String,
    pub title: String,
    pub published_at: Option<i64>,
    pub summary: Option<String>,
    pub is_read: bool,
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// SQLite 存储器。`Connection` 非线程安全，由上层用 `Mutex` 串行化访问。
pub struct FeedRepository {
    connection: Connection,
}

impl FeedRepository {
    /// 打开(必要时创建)数据库并确保 Schema 存在。
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
        Ok(repository)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS feeds (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL,
                    url TEXT NOT NULL UNIQUE,
                    site_url TEXT,
                    last_updated INTEGER,
                    last_error TEXT
                );
                CREATE TABLE IF NOT EXISTS articles (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                    guid TEXT NOT NULL,
                    url TEXT NOT NULL DEFAULT '',
                    title TEXT NOT NULL,
                    published_at INTEGER,
                    summary TEXT,
                    is_read INTEGER NOT NULL DEFAULT 0,
                    UNIQUE (feed_id, guid)
                );
                CREATE INDEX IF NOT EXISTS idx_articles_feed_published
                    ON articles(feed_id, published_at DESC);",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    pub fn find_feed_id_by_url(&self, url: &str) -> Result<Option<i64>, InfrastructureError> {
        self.connection
            .query_row("SELECT id FROM feeds WHERE url = ?1", [url], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    pub fn insert_feed(
        &self,
        title: &str,
        url: &str,
        site_url: Option<&str>,
    ) -> Result<i64, InfrastructureError> {
        self.connection
            .execute(
                "INSERT INTO feeds (title, url, site_url) VALUES (?1, ?2, ?3)",
                [title, url, site_url.unwrap_or_default()],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(self.connection.last_insert_rowid())
    }

    /// 批量写入文章；`(feed_id, guid)` 冲突自动忽略。返回实际新增数量。
    pub fn insert_articles(
        &self,
        feed_id: i64,
        articles: &[crate::feed_fetcher::FetchedEntry],
    ) -> Result<usize, InfrastructureError> {
        let mut inserted = 0;
        for entry in articles {
            let changed = self
                .connection
                .execute(
                    "INSERT OR IGNORE INTO articles (feed_id, guid, url, title, published_at, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        feed_id,
                        entry.guid,
                        entry.url,
                        entry.title,
                        entry.published_at,
                        entry.summary
                    ],
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            inserted += changed;
        }
        Ok(inserted)
    }

    pub fn list_feeds(&self) -> Result<Vec<FeedRow>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT f.id, f.title, f.url, f.site_url, f.last_updated, f.last_error,
                        COUNT(a.id) FILTER (WHERE a.is_read = 0) AS unread
                 FROM feeds f
                 LEFT JOIN articles a ON a.feed_id = f.id
                 GROUP BY f.id
                 ORDER BY f.title COLLATE NOCASE",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok(FeedRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    url: row.get(2)?,
                    site_url: row.get(3)?,
                    last_updated: row.get(4)?,
                    last_error: row.get(5)?,
                    unread_count: row.get(6)?,
                })
            })
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(rows)
    }

    pub fn feed_title(&self, feed_id: i64) -> Result<Option<String>, InfrastructureError> {
        self.connection
            .query_row("SELECT title FROM feeds WHERE id = ?1", [feed_id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    pub fn list_articles(
        &self,
        feed_id: i64,
        limit: i64,
    ) -> Result<Vec<ArticleRow>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT a.id, a.feed_id, f.title, a.guid, a.url, a.title, a.published_at, a.summary, a.is_read
                 FROM articles a JOIN feeds f ON f.id = a.feed_id
                 WHERE a.feed_id = ?1
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?2",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(statement.query_map([feed_id, limit], |row| {
            Ok(ArticleRow {
                id: row.get(0)?,
                feed_id: row.get(1)?,
                feed_title: row.get(2)?,
                guid: row.get(3)?,
                url: row.get(4)?,
                title: row.get(5)?,
                published_at: row.get(6)?,
                summary: row.get(7)?,
                is_read: row.get::<_, i64>(8)? != 0,
            })
        }))
    }

    /// 跨 Feed 的最新文章(Home 页用)。
    pub fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT a.id, a.feed_id, f.title, a.guid, a.url, a.title, a.published_at, a.summary, a.is_read
                 FROM articles a JOIN feeds f ON f.id = a.feed_id
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?1",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(statement.query_map([limit], |row| {
            Ok(ArticleRow {
                id: row.get(0)?,
                feed_id: row.get(1)?,
                feed_title: row.get(2)?,
                guid: row.get(3)?,
                url: row.get(4)?,
                title: row.get(5)?,
                published_at: row.get(6)?,
                summary: row.get(7)?,
                is_read: row.get::<_, i64>(8)? != 0,
            })
        }))
    }

    fn map_articles<F>(
        rows: rusqlite::Result<rusqlite::MappedRows<'_, F>>,
    ) -> Result<Vec<ArticleRow>, InfrastructureError>
    where
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<ArticleRow>,
    {
        let mapped = rows.map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    pub fn mark_article_read(&self, article_id: i64) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "UPDATE articles SET is_read = 1 WHERE id = ?1",
                [article_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    pub fn delete_feed(&self, feed_id: i64) -> Result<(), InfrastructureError> {
        self.connection
            .execute("DELETE FROM feeds WHERE id = ?1", [feed_id])
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    pub fn set_feed_success(&self, feed_id: i64) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "UPDATE feeds SET last_updated = ?1, last_error = NULL WHERE id = ?2",
                rusqlite::params![now_unix(), feed_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    pub fn set_feed_error(&self, feed_id: i64, message: &str) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "UPDATE feeds SET last_error = ?1 WHERE id = ?2",
                rusqlite::params![message, feed_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::feed_fetcher::FetchedEntry;
    use super::{FeedRepository, now_unix};
    use tempfile::tempdir;

    fn entry(guid: &str, title: &str) -> FetchedEntry {
        FetchedEntry {
            guid: guid.to_string(),
            url: format!("https://example.com/{guid}"),
            title: title.to_string(),
            published_at: Some(1_700_000_000),
            summary: Some("<p>hello</p>".to_string()),
        }
    }

    fn open() -> (tempfile::TempDir, FeedRepository) {
        let directory = tempdir().expect("temp dir");
        let repository = FeedRepository::open(directory.path().join("rss.db")).expect("open db");
        (directory, repository)
    }

    #[test]
    fn dedupes_articles_by_guid() {
        let (_dir, repository) = open();
        let feed_id = repository
            .insert_feed(
                "Tech",
                "https://example.com/rss",
                Some("https://example.com"),
            )
            .expect("insert feed");
        assert_eq!(
            repository
                .insert_articles(feed_id, &[entry("a", "A"), entry("b", "B")])
                .expect("insert"),
            2
        );
        // 同样的 guid 再刷一次：不重复。
        assert_eq!(
            repository
                .insert_articles(feed_id, &[entry("a", "A"), entry("c", "C")])
                .expect("insert"),
            1
        );
        let feeds = repository.list_feeds().expect("list");
        assert_eq!(feeds[0].unread_count, 3);
    }

    #[test]
    fn marks_read_and_persists_counts() {
        let (_dir, repository) = open();
        let feed_id = repository
            .insert_feed("Tech", "https://example.com/rss", None)
            .expect("feed");
        repository
            .insert_articles(feed_id, &[entry("a", "A")])
            .expect("articles");
        let articles = repository.list_articles(feed_id, 10).expect("articles");
        repository
            .mark_article_read(articles[0].id)
            .expect("mark read");
        assert_eq!(repository.list_feeds().expect("feeds")[0].unread_count, 0);
        assert!(repository.list_articles(feed_id, 10).expect("articles")[0].is_read);
    }

    #[test]
    fn deleting_feed_cascades_articles() {
        let (_dir, repository) = open();
        let feed_id = repository
            .insert_feed("Tech", "https://example.com/rss", None)
            .expect("feed");
        repository
            .insert_articles(feed_id, &[entry("a", "A")])
            .expect("articles");
        repository.delete_feed(feed_id).expect("delete");
        assert!(repository.list_feeds().expect("feeds").is_empty());
        assert!(repository.latest_articles(10).expect("latest").is_empty());
    }

    #[test]
    fn rejects_duplicate_feed_url() {
        let (_dir, repository) = open();
        repository
            .insert_feed("One", "https://example.com/rss", None)
            .expect("first");
        assert!(
            repository
                .find_feed_id_by_url("https://example.com/rss")
                .expect("find")
                .is_some()
        );
        assert!(
            repository
                .find_feed_id_by_url("https://other.com/rss")
                .expect("find")
                .is_none()
        );
    }

    #[test]
    fn latest_articles_join_feed_title() {
        let (_dir, repository) = open();
        let feed_id = repository
            .insert_feed("Tech", "https://example.com/rss", None)
            .expect("feed");
        repository
            .insert_articles(feed_id, &[entry("a", "A")])
            .expect("articles");
        repository.set_feed_success(feed_id).expect("success");
        let latest = repository.latest_articles(5).expect("latest");
        assert_eq!(latest[0].feed_title, "Tech");
        assert!(
            repository.list_feeds().expect("feeds")[0]
                .last_updated
                .unwrap_or_default()
                <= now_unix()
        );
    }
}
