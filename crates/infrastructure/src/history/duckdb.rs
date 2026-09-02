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

    pub fn get_person(&self, name: &str) -> Result<Option<PersonResult>, InfrastructureError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id,canonical_name_zh_cn,name_raw,birth_year,death_year,gender,quality_status,created_from_source
                     FROM people WHERE canonical_name_zh_cn = ?1 OR search_name = ?1 ORDER BY id LIMIT 1",
                )
                .map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let mut rows = statement
                .query(params![name])
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
                "SELECT id,canonical_name_zh_cn,name_raw,birth_year,death_year,gender,quality_status,created_from_source
                 FROM people WHERE canonical_name_zh_cn LIKE ?1 OR search_name LIKE ?1 OR search_text LIKE ?1
                 ORDER BY canonical_name_zh_cn,id LIMIT ?2",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![format!("%{query}%"), limit.max(1).min(100)], |row| {
                Ok(PersonResult {
                    id: row.get(0)?, canonical_name_zh_cn: row.get(1)?, name_raw: row.get(2)?,
                    birth_year: row.get(3)?, death_year: row.get(4)?, gender: row.get(5)?,
                    quality_status: row.get(6)?, created_from_source: row.get(7)?,
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
                        pr.relation_type,pr.start_year,pr.end_year,pr.source_ids,pr.confidence
                 FROM person_relations pr LEFT JOIN people a ON a.id=pr.person_a_id LEFT JOIN people b ON b.id=pr.person_b_id
                 WHERE pr.person_a_id=?1 OR pr.person_b_id=?1 ORDER BY pr.relation_type,pr.person_a_id,pr.person_b_id",
            ).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![person_id], |row| Ok(PersonRelationResult {
                person_a_id: row.get(0)?, person_a_name: row.get(1)?, person_b_id: row.get(2)?, person_b_name: row.get(3)?,
                relation_type: row.get(4)?, start_year: row.get(5)?, end_year: row.get(6)?, source_ids: row.get(7)?, confidence: row.get(8)?,
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
                    params![format!("%{title}%"), limit.max(1).min(100)],
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
                              ht.translation_zh_cn,ht.translation_source,ht.alignment_quality,ht.source_id
                       FROM historical_texts ht LEFT JOIN works w ON w.id=ht.book_id
                       WHERE (?1 IS NULL OR w.title LIKE ?2 OR w.title_zh_cn LIKE ?2)
                       ORDER BY ht.title_zh_cn,ht.chapter,ht.id LIMIT ?3";
            let pattern = work.map(|value| format!("%{value}%"));
            let mut statement = connection.prepare(sql).map_err(|error| InfrastructureError::DuckDb(error.to_string()))?;
            let rows = statement.query_map(params![work, pattern, limit.max(1).min(100)], |row| Ok(HistoricalTextResult {
                id: row.get(0)?, title_zh_cn: row.get(1)?, book_id: row.get(2)?, work_title: row.get(3)?, chapter: row.get(4)?,
                original_text: row.get(5)?, original_simplified: row.get(6)?, translation_zh_cn: row.get(7)?, translation_source: row.get(8)?,
                alignment_quality: row.get(9)?, source_id: row.get(10)?,
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
