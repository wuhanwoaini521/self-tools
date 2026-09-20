//! history.duckdb 的只读 Rust 访问层。
//!
//! 本模块不提供 INSERT、UPDATE 或 DELETE；每个调用打开 DuckDB 只读连接并只执行 SELECT。
//! 查询结果记录类型（*Result / DatasetStats 等）是 application 端口与前端 JSON 的
//! 公共契约，自 Gate 5.5 起由 `devtoolbox_core::history_records` 持有：本文件只做 re-export，
//! 不定义任何 DTO。

use std::path::{Path, PathBuf};

use duckdb::{AccessMode, Config, Connection, params};

use crate::error::InfrastructureError;

pub use devtoolbox_core::history_records::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
    PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
    PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
    WorkResult,
};

#[derive(Debug, Clone)]
pub struct HistoryDuckDbRepository {
    path: PathBuf,
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
                 LEFT JOIN relation_type_dictionary rtd ON rtd.source_dataset IN ('cbdb','curated') AND rtd.source_relation_code=pr.relation_type
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
                .query_map(params![format!("%{title}%"), limit.clamp(1, 100)], |row| {
                    Ok(WorkResult {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        title_zh_cn: row.get(2)?,
                        source_id: row.get(3)?,
                        quality_status: row.get(4)?,
                    })
                })
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    pub fn get_work_by_id(&self, id: &str) -> Result<Option<WorkResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id,title,title_zh_cn,source_id,quality_status FROM works
                 WHERE id=?1 ORDER BY id LIMIT 1",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let mut rows = statement
                .query(params![id])
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let Some(row) = rows
                .next()
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?
            else {
                return Ok(None);
            };
            Ok(Some(WorkResult {
                id: row
                    .get(0)
                    .map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                title: row
                    .get(1)
                    .map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                title_zh_cn: row
                    .get(2)
                    .map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                source_id: row
                    .get(3)
                    .map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                quality_status: row
                    .get(4)
                    .map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
            }))
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
                        background_zh_cn,process_zh_cn,result_zh_cn,impact_zh_cn,importance,quality_status,source_type,source_ids,period_ids,dynasty_ids,regime_ids,source_reference
                 FROM events WHERE id=?1 OR name_zh_cn=?1 ORDER BY id LIMIT 1",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let mut rows = statement.query(params![query]).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let Some(row) = rows.next().map_err(|error| InfrastructureError::DuckDb(error.to_string()))? else { return Ok(None); };
            Ok(Some(EventResult {
                id: row.get(0).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, name_zh_cn: row.get(1).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, event_type: row.get(2).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, start_year: row.get(3).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, end_year: row.get(4).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                date_precision: row.get(5).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, period_id: row.get(6).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, regime_id: row.get(7).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, summary_zh_cn: row.get(8).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, background_zh_cn: row.get(9).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
                process_zh_cn: row.get(10).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, result_zh_cn: row.get(11).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, impact_zh_cn: row.get(12).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, importance: row.get(13).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, quality_status: row.get(14).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_type: row.get(15).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_ids: row.get(16).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, period_ids: row.get(17).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, dynasty_ids: row.get(18).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, regime_ids: row.get(19).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?, source_reference: row.get(20).map_err(|e| InfrastructureError::DuckDb(e.to_string()))?,
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
                        background_zh_cn,process_zh_cn,result_zh_cn,impact_zh_cn,importance,quality_status,source_type,source_ids,period_ids,dynasty_ids,regime_ids,source_reference
                 FROM events ORDER BY start_year NULLS LAST,id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map([], |row| Ok(EventResult {
                id: row.get(0)?, name_zh_cn: row.get(1)?, event_type: row.get(2)?, start_year: row.get(3)?, end_year: row.get(4)?, date_precision: row.get(5)?, period_id: row.get(6)?, regime_id: row.get(7)?, summary_zh_cn: row.get(8)?, background_zh_cn: row.get(9)?, process_zh_cn: row.get(10)?, result_zh_cn: row.get(11)?, impact_zh_cn: row.get(12)?, importance: row.get(13)?, quality_status: row.get(14)?, source_type: row.get(15)?, source_ids: row.get(16)?, period_ids: row.get(17)?, dynasty_ids: row.get(18)?, regime_ids: row.get(19)?, source_reference: row.get(20)?,
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

    /// 单事件的章节级史料证据（V2 `event_evidence`）；primary 证据优先。
    pub fn get_event_evidences(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id,event_id,historical_text_id,work,term,chapter_hint,context_keywords,
                        evidence_role,link_status,link_quality_status,link_confidence,review_note,
                        source_type,source_id,quality_status,rejected_text_ids
                 FROM event_evidence WHERE event_id=?1
                 ORDER BY CASE evidence_role WHEN 'primary' THEN 0 ELSE 1 END,term,id",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement
                .query_map(params![event_id], |row| {
                    Ok(EventEvidenceResult {
                        id: row.get(0)?,
                        event_id: row.get(1)?,
                        historical_text_id: row.get(2)?,
                        work: row.get(3)?,
                        term: row.get(4)?,
                        chapter_hint: row.get(5)?,
                        context_keywords: row.get(6)?,
                        evidence_role: row.get(7)?,
                        link_status: row.get(8)?,
                        link_quality_status: row.get(9)?,
                        link_confidence: row.get(10)?,
                        review_note: row.get(11)?,
                        source_type: row.get(12)?,
                        source_id: row.get(13)?,
                        quality_status: row.get(14)?,
                        rejected_text_ids: row.get(15)?,
                    })
                })
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    /// Story 内全部事件的史料证据（沿 `story_events` 顺序展开）。
    pub fn get_story_evidences(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT ev.id,ev.event_id,ev.historical_text_id,ev.work,ev.term,ev.chapter_hint,ev.context_keywords,
                        ev.evidence_role,ev.link_status,ev.link_quality_status,ev.link_confidence,ev.review_note,
                        ev.source_type,ev.source_id,ev.quality_status,ev.rejected_text_ids
                 FROM story_events se JOIN event_evidence ev ON ev.event_id=se.event_id
                 WHERE se.story_id=?1
                 ORDER BY se.sequence NULLS LAST,CASE ev.evidence_role WHEN 'primary' THEN 0 ELSE 1 END,ev.term,ev.id",
            )
            .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows =
                statement
                    .query_map(params![story_id], |row| {
                        Ok(EventEvidenceResult {
                            id: row.get(0)?,
                            event_id: row.get(1)?,
                            historical_text_id: row.get(2)?,
                            work: row.get(3)?,
                            term: row.get(4)?,
                            chapter_hint: row.get(5)?,
                            context_keywords: row.get(6)?,
                            evidence_role: row.get(7)?,
                            link_status: row.get(8)?,
                            link_quality_status: row.get(9)?,
                            link_confidence: row.get(10)?,
                            review_note: row.get(11)?,
                            source_type: row.get(12)?,
                            source_id: row.get(13)?,
                            quality_status: row.get(14)?,
                            rejected_text_ids: row.get(15)?,
                        })
                    })
                    .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
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
                events: count("events")?,
                periods: count("periods")?,
                regimes: count("regimes")?,
                stories: count("stories")?,
                event_relations: count("event_relations")?,
                event_evidences: count("event_evidence")?,
            })
        })
    }

    /// 时期页事件列表：按起年排序，并带该事件的人物/关系/证据条数。
    pub fn get_events_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<PeriodEventItem>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT e.id, e.name_zh_cn, e.event_type, e.start_year, e.end_year, e.importance,
                        e.summary_zh_cn, e.result_zh_cn,
                        (SELECT COUNT(*) FROM event_person ep WHERE ep.event_id = e.id) AS people_count,
                        (SELECT COUNT(*) FROM event_relations r WHERE r.source_event_id = e.id OR r.target_event_id = e.id) AS relation_count,
                        (SELECT COUNT(*) FROM event_evidence ev WHERE ev.event_id = e.id) AS evidence_count
                 FROM events e WHERE e.period_id = ?1
                 ORDER BY e.start_year NULLS LAST, e.id",
            )
            .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement
                .query_map(params![period_id], |row| {
                    Ok(PeriodEventItem {
                        id: row.get(0)?,
                        name_zh_cn: row.get(1)?,
                        event_type: row.get(2)?,
                        start_year: row.get(3)?,
                        end_year: row.get(4)?,
                        importance: row.get(5)?,
                        summary_zh_cn: row.get(6)?,
                        result_zh_cn: row.get(7)?,
                        people_count: row.get(8)?,
                        relation_count: row.get(9)?,
                        evidence_count: row.get(10)?,
                    })
                })
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    /// 时期页核心人物：参与事件数倒序，`limit` 限制返回条数。
    pub fn get_people_for_period(
        &self,
        period_id: &str,
        limit: i64,
    ) -> Result<Vec<PeriodPersonItem>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT p.id, p.canonical_name_zh_cn, COUNT(DISTINCT ep.event_id) AS event_count,
                        p.birth_year, p.death_year, p.intro_zh_cn
                 FROM event_person ep
                 JOIN events e ON e.id = ep.event_id
                 JOIN people p ON p.id = ep.person_id
                 WHERE e.period_id = ?1
                 GROUP BY p.id, p.canonical_name_zh_cn, p.birth_year, p.death_year, p.intro_zh_cn
                 ORDER BY event_count DESC, p.canonical_name_zh_cn
                 LIMIT ?2",
            )
            .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement
                .query_map(params![period_id, limit.clamp(1, 200)], |row| {
                    Ok(PeriodPersonItem {
                        person_id: row.get(0)?,
                        canonical_name_zh_cn: row.get(1)?,
                        event_count: row.get(2)?,
                        birth_year: row.get(3)?,
                        death_year: row.get(4)?,
                        intro_zh_cn: row.get(5)?,
                    })
                })
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }

    /// 时期页关系池：至少一端属于该时期的全部事件关系（含事件名）。
    ///
    /// 供 Period Detail 的时序链利用：turning point 的「它连接到了什么」、
    /// Timeline 每事件的主叙事关系都从这里取，避免 N+1 逐事件查询。
    pub fn get_relations_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<EventRelationResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT er.source_event_id,er.target_event_id,er.relation_type,er.confidence,er.description_zh_cn,er.source_id,er.quality_status,
                        se.name_zh_cn,te.name_zh_cn FROM event_relations er
                 LEFT JOIN events se ON se.id=er.source_event_id LEFT JOIN events te ON te.id=er.target_event_id
                 WHERE se.period_id=?1 OR te.period_id=?1
                 ORDER BY se.period_id,te.period_id,er.source_event_id,er.target_event_id,er.relation_type",
            )
            .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![period_id], |row| Ok(EventRelationResult {
                source_event_id: row.get(0)?, target_event_id: row.get(1)?, relation_type: row.get(2)?, confidence: row.get(3)?,
                description_zh_cn: row.get(4)?, source_id: row.get(5)?, quality_status: row.get(6)?, source_event_name: row.get(7)?, target_event_name: row.get(8)?,
            })).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            rows.map(|row| row.map_err(|error| InfrastructureError::DuckDb(error.to_string())))
                .collect()
        })
    }
}

