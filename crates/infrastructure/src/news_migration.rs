//! News / RSS 一次性数据迁移（ADR-010）。
//!
//! ## 为什么需要它
//!
//! 曾经把 News 建模成 `feeds.kind = 'news'`（**错误的 bounded context**：
//! News 是 RSS 的一个分类）。ADR-010 拆成两个 context 后，老库里的
//! `feeds.kind` 必须被**有向地**处理：
//!
//! - `kind='news'` 的行 → 搬进 `news.db`（`news_sources` + `news_articles`）；
//! - `kind='rss'` / 默认行 → **原地不动**，继续是 RSS 订阅（用户数据与行为不变）；
//! - 搬完 `ALTER TABLE feeds DROP COLUMN kind`，把错误字段从库里删掉。
//!
//! ## 关键性质
//!
//! - **不是 URL 猜测**：读的是用户当时明确写下的 `kind` 标记。
//! - **有向、幂等**：`news.db` 的 `schema_meta.rss_news_migrated` 做一次性
//!   标记；重复调用只做「补 DROP COLUMN」。
//! - **先搬后删**：搬失败绝不 DROP（宁可留死字段，不丢数据）。
//! - **必须先于 `FeedRepository::open` 调用**（否则列已在 store 层被感知）；
//!   `rss_store` 自身不再认识 `kind`，它对残留列无感。
//!
//! ## 调用时机
//!
//! - 桌面：`apps/desktop/src/lib.rs` setup，开 `FeedRepository` / `NewsRepository`
//!   **之前**；
//! - MCP：`apps/mcp/src/compose.rs::build_stores`，开两侧库之前。

use std::path::Path;

use rusqlite::Connection;

use crate::error::InfrastructureError;

/// 迁移结果（观测 / 日志用）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NewsMigrationReport {
    /// 搬进 `news_sources` 的新闻源数。
    pub sources: usize,
    /// 搬进 `news_articles` 的新闻条数。
    pub articles: usize,
    /// 是否从 `feeds` 删除了残留的 `kind` 列。
    pub dropped_kind_column: bool,
    /// 本次是否跳过（已迁移过 / 无需迁移）。
    pub skipped: bool,
}

/// 执行一次性迁移：RSS `feeds.kind='news'` → `news.db`，然后 DROP 列。
///
/// 任一前置不满足（无 RSS 库 / 无 `kind` 列）→ `skipped: true`，
/// 不报错 —— 新装环境本来就没有这个字段。
pub fn migrate_news_from_rss(
    rss_db: &Path,
    news_db: &Path,
) -> Result<NewsMigrationReport, InfrastructureError> {
    let mut report = NewsMigrationReport::default();

    if !rss_db.exists() {
        report.skipped = true;
        return Ok(report);
    }

    let rss = Connection::open(rss_db)
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
    if !has_column(&rss, "feeds", "kind")? {
        report.skipped = true;
        return Ok(report);
    }

    // news.db：确保 schema 与 seed 就绪（open 自带 seed，可重复调用）。
    let news = crate::news_store::NewsRepository::open(news_db)?;

    if !news.rss_news_migrated()? {
        report = copy_sources_and_articles(&rss, &news)?;
        // **先搬后写标记**：搬运失败绝不标记，下次重试；标记成功后才允许 DROP。
        news.mark_rss_news_migrated()?;
    } else {
        report.skipped = true;
    }

    // 残留列清理：无论本次是否搬运都尝试（对「标记已置但列还在」的场景幂等）。
    report.dropped_kind_column = drop_column(&rss, "feeds", "kind")?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, InfrastructureError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
    Ok(names.iter().any(|name| name == column))
}

/// 搬运：`kind='news'` 的 feeds → news_sources，其 articles → news_articles。
fn copy_sources_and_articles(
    rss: &Connection,
    news: &crate::news_store::NewsRepository,
) -> Result<NewsMigrationReport, InfrastructureError> {
    let mut report = NewsMigrationReport::default();

    let mut statement = rss
        .prepare("SELECT id, title, url, site_url FROM feeds WHERE kind = 'news' ORDER BY id")
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
    let sources = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;

    let category = devtoolbox_core::news::NewsCategory::General;
    for (rss_id, name, url, site_url) in sources {
        if news.find_source_id_by_url(&url)?.is_some() {
            continue;
        }
        news.insert_source(&name, &url, category, site_url.as_deref())?;
        report.sources += 1;

        // 该源的文章整体搬过来（RSS 语义已消费完，不再回写）。
        let articles = rss
            .prepare(
                "SELECT guid, url, title, author, image_url, published_at, summary,
                        is_read, starred
                 FROM articles WHERE feed_id = ?1 ORDER BY id",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .query_map([rss_id], |row| {
                Ok(MigratedArticle {
                    guid: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    author: row.get(3)?,
                    image_url: row.get(4)?,
                    published_at: row.get(5)?,
                    summary: row.get(6)?,
                    is_read: row.get::<_, i64>(7)? != 0,
                    starred: row.get::<_, i64>(8)? != 0,
                })
            })
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;

        let target_id = news
            .find_source_id_by_url(&url)?
            .ok_or_else(|| InfrastructureError::Sqlite("news source missing".into()))?;
        report.articles += news.insert_migrated_articles(target_id, &articles)?;
    }

    Ok(report)
}

/// 从 RSS articles 读出的一条（迁移用中间形状）。
#[derive(Clone, Debug)]
pub struct MigratedArticle {
    pub guid: String,
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub published_at: Option<i64>,
    pub summary: Option<String>,
    pub is_read: bool,
    pub starred: bool,
}

