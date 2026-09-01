use std::collections::HashMap;
use std::path::Path;

use devtoolbox_core::geography::{
    GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelation, GeoRelationKind, GeoSource,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{InfrastructureError, now_unix};

pub struct GeographyStore {
    connection: Connection,
}

impl GeographyStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        let mut store = Self { connection };
        store.ensure_schema()?;
        store.seed_if_empty()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS geo_entities (
                    id TEXT PRIMARY KEY, type TEXT NOT NULL, name TEXT NOT NULL,
                    name_en TEXT, aliases_json TEXT NOT NULL, coordinates_json TEXT,
                    geometry_json TEXT, parent_id TEXT, summary TEXT NOT NULL,
                    properties_json TEXT NOT NULL, source_ids_json TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS geo_relations (
                    from_id TEXT NOT NULL, to_id TEXT NOT NULL, kind TEXT NOT NULL,
                    note TEXT, source_ids_json TEXT NOT NULL,
                    PRIMARY KEY(from_id, to_id, kind)
                );
                CREATE TABLE IF NOT EXISTS geo_sources (
                    id TEXT PRIMARY KEY, payload_json TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS geo_favorites (
                    entity_id TEXT PRIMARY KEY REFERENCES geo_entities(id) ON DELETE CASCADE,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS geo_views (
                    entity_id TEXT PRIMARY KEY REFERENCES geo_entities(id) ON DELETE CASCADE,
                    viewed_at INTEGER NOT NULL, view_count INTEGER NOT NULL DEFAULT 1
                );
                CREATE TABLE IF NOT EXISTS geo_searches (
                    query TEXT PRIMARY KEY, searched_at INTEGER NOT NULL,
                    search_count INTEGER NOT NULL DEFAULT 1
                );
                CREATE INDEX IF NOT EXISTS idx_geo_entities_type ON geo_entities(type);
                CREATE INDEX IF NOT EXISTS idx_geo_views_recent ON geo_views(viewed_at DESC);",
            )
            .map_err(sqlite)
    }

    fn seed_if_empty(&mut self) -> Result<(), InfrastructureError> {
        let transaction = self.connection.transaction().map_err(sqlite)?;
        for source in seed_sources() {
            let payload = serde_json::to_string(&source).map_err(json_error)?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO geo_sources(id, payload_json) VALUES(?1, ?2)",
                    params![source.id, payload],
                )
                .map_err(sqlite)?;
        }
        for entity in seed_entities() {
            insert_entity(&transaction, &entity)?;
        }
        for relation in seed_relations() {
            let sources = serde_json::to_string(&relation.source_ids).map_err(json_error)?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO geo_relations VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![
                        relation.from_id,
                        relation.to_id,
                        relation_kind_name(relation.kind),
                        relation.note,
                        sources
                    ],
                )
                .map_err(sqlite)?;
        }
        transaction.commit().map_err(sqlite)
    }

    pub fn all_entities(&self) -> Result<Vec<GeoEntity>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, type, name, name_en, aliases_json, coordinates_json, geometry_json, parent_id, summary, properties_json, source_ids_json FROM geo_entities ORDER BY name")
            .map_err(sqlite)?;
        statement
            .query_map([], map_entity)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }

    pub fn entity(&self, id: &str) -> Result<Option<GeoEntity>, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT id, type, name, name_en, aliases_json, coordinates_json, geometry_json, parent_id, summary, properties_json, source_ids_json FROM geo_entities WHERE id = ?1",
                [id],
                map_entity,
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn search(
        &self,
        query: &str,
        entity_type: Option<GeoEntityType>,
        limit: usize,
    ) -> Result<Vec<GeoEntity>, InfrastructureError> {
        let needle = query.trim();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        self.connection
            .execute(
                "INSERT INTO geo_searches(query, searched_at, search_count) VALUES(?1, ?2, 1) ON CONFLICT(query) DO UPDATE SET searched_at = excluded.searched_at, search_count = search_count + 1",
                params![needle, now_unix()],
            )
            .map_err(sqlite)?;
        let lowered = needle.to_lowercase();
        let mut entities = self.all_entities()?;
        entities.retain(|entity| {
            entity_type.is_none_or(|kind| kind == entity.entity_type)
                && format!(
                    "{} {} {} {}",
                    entity.name,
                    entity.name_en.clone().unwrap_or_default(),
                    entity.aliases.join(" "),
                    entity.summary
                )
                .to_lowercase()
                .contains(&lowered)
        });
        entities.sort_by_key(|entity| {
            let exact = usize::from(
                entity.name.to_lowercase() == lowered
                    || entity
                        .name_en
                        .as_deref()
                        .is_some_and(|name| name.to_lowercase() == lowered),
            );
            (
                usize::MAX - exact,
                entity.entity_type.label(),
                entity.name.clone(),
            )
        });
        entities.truncate(limit);
        Ok(entities)
    }

    pub fn relations_for(&self, id: &str) -> Result<Vec<GeoRelation>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare("SELECT from_id, to_id, kind, note, source_ids_json FROM geo_relations WHERE from_id = ?1 OR to_id = ?1 ORDER BY kind")
            .map_err(sqlite)?;
        statement
            .query_map([id], |row| {
                Ok(GeoRelation {
                    from_id: row.get(0)?,
                    to_id: row.get(1)?,
                    kind: parse_relation_kind(&row.get::<_, String>(2)?)?,
                    note: row.get(3)?,
                    source_ids: decode(row.get(4)?)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }

    pub fn sources(&self, ids: &[String]) -> Result<Vec<GeoSource>, InfrastructureError> {
        let mut sources: Vec<GeoSource> = Vec::new();
        for id in ids {
            if let Some(source) = self
                .connection
                .query_row(
                    "SELECT payload_json FROM geo_sources WHERE id = ?1",
                    [id],
                    |row| decode(row.get(0)?),
                )
                .optional()
                .map_err(sqlite)?
            {
                sources.push(source);
            }
        }
        sources.sort_by(|left, right| left.dataset.cmp(&right.dataset));
        Ok(sources)
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, InfrastructureError> {
        let exists: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM geo_favorites WHERE entity_id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        if exists.is_some() {
            self.connection
                .execute("DELETE FROM geo_favorites WHERE entity_id = ?1", [id])
                .map_err(sqlite)?;
            Ok(false)
        } else {
            self.connection
                .execute(
                    "INSERT INTO geo_favorites(entity_id, created_at) VALUES(?1, ?2)",
                    params![id, now_unix()],
                )
                .map_err(sqlite)?;
            Ok(true)
        }
    }

    pub fn favorite_ids(&self) -> Result<Vec<String>, InfrastructureError> {
        ids(
            &self.connection,
            "SELECT entity_id FROM geo_favorites ORDER BY created_at DESC",
            None,
        )
    }

    pub fn recent_ids(&self, limit: i64) -> Result<Vec<String>, InfrastructureError> {
        ids(
            &self.connection,
            "SELECT entity_id FROM geo_views ORDER BY viewed_at DESC LIMIT ?1",
            Some(limit),
        )
    }

    pub fn record_view(&self, id: &str) -> Result<(), InfrastructureError> {
        self.connection.execute("INSERT INTO geo_views(entity_id, viewed_at, view_count) VALUES(?1, ?2, 1) ON CONFLICT(entity_id) DO UPDATE SET viewed_at = excluded.viewed_at, view_count = view_count + 1", params![id, now_unix()]).map_err(sqlite)?;
        Ok(())
    }

    pub fn map_snapshot(&self) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), InfrastructureError> {
        let entities = self.all_entities()?;
        let by_id: HashMap<_, _> = entities
            .iter()
            .map(|entity| (entity.id.as_str(), entity))
            .collect();
        let points = entities
            .iter()
            .filter_map(|entity| {
                entity.coordinates.map(|coordinate| GeoMapPoint {
                    entity_id: entity.id.clone(),
                    name: entity.name.clone(),
                    entity_type: entity.entity_type,
                    coordinate,
                })
            })
            .collect();
        let lines = self
            .all_relations()?
            .into_iter()
            .filter_map(|relation| {
                let from = by_id.get(relation.from_id.as_str())?.coordinates?;
                let to = by_id.get(relation.to_id.as_str())?.coordinates?;
                Some(GeoMapLine {
                    from_id: relation.from_id,
                    to_id: relation.to_id,
                    kind: relation.kind,
                    from,
                    to,
                })
            })
            .collect();
        Ok((points, lines))
    }

    fn all_relations(&self) -> Result<Vec<GeoRelation>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare("SELECT from_id, to_id, kind, note, source_ids_json FROM geo_relations")
            .map_err(sqlite)?;
        statement
            .query_map([], |row| {
                Ok(GeoRelation {
                    from_id: row.get(0)?,
                    to_id: row.get(1)?,
                    kind: parse_relation_kind(&row.get::<_, String>(2)?)?,
                    note: row.get(3)?,
                    source_ids: decode(row.get(4)?)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }
}

fn insert_entity(
    transaction: &rusqlite::Transaction<'_>,
    entity: &GeoEntity,
) -> Result<(), InfrastructureError> {
    transaction.execute("INSERT OR IGNORE INTO geo_entities VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)", params![entity.id, entity.entity_type.label(), entity.name, entity.name_en, encode(&entity.aliases)?, encode_opt(&entity.coordinates)?, encode_opt(&entity.geometry)?, entity.parent_id, entity.summary, encode(&entity.properties)?, encode(&entity.source_ids)?]).map_err(sqlite)?;
    Ok(())
}

fn map_entity(row: &rusqlite::Row<'_>) -> rusqlite::Result<GeoEntity> {
    Ok(GeoEntity {
        id: row.get(0)?,
        entity_type: parse_entity_type(&row.get::<_, String>(1)?)?,
        name: row.get(2)?,
        name_en: row.get(3)?,
        aliases: decode(row.get(4)?)?,
        coordinates: decode_opt(row.get(5)?)?,
        geometry: decode_opt(row.get(6)?)?,
        parent_id: row.get(7)?,
        summary: row.get(8)?,
        properties: decode(row.get(9)?)?,
        source_ids: decode(row.get(10)?)?,
    })
}

fn ids(
    connection: &Connection,
    query: &str,
    limit: Option<i64>,
) -> Result<Vec<String>, InfrastructureError> {
    let mut statement = connection.prepare(query).map_err(sqlite)?;
    let mut result = Vec::new();
    if let Some(limit) = limit {
        let rows = statement
            .query_map([limit], |row| row.get(0))
            .map_err(sqlite)?;
        for row in rows {
            result.push(row.map_err(sqlite)?);
        }
    } else {
        let rows = statement.query_map([], |row| row.get(0)).map_err(sqlite)?;
        for row in rows {
            result.push(row.map_err(sqlite)?);
        }
    }
    Ok(result)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String, InfrastructureError> {
    serde_json::to_string(value).map_err(json_error)
}
fn encode_opt<T: serde::Serialize>(
    value: &Option<T>,
) -> Result<Option<String>, InfrastructureError> {
    value.as_ref().map(encode).transpose()
}
fn decode<T: serde::de::DeserializeOwned>(value: String) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn decode_opt<T: serde::de::DeserializeOwned>(
    value: Option<String>,
) -> rusqlite::Result<Option<T>> {
    value.map(decode).transpose()
}
fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}
fn json_error(error: serde_json::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

fn parse_entity_type(value: &str) -> rusqlite::Result<GeoEntityType> {
    match value {
        "世界" => Ok(GeoEntityType::World),
        "国家" => Ok(GeoEntityType::Country),
        "区域" => Ok(GeoEntityType::Region),
        "省级行政区" => Ok(GeoEntityType::Province),
        "城市" => Ok(GeoEntityType::City),
        "河流" => Ok(GeoEntityType::River),
        "山体" => Ok(GeoEntityType::Mountain),
        "山脉" => Ok(GeoEntityType::MountainRange),
        "高原" => Ok(GeoEntityType::Plateau),
        "平原" => Ok(GeoEntityType::Plain),
        "盆地" => Ok(GeoEntityType::Basin),
        "沙漠" => Ok(GeoEntityType::Desert),
        "湖泊" => Ok(GeoEntityType::Lake),
        "海洋" => Ok(GeoEntityType::Ocean),
        "海域" => Ok(GeoEntityType::Sea),
        "岛屿" => Ok(GeoEntityType::Island),
        "群岛" => Ok(GeoEntityType::Archipelago),
        "气候区" => Ok(GeoEntityType::ClimateZone),
        "构造板块" => Ok(GeoEntityType::TectonicPlate),
        _ => Err(rusqlite::Error::InvalidColumnType(
            1,
            "type".into(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn relation_kind_name(kind: GeoRelationKind) -> &'static str {
    match kind {
        GeoRelationKind::LocatedIn => "LOCATED_IN",
        GeoRelationKind::PartOf => "PART_OF",
        GeoRelationKind::Borders => "BORDERS",
        GeoRelationKind::FlowsThrough => "FLOWS_THROUGH",
        GeoRelationKind::SourceOf => "SOURCE_OF",
        GeoRelationKind::FlowsInto => "FLOWS_INTO",
        GeoRelationKind::ConnectedTo => "CONNECTED_TO",
        GeoRelationKind::Near => "NEAR",
        GeoRelationKind::Crosses => "CROSSES",
        GeoRelationKind::Surrounds => "SURROUNDS",
        GeoRelationKind::Affects => "AFFECTS",
        GeoRelationKind::Influences => "INFLUENCES",
        GeoRelationKind::FormedBy => "FORMED_BY",
        GeoRelationKind::Causes => "CAUSES",
        GeoRelationKind::BelongsToClimateZone => "BELONGS_TO_CLIMATE_ZONE",
    }
}
fn parse_relation_kind(value: &str) -> rusqlite::Result<GeoRelationKind> {
    match value {
        "LOCATED_IN" => Ok(GeoRelationKind::LocatedIn),
        "PART_OF" => Ok(GeoRelationKind::PartOf),
        "BORDERS" => Ok(GeoRelationKind::Borders),
        "FLOWS_THROUGH" => Ok(GeoRelationKind::FlowsThrough),
        "SOURCE_OF" => Ok(GeoRelationKind::SourceOf),
        "FLOWS_INTO" => Ok(GeoRelationKind::FlowsInto),
        "CONNECTED_TO" => Ok(GeoRelationKind::ConnectedTo),
        "NEAR" => Ok(GeoRelationKind::Near),
        "CROSSES" => Ok(GeoRelationKind::Crosses),
        "SURROUNDS" => Ok(GeoRelationKind::Surrounds),
        "AFFECTS" => Ok(GeoRelationKind::Affects),
        "INFLUENCES" => Ok(GeoRelationKind::Influences),
        "FORMED_BY" => Ok(GeoRelationKind::FormedBy),
        "CAUSES" => Ok(GeoRelationKind::Causes),
        "BELONGS_TO_CLIMATE_ZONE" => Ok(GeoRelationKind::BelongsToClimateZone),
        _ => Err(rusqlite::Error::InvalidColumnType(
            2,
            "kind".into(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn coordinate(latitude: f64, longitude: f64) -> devtoolbox_core::geography::GeoCoordinate {
    devtoolbox_core::geography::GeoCoordinate {
        system: devtoolbox_core::geography::CoordinateSystem::WGS84,
        latitude,
        longitude,
    }
}
fn property(
    key: &str,
    label: &str,
    value: &str,
    unit: Option<&str>,
    sources: &[&str],
) -> devtoolbox_core::geography::GeoProperty {
    devtoolbox_core::geography::GeoProperty {
        key: key.into(),
        label: label.into(),
        value: value.into(),
        unit: unit.map(str::to_string),
        source_ids: sources.iter().map(|source| (*source).into()).collect(),
    }
}
#[allow(clippy::too_many_arguments)]
fn entity(
    id: &str,
    entity_type: GeoEntityType,
    name: &str,
    name_en: Option<&str>,
    aliases: &[&str],
    coordinates: Option<(f64, f64)>,
    parent_id: Option<&str>,
    summary: &str,
    properties: Vec<devtoolbox_core::geography::GeoProperty>,
    sources: &[&str],
) -> GeoEntity {
    GeoEntity {
        id: id.into(),
        entity_type,
        name: name.into(),
        name_en: name_en.map(str::to_string),
        aliases: aliases.iter().map(|item| (*item).into()).collect(),
        coordinates: coordinates.map(|(latitude, longitude)| coordinate(latitude, longitude)),
        geometry: None,
        parent_id: parent_id.map(str::to_string),
        properties,
        summary: summary.into(),
        source_ids: sources.iter().map(|source| (*source).into()).collect(),
    }
}
fn relation(
    from: &str,
    to: &str,
    kind: GeoRelationKind,
    note: &str,
    sources: &[&str],
) -> GeoRelation {
    GeoRelation {
        from_id: from.into(),
        to_id: to.into(),
        kind,
        note: Some(note.into()),
        source_ids: sources.iter().map(|source| (*source).into()).collect(),
    }
}

fn seed_sources() -> Vec<GeoSource> {
    vec![
        GeoSource {
            id: "natural-earth".into(),
            dataset: "Natural Earth".into(),
            version: "candidate / not bundled".into(),
            url: "https://www.naturalearthdata.com/about/terms-of-use/".into(),
            license: "Public domain".into(),
            updated_at: "2026-09-01".into(),
            fields: vec!["world overview".into()],
        },
        GeoSource {
            id: "geonames".into(),
            dataset: "GeoNames".into(),
            version: "candidate / not bundled".into(),
            url: "https://www.geonames.org/export/".into(),
            license: "CC BY 4.0".into(),
            updated_at: "2026-09-01".into(),
            fields: vec!["names".into(), "coordinates".into()],
        },
        GeoSource {
            id: "wikidata".into(),
            dataset: "Wikidata structured data".into(),
            version: "live source / seed reviewed".into(),
            url: "https://www.wikidata.org/wiki/Wikidata:Licensing".into(),
            license: "CC0".into(),
            updated_at: "2026-09-01".into(),
            fields: vec!["aliases".into(), "relations".into()],
        },
        GeoSource {
            id: "world-bank".into(),
            dataset: "World Bank Open Data".into(),
            version: "2023 indicators".into(),
            url: "https://datacatalog.worldbank.org/public-licenses".into(),
            license: "CC BY 4.0 (dataset default)".into(),
            updated_at: "2026-09-01".into(),
            fields: vec!["population".into()],
        },
        GeoSource {
            id: "geography-seed".into(),
            dataset: "Geography Explorer embedded seed".into(),
            version: "geography-seed-v1".into(),
            url: "https://github.com/wuhanwoaini521/self-tools".into(),
            license: "Project license; facts link to source datasets".into(),
            updated_at: "2026-09-01".into(),
            fields: vec!["curated summaries".into(), "exploration relations".into()],
        },
    ]
}

fn seed_entities() -> Vec<GeoEntity> {
    let s = ["geography-seed"];
    vec![
        entity(
            "world",
            GeoEntityType::World,
            "世界",
            Some("World"),
            &["Earth"],
            Some((20.0, 0.0)),
            None,
            "从自然地理到人文地理，世界是所有探索关系的起点。",
            vec![property(
                "terrain",
                "观察方式",
                "自然、人文与空间关系",
                None,
                &s,
            )],
            &s,
        ),
        entity(
            "china",
            GeoEntityType::Country,
            "中国",
            Some("China"),
            &["中华人民共和国"],
            Some((35.86, 104.2)),
            Some("world"),
            "中国的地形阶梯、季风水系与人口城市分布彼此相连。",
            vec![
                property(
                    "area",
                    "面积",
                    "9,596,960",
                    Some("km²"),
                    &["geography-seed"],
                ),
                property(
                    "population",
                    "人口",
                    "1,410,710,000",
                    Some("人 · 2023"),
                    &["world-bank"],
                ),
                property(
                    "population_density",
                    "人口密度",
                    "147",
                    Some("人/km² · 2023"),
                    &["world-bank"],
                ),
                property("terrain", "地形", "青藏高原—高原盆地—平原丘陵", None, &s),
                property(
                    "climate",
                    "气候",
                    "季风气候为主，南北东西差异显著",
                    None,
                    &s,
                ),
                property(
                    "major_cities",
                    "主要城市",
                    "北京、上海、成都、重庆、武汉",
                    None,
                    &s,
                ),
            ],
            &s,
        ),
        entity(
            "japan",
            GeoEntityType::Country,
            "日本",
            Some("Japan"),
            &["日本国"],
            Some((36.2, 138.25)),
            Some("world"),
            "日本是岛弧国家；板块边界、山地与沿海平原共同塑造了地震风险和城市分布。",
            vec![
                property("area", "面积", "377,975", Some("km²"), &["geography-seed"]),
                property(
                    "population",
                    "人口",
                    "124,516,000",
                    Some("人 · 2023"),
                    &["world-bank"],
                ),
                property(
                    "population_density",
                    "人口密度",
                    "330",
                    Some("人/km² · 2023"),
                    &["world-bank"],
                ),
                property("terrain", "地形", "山地为主，平原沿海分布", None, &s),
                property("climate", "气候", "南北跨度大，受季风与海洋影响", None, &s),
                property(
                    "major_cities",
                    "主要城市",
                    "东京、大阪、名古屋、福冈",
                    None,
                    &s,
                ),
            ],
            &s,
        ),
        entity(
            "sichuan",
            GeoEntityType::Province,
            "四川",
            Some("Sichuan"),
            &["四川省"],
            Some((30.65, 102.7)),
            Some("china"),
            "四川位于中国西南，盆地、山地、河流与季风共同解释了这里的自然和人口格局。",
            vec![
                property("terrain", "地形", "四川盆地与周缘山地", None, &s),
                property(
                    "climate",
                    "气候",
                    "湿润、多云，盆地内水热条件较好",
                    None,
                    &s,
                ),
                property(
                    "major_cities",
                    "主要城市",
                    "成都、绵阳、乐山、宜宾",
                    None,
                    &s,
                ),
            ],
            &s,
        ),
        entity(
            "chengdu",
            GeoEntityType::City,
            "成都",
            Some("Chengdu"),
            &["成都市"],
            Some((30.57, 104.07)),
            Some("sichuan"),
            "成都位于成都平原；盆地地形、河流灌溉和稳定农业基础共同支撑了城市发展。",
            vec![
                property("elevation", "海拔", "约 500", Some("m"), &s),
                property("terrain", "地形", "成都平原，位于四川盆地西部", None, &s),
                property("climate", "气候", "亚热带湿润季风气候，多云雾", None, &s),
                property(
                    "population",
                    "人口",
                    "21,400,000",
                    Some("人 · 2023口径"),
                    &["world-bank"],
                ),
                property("river_system", "水系", "都江堰—岷江水系", None, &s),
            ],
            &s,
        ),
        entity(
            "chongqing",
            GeoEntityType::City,
            "重庆",
            Some("Chongqing"),
            &["重庆市"],
            Some((29.56, 106.55)),
            Some("china"),
            "重庆处在长江与嘉陵江交汇区域，山地河谷塑造了立体城市结构。",
            vec![
                property("elevation", "海拔", "约 260", Some("m"), &s),
                property("terrain", "地形", "山地与河谷", None, &s),
                property("river_system", "水系", "长江与嘉陵江交汇", None, &s),
                property(
                    "population",
                    "人口",
                    "32,100,000",
                    Some("人 · 2023口径"),
                    &["world-bank"],
                ),
            ],
            &s,
        ),
        entity(
            "shanghai",
            GeoEntityType::City,
            "上海",
            Some("Shanghai"),
            &["上海市"],
            Some((31.23, 121.47)),
            Some("china"),
            "上海位于长江入海口和长江三角洲，河口、航运与平原共同推动了城市集聚。",
            vec![
                property("terrain", "地形", "长江三角洲冲积平原", None, &s),
                property("river_system", "水系", "长江入海口", None, &s),
                property(
                    "population",
                    "人口",
                    "24,890,000",
                    Some("人 · 2023口径"),
                    &["world-bank"],
                ),
            ],
            &s,
        ),
        entity(
            "yangtze",
            GeoEntityType::River,
            "长江",
            Some("Yangtze River"),
            &["扬子江"],
            Some((30.0, 112.0)),
            Some("china"),
            "长江连接高原、盆地、平原和海洋，是理解中国人口与城市空间关系的一条主线。",
            vec![
                property("length", "长度", "约 6,300", Some("km"), &s),
                property("source_region", "源头区域", "青藏高原腹地", None, &s),
                property(
                    "major_cities",
                    "沿线城市",
                    "重庆、武汉、南京、上海",
                    None,
                    &s,
                ),
                property("terrain", "流经地形", "高原、峡谷、盆地、平原", None, &s),
            ],
            &s,
        ),
        entity(
            "tibetan-plateau",
            GeoEntityType::Plateau,
            "青藏高原",
            Some("Tibetan Plateau"),
            &["世界屋脊"],
            Some((33.0, 88.0)),
            Some("china"),
            "青藏高原的高海拔来自印度板块与欧亚板块持续碰撞，是亚洲许多大河和季风过程的重要背景。",
            vec![
                property("elevation", "平均海拔", "超过 4,000", Some("m"), &s),
                property("terrain", "地形", "高原与高山系统", None, &s),
                property("climate", "气候", "高寒，影响亚洲季风与水汽输送", None, &s),
            ],
            &s,
        ),
        entity(
            "sichuan-basin",
            GeoEntityType::Basin,
            "四川盆地",
            Some("Sichuan Basin"),
            &["红盆地"],
            Some((30.5, 105.5)),
            Some("china"),
            "周缘山地限制气流交换，盆地内水汽与云雾更易积聚；平原与河流又提供了稳定的人类生存条件。",
            vec![
                property("terrain", "地形", "盆地，西部连接成都平原", None, &s),
                property("climate", "气候", "湿润、多云雾", None, &s),
                property(
                    "population",
                    "人口关系",
                    "平原与水系支持高密度聚居",
                    None,
                    &s,
                ),
            ],
            &s,
        ),
        entity(
            "himalayas",
            GeoEntityType::MountainRange,
            "喜马拉雅山脉",
            Some("Himalayas"),
            &["喜马拉雅山"],
            Some((28.0, 84.0)),
            Some("world"),
            "喜马拉雅山脉是印度板块向北与欧亚板块碰撞的地表结果之一，也构成亚洲重要的地形与气候屏障。",
            vec![
                property("elevation", "最高峰", "珠穆朗玛峰约 8,849", Some("m"), &s),
                property("formed_by", "形成过程", "印度板块与欧亚板块碰撞", None, &s),
                property("climate", "气候影响", "改变水汽输送与季风分布", None, &s),
            ],
            &s,
        ),
        entity(
            "qinling",
            GeoEntityType::MountainRange,
            "秦岭",
            Some("Qinling Mountains"),
            &["秦岭山脉"],
            Some((33.8, 108.0)),
            Some("china"),
            "秦岭横贯中国中部，是南北气候、植被和水系差异的重要地理分界。",
            vec![
                property(
                    "climate",
                    "气候影响",
                    "中国南北气候分界的重要山脉",
                    None,
                    &s,
                ),
                property("river_system", "水系", "长江与黄河流域分水背景", None, &s),
            ],
            &s,
        ),
        entity(
            "northeast-china",
            GeoEntityType::Region,
            "中国东北",
            Some("Northeast China"),
            &["东北"],
            Some((45.7, 126.6)),
            Some("china"),
            "东北平原、黑土地、较高纬度和季节性寒冷共同塑造了农业与人口分布。",
            vec![
                property("terrain", "地形", "东北平原与大小兴安岭", None, &s),
                property("climate", "气候", "温带季风，冬季寒冷", None, &s),
                property(
                    "major_cities",
                    "主要城市",
                    "哈尔滨、长春、沈阳、大连",
                    None,
                    &s,
                ),
            ],
            &s,
        ),
        entity(
            "japan-plate",
            GeoEntityType::TectonicPlate,
            "太平洋板块",
            Some("Pacific Plate"),
            &[],
            Some((30.0, 160.0)),
            Some("world"),
            "太平洋板块与日本附近其他板块的相互作用，是解释日本地震与火山活动的重要背景。",
            vec![property(
                "terrain",
                "地质背景",
                "板块边界活动显著",
                None,
                &s,
            )],
            &s,
        ),
    ]
}

fn seed_relations() -> Vec<GeoRelation> {
    let s = ["geography-seed"];
    vec![
        relation(
            "china",
            "world",
            GeoRelationKind::PartOf,
            "中国是世界地理探索中的国家实体。",
            &s,
        ),
        relation(
            "japan",
            "world",
            GeoRelationKind::PartOf,
            "日本是东亚岛弧国家。",
            &s,
        ),
        relation(
            "sichuan",
            "china",
            GeoRelationKind::PartOf,
            "四川是中国的省级行政区。",
            &s,
        ),
        relation(
            "chengdu",
            "sichuan",
            GeoRelationKind::LocatedIn,
            "成都位于四川盆地西部的成都平原。",
            &s,
        ),
        relation(
            "chongqing",
            "china",
            GeoRelationKind::LocatedIn,
            "重庆位于长江上游河谷区域。",
            &s,
        ),
        relation(
            "shanghai",
            "china",
            GeoRelationKind::LocatedIn,
            "上海位于长江三角洲和入海口。",
            &s,
        ),
        relation(
            "sichuan-basin",
            "china",
            GeoRelationKind::PartOf,
            "四川盆地是中国西南的重要地貌单元。",
            &s,
        ),
        relation(
            "sichuan-basin",
            "himalayas",
            GeoRelationKind::Surrounds,
            "周缘山地共同影响盆地气流和水汽。",
            &s,
        ),
        relation(
            "yangtze",
            "tibetan-plateau",
            GeoRelationKind::SourceOf,
            "长江源区位于青藏高原腹地。",
            &s,
        ),
        relation(
            "yangtze",
            "chongqing",
            GeoRelationKind::FlowsThrough,
            "长江穿过重庆并与嘉陵江交汇。",
            &s,
        ),
        relation(
            "yangtze",
            "shanghai",
            GeoRelationKind::FlowsInto,
            "长江在上海附近汇入东海。",
            &s,
        ),
        relation(
            "yangtze",
            "qinling",
            GeoRelationKind::Near,
            "秦岭提供长江流域北侧的分水背景。",
            &s,
        ),
        relation(
            "tibetan-plateau",
            "himalayas",
            GeoRelationKind::ConnectedTo,
            "青藏高原与喜马拉雅山脉共同构成高亚洲地形系统。",
            &s,
        ),
        relation(
            "himalayas",
            "japan-plate",
            GeoRelationKind::Affects,
            "亚洲板块构造格局是理解东亚地质活动的背景。",
            &s,
        ),
        relation(
            "japan",
            "japan-plate",
            GeoRelationKind::Influences,
            "日本位于多个活跃板块边界附近。",
            &s,
        ),
        relation(
            "northeast-china",
            "china",
            GeoRelationKind::PartOf,
            "中国东北是具有地理意义的区域，而非行政层级。",
            &s,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{GeoEntityType, GeographyStore};

    #[test]
    fn seeds_entities_relations_and_sources_offline() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = GeographyStore::open(directory.path().join("geography.db")).expect("store");
        assert!(store.entity("chengdu").expect("entity").is_some());
        let search = store.search("长江", None, 30).expect("search");
        assert_eq!(
            search.first().map(|entity| entity.id.as_str()),
            Some("yangtze")
        );
        assert!(search.len() >= 3, "related cities remain discoverable");
        assert_eq!(
            store
                .search("YANGTZE RIVER", None, 30)
                .expect("english search")
                .first()
                .map(|entity| entity.id.as_str()),
            Some("yangtze")
        );
        assert_eq!(
            store
                .search("扬子江", Some(GeoEntityType::River), 1)
                .expect("alias search")
                .len(),
            1
        );
        assert!(
            store
                .search("不存在的地方", None, 30)
                .expect("empty search")
                .is_empty()
        );
        assert_eq!(store.search("中", None, 2).expect("pagination").len(), 2);
        assert!(
            !store
                .relations_for("yangtze")
                .expect("relations")
                .is_empty()
        );
        let snapshot = store.map_snapshot().expect("map");
        assert!(snapshot.0.len() > 5 && !snapshot.1.is_empty());
    }

    #[test]
    fn favorite_and_recent_are_persistent() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = GeographyStore::open(directory.path().join("geography.db")).expect("store");
        assert!(store.toggle_favorite("china").expect("favorite"));
        assert_eq!(store.favorite_ids().expect("favorites"), vec!["china"]);
        store.record_view("chengdu").expect("view");
        assert_eq!(store.recent_ids(5).expect("recent"), vec!["chengdu"]);
    }
}
