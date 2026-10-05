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
use devtoolbox_core::news::{
    NewsArticle, NewsCategory, NewsSource, NewsSourceType, SOURCE_STALE_AFTER_DAYS, SourceHealth,
};

/// 当前 schema 版本（新增列时递增并补非破坏性迁移）。
pub const NEWS_SCHEMA_VERSION: u32 = 1;

/// 当前 Unix 秒。
/// 上一版目录里的死源 → 新目录里已实测可用的替代源。
///
/// 2026-10-05 实测：`curl -I` 逐个验证。新目录（`core::news::recommended_sources`）
/// 里每条都有 `verified_on`，**不要往这里塞没验证过的地址**。
/// 跨源同标题去重条件（拼进 SQL 用）。
///
/// 同一条通稿常被同一机构的多个频道重复推送：实测「出行热 体验丰 门市旺——国庆假期
/// 消费市场观察」在**中国新闻网**与**中国新闻网·财经**各出现一次，列表里连着两条
/// 同样的标题，看起来像「抓重了」——其实它确实是两条记录。
///
/// 做法：**按标题保留最早入库的那条**（`MIN(id)`，稳定且可重放）。按源查看
/// （`latest_by_source`）不做去重：用户筛某个源时，看到该源自己的记录是对的。
const TITLE_DEDUP: &str =
    "a.id = (SELECT MIN(dup.id) FROM news_articles dup WHERE dup.title = a.title)";

/// 死因见各行注释（2026-10-05 实测）。
const OUTDATED_SEED_SOURCES: &[(&str, &str, &str, NewsCategory)] = &[
    // 新华网 RSS 已 404
    (
        "http://www.news.cn/rss/politics.xml",
        "https://www.chinanews.com.cn/rss/china.xml",
        "中国新闻网·中国",
        NewsCategory::China,
    ),
    // 财联社 telegraphList 接口已 404
    (
        "https://www.cls.cn/nodeapi/telegraphList",
        "http://rss.eastmoney.com/rss_partener.xml",
        "东方财富",
        NewsCategory::Finance,
    ),
    // 第一财经 RSS 已 404
    (
        "https://www.yicai.com/rss/news.xml",
        "https://www.chinanews.com.cn/rss/finance.xml",
        "中国新闻网·财经",
        NewsCategory::Finance,
    ),
    // 路透中文站已下线（301 → reuters.com，401）
    (
        "https://cn.reuters.com/tools/rss",
        "https://news.un.org/feed/subscribe/zh/news/all/rss.xml",
        "联合国新闻（中文）",
        NewsCategory::World,
    ),
    // 36 氪 /feed 已返回 HTML 页面
    (
        "https://36kr.com/feed",
        "https://www.ithome.com/rss/",
        "IT之家",
        NewsCategory::Tech,
    ),
    // 澎湃 rss.xml 已 302 到首页
    (
        "https://www.thepaper.cn/rss.xml",
        "http://www.geekpark.net/rss",
        "极客公园",
        NewsCategory::Tech,
    ),
];

/// **不报错但内容停更**的种子源（200 却半年没更新 —— 最容易被忽略的一种坏）。
const SILENTLY_STALE_SEED_SOURCES: &[(&str, &str)] = &[(
    "http://www.people.com.cn/rss/politics.xml",
    "人民网 politics RSS 仍返回 200，但最新条目停在 2025-06（已停更）",
)];