/// 删除残留列（SQLite ≥ 3.35；失败只返回 false，不丢数据）。
fn drop_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, InfrastructureError> {
    if !has_column(connection, table, column)? {
        return Ok(false);
    }
    match connection.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}")) {
        Ok(()) => Ok(true),
        Err(source) => {
            // 老 SQLite 不支持 DROP COLUMN：保留死字段（无代码读它），下次再试。
            let message = source.to_string();
            if message.contains("near \"DROP\"") {
                Ok(false)
            } else {
                Err(InfrastructureError::Sqlite(message))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// 造一个「ADR-010 之前」的库：feeds 含 kind，两类行并存。
    fn legacy_rss_db(path: &Path) {
        let connection = Connection::open(path).expect("open legacy rss");
        connection
            .execute_batch(
                "CREATE TABLE feeds (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL,
                    url TEXT NOT NULL UNIQUE,
                    site_url TEXT,
                    last_updated INTEGER,
                    last_error TEXT,
                    kind TEXT NOT NULL DEFAULT 'rss'
                );
                CREATE TABLE articles (
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
                INSERT INTO feeds (title, url, kind) VALUES
                    ('V2EX', 'https://v2ex.com/feed.xml', 'rss'),
                    ('时政频道', 'https://news.example.com/rss', 'news'),
                    ('blog', 'https://blog.example.com/feed', 'rss');
                INSERT INTO articles (feed_id, guid, url, title, is_read, starred) VALUES
                    (1, 'g1', 'https://v2ex.com/t/1', '[程序员] 今天写了啥', 1, 1),
                    (2, 'g2', 'https://news.example.com/1', 'Summit opens', 0, 0),
                    (2, 'g3', 'https://news.example.com/2', 'Typhoon landfall', 0, 1),
                    (3, 'g4', 'https://blog.example.com/1', 'Hello world', 0, 0);",
            )
            .expect("seed legacy");
    }

    #[test]
    fn moves_only_marked_news_rows_and_drops_column() {
        let directory = tempdir().expect("temp dir");
        let rss_path = directory.path().join("dashboard.db");
        let news_path = directory.path().join("news.db");
        legacy_rss_db(&rss_path);

        let report = migrate_news_from_rss(&rss_path, &news_path).expect("migrate");
        assert_eq!(report.sources, 1, "只有被标 news 的源被搬");
        assert_eq!(report.articles, 2);
        assert!(report.dropped_kind_column);
        assert!(!report.skipped);

        // RSS 数据保持 RSS 语义：两条 rss 行 + 它们的文章原地不动。
        let rss = Connection::open(&rss_path).expect("reopen rss");
        assert!(!has_column(&rss, "feeds", "kind").expect("column check"));
        let feeds: i64 = rss
            .query_row("SELECT COUNT(*) FROM feeds", [], |row| row.get(0))
            .expect("count feeds");
        assert_eq!(feeds, 3, "RSS 订阅一条都不能少");
        let rss_articles: i64 = rss
            .query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))
            .expect("count articles");
        assert_eq!(rss_articles, 4, "RSS 文章原地保留（不清除）");

        // News 侧拿到新闻源与文章，且保留用户状态。
        let news = crate::news_store::NewsRepository::open(&news_path).expect("open news");
        let sources = news.list_sources().expect("list sources");
        let migrated: Vec<_> = sources
            .iter()
            .filter(|source| source.url == "https://news.example.com/rss")
            .collect();
        assert_eq!(migrated.len(), 1);
        let news_articles = news.latest(10).expect("news articles");
        assert!(news_articles.iter().all(|a| a.source_id == migrated[0].id));
        assert_eq!(news_articles.len(), 2);
        assert_eq!(
            news_articles.iter().filter(|a| a.starred).count(),
            1,
            "用户收藏状态必须保留"
        );
    }

    #[test]
    fn idempotent_and_safe_to_rerun() {
        let directory = tempdir().expect("temp dir");
        let rss_path = directory.path().join("dashboard.db");
        let news_path = directory.path().join("news.db");
        legacy_rss_db(&rss_path);

        migrate_news_from_rss(&rss_path, &news_path).expect("first");
        let second = migrate_news_from_rss(&rss_path, &news_path).expect("second");
        assert!(second.skipped, "列已删 → 跳过，不重复搬");

        let news = crate::news_store::NewsRepository::open(&news_path).expect("open news");
        assert!(
            news.list_sources()
                .expect("list")
                .iter()
                .filter(|source| source.url == "https://news.example.com/rss")
                .count()
                == 1,
            "重复迁移不得产生重复新闻源"
        );
    }

    #[test]
    fn fresh_database_is_skipped() {
        let directory = tempdir().expect("temp dir");
        let rss_path = directory.path().join("dashboard.db");
        let news_path = directory.path().join("news.db");
        // 全新 RSS 库（无 kind 列）：NEWS_SCHEMA 不需要搬任何东西。
        {
            let connection = Connection::open(&rss_path).expect("open fresh");
            connection
                .execute_batch("CREATE TABLE feeds (id INTEGER PRIMARY KEY, title TEXT)")
                .expect("create");
        }
        let report = migrate_news_from_rss(&rss_path, &news_path).expect("migrate");
        assert!(report.skipped);
        assert_eq!(report.sources, 0);
        assert_eq!(report.articles, 0);
        // news.db 不该因此被创建（seed 由 NewsRepository::open 负责）。
        let news = crate::news_store::NewsRepository::open(&news_path).expect("open news");
        assert_eq!(
            news.list_sources().expect("list").len(),
            devtoolbox_core::news::recommended_sources().len()
        );
    }

    #[test]
    fn missing_rss_database_is_skipped() {
        let directory = tempdir().expect("temp dir");
        let report = migrate_news_from_rss(
            &directory.path().join("nonexistent.db"),
            &directory.path().join("news.db"),
        )
        .expect("migrate");
        assert!(report.skipped);
    }
}