#[cfg(test)]
mod semantic_tests {
    use super::*;

    /// V2 Backbone 产物路径（`dist/` 是唯一事实源）；仅在本机有构建产物时运行，
    /// 缺失时静默跳过（`dist/` 不在 Git 中）。刻意不回退 legacy `data/normalized/`。
    fn available_dist_repository() -> Option<HistoryDuckDbRepository> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../history-data-pipeline");
        let path = root.join("dist").join("history.duckdb");
        if !path.is_file() {
            return None;
        }
        Some(HistoryDuckDbRepository::open(path).expect("open semantic database"))
    }

    #[test]
    fn semantic_repository_reads_curated_story_flow_and_calibration_evidence() {
        let Some(repository) = available_dist_repository() else {
            return;
        };
        let periods = repository.get_periods().expect("period query");
        assert!(periods.len() >= 31, "最新 Backbone 必须暴露完整时期列表");
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
                .all(|pair| pair[0].sequence < pair[1].sequence),
            "story_events 必须按 sequence 升序"
        );

        let people = repository
            .get_story_people(&story.id)
            .expect("story people query");
        assert!(
            people
                .iter()
                .any(|person| person.person_name.as_deref() == Some("曹操"))
        );

        // Calibration 产物：关键事件必须携带章节级证据行。
        let evidences = repository
            .get_event_evidences("event-feishui-zhizhan")
            .expect("evidence query");
        // 合法数据增长（2026-09-12，source-batch02 Queue 11 重定位 + Ready-43 字段锚）：
        // 3 → 7 条（1 legacy exact + 6 manual 段落锚，全部 reviewed）。
        assert_eq!(evidences.len(), 7);
        assert!(
            evidences
                .iter()
                .any(|evidence| evidence.evidence_role.as_deref() == Some("primary")),
            "淝水之战必须有 primary 证据"
        );
        assert!(
            evidences
                .iter()
                .all(|evidence| evidence.quality_status.as_deref() == Some("reviewed"))
        );

        // V2 不内嵌 historical_texts 全文；event_text 查询必须保持可读、不报错。
        let event = repository
            .get_event("event-three-chibi")
            .expect("event query")
            .expect("chibi event");
        let _texts = repository.get_event_texts(&event.id).expect("text query");
    }

    #[test]
    fn semantic_repository_reads_calibration_backbone_counts_and_story_evidence() {
        let Some(repository) = available_dist_repository() else {
            return;
        };
        let events = repository.get_events().expect("get event list");
        assert!(events.len() >= 618, "最新 Backbone 应至少 618 个事件");
        assert!(
            events
                .iter()
                .any(|event| event.importance.as_deref() == Some("critical")),
            "校准期必须包含 critical 事件"
        );

        let story = repository
            .get_story("story-chu-han")
            .expect("story query")
            .expect("story");
        let evidences = repository
            .get_story_evidences(&story.id)
            .expect("story evidence query");
        assert!(
            evidences.len() >= 9,
            "楚汉争霸 story 应携带章节级证据（当前 {} 条）",
            evidences.len()
        );
        assert!(
            evidences
                .iter()
                .all(|evidence| evidence.event_id.starts_with("event-")),
            "story 证据必须能回溯到具体事件"
        );
    }

    #[test]
    fn semantic_repository_reads_person_by_stable_id() {
        let Some(repository) = available_dist_repository() else {
            return;
        };
        let story = repository
            .get_story("story-chu-han")
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
    fn person_relations_stay_queryable_when_dataset_is_empty() {
        // V2 dist 当前不携带 person_relations（0 行）；仓库必须保持可用并返回空集。
        let Some(repository) = available_dist_repository() else {
            return;
        };
        let relations = repository
            .get_person_relations("cbdb-person-30257")
            .expect("person relation query");
        assert!(
            relations.is_empty(),
            "V2 dist 不应包含 person_relations 数据"
        );
    }

    #[test]
    fn semantic_repository_reads_period_events_and_people() {
        let Some(repository) = available_dist_repository() else {
            return;
        };
        let events = repository
            .get_events_for_period("period-three-kingdoms")
            .expect("period events query");
        assert!(
            events.len() >= 10,
            "三国时期应有 ≥10 事件，当前 {} 条",
            events.len()
        );
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].start_year.unwrap_or(i32::MIN)
                    <= pair[1].start_year.unwrap_or(i32::MIN)),
            "period events 应按起始年排序"
        );
        assert!(
            events.iter().any(|event| event.people_count > 0),
            "三国时期事件至少一个关联人物"
        );
        assert!(
            events.iter().any(|event| event.evidence_count > 0),
            "三国时期事件至少一个章节级证据"
        );

        let people = repository
            .get_people_for_period("period-three-kingdoms", 20)
            .expect("period people query");
        assert!(!people.is_empty(), "三国时期应有关键人物");
        assert!(
            people
                .windows(2)
                .all(|pair| pair[0].event_count >= pair[1].event_count),
            "key people 应按事件数倒序"
        );
        assert!(
            people
                .iter()
                .all(|item| !item.canonical_name_zh_cn.is_empty()),
            "key people 必须有名字"
        );
    }

    #[test]
    fn semantic_repository_reads_dataset_totals() {
        let Some(repository) = available_dist_repository() else {
            return;
        };
        let stats = repository.get_dataset_stats().expect("stats query");
        assert!(
            stats.events >= 600,
            "events 总量至少 600，当前 {}",
            stats.events
        );
        assert!(
            stats.periods >= 31,
            "periods 至少 31（含上古），当前 {}",
            stats.periods
        );
        assert!(
            stats.people >= 230,
            "people 至少 230，当前 {}",
            stats.people
        );
        assert!(
            stats.event_relations >= 1000,
            "事件关系至少 1000，当前 {}",
            stats.event_relations
        );
        assert!(
            stats.event_evidences >= 120,
            "章节级证据至少 120，当前 {}",
            stats.event_evidences
        );
    }

    #[test]
    fn semantic_repository_search_and_navigation_roundtrip() {
        let Some(repository) = available_dist_repository() else {
            return;
        };
        // 搜索命中 → 按稳定 id 打开：event / person / work
        let events = repository.search_events("赤壁", 10).expect("search events");
        assert!(!events.is_empty(), "搜索「赤壁」必须命中事件");
        let event = repository
            .get_event(&events[0].id)
            .expect("event query")
            .expect("event exists");
        assert!(!event.name_zh_cn.is_empty());

        let people = repository.search_people("曹操", 10).expect("search people");
        assert!(!people.is_empty(), "搜索「曹操」必须命中人物");
        assert!(
            repository.get_person_relations(&people[0].id).is_ok(),
            "人物关系可打开"
        );
        assert!(
            repository.get_person_places(&people[0].id).is_ok(),
            "人物地点可打开"
        );

        let works = repository.get_work("春秋", 10).expect("search works");
        assert!(!works.is_empty(), "搜索「春秋」必须命中作品");
        let work = repository
            .get_work_by_id(&works[0].id)
            .expect("work query")
            .expect("work exists");
        assert!(!work.id.is_empty());
        // V2 dist 当前不内置全文（historical_texts 可为 0 行），查询本身不得报错
        let texts = repository
            .get_historical_texts(Some(&work.title), 50)
            .expect("text query");

        // period/regime 导航
        let regimes = repository
            .get_regimes_by_period("period-three-kingdoms")
            .expect("regimes by period");
        assert!(!regimes.is_empty(), "三国时期应有关键政权");
        println!(
            "search→open roundtrip: {} events, {} people, {} works, {} texts, {} regimes",
            events.len(),
            people.len(),
            works.len(),
            texts.len(),
            regimes.len()
        );
    }

    #[test]
    fn semantic_repository_missing_database_fails_explicitly() {
        let missing =
            std::env::temp_dir().join(format!("zcode-missing-{}.duckdb", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        let error = HistoryDuckDbRepository::open(&missing).expect_err("must fail");
        let message = error.to_string();
        assert!(
            message.contains("not found"),
            "错误信息必须明确提示缺失：{message}"
        );
    }
}
