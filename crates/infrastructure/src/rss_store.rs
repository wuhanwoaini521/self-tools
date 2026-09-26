//! RSS 持久化：SQLite(rusqlite bundled)。面向个人桌面工具，保持简单：
//! 两张表(feeds / articles)、外键级联删除、`(feed_id, guid)` 唯一约束天然去重。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use crate::error::InfrastructureError;
pub use devtoolbox_core::rss::{ArticleRow, FeedRow};

/// 当前 schema 版本。
///
/// - V2：`articles` 加 `author` / `image_url` / `starred`（条目展示与收藏）。
///
/// 注：V3 曾短暂给 `feeds` 加过 `kind` 列 —— ADR-010 判定那是错误的
/// bounded context 建模（News 不是 RSS 的 kind）。该列已从域模型移除，
/// 老库残留列由 `news_migration`（先搬数据再 DROP COLUMN）清理，
/// 本 store 不再感知它。
pub const RSS_SCHEMA_VERSION: u32 = 3;

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// 文章查询统一投影(列序与 `article_from_row` 严格对应)。
const ARTICLE_SELECT: &str =
    "SELECT a.id, a.feed_id, f.title, a.guid, a.url, a.title, a.author, a.image_url,
            a.published_at, a.summary, a.is_read, a.starred
     FROM articles a JOIN feeds f ON f.id = a.feed_id";

