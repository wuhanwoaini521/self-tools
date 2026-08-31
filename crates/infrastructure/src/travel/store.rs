//! Travel 本地缓存（需求 #十七）：SQLite（与 RSS 同款 rusqlite 方案，不引入新数据库）。
//!
//! 三类缓存：
//! - 攻略 `travel_guides`：TTL 24h（需求 #十七）
//! - 搜索结果 `travel_search_cache`：TTL 24h
//! - 网页文档 `travel_document_cache`：TTL 7d
//!
//! 缓存 JSON 损坏（数据库缓存损坏场景）→ 返回错误，由上层当作 miss 处理，绝不崩溃。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::error::InfrastructureError;
use devtoolbox_core::travel::{
    CityGuide, DOCUMENT_TTL_SECS, GUIDE_TTL_SECS, GuideSummary, SEARCH_TTL_SECS, SearchResult,
    TravelDocument, is_fresh,
};

/// 缓存条目（搜索结果 / 文档共用）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedDocument {
    pub url: String,
    pub document: TravelDocument,
    pub fetched_at: i64,
}

/// SQLite 缓存。`Connection` 非线程安全，由上层用 `Mutex` 串行化（与 `FeedRepository` 一致）。
pub struct TravelStore {
    connection: Connection,
}

impl TravelStore {
    /// 打开（必要时创建）数据库并确保 Schema 存在。
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
        let store = Self { connection };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS travel_guides (
                    city TEXT NOT NULL,
                    days INTEGER NOT NULL,
                    generated_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    guide_json TEXT NOT NULL,
                    PRIMARY KEY (city, days)
                );
                CREATE TABLE IF NOT EXISTS travel_search_cache (
                    query TEXT PRIMARY KEY,
                    results_json TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS travel_document_cache (
                    url TEXT PRIMARY KEY,
                    document_json TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_travel_guides_updated
                    ON travel_guides(updated_at DESC);",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))
    }

    /// 读取攻略（未命中 / 过期返回 `None`；JSON 损坏返回 Err）。
    pub fn get_guide(
        &self,
        city: &str,
        days: u8,
        now: i64,
    ) -> Result<Option<CityGuide>, InfrastructureError> {
        let row = self
            .connection
            .query_row(
                "SELECT guide_json, updated_at FROM travel_guides WHERE city = ?1 AND days = ?2",
                rusqlite::params![city, days],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let Some((guide_json, updated_at)) = row else {
            return Ok(None);
        };
        if !is_fresh(updated_at, GUIDE_TTL_SECS, now) {
            return Ok(None);
        }
        let mut guide: CityGuide = serde_json::from_str(&guide_json)
            .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))?;
        guide.meta.updated_at = updated_at;
        Ok(Some(guide))
    }

