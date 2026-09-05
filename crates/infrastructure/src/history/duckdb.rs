//! history.duckdb 的只读 Rust 访问层。
//!
//! 本模块不提供 INSERT、UPDATE 或 DELETE；每个调用打开 DuckDB 只读连接并只执行 SELECT。

use std::path::{Path, PathBuf};

use duckdb::{AccessMode, Config, Connection, params};
use serde::Serialize;

use crate::error::InfrastructureError;

#[derive(Debug, Clone)]
pub struct HistoryDuckDbRepository {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonResult {
    pub id: String,
    pub canonical_name_zh_cn: String,
    pub name_raw: Option<String>,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub gender: Option<String>,
    pub quality_status: Option<String>,
    pub created_from_source: Option<String>,
    pub intro_zh_cn: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonRelationResult {
    pub person_a_id: String,
    pub person_a_name: Option<String>,
    pub person_b_id: String,
    pub person_b_name: Option<String>,
    pub relation_type: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub source_ids: Option<String>,
    pub confidence: Option<f64>,
    /// 面向当前查询人物的可读关系名；在反向查询时使用 CBDB 的反向关系名。
    pub relation_name_zh_cn: Option<String>,
    pub relation_category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonPlaceResult {
    pub person_id: String,
    pub place_id: String,
    pub place_name: Option<String>,
    pub historical_name: Option<String>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
    pub relation_type: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkResult {
    pub id: String,
    pub title: String,
    pub title_zh_cn: Option<String>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoricalTextResult {
    pub id: String,
    pub title_zh_cn: Option<String>,
    pub book_id: Option<String>,
    pub work_title: Option<String>,
    pub chapter: Option<String>,
    pub original_text: Option<String>,
    pub original_simplified: Option<String>,
    pub translation_zh_cn: Option<String>,
    pub translation_source: Option<String>,
    pub alignment_quality: Option<String>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
    pub translation_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodResult {
    pub id: String,
    pub name_zh_cn: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub date_precision: Option<String>,
    pub description_zh_cn: Option<String>,
    pub quality_status: Option<String>,
    pub source_type: Option<String>,
    pub source_ids: Option<String>,
    pub name_raw: Option<String>,
    pub source_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegimeResult {
    pub id: String,
    pub name_zh_cn: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub date_precision: Option<String>,
    pub period_id: Option<String>,
    pub parent_regime_id: Option<String>,
    pub capital_place_id: Option<String>,
    pub description_zh_cn: Option<String>,
    pub quality_status: Option<String>,
    pub source_type: Option<String>,
    pub source_ids: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryResult {
    pub id: String,
    pub title_zh_cn: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub summary_zh_cn: Option<String>,
    pub background_zh_cn: Option<String>,
    pub result_zh_cn: Option<String>,
    pub story_type: Option<String>,
    pub importance: Option<String>,
    pub period_id: Option<String>,
    pub quality_status: Option<String>,
    pub source_type: Option<String>,
    pub source_ids: Option<String>,
    pub usable: Option<bool>,
    pub title_raw: Option<String>,
    pub period_ids: Option<String>,
    pub source_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryEventResult {
    pub story_id: String,
    pub event_id: String,
    pub sequence: Option<i64>,
    pub role: Option<String>,
    pub importance: Option<String>,
    pub transition_text_zh_cn: Option<String>,
    pub quality_status: Option<String>,
    pub name_zh_cn: String,
    pub event_type: Option<String>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub date_precision: Option<String>,
    pub summary_zh_cn: Option<String>,
    pub result_zh_cn: Option<String>,
    pub event_quality_status: Option<String>,
    pub source_type: Option<String>,
    pub source_ids: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventResult {
    pub id: String,
    pub name_zh_cn: String,
    pub event_type: Option<String>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub date_precision: Option<String>,
    pub period_id: Option<String>,
    pub regime_id: Option<String>,
    pub summary_zh_cn: Option<String>,
    pub background_zh_cn: Option<String>,
    pub result_zh_cn: Option<String>,
    pub importance: Option<String>,
    pub quality_status: Option<String>,
    pub source_type: Option<String>,
    pub source_ids: Option<String>,
    pub period_ids: Option<String>,
    pub dynasty_ids: Option<String>,
    pub regime_ids: Option<String>,
    pub source_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPersonResult {
    pub event_id: String,
    pub person_id: String,
    pub role: String,
    pub role_zh_cn: Option<String>,
    pub side: Option<String>,
    pub importance: Option<String>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
    pub person_name: Option<String>,
    pub description: Option<String>,
    pub link_quality_status: Option<String>,
    pub link_confidence: Option<f64>,
    pub link_reason: Option<String>,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub person_quality_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPlaceResult {
    pub event_id: String,
    pub place_id: Option<String>,
    pub place_name_raw: Option<String>,
    pub role: Option<String>,
    pub sequence: Option<i64>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
    pub link_status: Option<String>,
    pub place_name: Option<String>,
    pub description_zh_cn: Option<String>,
    pub link_quality_status: Option<String>,
    pub link_confidence: Option<f64>,
    pub link_reason: Option<String>,
    pub historical_name: Option<String>,
    pub modern_name: Option<String>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRelationResult {
    pub source_event_id: String,
    pub target_event_id: String,
    pub relation_type: String,
    pub confidence: Option<f64>,
    pub description_zh_cn: Option<String>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
    pub source_event_name: Option<String>,
    pub target_event_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventHistoricalTextResult {
    pub event_id: String,
    pub historical_text_id: String,
    pub role: Option<String>,
    pub sequence: Option<i64>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
    pub title_zh_cn: Option<String>,
    pub work_title: Option<String>,
    pub chapter: Option<String>,
    pub original_text: Option<String>,
    pub original_simplified: Option<String>,
    pub translation_zh_cn: Option<String>,
    pub source_quality_status: Option<String>,
    pub link_quality_status: Option<String>,
    pub link_confidence: Option<f64>,
    pub link_reason: Option<String>,
    pub temporal_score: Option<f64>,
    pub person_score: Option<f64>,
    pub place_score: Option<f64>,
    pub keyword_score: Option<f64>,
    pub work_score: Option<f64>,
    pub context_score: Option<f64>,
    pub chapter_score: Option<f64>,
    pub translation_source: Option<String>,
    pub alignment_quality: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceResult {
    pub id: String,
    pub dataset: Option<String>,
    pub snapshot_version: Option<String>,
    pub dataset_version: Option<String>,
    pub source_type: Option<String>,
    pub license: Option<String>,
    pub quality: Option<String>,
    pub quality_status: Option<String>,
    pub original_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonEventResult {
    pub event_id: String,
    pub event_name: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub summary_zh_cn: Option<String>,
    pub role_zh_cn: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonStoryResult {
    pub story_id: String,
    pub title_zh_cn: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub summary_zh_cn: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DatasetStats {
    pub people: i64,
    pub places: i64,
    pub person_relations: i64,
    pub person_places: i64,
    pub works: i64,
    pub historical_texts: i64,
}

impl HistoryDuckDbRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(InfrastructureError::DuckDb(format!(
                "history.duckdb not found: {}",
                path.display()
            )));
        }
        let repository = Self { path };
        repository.with_connection(|connection| {
            connection
                .execute("SELECT 1 FROM people LIMIT 1", [])
                .map(|_| ())
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))
        })?;
        Ok(repository)
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, InfrastructureError>,
    ) -> Result<T, InfrastructureError> {
        let config = Config::default()
            .access_mode(AccessMode::ReadOnly)
            .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
        let connection = Connection::open_with_flags(&self.path, config)
            .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
        operation(&connection)
    }

    /// 按稳定 ID 或名称读取人物。
    ///
    /// 详情页和事件关系使用 `people.id`，而搜索结果展示的是名称；两种入口
    /// 都必须可用，不能把 ID 当作 `search_name` 查询。
    pub fn get_person(&self, query: &str) -> Result<Option<PersonResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id,canonical_name_zh_cn,name_raw,birth_year,death_year,gender,quality_status,created_from_source,intro_zh_cn
                     FROM people
                     WHERE id = ?1 OR canonical_name_zh_cn = ?1 OR search_name = ?1
                     ORDER BY id
                     LIMIT 1",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let mut rows = statement
                .query(params![query])
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let Some(row) = rows
                .next()
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?
            else {
                return Ok(None);
            };
            Ok(Some(PersonResult {
                id: row.get(0).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                canonical_name_zh_cn: row.get(1).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                name_raw: row.get(2).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                birth_year: row.get(3).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                death_year: row.get(4).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                gender: row.get(5).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                quality_status: row.get(6).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                created_from_source: row.get(7).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
                intro_zh_cn: row.get(8).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?,
            }))
        })
    }

    pub fn search_people(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<PersonResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,canonical_name_zh_cn,name_raw,birth_year,death_year,gender,quality_status,created_from_source,intro_zh_cn
                 FROM people WHERE canonical_name_zh_cn LIKE ?1 OR search_name LIKE ?1 OR search_text LIKE ?1
                 ORDER BY canonical_name_zh_cn,id LIMIT ?2",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![format!("%{query}%"), limit.clamp(1, 100)], |row| {
                Ok(PersonResult {
                    id: row.get(0)?, canonical_name_zh_cn: row.get(1)?, name_raw: row.get(2)?,
                    birth_year: row.get(3)?, death_year: row.get(4)?, gender: row.get(5)?,
                    quality_status: row.get(6)?, created_from_source: row.get(7)?,
                    intro_zh_cn: row.get(8)?,
                })
            }).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    pub fn get_person_relations(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonRelationResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT pr.person_a_id,a.canonical_name_zh_cn,pr.person_b_id,b.canonical_name_zh_cn,
                        pr.relation_type,pr.start_year,pr.end_year,pr.source_ids,pr.confidence,
                        CASE WHEN pr.person_a_id=?1 THEN rtd.relation_name_zh_cn ELSE rtd.inverse_relation_name_zh_cn END,
                        rtd.relation_category
                 FROM person_relations pr LEFT JOIN people a ON a.id=pr.person_a_id LEFT JOIN people b ON b.id=pr.person_b_id
                 LEFT JOIN relation_type_dictionary rtd ON rtd.source_dataset='cbdb' AND rtd.source_relation_code=pr.relation_type
                 WHERE pr.person_a_id=?1 OR pr.person_b_id=?1 ORDER BY pr.relation_type,pr.person_a_id,pr.person_b_id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![person_id], |row| Ok(PersonRelationResult {
                person_a_id: row.get(0)?, person_a_name: row.get(1)?, person_b_id: row.get(2)?, person_b_name: row.get(3)?,
                relation_type: row.get(4)?, start_year: row.get(5)?, end_year: row.get(6)?, source_ids: row.get(7)?, confidence: row.get(8)?,
                relation_name_zh_cn: row.get(9)?, relation_category: row.get(10)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_person_places(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonPlaceResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT pp.person_id,pp.place_id,pl.canonical_name_zh_cn,pl.historical_name,pl.longitude,pl.latitude,
                        pp.relation_type,pp.start_year,pp.end_year,pp.source_id
                 FROM person_place pp LEFT JOIN places pl ON pl.id=pp.place_id WHERE pp.person_id=?1
                 ORDER BY pp.relation_type,pp.start_year NULLS LAST,pp.place_id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![person_id], |row| Ok(PersonPlaceResult {
                person_id: row.get(0)?, place_id: row.get(1)?, place_name: row.get(2)?, historical_name: row.get(3)?,
                longitude: row.get(4)?, latitude: row.get(5)?, relation_type: row.get(6)?, start_year: row.get(7)?,
                end_year: row.get(8)?, source_id: row.get(9)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_work(
        &self,
        title: &str,
        limit: i64,
    ) -> Result<Vec<WorkResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id,title,title_zh_cn,source_id,quality_status FROM works
                 WHERE title LIKE ?1 OR title_zh_cn LIKE ?1 ORDER BY title,id LIMIT ?2",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement
                .query_map(
                    params![format!("%{title}%"), limit.clamp(1, 100)],
                    |row| {
                        Ok(WorkResult {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            title_zh_cn: row.get(2)?,
                            source_id: row.get(3)?,
                            quality_status: row.get(4)?,
                        })
                    },
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    pub fn get_historical_texts(
        &self,
        work: Option<&str>,
        limit: i64,
    ) -> Result<Vec<HistoricalTextResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let sql = "SELECT ht.id,ht.title_zh_cn,ht.book_id,w.title,ht.chapter,ht.original_text,ht.original_simplified,
                              ht.translation_zh_cn,ht.translation_source,ht.alignment_quality,ht.source_id,ht.quality_status,ht.translation_type
                       FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id
                       WHERE (?1 IS NULL OR w.title LIKE ?2 OR w.title_zh_cn LIKE ?2)
                       ORDER BY ht.title_zh_cn,ht.chapter,ht.id LIMIT ?3";
            let pattern = work.map(|value| format!("%{value}%"));
            let mut statement = connection.prepare(sql).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![work, pattern, limit.clamp(1, 100)], |row| Ok(HistoricalTextResult {
                id: row.get(0)?, title_zh_cn: row.get(1)?, book_id: row.get(2)?, work_title: row.get(3)?, chapter: row.get(4)?,
                original_text: row.get(5)?, original_simplified: row.get(6)?, translation_zh_cn: row.get(7)?, translation_source: row.get(8)?,
                alignment_quality: row.get(9)?, source_id: row.get(10)?, quality_status: row.get(11)?, translation_type: row.get(12)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_periods(&self) -> Result<Vec<PeriodResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,name_zh_cn,start_year,end_year,date_precision,description_zh_cn,quality_status,source_type,source_ids,name_raw,source_reference
                 FROM periods ORDER BY start_year NULLS LAST,id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map([], |row| Ok(PeriodResult {
                id: row.get(0)?, name_zh_cn: row.get(1)?, start_year: row.get(2)?, end_year: row.get(3)?,
                date_precision: row.get(4)?, description_zh_cn: row.get(5)?, quality_status: row.get(6)?,
                source_type: row.get(7)?, source_ids: row.get(8)?, name_raw: row.get(9)?, source_reference: row.get(10)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_regimes_by_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<RegimeResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,name_zh_cn,start_year,end_year,date_precision,period_id,parent_regime_id,capital_place_id,
                        description_zh_cn,quality_status,source_type,source_ids
                 FROM regimes WHERE period_id=?1 ORDER BY start_year NULLS LAST,id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![period_id], |row| Ok(RegimeResult {
                id: row.get(0)?, name_zh_cn: row.get(1)?, start_year: row.get(2)?, end_year: row.get(3)?, date_precision: row.get(4)?,
                period_id: row.get(5)?, parent_regime_id: row.get(6)?, capital_place_id: row.get(7)?, description_zh_cn: row.get(8)?,
                quality_status: row.get(9)?, source_type: row.get(10)?, source_ids: row.get(11)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_stories(&self) -> Result<Vec<StoryResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,title_zh_cn,start_year,end_year,summary_zh_cn,background_zh_cn,result_zh_cn,story_type,
                        importance,period_id,quality_status,source_type,source_ids,usable,title_raw,period_ids,source_reference
                 FROM stories ORDER BY start_year NULLS LAST,id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map([], |row| Ok(StoryResult {
                id: row.get(0)?, title_zh_cn: row.get(1)?, start_year: row.get(2)?, end_year: row.get(3)?, summary_zh_cn: row.get(4)?,
                background_zh_cn: row.get(5)?, result_zh_cn: row.get(6)?, story_type: row.get(7)?, importance: row.get(8)?,
                period_id: row.get(9)?, quality_status: row.get(10)?, source_type: row.get(11)?, source_ids: row.get(12)?, usable: row.get(13)?, title_raw: row.get(14)?, period_ids: row.get(15)?, source_reference: row.get(16)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_story(&self, query: &str) -> Result<Option<StoryResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,title_zh_cn,start_year,end_year,summary_zh_cn,background_zh_cn,result_zh_cn,story_type,
                        importance,period_id,quality_status,source_type,source_ids,usable,title_raw,period_ids,source_reference
                 FROM stories WHERE id=?1 OR title_zh_cn=?1 ORDER BY id LIMIT 1",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let mut rows = statement.query(params![query]).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let Some(row) = rows.next().map_err(|error| InfrastructureError::DuckDb(error.to_string()))? else { return Ok(None); };
            Ok(Some(StoryResult {
                id: row.get(0).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, title_zh_cn: row.get(1).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, start_year: row.get(2).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, end_year: row.get(3).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, summary_zh_cn: row.get(4).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                background_zh_cn: row.get(5).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, result_zh_cn: row.get(6).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, story_type: row.get(7).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, importance: row.get(8).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                period_id: row.get(9).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, quality_status: row.get(10).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_type: row.get(11).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_ids: row.get(12).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, usable: row.get(13).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, title_raw: row.get(14).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, period_ids: row.get(15).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_reference: row.get(16).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
            }))
        })
    }

    pub fn get_story_events(
        &self,
        story_id: &str,
    ) -> Result<Vec<StoryEventResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT se.story_id,se.event_id,se.sequence,se.role,se.importance,se.transition_text_zh_cn,se.quality_status,
                        e.name_zh_cn,e.event_type,e.start_year,e.end_year,e.date_precision,e.summary_zh_cn,e.result_zh_cn,
                        e.quality_status,e.source_type,e.source_ids
                 FROM story_events se JOIN events e ON e.id=se.event_id WHERE se.story_id=?1 ORDER BY se.sequence NULLS LAST,se.event_id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![story_id], |row| Ok(StoryEventResult {
                story_id: row.get(0)?, event_id: row.get(1)?, sequence: row.get(2)?, role: row.get(3)?, importance: row.get(4)?,
                transition_text_zh_cn: row.get(5)?, quality_status: row.get(6)?, name_zh_cn: row.get(7)?, event_type: row.get(8)?,
                start_year: row.get(9)?, end_year: row.get(10)?, date_precision: row.get(11)?, summary_zh_cn: row.get(12)?, result_zh_cn: row.get(13)?, event_quality_status: row.get(14)?, source_type: row.get(15)?, source_ids: row.get(16)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_event(&self, query: &str) -> Result<Option<EventResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,name_zh_cn,event_type,start_year,end_year,date_precision,period_id,regime_id,summary_zh_cn,
                        background_zh_cn,result_zh_cn,importance,quality_status,source_type,source_ids,period_ids,dynasty_ids,regime_ids,source_reference
                 FROM events WHERE id=?1 OR name_zh_cn=?1 ORDER BY id LIMIT 1",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let mut rows = statement.query(params![query]).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let Some(row) = rows.next().map_err(|error| InfrastructureError::DuckDb(error.to_string()))? else { return Ok(None); };
            Ok(Some(EventResult {
                id: row.get(0).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, name_zh_cn: row.get(1).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, event_type: row.get(2).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, start_year: row.get(3).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, end_year: row.get(4).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                date_precision: row.get(5).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, period_id: row.get(6).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, regime_id: row.get(7).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, summary_zh_cn: row.get(8).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, background_zh_cn: row.get(9).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                result_zh_cn: row.get(10).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, importance: row.get(11).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, quality_status: row.get(12).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_type: row.get(13).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_ids: row.get(14).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, period_ids: row.get(15).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, dynasty_ids: row.get(16).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, regime_ids: row.get(17).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_reference: row.get(18).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
            }))
        })
    }

    pub fn search_events(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<EventResult>, InfrastructureError> {
        let mut matches = Vec::new();
        for event in self.get_events()? {
            if event.name_zh_cn.contains(query)
                || event
                    .summary_zh_cn
                    .as_deref()
                    .is_some_and(|text| text.contains(query))
            {
                matches.push(event);
                if matches.len() >= limit.clamp(1, 100) as usize {
                    break;
                }
            }
        }
        Ok(matches)
    }

    fn get_events(&self) -> Result<Vec<EventResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,name_zh_cn,event_type,start_year,end_year,date_precision,period_id,regime_id,summary_zh_cn,
                        background_zh_cn,result_zh_cn,importance,quality_status,source_type,source_ids,period_ids,dynasty_ids,regime_ids,source_reference
                 FROM events ORDER BY start_year NULLS LAST,id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map([], |row| Ok(EventResult {
                id: row.get(0)?, name_zh_cn: row.get(1)?, event_type: row.get(2)?, start_year: row.get(3)?, end_year: row.get(4)?, date_precision: row.get(5)?, period_id: row.get(6)?, regime_id: row.get(7)?, summary_zh_cn: row.get(8)?, background_zh_cn: row.get(9)?, result_zh_cn: row.get(10)?, importance: row.get(11)?, quality_status: row.get(12)?, source_type: row.get(13)?, source_ids: row.get(14)?, period_ids: row.get(15)?, dynasty_ids: row.get(16)?, regime_ids: row.get(17)?, source_reference: row.get(18)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_event_people(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventPersonResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT ep.event_id,ep.person_id,ep.role,ep.role_zh_cn,ep.side,ep.importance,ep.source_id,ep.quality_status,
                        p.canonical_name_zh_cn,ep.description,ep.link_quality_status,ep.link_confidence,ep.link_reason,
                        p.birth_year,p.death_year,p.quality_status FROM event_person ep LEFT JOIN people p ON p.id=ep.person_id
                 WHERE ep.event_id=?1 ORDER BY ep.importance DESC,p.canonical_name_zh_cn,p.id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![event_id], |row| Ok(EventPersonResult {
                event_id: row.get(0)?, person_id: row.get(1)?, role: row.get(2)?, role_zh_cn: row.get(3)?, side: row.get(4)?,
                importance: row.get(5)?, source_id: row.get(6)?, quality_status: row.get(7)?, person_name: row.get(8)?, description: row.get(9)?, link_quality_status: row.get(10)?, link_confidence: row.get(11)?, link_reason: row.get(12)?, birth_year: row.get(13)?, death_year: row.get(14)?, person_quality_status: row.get(15)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_event_places(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventPlaceResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT ep.event_id,ep.place_id,ep.place_name_raw,ep.role,ep.sequence,ep.source_id,ep.quality_status,ep.link_status,
                        p.canonical_name_zh_cn,ep.description_zh_cn,ep.link_quality_status,ep.link_confidence,ep.link_reason,
                        p.historical_name,p.modern_name,p.longitude,p.latitude FROM event_place ep LEFT JOIN places p ON p.id=ep.place_id
                 WHERE ep.event_id=?1 ORDER BY ep.sequence NULLS LAST,ep.id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![event_id], |row| Ok(EventPlaceResult {
                event_id: row.get(0)?, place_id: row.get(1)?, place_name_raw: row.get(2)?, role: row.get(3)?, sequence: row.get(4)?,
                source_id: row.get(5)?, quality_status: row.get(6)?, link_status: row.get(7)?, place_name: row.get(8)?, description_zh_cn: row.get(9)?, link_quality_status: row.get(10)?, link_confidence: row.get(11)?, link_reason: row.get(12)?, historical_name: row.get(13)?, modern_name: row.get(14)?, longitude: row.get(15)?, latitude: row.get(16)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_event_relations(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventRelationResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT er.source_event_id,er.target_event_id,er.relation_type,er.confidence,er.description_zh_cn,er.source_id,er.quality_status,
                        se.name_zh_cn,te.name_zh_cn FROM event_relations er
                 LEFT JOIN events se ON se.id=er.source_event_id LEFT JOIN events te ON te.id=er.target_event_id
                 WHERE er.source_event_id=?1 OR er.target_event_id=?1 ORDER BY er.source_event_id,er.target_event_id,er.relation_type",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![event_id], |row| Ok(EventRelationResult {
                source_event_id: row.get(0)?, target_event_id: row.get(1)?, relation_type: row.get(2)?, confidence: row.get(3)?,
                description_zh_cn: row.get(4)?, source_id: row.get(5)?, quality_status: row.get(6)?, source_event_name: row.get(7)?, target_event_name: row.get(8)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_event_texts(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT et.event_id,et.historical_text_id,et.role,et.sequence,et.source_id,et.quality_status,ht.title_zh_cn,w.title,ht.chapter,
                        ht.original_text,ht.original_simplified,ht.translation_zh_cn,et.source_quality_status,et.link_quality_status,
                        et.link_confidence,et.link_reason,et.temporal_score,et.person_score,et.place_score,et.keyword_score,et.work_score,
                        et.context_score,et.chapter_score,ht.translation_source,ht.alignment_quality FROM event_text et JOIN historical_texts ht ON ht.id=et.historical_text_id
                 LEFT JOIN works w ON w.id=ht.book_id WHERE et.event_id=?1 ORDER BY et.sequence,ht.id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![event_id], |row| Ok(EventHistoricalTextResult {
                event_id: row.get(0)?, historical_text_id: row.get(1)?, role: row.get(2)?, sequence: row.get(3)?, source_id: row.get(4)?,
                quality_status: row.get(5)?, title_zh_cn: row.get(6)?, work_title: row.get(7)?, chapter: row.get(8)?, original_text: row.get(9)?,
                original_simplified: row.get(10)?, translation_zh_cn: row.get(11)?, source_quality_status: row.get(12)?, link_quality_status: row.get(13)?, link_confidence: row.get(14)?, link_reason: row.get(15)?, temporal_score: row.get(16)?, person_score: row.get(17)?, place_score: row.get(18)?, keyword_score: row.get(19)?, work_score: row.get(20)?, context_score: row.get(21)?, chapter_score: row.get(22)?, translation_source: row.get(23)?, alignment_quality: row.get(24)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_stories_for_period(
        &self,
        period_id: Option<&str>,
    ) -> Result<Vec<StoryResult>, InfrastructureError> {
        let stories = self.get_stories()?;
        Ok(stories
            .into_iter()
            .filter(|story| period_id.is_none_or(|id| story.period_id.as_deref() == Some(id)))
            .collect())
    }

    pub fn get_story_people(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventPersonResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT ep.event_id,ep.person_id,ep.role,ep.role_zh_cn,ep.side,ep.importance,ep.source_id,ep.quality_status,
                        p.canonical_name_zh_cn,ep.description,ep.link_quality_status,ep.link_confidence,ep.link_reason,
                        p.birth_year,p.death_year,p.quality_status
                 FROM story_events se JOIN event_person ep ON ep.event_id=se.event_id
                 LEFT JOIN people p ON p.id=ep.person_id WHERE se.story_id=?1
                 ORDER BY se.sequence,ep.importance DESC,p.canonical_name_zh_cn,p.id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![story_id], |row| Ok(EventPersonResult {
                event_id: row.get(0)?, person_id: row.get(1)?, role: row.get(2)?, role_zh_cn: row.get(3)?, side: row.get(4)?,
                importance: row.get(5)?, source_id: row.get(6)?, quality_status: row.get(7)?, person_name: row.get(8)?, description: row.get(9)?,
                link_quality_status: row.get(10)?, link_confidence: row.get(11)?, link_reason: row.get(12)?, birth_year: row.get(13)?, death_year: row.get(14)?, person_quality_status: row.get(15)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_story_places(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventPlaceResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT ep.event_id,ep.place_id,ep.place_name_raw,ep.role,ep.sequence,ep.source_id,ep.quality_status,ep.link_status,
                        p.canonical_name_zh_cn,ep.description_zh_cn,ep.link_quality_status,ep.link_confidence,ep.link_reason,
                        p.historical_name,p.modern_name,p.longitude,p.latitude
                 FROM story_events se JOIN event_place ep ON ep.event_id=se.event_id
                 LEFT JOIN places p ON p.id=ep.place_id WHERE se.story_id=?1
                 ORDER BY se.sequence,ep.sequence NULLS LAST,ep.id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![story_id], |row| Ok(EventPlaceResult {
                event_id: row.get(0)?, place_id: row.get(1)?, place_name_raw: row.get(2)?, role: row.get(3)?, sequence: row.get(4)?,
                source_id: row.get(5)?, quality_status: row.get(6)?, link_status: row.get(7)?, place_name: row.get(8)?, description_zh_cn: row.get(9)?,
                link_quality_status: row.get(10)?, link_confidence: row.get(11)?, link_reason: row.get(12)?, historical_name: row.get(13)?, modern_name: row.get(14)?, longitude: row.get(15)?, latitude: row.get(16)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_story_texts(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT et.event_id,et.historical_text_id,et.role,et.sequence,et.source_id,et.quality_status,ht.title_zh_cn,w.title,ht.chapter,
                        ht.original_text,ht.original_simplified,ht.translation_zh_cn,et.source_quality_status,et.link_quality_status,et.link_confidence,et.link_reason,
                        et.temporal_score,et.person_score,et.place_score,et.keyword_score,et.work_score,et.context_score,et.chapter_score,ht.translation_source,ht.alignment_quality
                 FROM story_events se JOIN event_text et ON et.event_id=se.event_id JOIN historical_texts ht ON ht.id=et.historical_text_id
                 LEFT JOIN works w ON w.id=ht.book_id WHERE se.story_id=?1 ORDER BY se.sequence,et.sequence,ht.id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![story_id], |row| Ok(EventHistoricalTextResult {
                event_id: row.get(0)?, historical_text_id: row.get(1)?, role: row.get(2)?, sequence: row.get(3)?, source_id: row.get(4)?, quality_status: row.get(5)?,
                title_zh_cn: row.get(6)?, work_title: row.get(7)?, chapter: row.get(8)?, original_text: row.get(9)?, original_simplified: row.get(10)?, translation_zh_cn: row.get(11)?,
                source_quality_status: row.get(12)?, link_quality_status: row.get(13)?, link_confidence: row.get(14)?, link_reason: row.get(15)?, temporal_score: row.get(16)?, person_score: row.get(17)?, place_score: row.get(18)?, keyword_score: row.get(19)?, work_score: row.get(20)?, context_score: row.get(21)?, chapter_score: row.get(22)?, translation_source: row.get(23)?, alignment_quality: row.get(24)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_person_events(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonEventResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT e.id,e.name_zh_cn,e.start_year,e.end_year,e.summary_zh_cn,ep.role_zh_cn
                 FROM event_person ep JOIN events e ON e.id=ep.event_id WHERE ep.person_id=?1
                 ORDER BY e.start_year NULLS LAST,e.id LIMIT 100",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement
                .query_map(params![person_id], |row| {
                    Ok(PersonEventResult {
                        event_id: row.get(0)?,
                        event_name: row.get(1)?,
                        start_year: row.get(2)?,
                        end_year: row.get(3)?,
                        summary_zh_cn: row.get(4)?,
                        role_zh_cn: row.get(5)?,
                    })
                })
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    pub fn get_person_stories(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonStoryResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT s.id,s.title_zh_cn,s.start_year,s.end_year,s.summary_zh_cn
                 FROM story_person sp JOIN stories s ON s.id=sp.story_id WHERE sp.person_id=?1
                 ORDER BY s.start_year NULLS LAST,s.id LIMIT 100",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement
                .query_map(params![person_id], |row| {
                    Ok(PersonStoryResult {
                        story_id: row.get(0)?,
                        title_zh_cn: row.get(1)?,
                        start_year: row.get(2)?,
                        end_year: row.get(3)?,
                        summary_zh_cn: row.get(4)?,
                    })
                })
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    pub fn get_sources_for_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<SourceResult>, InfrastructureError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.with_connection(|connection| {
            let placeholders = (1..=ids.len()).map(|index| format!("?{index}")).collect::<Vec<_>>().join(",");
            let sql = format!("SELECT id,dataset,snapshot_version,dataset_version,source_type,license,quality,quality_status,original_url FROM sources WHERE id IN ({placeholders}) ORDER BY dataset,id");
            let mut statement = connection.prepare(&sql).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let values = ids.iter().map(|id| id.as_str()).collect::<Vec<_>>();
            let rows = statement.query_map(duckdb::params_from_iter(values), |row| Ok(SourceResult {
                id: row.get(0)?, dataset: row.get(1)?, snapshot_version: row.get(2)?, dataset_version: row.get(3)?, source_type: row.get(4)?, license: row.get(5)?, quality: row.get(6)?, quality_status: row.get(7)?, original_url: row.get(8)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string()))).collect()
        })
    }

    pub fn get_dataset_stats(&self) -> Result<DatasetStats, InfrastructureError> {
        self.with_connection(|connection| {
            let count = |table: &str| -> Result<i64, InfrastructureError> {
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .map_err(|error| InfrastructureError::DuckDb(error.to_string()))
            };
            Ok(DatasetStats {
                people: count("people")?,
                places: count("places")?,
                person_relations: count("person_relations")?,
                person_places: count("person_place")?,
                works: count("works")?,
                historical_texts: count("historical_texts")?,
            })
        })
    }
}

#[cfg(test)]
mod semantic_tests {
    use super::*;

    #[test]
    fn semantic_repository_reads_curated_story_flow_and_evidence() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../history-data-pipeline/data/normalized/history.duckdb");
        if !path.is_file() {
            return;
        }
        let repository = HistoryDuckDbRepository::open(path).expect("open semantic database");
        let periods = repository.get_periods().expect("period query");
        assert!(periods.len() >= 27, "正式历史库必须暴露完整的时期列表");
        assert!(
            periods
                .iter()
                .any(|period| period.id == "period-three-kingdoms")
        );
        let story = repository
            .get_story("story-three-kingdoms")
            .expect("story query")
            .expect("curated story");
        assert_eq!(story.usable, Some(true));

        let events = repository
            .get_story_events(&story.id)
            .expect("story events query");
        assert_eq!(events.len(), 8);
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );

        let people = repository
            .get_story_people(&story.id)
            .expect("story people query");
        assert!(
            people
                .iter()
                .any(|person| person.person_name.as_deref() == Some("曹操"))
        );

        let event = repository
            .get_event("event-three-chibi")
            .expect("event query")
            .expect("chibi event");
        let texts = repository.get_event_texts(&event.id).expect("text query");
        assert!(!texts.is_empty());
        assert!(texts.iter().all(|text| text.original_text.is_some()));
    }

    #[test]
    fn semantic_repository_reads_person_by_stable_id() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../history-data-pipeline/data/normalized/history.duckdb");
        if !path.is_file() {
            return;
        }
        let repository = HistoryDuckDbRepository::open(path).expect("open semantic database");
        let story = repository
            .get_story("story-three-kingdoms")
            .expect("story query")
            .expect("curated story");
        let person_id = repository
            .get_story_people(&story.id)
            .expect("story people query")
            .into_iter()
            .map(|person| person.person_id)
            .next()
            .expect("story person id");

        let person = repository
            .get_person(&person_id)
            .expect("person query by id")
            .expect("person detail");
        assert_eq!(person.id, person_id);
    }

    #[test]
    fn person_relations_use_directional_cbdb_labels() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../history-data-pipeline/data/normalized/history.duckdb");
        if !path.is_file() {
            return;
        }
        let repository = HistoryDuckDbRepository::open(path).expect("open semantic database");
        let relations = repository
            .get_person_relations("cbdb-person-30257")
            .expect("person relation query");
        assert!(!relations.is_empty(), "曹操应有 CBDB 人物关系");
        assert!(
            relations
                .iter()
                .all(|relation| relation.relation_name_zh_cn.is_some()),
            "已收录的 CBDB 关系必须映射为可读称谓"
        );
    }
}