/// 行 → 健康度。
///
/// 判定顺序（有优先级，不能反过来）：
/// 1. `disabled`：系统明确停用；
/// 2. `failing`：有 `last_error`（抓取真的失败了）；
/// 3. `stale`：抓取没报错，但**最新文章超过阈值** —— 「假活」源；
/// 4. `ok`。
///
/// `stale` 只在源确实有历史文章时判定：一个刚添加、还没抓到东西的源
/// 报「停更」是误导（应该是「还没抓到」）。
fn source_health(row: &rusqlite::Row<'_>) -> SourceHealth {
    let stored: String = row.get(9).unwrap_or_default();
    let disabled_reason: Option<String> = row.get(10).ok().flatten();
    if disabled_reason.is_some() {
        return SourceHealth::Disabled;
    }
    // 显式标记优先（`disabled` / `stale` 可以被系统写入，不靠推导）。
    if let Some(explicit) =
        SourceHealth::from_id(&stored).filter(|value| *value != SourceHealth::Ok)
    {
        return explicit;
    }
    let last_error: Option<String> = row.get(7).ok().flatten();
    if last_error.is_some() {
        return SourceHealth::Failing;
    }
    let latest: Option<i64> = row.get(11).ok().flatten();
    match latest {
        Some(latest) if news_now() - latest > SOURCE_STALE_AFTER_DAYS * 86_400 => {
            SourceHealth::Stale
        }
        _ => SourceHealth::Ok,
    }
}

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
        let connection = crate::sqlite::open_sqlite(path)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let repository = Self { connection };
        repository.ensure_schema()?;
        repository.migrate_health_columns()?;
        repository.seed_sources()?;
        repository.repair_outdated_seeded_sources()?;
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
                    last_error TEXT,
                    health TEXT NOT NULL DEFAULT 'ok',
                    disabled_reason TEXT
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

    /// 老库补 `health` / `disabled_reason` 两列（幂等；新库由 schema 直接建好）。
    ///
    /// 用 `ALTER TABLE` 的重复执行会报错，所以先查 `PRAGMA table_info`：
    /// 「列在不在」是唯一可靠的幂等判据。
    fn migrate_health_columns(&self) -> Result<(), InfrastructureError> {
        let columns: Vec<String> = self
            .connection
            .prepare("PRAGMA table_info(news_sources)")
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .query_map([], |row| row.get(1))
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        for (name, ddl) in [
            (
                "health",
                "ALTER TABLE news_sources ADD COLUMN health TEXT NOT NULL DEFAULT 'ok'",
            ),
            (
                "disabled_reason",
                "ALTER TABLE news_sources ADD COLUMN disabled_reason TEXT",
            ),
        ] {
            if columns.iter().any(|column| column == name) {
                continue;
            }
            self.connection
                .execute(ddl, [])
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        }
        Ok(())
    }

    /// 修复「上一版目录种下、但其实已经死了」的种子源。
    ///
    /// ## 为什么要做这一步
    ///
    /// seed 只跑一次（`schema_meta.sources_seeded`），所以**老用户的库里会一直躺着
    /// 6 个死源**：新华网 404、财联社 404、第一财经 404、路透中文 401、
    /// 36 氪返回 HTML、澎湃新闻 302。用户只能看到「今日新闻少得可怜」，
    /// 却又不知道为什么。
    ///
    /// 这里按 URL 精确匹配做替换（不按名字 —— 名字可能撞车），并：
    /// - 替换成**新目录里已实测可用**的同分类源；
    /// - 写 `disabled_reason`，让 UI 明确说「已下线」而不是让它继续报错；
    /// - 幂等：URL 已改过就不会再动。
    fn repair_outdated_seeded_sources(&self) -> Result<(), InfrastructureError> {
        // 先**只读**检查有没有要修的行，没有就完全不碰写锁。
        // 为什么重要：应用启动时若有别的进程（桌面端 + 网页端同时开着）正在写
        // news.db，一次多余的 UPDATE 就可能撞上 SQLITE_BUSY，直接启动失败。
        let pending = |url: &str| -> Result<bool, InfrastructureError> {
            let count: i64 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM news_sources WHERE url = ?1",
                    [url],
                    |row| row.get(0),
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            Ok(count > 0)
        };

        for (old_url, new_url, new_name, new_category) in OUTDATED_SEED_SOURCES {
            if !pending(old_url)? {
                continue;
            }
            let affected = self
                .connection
                .execute(
                    "UPDATE news_sources
                     SET url = ?2, name = ?3, category = ?4, health = 'ok', last_error = NULL
                     WHERE url = ?1",
                    rusqlite::params![old_url, new_url, new_name, new_category.id()],
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
            if affected > 0 {
                eprintln!("[news] 已替换失效源 {old_url} → {new_name}（{new_url}）");
            }
        }
        // 停更但「不报错」的源：内容停在 2025-06，URL 仍然 200。
        for (url, reason) in SILENTLY_STALE_SEED_SOURCES {
            if !pending(url)? {
                continue;
            }
            self.connection
                .execute(
                    "UPDATE news_sources
                     SET health = 'disabled', disabled_reason = ?2
                     WHERE url = ?1 AND (health IS NULL OR health = 'ok')",
                    rusqlite::params![url, reason],
                )
                .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        }
        Ok(())
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
                        COUNT(a.id) FILTER (WHERE a.is_read = 0) AS unread,
                        s.health, s.disabled_reason,
                        MAX(a.published_at) AS latest_article_at
                 FROM news_sources s
                 LEFT JOIN news_articles a ON a.source_id = s.id
                 GROUP BY s.id
                 ORDER BY s.category, s.name COLLATE NOCASE",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                // site_url 可空：任何「没有站点首页」的行都不该让整张列表查询失败
                // （此前用 `String` 读取，遇到 NULL 直接报 Sqlite 错误）。
                let site_url: Option<String> = row.get(5)?;
                Ok(NewsSource {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    url: row.get(2)?,
                    source_type: NewsSourceType::from_id(&row.get::<_, String>(3)?)
                        .unwrap_or_default(),
                    category: NewsCategory::from_id(&row.get::<_, String>(4)?).unwrap_or_default(),
                    site_url: site_url.filter(|value| !value.is_empty()),
                    last_updated: row.get(6)?,
                    last_error: row.get(7)?,
                    unread_count: row.get(8)?,
                    health: source_health(row),
                    latest_article_at: row.get(11)?,
                    disabled_reason: row.get(10)?,
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
                 WHERE {TITLE_DEDUP}
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
                 WHERE s.category = ?1 AND {TITLE_DEDUP}
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

#[cfg(test)]
mod health_tests {
    //! 源健康度与「失效源自动替换」的回归测试。
    //!
    //! 背景：2026-10-05 实测发现 11 个种子源里 6 个已死、1 个「不报错但停更」。
    //! 这些都属于**静默失败**——用户只看到「新闻怎么不更新」，不知道为什么。

    use super::*;
    use devtoolbox_core::feed::FetchedEntry;
    use tempfile::tempdir;

    fn entry(guid: &str, title: &str, published_at: i64) -> FetchedEntry {
        FetchedEntry {
            guid: guid.to_string(),
            url: format!("https://example.com/{guid}"),
            title: title.to_string(),
            author: None,
            image_url: None,
            published_at: Some(published_at),
            summary: Some("摘要".to_string()),
        }
    }

    fn store_with_old_seed() -> (tempfile::TempDir, NewsRepository) {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("news.db");
        // 造一个「上一版目录」的库：只含已下线的死源。
        let store = NewsRepository::open(&path).expect("open");
        store
            .connection
            .execute("DELETE FROM news_sources", [])
            .expect("clear");
        for (old_url, _new, _name, _category) in OUTDATED_SEED_SOURCES {
            store
                .connection
                .execute(
                    "INSERT INTO news_sources (name, url, source_type, category) VALUES (?1, ?2, 'rss', 'general')",
                    rusqlite::params!["旧源", old_url],
                )
                .expect("insert legacy");
        }
        store
            .connection
            .execute(
                "INSERT INTO news_sources (name, url, source_type, category) VALUES ('人民网', ?1, 'rss', 'china')",
                ["http://www.people.com.cn/rss/politics.xml"],
            )
            .expect("insert people");
        (dir, store)
    }

    #[test]
    fn dead_seeded_sources_are_replaced_on_open() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("news.db");
        {
            let _ = NewsRepository::open(&path).expect("first open");
        }
        // 第二次打开 = 老用户升级路径。
        let store = NewsRepository::open(&path).expect("reopen");
        let urls: Vec<String> = store
            .list_sources()
            .expect("list")
            .into_iter()
            .map(|source| source.url)
            .collect();
        for (old_url, new_url, _name, _category) in OUTDATED_SEED_SOURCES {
            assert!(
                !urls.iter().any(|url| url == old_url),
                "死源应被替换：{old_url}"
            );
            assert!(
                urls.iter().any(|url| url == new_url),
                "替代源应已就位：{new_url}"
            );
        }
    }

    #[test]
    fn silently_stale_source_is_disabled_with_a_reason() {
        let (_dir, store) = store_with_old_seed();
        store
            .connection
            .execute(
                "UPDATE news_sources SET health='ok', disabled_reason=NULL
                 WHERE url = 'http://www.people.com.cn/rss/politics.xml'",
                [],
            )
            .expect("reset");
        // 重新执行修复（模拟再次打开）
        store.repair_outdated_seeded_sources().expect("repair");
        let people = store
            .list_sources()
            .expect("list")
            .into_iter()
            .find(|source| source.url == "http://www.people.com.cn/rss/politics.xml")
            .expect("人民网仍在库里（不删用户数据）");
        assert_eq!(people.health, SourceHealth::Disabled);
        assert!(
            people
                .disabled_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("停更"))
        );
    }

    #[test]
    fn fresh_source_is_ok_and_old_articles_make_it_stale() {
        let dir = tempdir().expect("tempdir");
        let store = NewsRepository::open(dir.path().join("news.db")).expect("open");
        let source = store.list_sources().expect("list").remove(0);
        // 没有任何文章 → 不判定停更（刚加的源还没抓到东西，不该说它坏了）。
        assert_eq!(source.health, SourceHealth::Ok);

        let now = news_now();
        store
            .insert_articles(source.id, &[entry("fresh", "今天", now - 3600)])
            .expect("fresh");
        assert_eq!(
            store
                .list_sources()
                .expect("list")
                .into_iter()
                .find(|item| item.id == source.id)
                .expect("row")
                .health,
            SourceHealth::Ok
        );

        // 清掉刚才那条新文章，只留半年前的 → 停更（抓取没报错，但内容不动了）。
        store
            .connection
            .execute("DELETE FROM news_articles WHERE guid = 'fresh'", [])
            .expect("clear fresh");
        store
            .insert_articles(
                source.id,
                &[entry(
                    "old",
                    "半年前",
                    now - (SOURCE_STALE_AFTER_DAYS + 3) * 86_400,
                )],
            )
            .expect("old");
        let row = store
            .list_sources()
            .expect("list")
            .into_iter()
            .find(|item| item.id == source.id)
            .expect("row");
        assert_eq!(row.health, SourceHealth::Stale, "旧内容必须被标记为停更");
        assert!(row.latest_article_at.is_some());
    }

    #[test]
    fn failing_beats_ok_but_disabled_beats_all() {
        let dir = tempdir().expect("tempdir");
        let store = NewsRepository::open(dir.path().join("news.db")).expect("open");
        let source = store.list_sources().expect("list").remove(0);
        store
            .set_source_health(source.id, Some("server returned 404 Not Found"))
            .expect("fail");
        assert_eq!(
            store
                .list_sources()
                .expect("list")
                .into_iter()
                .find(|item| item.id == source.id)
                .expect("row")
                .health,
            SourceHealth::Failing
        );
        store.set_source_health(source.id, None).expect("recover");
        assert_eq!(
            store
                .list_sources()
                .expect("list")
                .into_iter()
                .find(|item| item.id == source.id)
                .expect("row")
                .health,
            SourceHealth::Ok,
            "恢复后回到 ok"
        );
    }

    #[test]
    fn catalog_entries_all_carry_a_verification_date() {
        for source in devtoolbox_core::news::recommended_sources() {
            assert!(
                source.verified_on.len() == 10 && source.verified_on.contains('-'),
                "{} 缺少实测日期：{}",
                source.name,
                source.verified_on
            );
            assert!(
                source.url.starts_with("http://") || source.url.starts_with("https://"),
                "{} 的地址不是 http(s)",
                source.name
            );
        }
    }
}

#[cfg(test)]
mod dedup_tests {
    //! 跨源同标题去重（2026-10-05 实测：同一条通稿在两个频道各出现一次）。

    use super::*;
    use devtoolbox_core::feed::FetchedEntry;
    use tempfile::tempdir;

    fn entry(guid: &str, title: &str, published_at: i64) -> FetchedEntry {
        FetchedEntry {
            guid: guid.to_string(),
            url: format!("https://example.com/{guid}"),
            title: title.to_string(),
            author: None,
            image_url: None,
            published_at: Some(published_at),
            summary: None,
        }
    }

    #[test]
    fn same_title_from_two_sources_appears_once_in_the_feed() {
        let dir = tempdir().expect("tempdir");
        let store = NewsRepository::open(dir.path().join("news.db")).expect("open");
        let sources = store.list_sources().expect("list");
        let now = news_now();
        store
            .insert_articles(sources[0].id, &[entry("a1", "同一条通稿", now - 60)])
            .expect("first");
        store
            .insert_articles(sources[1].id, &[entry("a2", "同一条通稿", now - 30)])
            .expect("duplicate");

        let feed = store.latest(50).expect("latest");
        assert_eq!(
            feed.iter()
                .filter(|article| article.title == "同一条通稿")
                .count(),
            1,
            "跨源同标题只应出现一次"
        );
        // 按源看仍然各自可见（用户筛源时不希望被去重掉）。
        assert_eq!(
            store
                .latest_by_source(sources[1].id, 10)
                .expect("by source")
                .iter()
                .filter(|article| article.title == "同一条通稿")
                .count(),
            1,
            "按源查看不受跨源去重影响"
        );
    }

    #[test]
    fn different_titles_are_untouched() {
        let dir = tempdir().expect("tempdir");
        let store = NewsRepository::open(dir.path().join("news.db")).expect("open");
        let sources = store.list_sources().expect("list");
        let now = news_now();
        store
            .insert_articles(
                sources[0].id,
                &[
                    entry("a", "标题一", now - 30),
                    entry("b", "标题二", now - 20),
                ],
            )
            .expect("insert");
        assert_eq!(store.latest(50).expect("latest").len(), 2);
    }
}