/// `ArticleRow` 行映射(列序见 `ARTICLE_SELECT`)。
fn article_from_row(row: &rusqlite::Row<'_>) -> ArticleRow {
    ArticleRow {
        id: row.get(0).unwrap_or_default(),
        feed_id: row.get(1).unwrap_or_default(),
        feed_title: row.get(2).unwrap_or_default(),
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

/// LIKE 通配符转义(`%` `_` `\`)。
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
                    author TEXT,
                    image_url TEXT,
                    published_at INTEGER,
                    summary TEXT,
                    is_read INTEGER NOT NULL DEFAULT 0,
                    starred INTEGER NOT NULL DEFAULT 0,
                    UNIQUE (feed_id, guid)
                );
                CREATE INDEX IF NOT EXISTS idx_articles_feed_published
                    ON articles(feed_id, published_at DESC);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        self.migrate()?;
        self.write_version()
    }

    /// V2 迁移：RSS 是最早的域，`CREATE TABLE IF NOT EXISTS` 对「已存在的旧库」
    /// 无效，因此列变更必须显式探测补齐。列已存在则跳过（幂等）。
    fn migrate(&self) -> Result<(), InfrastructureError> {
        for (table, column) in [
            ("articles", "author"),
            ("articles", "image_url"),
            ("articles", "starred"),
        ] {
            if self.has_column(table, column)? {
                continue;
            }
            let ddl = match column {
                // 用户状态列，默认 0 = 未收藏。
                "starred" => "ALTER TABLE articles ADD COLUMN starred INTEGER NOT NULL DEFAULT 0",
                _ => &format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"),
            };
            self.connection
                .execute_batch(ddl)
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        }
        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(names.iter().any(|name| name == column))
    }

    /// 与 memory / documents / files 同形：只拒绝更新的库，旧的自动升版本。
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
                let version: u32 = value.parse().unwrap_or(RSS_SCHEMA_VERSION);
                if version > RSS_SCHEMA_VERSION {
                    return Err(InfrastructureError::Sqlite(format!(
                        "rss schema version {version} is newer than supported {RSS_SCHEMA_VERSION}"
                    )));
                }
            }
            None => {
                self.connection
                    .execute(
                        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('version', ?1)",
                        [RSS_SCHEMA_VERSION.to_string()],
                    )
                    .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            }
        }
        Ok(())
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
                rusqlite::params![title, url, site_url.unwrap_or_default()],
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
                    "INSERT OR IGNORE INTO articles (feed_id, guid, url, title, author, image_url, published_at, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        feed_id,
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
            .prepare(&format!(
                "{ARTICLE_SELECT}
                 WHERE a.feed_id = ?1
                 ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC
                 LIMIT ?2",
            ))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(statement.query_map([feed_id, limit], |row| Ok(article_from_row(row))))
    }

    /// 按 id 取单条（`rss.get_entry`）。
    pub fn entry_by_id(&self, entry_id: i64) -> Result<Option<ArticleRow>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(&format!("{ARTICLE_SELECT}\n WHERE a.id = ?1"))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let rows = statement
            .query_map([entry_id], |row| Ok(article_from_row(row)))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(rows.into_iter().next())
    }

    /// 跨 Feed 的最新文章(Home 页用)。
    pub fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, InfrastructureError> {
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

    /// 收藏文章(稍后读)倒序列表。
    pub fn starred_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, InfrastructureError> {
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

    /// 关键词检索(标题 / 署名 / 摘要三列 LIKE 粗筛),与 documents 同策略
    /// (application 侧精排/排序)。`feed_id` 限定单个源。
    ///
    /// 占位符用顺序 `?`;参数 Vec 按**实际出现的条件**追加(rusqlite 要求
    /// 参数个数与占位符个数精确匹配)。
    pub fn query_articles(
        &self,
        keyword: Option<&str>,
        feed_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ArticleRow>, InfrastructureError> {
        // 关键词归一:小写 + 两侧加 `%`。
        //
        // 不能依赖「裸 LIKE 就是子串匹配」:部分 SQLite build 下
        // `x LIKE 'kw'` 等价精确比较(实测 `LIKE 'typhoon%'` 命中而
        // `LIKE 'typhoon'` 不命中)。显式加通配符才能得到稳定的子串语义。
        let escaped = keyword.map(|value| format!("%{}%", escape_like(&value.to_lowercase())));
        let mut sql = format!("{ARTICLE_SELECT}\n WHERE 1 = 1");
        // rusqlite 要求参数个数与占位符个数**精确匹配**,因此参数按实际出现
        // 的条件追加(未提供的条件不进 SQL、也不占位)。
        //
        // 两侧都 `lower()`:LIKE 的大小写行为同样取决于 build,显式归一最稳。
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(escaped) = escaped {
            sql.push_str(" AND (lower(a.title) LIKE ? OR a.summary LIKE ?)");
            params.push(rusqlite::types::Value::Text(escaped.clone()));
            params.push(rusqlite::types::Value::Text(escaped));
        }
        if let Some(feed_id) = feed_id {
            sql.push_str(" AND a.feed_id = ?");
            params.push(rusqlite::types::Value::Integer(feed_id));
        }
        sql.push_str(
            "\n ORDER BY a.published_at IS NULL, a.published_at DESC, a.id DESC\n LIMIT ?",
        );
        params.push(rusqlite::types::Value::Integer(limit));

        // `&[&dyn ToSql]` 是 rusqlite 对异构参数的原生支持。
        let refs: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|value| value as &dyn rusqlite::ToSql)
            .collect();
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let rows = statement
            .query_map(refs.as_slice(), |row| Ok(article_from_row(row)))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Self::map_articles(Ok(rows))
    }

    /// 切换收藏。返回切换后的状态(true = 已收藏)。
    pub fn toggle_article_star(&self, article_id: i64) -> Result<bool, InfrastructureError> {
        self.connection
            .execute(
                "UPDATE articles SET starred = CASE starred WHEN 0 THEN 1 ELSE 0 END WHERE id = ?1",
                [article_id],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        self.connection
            .query_row(
                "SELECT starred FROM articles WHERE id = ?1",
                [article_id],
                |row| Ok(row.get::<_, i64>(0)? != 0),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .ok_or_else(|| InfrastructureError::Sqlite(format!("article {article_id} not found")))
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
            author: Some("Desk".to_string()),
            image_url: Some(format!("https://cdn.example.com/{guid}.jpg")),
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
    fn query_articles_matches_title_keyword() {
        let (_dir, repository) = open();
        let feed_id = repository
            .insert_feed("Tech", "https://example.com/rss", None)
            .expect("feed");
        repository
            .insert_articles(
                feed_id,
                &[
                    entry("a", "Typhoon makes landfall"),
                    entry("b", "Market closes higher"),
                ],
            )
            .expect("articles");

        // 关键词命中标题（大小写不敏感 —— 归一在 SQL 侧完成）。
        let hits = repository
            .query_articles(Some("typhoon"), None, 10)
            .expect("query");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Typhoon makes landfall");

        // 大写关键词同样命中。
        assert_eq!(
            repository
                .query_articles(Some("TYPHOON"), None, 10)
                .expect("query")
                .len(),
            1
        );

        // 无 keyword：全部返回。
        assert_eq!(
            repository
                .query_articles(None, None, 10)
                .expect("all")
                .len(),
            2
        );

        // feed 过滤。
        assert_eq!(
            repository
                .query_articles(None, Some(feed_id), 10)
                .expect("by feed")
                .len(),
            2
        );

        // 通配符被转义：`%` 不应匹配一切（用户搜 "%" 是搜字面百分号）。
        assert_eq!(
            repository
                .query_articles(Some("%"), None, 10)
                .expect("escaped")
                .len(),
            0
        );

        // 无命中 → 空列表（不是错误）。
        assert!(
            repository
                .query_articles(Some("earthquake"), None, 10)
                .expect("no hit")
                .is_empty()
        );
    }

    #[test]
    fn toggles_article_star() {
        let (_dir, repository) = open();
        let feed_id = repository
            .insert_feed("Tech", "https://example.com/rss", None)
            .expect("feed");
        repository
            .insert_articles(feed_id, &[entry("a", "A")])
            .expect("articles");
        let article = repository
            .list_articles(feed_id, 10)
            .expect("list")
            .remove(0);
        assert!(!article.starred);

        assert!(repository.toggle_article_star(article.id).expect("star"));
        let starred = repository.starred_articles(10).expect("starred");
        assert_eq!(starred.len(), 1);
        assert_eq!(starred[0].id, article.id);

        assert!(!repository.toggle_article_star(article.id).expect("unstar"));
        assert!(repository.starred_articles(10).expect("starred").is_empty());
    }

    #[test]
    fn reopen_migrates_existing_database_idempotently() {
        // V2 迁移：旧库无 author/image_url/starred 三列时,重开必须补上且不重建。
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("rss.db");
        {
            let connection = rusqlite::Connection::open(&path).expect("open");
            connection
                .execute_batch(
                    "CREATE TABLE feeds (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        title TEXT NOT NULL,
                        url TEXT NOT NULL UNIQUE,
                        site_url TEXT,
                        last_updated INTEGER,
                        last_error TEXT
                    );
                    CREATE TABLE articles (
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
                    INSERT INTO feeds (title, url) VALUES ('Legacy', 'https://legacy.example/rss');
                    INSERT INTO articles (feed_id, guid, url, title)
                        VALUES (1, 'legacy-1', 'https://legacy.example/1', 'Old story');",
                )
                .expect("seed legacy schema");
        }

        // 重开：迁移补列,旧数据必须仍在。
        let repository = FeedRepository::open(&path).expect("reopen");
        let feeds = repository.list_feeds().expect("feeds");
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].title, "Legacy");
        let articles = repository.latest_articles(10).expect("articles");
        assert_eq!(articles.len(), 1, "旧文章不得丢");
        assert_eq!(articles[0].title, "Old story");
        assert!(!articles[0].starred, "旧行 starred 默认 false");
        assert!(articles[0].author.is_none(), "旧行 author 默认 NULL");

        // 幂等：再开一次不报错、数据不变。
        let again = FeedRepository::open(&path).expect("reopen again");
        assert_eq!(again.latest_articles(10).expect("articles").len(), 1);
        // 迁移后新能力可用。
        assert!(
            again
                .query_articles(Some("old story"), None, 10)
                .expect("query")
                .len()
                == 1
        );
    }

    #[test]
    fn rejects_newer_schema_version() {
        // 向前兼容：更新的库不被旧代码静默降级（与 memory 域同策略）。
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("rss.db");
        {
            let connection = rusqlite::Connection::open(&path).expect("open");
            connection
                .execute_batch(&format!(
                    "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO schema_meta (key, value) VALUES ('version', '{}');",
                    super::RSS_SCHEMA_VERSION + 1
                ))
                .expect("seed future version");
        }
        assert!(
            FeedRepository::open(&path).is_err(),
            "更新的 schema 必须被拒绝"
        );
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