    /// 写入 / 更新攻略（updated_at 落库）。
    pub fn upsert_guide(
        &self,
        guide: &CityGuide,
        now: i64,
    ) -> Result<CityGuide, InfrastructureError> {
        let mut stored = guide.clone();
        stored.meta.updated_at = now;
        let json = serde_json::to_string(&stored)
            .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))?;
        self.connection
            .execute(
                "INSERT INTO travel_guides (city, days, generated_at, updated_at, guide_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(city, days) DO UPDATE SET
                    generated_at = excluded.generated_at,
                    updated_at = excluded.updated_at,
                    guide_json = excluded.guide_json",
                rusqlite::params![
                    stored.city.name,
                    stored.meta.days,
                    stored.meta.generated_at,
                    now,
                    json
                ],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(stored)
    }

    /// 最近攻略列表（Home / Travel 历史用）。
    pub fn list_guides(&self, limit: u8) -> Result<Vec<GuideSummary>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT city, days, updated_at FROM travel_guides
                 ORDER BY updated_at DESC LIMIT ?1",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let rows = statement
            .query_map([limit], |row| {
                Ok(GuideSummary {
                    city: row.get(0)?,
                    days: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            })
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(rows)
    }

    /// 按城市读取攻略（不校验 TTL，供历史打开）。
    pub fn load_guide(
        &self,
        city: &str,
        days: u8,
    ) -> Result<Option<CityGuide>, InfrastructureError> {
        let row = self
            .connection
            .query_row(
                "SELECT guide_json FROM travel_guides WHERE city = ?1 AND days = ?2",
                rusqlite::params![city, days],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        row.map(|json| {
            serde_json::from_str(&json)
                .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))
        })
        .transpose()
    }

    /// 读取搜索结果缓存（未命中 / 过期返回 `None`）。
    pub fn get_search_results(
        &self,
        query: &str,
        now: i64,
    ) -> Result<Option<Vec<SearchResult>>, InfrastructureError> {
        let row = self
            .connection
            .query_row(
                "SELECT results_json, fetched_at FROM travel_search_cache WHERE query = ?1",
                [query],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let Some((results_json, fetched_at)) = row else {
            return Ok(None);
        };
        if !is_fresh(fetched_at, SEARCH_TTL_SECS, now) {
            return Ok(None);
        }
        serde_json::from_str(&results_json)
            .map(Some)
            .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))
    }

    /// 写入搜索结果缓存。
    pub fn put_search_results(
        &self,
        query: &str,
        results: &[SearchResult],
        now: i64,
    ) -> Result<(), InfrastructureError> {
        let json = serde_json::to_string(results)
            .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))?;
        self.connection
            .execute(
                "INSERT INTO travel_search_cache (query, results_json, fetched_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(query) DO UPDATE SET
                    results_json = excluded.results_json,
                    fetched_at = excluded.fetched_at",
                rusqlite::params![query, json, now],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    /// 读取文档缓存（TTL 7 天）。
    pub fn get_document(
        &self,
        url: &str,
        now: i64,
    ) -> Result<Option<TravelDocument>, InfrastructureError> {
        let row = self
            .connection
            .query_row(
                "SELECT document_json, fetched_at FROM travel_document_cache WHERE url = ?1",
                [url],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let Some((document_json, fetched_at)) = row else {
            return Ok(None);
        };
        if !is_fresh(fetched_at, DOCUMENT_TTL_SECS, now) {
            return Ok(None);
        }
        serde_json::from_str(&document_json)
            .map(Some)
            .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))
    }

    /// 写入文档缓存。
    pub fn put_document(
        &self,
        document: &TravelDocument,
        now: i64,
    ) -> Result<(), InfrastructureError> {
        let json = serde_json::to_string(document)
            .map_err(|source| InfrastructureError::TravelFetch(source.to_string()))?;
        self.connection
            .execute(
                "INSERT INTO travel_document_cache (url, document_json, fetched_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(url) DO UPDATE SET
                    document_json = excluded.document_json,
                    fetched_at = excluded.fetched_at",
                rusqlite::params![document.url, json, now],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use devtoolbox_core::travel::{
        CityGuide, CityInfo, ContentState, GuideMeta, SearchResult, TravelDocument, is_fresh,
    };

    use super::{DOCUMENT_TTL_SECS, TravelStore};

    fn open() -> (tempfile::TempDir, TravelStore) {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("travel.db");
        let store = TravelStore::open(path).expect("open db");
        (directory, store)
    }

    fn guide(city: &str) -> CityGuide {
        CityGuide {
            city: CityInfo {
                name: city.to_string(),
                ..CityInfo::default()
            },
            meta: GuideMeta {
                generated_at: 1_700_000_000,
                updated_at: 1_700_000_000,
                days: 3,
                llm_used: true,
                notes: vec![],
            },
            ..CityGuide::default()
        }
    }

    fn document(url: &str, now: i64) -> TravelDocument {
        TravelDocument {
            url: url.to_string(),
            title: "标题".to_string(),
            state: ContentState::Full,
            content: Some("正文内容".to_string()),
            snippet: None,
            published_at: None,
            fetched_at: now,
            provider: Some("mock".to_string()),
        }
    }

    #[test]
    fn guide_round_trips_with_fresh_ttl() {
        let (_dir, store) = open();
        let now = 1_700_000_000;
        let stored = store.upsert_guide(&guide("杭州"), now).expect("upsert");
        assert_eq!(stored.meta.updated_at, now);
        let loaded = store
            .get_guide("杭州", 3, now)
            .expect("get")
            .expect("guide exists");
        assert_eq!(loaded.city.name, "杭州");
        // 24h 后过期
        assert!(
            store
                .get_guide("杭州", 3, now + 25 * 3600)
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn guide_keyed_by_city_and_days() {
        let (_dir, store) = open();
        store
            .upsert_guide(&guide("杭州"), 1_700_000_000)
            .expect("upsert");
        // days 不同 → 未命中
        assert!(
            store
                .get_guide("杭州", 5, 1_700_000_000)
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn search_cache_respects_ttl() {
        let (_dir, store) = open();
        let now = 1_700_000_000;
        let results = vec![SearchResult {
            title: "杭州 攻略".to_string(),
            url: "https://example.com".to_string(),
            snippet: "s".to_string(),
            provider: "mock".to_string(),
            published_at: None,
            fetched_at: now,
        }];
        store
            .put_search_results("杭州 攻略", &results, now)
            .expect("put");
        assert_eq!(
            store
                .get_search_results("杭州 攻略", now)
                .expect("get")
                .expect("hit")
                .len(),
            1
        );
        assert!(
            store
                .get_search_results("杭州 攻略", now + 25 * 3600)
                .expect("get")
                .is_none()
        );
        assert!(
            store
                .get_search_results("别的", now)
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn corrupt_cache_json_is_an_error_not_a_panic() {
        let (_dir, store) = open();
        let now = 1_700_000_000;
        store
            .connection
            .execute(
                "INSERT INTO travel_search_cache (query, results_json, fetched_at) VALUES (?1, ?2, ?3)",
                rusqlite::params!["损坏", "{not json", now],
            )
            .expect("seed corrupt row");
        assert!(store.get_search_results("损坏", now).is_err());
    }

    #[test]
    fn document_cache_uses_seven_day_ttl() {
        let (_dir, store) = open();
        let now = 1_700_000_000;
        store
            .put_document(&document("https://example.com/a", now), now)
            .expect("put");
        assert!(
            store
                .get_document("https://example.com/a", now)
                .expect("hit")
                .is_some()
        );
        assert!(is_fresh(now - 6 * 86_400, DOCUMENT_TTL_SECS, now));
        assert!(!is_fresh(now - 8 * 86_400, DOCUMENT_TTL_SECS, now));
        assert!(
            store
                .get_document("https://example.com/a", now + 8 * 86_400)
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn lists_guides_newest_first() {
        let (_dir, store) = open();
        store
            .upsert_guide(&guide("杭州"), 1_700_000_000)
            .expect("upsert");
        store
            .upsert_guide(&guide("苏州"), 1_800_000_000)
            .expect("upsert");
        let list = store.list_guides(10).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].city, "苏州");
        assert_eq!(list[0].days, 3);
        assert_eq!(list[1].city, "杭州");
    }
}
