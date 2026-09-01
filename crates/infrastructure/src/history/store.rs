//! History V1 的离线优先 SQLite 存储。

use std::path::Path;

use devtoolbox_core::history::{
    Dynasty, HistoricalArtifact, HistoricalEvent, HistoricalInstitution, HistoricalPerson,
    HistoricalPlace, HistoryDetail, HistoryDocument, HistoryFact, HistoryNode, HistoryNodeKind,
    HistoryPeriod, HistoryRelation, HistoryRelationKind, HistorySection, HistorySource,
    SourceAuthority,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{InfrastructureError, now_unix};

pub struct HistoryStore {
    connection: Connection,
}

impl HistoryStore {
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
        store.sync_seed_data()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS history_periods (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, start_year INTEGER NOT NULL,
                end_year INTEGER NOT NULL, summary TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history_nodes (
                id TEXT PRIMARY KEY, kind TEXT NOT NULL, title TEXT NOT NULL, period_id TEXT,
                start_year INTEGER, end_year INTEGER, summary TEXT NOT NULL,
                tags_json TEXT NOT NULL, source_ids_json TEXT NOT NULL, detail_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history_relations (
                from_id TEXT NOT NULL, to_id TEXT NOT NULL, kind TEXT NOT NULL, note TEXT,
                PRIMARY KEY(from_id, to_id, kind)
            );
            CREATE TABLE IF NOT EXISTS history_sources (
                id TEXT PRIMARY KEY, payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history_facts (
                subject_id TEXT NOT NULL, predicate TEXT NOT NULL, value TEXT NOT NULL,
                source_ids_json TEXT NOT NULL, PRIMARY KEY(subject_id, predicate, value)
            );
            CREATE TABLE IF NOT EXISTS history_favorites (
                node_id TEXT PRIMARY KEY REFERENCES history_nodes(id) ON DELETE CASCADE,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history_views (
                node_id TEXT PRIMARY KEY REFERENCES history_nodes(id) ON DELETE CASCADE,
                viewed_at INTEGER NOT NULL, view_count INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS history_searches (
                query TEXT PRIMARY KEY, searched_at INTEGER NOT NULL, search_count INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_history_nodes_time ON history_nodes(start_year, end_year);
            CREATE INDEX IF NOT EXISTS idx_history_views_recent ON history_views(viewed_at DESC);",
        ).map_err(|error| InfrastructureError::Sqlite(error.to_string()))
    }

    fn sync_seed_data(&mut self) -> Result<(), InfrastructureError> {
        // 内置资料按稳定 id 升级；收藏、浏览和搜索记录保存在独立表，
        // 因此资料修订不会影响用户状态。
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        for period in seed_periods() {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO history_periods VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        period.id,
                        period.name,
                        period.start_year,
                        period.end_year,
                        period.summary
                    ],
                )
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        }
        for source in seed_sources() {
            let payload = serde_json::to_string(&source)
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO history_sources VALUES (?1, ?2)",
                    params![source.id, payload],
                )
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        }
        for document in seed_documents() {
            insert_document(&transaction, &document)?;
        }
        for relation in seed_relations() {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO history_relations VALUES (?1, ?2, ?3, ?4)",
                    params![
                        relation.from_id,
                        relation.to_id,
                        relation_kind_name(relation.kind),
                        relation.note
                    ],
                )
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        }
        for fact in seed_facts() {
            let source_ids = serde_json::to_string(&fact.source_ids)
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO history_facts VALUES (?1, ?2, ?3, ?4)",
                    params![fact.subject_id, fact.predicate, fact.value, source_ids],
                )
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        }
        transaction
            .commit()
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))
    }

    pub fn periods(&self) -> Result<Vec<HistoryPeriod>, InfrastructureError> {
        let mut statement = self.connection.prepare("SELECT id, name, start_year, end_year, summary FROM history_periods ORDER BY start_year")
            .map_err(sqlite)?;
        statement
            .query_map([], |row| {
                Ok(HistoryPeriod {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    start_year: row.get(2)?,
                    end_year: row.get(3)?,
                    summary: row.get(4)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }

    pub fn all_nodes(&self) -> Result<Vec<HistoryNode>, InfrastructureError> {
        let mut statement = self.connection.prepare("SELECT id, kind, title, period_id, start_year, end_year, summary, tags_json, source_ids_json FROM history_nodes")
            .map_err(sqlite)?;
        statement
            .query_map([], map_node)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }

    /// 某时代（period_id）下的全部节点，按时间排序，供「认识这个时代」等探索入口使用。
    pub fn period_nodes(&self, period_id: &str) -> Result<Vec<HistoryNode>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, kind, title, period_id, start_year, end_year, summary, tags_json, source_ids_json FROM history_nodes WHERE period_id = ?1 ORDER BY COALESCE(start_year, 0)")
            .map_err(sqlite)?;
        statement
            .query_map([period_id], map_node)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }

    pub fn search(&self, query: &str) -> Result<Vec<HistoryNode>, InfrastructureError> {
        let needle = query.trim();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        self.record_search(needle)?;
        let nodes = self.all_nodes()?;
        let lowered = needle.to_lowercase();
        Ok(nodes
            .into_iter()
            .filter(|node| {
                let years = format!(
                    "{} {}",
                    node.start_year.unwrap_or_default(),
                    node.end_year.unwrap_or_default()
                );
                let searchable = format!(
                    "{} {} {} {}",
                    node.title,
                    node.summary,
                    node.tags.join(" "),
                    years
                )
                .to_lowercase();
                searchable.contains(&lowered)
            })
            .collect())
    }

    pub fn document(&self, id: &str) -> Result<Option<HistoryDocument>, InfrastructureError> {
        self.connection.query_row(
            "SELECT id, kind, title, period_id, start_year, end_year, summary, tags_json, source_ids_json, detail_json FROM history_nodes WHERE id = ?1",
            [id], |row| {
                let node = map_node(row)?;
                let detail: HistoryDetail = decode(row.get::<_, String>(9)?)?;
                Ok(HistoryDocument { node, detail })
            }).optional().map_err(sqlite)
    }

    pub fn relations_for(
        &self,
        node_id: &str,
    ) -> Result<Vec<HistoryRelation>, InfrastructureError> {
        let mut statement = self.connection.prepare(
            "SELECT from_id, to_id, kind, note FROM history_relations WHERE from_id = ?1 OR to_id = ?1")
            .map_err(sqlite)?;
        statement
            .query_map([node_id], |row| {
                Ok(HistoryRelation {
                    from_id: row.get(0)?,
                    to_id: row.get(1)?,
                    kind: parse_relation_kind(&row.get::<_, String>(2)?)?,
                    note: row.get(3)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }

    pub fn sources(&self, ids: &[String]) -> Result<Vec<HistorySource>, InfrastructureError> {
        let mut sources: Vec<_> = ids
            .iter()
            .map(|id| {
                self.connection
                    .query_row(
                        "SELECT payload_json FROM history_sources WHERE id = ?1",
                        [id],
                        |row| decode(row.get::<_, String>(0)?),
                    )
                    .optional()
                    .map_err(sqlite)
            })
            .collect::<Result<Vec<Option<HistorySource>>, _>>()
            .map(|items| items.into_iter().flatten().collect())?;
        sources.sort_by(|left, right| {
            left.authority
                .cmp(&right.authority)
                .then_with(|| left.title.cmp(&right.title))
        });
        Ok(sources)
    }

    pub fn toggle_favorite(&self, node_id: &str) -> Result<bool, InfrastructureError> {
        let exists: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM history_favorites WHERE node_id = ?1",
                [node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        if exists.is_some() {
            self.connection
                .execute(
                    "DELETE FROM history_favorites WHERE node_id = ?1",
                    [node_id],
                )
                .map_err(sqlite)?;
            Ok(false)
        } else {
            self.connection
                .execute(
                    "INSERT INTO history_favorites(node_id, created_at) VALUES(?1, ?2)",
                    params![node_id, now_unix()],
                )
                .map_err(sqlite)?;
            Ok(true)
        }
    }

    pub fn favorite_ids(&self) -> Result<Vec<String>, InfrastructureError> {
        self.ids("SELECT node_id FROM history_favorites ORDER BY created_at DESC")
    }
    pub fn recent_ids(&self, limit: i64) -> Result<Vec<String>, InfrastructureError> {
        self.ids_with_limit(
            "SELECT node_id FROM history_views ORDER BY viewed_at DESC LIMIT ?1",
            limit,
        )
    }
    pub fn recent_nodes(&self, limit: i64) -> Result<Vec<HistoryNode>, InfrastructureError> {
        let ids = self.recent_ids(limit)?;
        let nodes = self.all_nodes()?;
        Ok(ids
            .into_iter()
            .filter_map(|id| nodes.iter().find(|node| node.id == id).cloned())
            .collect())
    }
    fn ids(&self, query: &str) -> Result<Vec<String>, InfrastructureError> {
        self.connection
            .prepare(query)
            .map_err(sqlite)?
            .query_map([], |row| row.get(0))
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }
    fn ids_with_limit(&self, query: &str, limit: i64) -> Result<Vec<String>, InfrastructureError> {
        self.connection
            .prepare(query)
            .map_err(sqlite)?
            .query_map([limit], |row| row.get(0))
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
    }
    pub fn record_view(&self, node_id: &str) -> Result<(), InfrastructureError> {
        self.connection.execute("INSERT INTO history_views(node_id, viewed_at, view_count) VALUES(?1, ?2, 1) ON CONFLICT(node_id) DO UPDATE SET viewed_at = excluded.viewed_at, view_count = view_count + 1", params![node_id, now_unix()]).map_err(sqlite)?;
        Ok(())
    }
    fn record_search(&self, query: &str) -> Result<(), InfrastructureError> {
        self.connection.execute("INSERT INTO history_searches(query, searched_at, search_count) VALUES(?1, ?2, 1) ON CONFLICT(query) DO UPDATE SET searched_at = excluded.searched_at, search_count = search_count + 1", params![query, now_unix()]).map_err(sqlite)?;
        Ok(())
    }
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
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
fn map_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryNode> {
    Ok(HistoryNode {
        id: row.get(0)?,
        kind: parse_node_kind(&row.get::<_, String>(1)?)?,
        title: row.get(2)?,
        period_id: row.get(3)?,
        start_year: row.get(4)?,
        end_year: row.get(5)?,
        summary: row.get(6)?,
        tags: decode(row.get(7)?)?,
        source_ids: decode(row.get(8)?)?,
    })
}
fn insert_document(
    transaction: &rusqlite::Transaction<'_>,
    document: &HistoryDocument,
) -> Result<(), InfrastructureError> {
    let node = &document.node;
    let tags = serde_json::to_string(&node.tags)
        .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
    let sources = serde_json::to_string(&node.source_ids)
        .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
    let detail = serde_json::to_string(&document.detail)
        .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
    transaction
        .execute(
            "INSERT INTO history_nodes VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                title = excluded.title,
                period_id = excluded.period_id,
                start_year = excluded.start_year,
                end_year = excluded.end_year,
                summary = excluded.summary,
                tags_json = excluded.tags_json,
                source_ids_json = excluded.source_ids_json,
                detail_json = excluded.detail_json",
            params![
                node.id,
                node_kind_name(node.kind),
                node.title,
                node.period_id,
                node.start_year,
                node.end_year,
                node.summary,
                tags,
                sources,
                detail
            ],
        )
        .map_err(sqlite)?;
    Ok(())
}
fn node_kind_name(kind: HistoryNodeKind) -> &'static str {
    match kind {
        HistoryNodeKind::Person => "person",
        HistoryNodeKind::Event => "event",
        HistoryNodeKind::Dynasty => "dynasty",
        HistoryNodeKind::Place => "place",
        HistoryNodeKind::War => "war",
        HistoryNodeKind::Institution => "institution",
        HistoryNodeKind::Artifact => "artifact",
        HistoryNodeKind::Culture => "culture",
    }
}
fn parse_node_kind(value: &str) -> rusqlite::Result<HistoryNodeKind> {
    match value {
        "person" => Ok(HistoryNodeKind::Person),
        "event" => Ok(HistoryNodeKind::Event),
        "dynasty" => Ok(HistoryNodeKind::Dynasty),
        "place" => Ok(HistoryNodeKind::Place),
        "war" => Ok(HistoryNodeKind::War),
        "institution" => Ok(HistoryNodeKind::Institution),
        "artifact" => Ok(HistoryNodeKind::Artifact),
        "culture" => Ok(HistoryNodeKind::Culture),
        _ => Err(rusqlite::Error::InvalidColumnType(
            1,
            "kind".into(),
            rusqlite::types::Type::Text,
        )),
    }
}
fn relation_kind_name(kind: HistoryRelationKind) -> &'static str {
    match kind {
        HistoryRelationKind::OccurredIn => "occurred_in",
        HistoryRelationKind::ParticipatedIn => "participated_in",
        HistoryRelationKind::BelongsTo => "belongs_to",
        HistoryRelationKind::Cause => "cause",
        HistoryRelationKind::Consequence => "consequence",
        HistoryRelationKind::RelatedTo => "related_to",
        HistoryRelationKind::Family => "family",
        HistoryRelationKind::PoliticalAlly => "political_ally",
        HistoryRelationKind::PoliticalOpponent => "political_opponent",
        HistoryRelationKind::MonarchMinister => "monarch_minister",
        HistoryRelationKind::MilitaryOpponent => "military_opponent",
        HistoryRelationKind::Predecessor => "predecessor",
        HistoryRelationKind::Successor => "successor",
    }
}
fn parse_relation_kind(value: &str) -> rusqlite::Result<HistoryRelationKind> {
    match value {
        "occurred_in" => Ok(HistoryRelationKind::OccurredIn),
        "participated_in" => Ok(HistoryRelationKind::ParticipatedIn),
        "belongs_to" => Ok(HistoryRelationKind::BelongsTo),
        "cause" => Ok(HistoryRelationKind::Cause),
        "consequence" => Ok(HistoryRelationKind::Consequence),
        "related_to" => Ok(HistoryRelationKind::RelatedTo),
        "family" => Ok(HistoryRelationKind::Family),
        "political_ally" => Ok(HistoryRelationKind::PoliticalAlly),
        "political_opponent" => Ok(HistoryRelationKind::PoliticalOpponent),
        "monarch_minister" => Ok(HistoryRelationKind::MonarchMinister),
        "military_opponent" => Ok(HistoryRelationKind::MilitaryOpponent),
        "predecessor" => Ok(HistoryRelationKind::Predecessor),
        "successor" => Ok(HistoryRelationKind::Successor),
        _ => Err(rusqlite::Error::InvalidColumnType(
            2,
            "kind".into(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn period(id: &str, name: &str, start_year: i32, end_year: i32) -> HistoryPeriod {
    HistoryPeriod {
        id: id.into(),
        name: name.into(),
        start_year,
        end_year,
        summary: format!("{name}（{start_year} 至 {end_year}）"),
    }
}
fn seed_periods() -> Vec<HistoryPeriod> {
    vec![
        period("xia", "夏", -2070, -1600),
        period("shang", "商", -1600, -1046),
        period("western-zhou", "西周", -1046, -771),
        period("spring-autumn", "春秋", -770, -476),
        period("warring-states", "战国", -475, -221),
        period("qin", "秦", -221, -206),
        period("western-han", "西汉", -202, 8),
        period("eastern-han", "东汉", 25, 220),
        period("three-kingdoms", "三国", 220, 280),
        period("western-jin", "西晋", 266, 316),
        period("eastern-jin", "东晋", 317, 420),
        period("northern-southern", "南北朝", 420, 589),
        period("sui", "隋", 581, 618),
        period("tang", "唐", 618, 907),
        period("five-dynasties", "五代十国", 907, 979),
        period("northern-song", "北宋", 960, 1127),
        period("liao", "辽", 916, 1125),
        period("western-xia", "西夏", 1038, 1227),
        period("southern-song", "南宋", 1127, 1279),
        period("jin", "金", 1115, 1234),
        period("yuan", "元", 1271, 1368),
        period("ming", "明", 1368, 1644),
        period("qing", "清", 1636, 1912),
        period("modern", "近现代", 1912, 2026),
    ]
}
fn source(id: &str, title: &str, url: &str, authority: SourceAuthority) -> HistorySource {
    HistorySource {
        id: id.into(),
        title: title.into(),
        url: url.into(),
        source_type: "公开资料".into(),
        authority,
        published_at: None,
        fetched_at: None,
    }
}
fn seed_sources() -> Vec<HistorySource> {
    vec![
        source(
            "museum",
            "中国国家博物馆",
            "https://www.chnmuseum.cn/",
            SourceAuthority::Museum,
        ),
        source(
            "palace",
            "故宫博物院",
            "https://www.dpm.org.cn/",
            SourceAuthority::Museum,
        ),
        source(
            "baike",
            "中国大百科全书数据库",
            "https://www.zgbk.com/",
            SourceAuthority::Reference,
        ),
        source(
            "academic",
            "中国社会科学院历史理论研究所",
            "http://iht.cssn.cn/",
            SourceAuthority::Academic,
        ),
    ]
}
#[allow(clippy::too_many_arguments)]
fn node(
    id: &str,
    kind: HistoryNodeKind,
    title: &str,
    period: &str,
    start: i32,
    end: Option<i32>,
    summary: &str,
    tags: &[&str],
) -> HistoryNode {
    HistoryNode {
        id: id.into(),
        kind,
        title: title.into(),
        period_id: Some(period.into()),
        start_year: Some(start),
        end_year: end,
        summary: summary.into(),
        tags: tags.iter().map(|item| (*item).into()).collect(),
        source_ids: vec!["baike".into(), "academic".into()],
    }
}

fn era_topic(
    id: &str,
    name: &str,
    start: i32,
    end: i32,
    overview: &str,
    key_points: &[&str],
) -> HistoryDocument {
    HistoryDocument {
        node: node(
            id,
            HistoryNodeKind::Dynasty,
            name,
            id,
            start,
            Some(end),
            overview,
            &[name],
        ),
        detail: HistoryDetail::Topic {
            overview: overview.into(),
            key_points: key_points.iter().map(|point| (*point).into()).collect(),
        },
    }
}
fn seed_documents() -> Vec<HistoryDocument> {
    vec![
        era_topic("xia", "夏", -2070, -1600, "传统史学中的早期王朝，是探索早期国家形成的重要起点。", &["二里头遗址是理解早期国家形态的重要材料。", "文献与考古的对应关系仍在持续研究。"]),
        era_topic("shang", "商", -1600, -1046, "以甲骨文和青铜器闻名的早期王朝。", &["甲骨文为研究商代社会提供一手材料。", "青铜礼器反映政治与礼制结构。"]),
        era_topic("western-zhou", "西周", -1046, -771, "以宗法、分封与礼乐制度为主要特征的王朝。", &["分封与宗法共同维系政治秩序。", "王室东迁后进入东周时期。"]),
        era_topic("spring-autumn", "春秋", -770, -476, "诸侯争霸加剧、旧秩序不断调整的时期。", &["周王室权威下降，诸侯国力量上升。", "外交、战争与礼制均在变化。"]),
        era_topic("warring-states", "战国", -475, -221, "七雄竞争与制度创新并行，为统一帝国准备了条件。", &["变法推动军政与经济能力重组。", "诸子百家形成重要思想传统。"]),
        HistoryDocument { node: node("qin", HistoryNodeKind::Dynasty, "秦朝", "qin", -221, Some(-206), "中国首个完成大一统的中央集权王朝。", &["秦始皇", "郡县制", "统一六国"]), detail: HistoryDetail::Dynasty(Dynasty { name: "秦朝".into(), start_year: -221, end_year: -206, capital: Some("咸阳".into()), regime_type: "统一王朝".into(), overview: "秦以郡县制、统一文字度量衡等措施奠定大一统治理的制度基础。".into(), sections: vec![] }) },
        era_topic("western-han", "西汉", -202, 8, "汉初在秦制基础上调整，逐步形成稳定的大一统治理。", &["郡国并行是早期重要制度特征。", "与西域联系的开拓影响深远。"]),
        era_topic("eastern-han", "东汉", 25, 220, "东汉后期地方势力与政治矛盾加深，最终走向分裂。", &["外戚、宦官与豪强影响中央政治。", "黄巾起义后各地军阀力量扩张。"]),
        era_topic("three-kingdoms", "三国", 220, 280, "魏、蜀、吴并立的时期，不能简化为单一朝代。", &["政权并存与战争塑造了区域格局。", "赤壁之战是理解三方形成的重要节点。"]),
        era_topic("western-jin", "西晋", 266, 316, "短暂实现统一后，迅速因内乱与外部压力而瓦解。", &["八王之乱严重削弱统治基础。", "北方政治与人口格局发生显著变化。"]),
        era_topic("eastern-jin", "东晋", 317, 420, "以建康为中心的南方政权，与北方多个政权长期对峙。", &["江南地区持续开发。", "淝水之战巩固了短期稳定。"]),
        era_topic("northern-southern", "南北朝", 420, 589, "多个南北政权并存、民族与文化交流频繁的时期。", &["政权更替频繁，制度与文化发展并未中断。", "为隋唐统一积累条件。"]),
        era_topic("sui", "隋", 581, 618, "结束长期分裂并重新统一的短命王朝。", &["大运河影响南北交通与经济联系。", "制度建设对唐朝具有承接意义。"]),
        HistoryDocument {
            node: node("tang", HistoryNodeKind::Dynasty, "唐朝", "tang", 618, Some(907), "一个文化开放、制度成熟而后期经历深刻转折的王朝。", &["长安", "安史之乱", "科举"]),
            detail: HistoryDetail::Dynasty(Dynasty {
                name: "唐朝".into(),
                start_year: 618,
                end_year: 907,
                capital: Some("长安".into()),
                regime_type: "统一王朝".into(),
                overview: "唐朝（618—907）由李渊建立，以长安为政治中心。它承接隋代制度基础，在唐太宗、武则天、唐玄宗前期先后发展，形成兼具中央集权、开放交流与多元文化特征的帝国。安史之乱后，中央与地方关系、财政和军事体系持续重组，最终在晚唐的藩镇与政治危机中结束。".into(),
                sections: vec![
                    HistorySection { title: "时代脉络".into(), items: vec![
                        "618年李渊在长安称帝建唐；626年李世民即位，贞观时期完成对隋末秩序的整合。".into(),
                        "武则天执政与称帝期间，科举取士和官僚选拔继续发展，唐的统治范围与行政能力得到巩固。".into(),
                        "开元前期国力强盛，长安、洛阳和江淮经济区共同支撑帝国运转；755年安史之乱成为由盛转衰的关键节点。".into(),
                        "叛乱平定后，唐廷仍延续一个多世纪，但藩镇坐大、宦官干政和财政困境反复出现；907年朱温废唐，五代十国开始。".into(),
                    ]},
                    HistorySection { title: "政治与制度".into(), items: vec![
                        "中央以三省六部为骨架处理政务，中书出令、门下审议、尚书执行，实际运行会随皇权与宰相集团的力量变化而调整。".into(),
                        "州县是地方治理的基本层级；边疆与军事重镇设置节度使，本为边防安排，后期逐渐发展为拥有较强军政财权的地方力量。".into(),
                        "科举与门荫、荐举并行。进士科地位上升，扩大了士人进入官僚体系的渠道，但并未完全取代门第和政治关系。".into(),
                        "律、令、格、式共同构成制度规范；《唐律疏议》是研究唐代法律与后世东亚法制影响的重要材料。".into(),
                    ]},
                    HistorySection { title: "经济与社会".into(), items: vec![
                        "前期以均田、租庸调和府兵等制度为重要基础；人口流动、土地兼并与财政需求变化，使这些制度的实际运作逐步调整。".into(),
                        "安史之乱后，两税法按资产和土地征收，是财政从以人丁与实物为重转向以财产、货币联系为重的重要变化。".into(),
                        "农业生产重心继续向江南拓展，运河和水路连接南北。长安、洛阳、扬州等城市汇聚官员、商人、手工业者与外来人口。".into(),
                        "社会身份和财富来源日益多样，但宗族、地域、门第及官僚网络仍深刻影响个人上升与地方秩序。".into(),
                    ]},
                    HistorySection { title: "文化与对外交流".into(), items: vec![
                        "唐诗达到高峰，李白、杜甫、王维、白居易等人的创作展现了不同社会经验与审美传统；书法、绘画、音乐也高度繁荣。".into(),
                        "佛教、道教与儒家思想互动频繁，玄奘西行、译经与寺院经济体现了宗教网络的广泛影响。".into(),
                        "陆上丝绸之路与海上贸易把唐与中亚、南亚、西亚及东亚连接起来。长安城内的胡商、使节和宗教社群反映了这种交流。".into(),
                        "唐的制度、文字与文化通过朝贡、留学和贸易影响周边地区，但这种影响是双向交流，并非单向传播。".into(),
                    ]},
                    HistorySection { title: "危机、转型与覆亡".into(), items: vec![
                        "安史之乱的爆发与边镇权力膨胀、中央财政军事压力及政治矛盾相关，不能仅归因于个别人物。".into(),
                        "平乱后，河北等地部分藩镇长期保有较大自主性；宦官掌握禁军并影响皇位继承，中央权力受到多重制约。".into(),
                        "9世纪后期，财政压力、党争、民变与地方军事力量交织。黄巢起义冲击统治基础，朱温最终取代唐室。".into(),
                    ]},
                    HistorySection { title: "历史影响".into(), items: vec![
                        "唐代在政治制度、法律、科举、城市生活和文化生产方面留下深远遗产，常被后世视为理解中古中国的重要高峰。".into(),
                        "它的兴衰也说明：边防体系、财政结构与中央—地方关系彼此牵动，繁荣并不自动意味着长期稳定。".into(),
                        "研究唐朝应同时阅读正史、制度文献、诗文与墓志，并结合考古材料；对具体事件和数字需要注意史料立场与学界分歧。".into(),
                    ]},
                ],
            }),
        },
        era_topic("five-dynasties", "五代十国", 907, 979, "北方多朝更替、南方多政权并存的过渡时期。", &["不能用单一王朝线解释全国局势。", "区域经济与文化持续发展。"]),
        HistoryDocument { node: node("northern-song", HistoryNodeKind::Dynasty, "北宋", "northern-song", 960, Some(1127), "与辽、西夏长期并存的中原王朝。", &["辽", "西夏", "燕云十六州", "王安石"]), detail: HistoryDetail::Dynasty(Dynasty { name: "北宋".into(), start_year: 960, end_year: 1127, capital: Some("东京开封府".into()), regime_type: "并存政权".into(), overview: "北宋与辽、西夏同时存在，政治、财政与边防问题彼此交织。".into(), sections: vec![] }) },
        HistoryDocument { node: node("liao", HistoryNodeKind::Dynasty, "辽", "liao", 916, Some(1125), "契丹建立的政权，与北宋长期并存。", &["契丹", "北宋", "澶渊之盟"]), detail: HistoryDetail::Dynasty(Dynasty { name: "辽".into(), start_year: 916, end_year: 1125, capital: Some("上京临潢府".into()), regime_type: "并存政权".into(), overview: "辽的存在说明中国历史时间线并非单一王朝接续。".into(), sections: vec![] }) },
        HistoryDocument { node: node("western-xia", HistoryNodeKind::Dynasty, "西夏", "western-xia", 1038, Some(1227), "党项建立的政权，先后与北宋、辽、金、南宋并存。", &["党项", "北宋", "南宋", "金"]), detail: HistoryDetail::Dynasty(Dynasty { name: "西夏".into(), start_year: 1038, end_year: 1227, capital: Some("兴庆府".into()), regime_type: "并存政权".into(), overview: "西夏是理解宋辽金夏并存格局的重要节点。".into(), sections: vec![] }) },
        era_topic("southern-song", "南宋", 1127, 1279, "以临安为都，与金、西夏等政权长期并存。", &["江南经济文化进一步发展。", "与北方政权的战争与外交贯穿始终。"]),
        era_topic("jin", "金", 1115, 1234, "女真建立的政权，先后与北宋、南宋、西夏并存。", &["灭北宋后改变北方政治格局。", "其兴衰与蒙古崛起密切相关。"]),
        era_topic("yuan", "元", 1271, 1368, "蒙古建立的统一王朝，形成更广阔的欧亚交流网络。", &["行省制度对后世地方治理影响深远。", "交通与商业网络更加扩展。"]),
        era_topic("ming", "明", 1368, 1644, "以南京、北京为政治中心的统一王朝。", &["文官体系与中央集权持续发展。", "边防、财政与海洋事务是理解晚明的重要线索。"]),
        era_topic("qing", "清", 1636, 1912, "由满洲贵族建立并完成全国统一的王朝。", &["多民族国家治理与边疆事务具有重要意义。", "晚期面临内外压力与制度转型。"]),
        era_topic("modern", "近现代", 1912, 2026, "从民国建立到中华人民共和国时期，中国社会持续经历现代国家与社会转型。", &["该时期跨度大，后续会按事件、人物和制度继续细分。", "基础时间轴离线可用，专题资料可持续补充。"]),
        HistoryDocument { node: node("li-shimin", HistoryNodeKind::Person, "李世民", "tang", 598, Some(649), "唐太宗，唐初政治格局的核心塑造者。", &["唐太宗", "玄武门之变", "贞观"]), detail: HistoryDetail::Person(HistoricalPerson { name: "李世民".into(), born_year: Some(598), died_year: Some(649), identities: vec!["唐朝皇帝".into(), "政治家".into()], biography: vec!["参与唐朝建立。".into(), "626年发动玄武门之变后即位。".into()], achievements: vec!["贞观时期的政治实践成为后世重要参照。".into()], controversies: vec!["玄武门之变的性质与评价需结合唐初权力竞争理解。".into()] }) },
        HistoryDocument { node: node("wang-anshi", HistoryNodeKind::Person, "王安石", "northern-song", 1021, Some(1086), "北宋政治家，熙宁变法的主持者。", &["熙宁变法", "北宋", "科举"]), detail: HistoryDetail::Person(HistoricalPerson { name: "王安石".into(), born_year: Some(1021), died_year: Some(1086), identities: vec!["政治家".into(), "思想家".into()], biography: vec!["1069年起主持熙宁变法。".into()], achievements: vec!["围绕财政、军事与基层治理提出改革措施。".into()], controversies: vec!["变法成效与社会代价长期存在不同解释。".into()] }) },
        HistoryDocument { node: node("qin-shihuang", HistoryNodeKind::Person, "秦始皇", "qin", -259, Some(-210), "秦朝建立者，推动统一制度的关键人物。", &["嬴政", "统一六国", "郡县制", "长城"]), detail: HistoryDetail::Person(HistoricalPerson { name: "秦始皇".into(), born_year: Some(-259), died_year: Some(-210), identities: vec!["秦朝皇帝".into()], biography: vec!["完成统一后推行多项制度整合。".into()], achievements: vec!["统一文字、货币、度量衡等。".into()], controversies: vec!["严刑峻法与焚书相关政策的影响一直是研究重点。".into()] }) },
        HistoryDocument { node: node("xuanwu-gate", HistoryNodeKind::Event, "玄武门之变", "tang", 626, None, "唐初皇位继承冲突的一次决定性转折。", &["李世民", "李建成", "长安", "唐朝"]), detail: HistoryDetail::Event(HistoricalEvent { name: "玄武门之变".into(), start_year: 626, end_year: None, overview: "李世民在玄武门附近发动政变，随后成为皇太子并即位。".into(), background: vec!["唐初诸子在军政资源与继承权上存在激烈竞争。".into()], trigger: Some("皇位继承冲突持续升级。".into()), course: vec!["玄武门附近发生武装冲突。".into()], results: vec!["李世民取得继承权并在同年即位。".into()], impacts: vec!["奠定唐太宗统治的政治起点。".into()], debates: vec!["事件细节需结合《旧唐书》《新唐书》《资治通鉴》等史料审读。".into()] }) },
        HistoryDocument { node: node("an-lushan-rebellion", HistoryNodeKind::Event, "安史之乱", "tang", 755, Some(763), "唐朝由盛转衰的重大内战与政治危机。", &["755年", "唐玄宗", "藩镇", "长安"]), detail: HistoryDetail::Event(HistoricalEvent { name: "安史之乱".into(), start_year: 755, end_year: Some(763), overview: "安禄山、史思明等发动叛乱，战争延续多年。".into(), background: vec!["均田制和府兵制等制度运行出现问题。".into(), "节度使权力不断扩大。".into()], trigger: Some("安禄山起兵反唐。".into()), course: vec!["叛军攻陷东都、长安，唐廷一度西迁。".into(), "唐廷借助多方力量平叛。".into()], results: vec!["叛乱最终被平定。".into()], impacts: vec!["藩镇割据加深，中央权力相对削弱。".into(), "唐朝社会经济与政治结构受到重创。".into()], debates: vec!["关于起因权重与唐后期变化，学界存在制度、财政、边防等不同强调。".into()] }) },
        HistoryDocument { node: node("feishui-battle", HistoryNodeKind::War, "淝水之战", "eastern-jin", 383, None, "东晋以较少兵力击败前秦的大型会战。", &["383年", "东晋", "前秦", "谢安", "苻坚"]), detail: HistoryDetail::Event(HistoricalEvent { name: "淝水之战".into(), start_year: 383, end_year: None, overview: "前秦南下，东晋在淝水一带获胜，改变南北政治格局。".into(), background: vec!["前秦在北方扩张后试图统一全国。".into()], trigger: Some("前秦大军南下。".into()), course: vec!["双方在淝水附近对峙，前秦军阵在撤退调整时出现混乱。".into()], results: vec!["东晋取胜，前秦统治迅速瓦解。".into()], impacts: vec!["南北分立格局延续，东晋得以稳定江南。".into()], debates: vec!["兵力数字和若干战场细节存在史料解释差异。".into()] }) },
        HistoryDocument { node: node("changan", HistoryNodeKind::Place, "长安城", "tang", -202, Some(907), "汉唐时期最重要的都城之一，今西安地区。", &["西安", "唐朝", "玄武门", "安史之乱"]), detail: HistoryDetail::Place(HistoricalPlace { name: "长安城".into(), modern_name: Some("西安市".into()), latitude: Some(34.2658), longitude: Some(108.9541), overview: "长安长期是政治、经济与文化中心，唐长安城的坊市格局尤具代表性。".into(), historical_names: vec!["长安".into(), "京师".into()] }) },
        HistoryDocument { node: node("linan", HistoryNodeKind::Place, "临安", "southern-song", 1127, Some(1279), "南宋都城，今杭州的重要历史称谓。", &["杭州", "南宋", "西湖", "岳飞"]), detail: HistoryDetail::Place(HistoricalPlace { name: "临安".into(), modern_name: Some("杭州市".into()), latitude: Some(30.2741), longitude: Some(120.1551), overview: "南宋定都临安后，城市发展与江南经济文化紧密相连。".into(), historical_names: vec!["钱塘".into(), "杭州".into()] }) },
        HistoryDocument { node: node("keju", HistoryNodeKind::Institution, "科举制", "sui", 605, Some(1905), "通过考试选拔官员的制度，深刻影响中国社会与文化。", &["制度", "隋唐", "进士", "王安石"]), detail: HistoryDetail::Institution(HistoricalInstitution { name: "科举制".into(), overview: "科举制在隋唐发展，并在后世不断调整，是理解官僚选拔与教育文化的重要制度。".into(), key_points: vec!["考试科目和录取规模因时代而变。".into(), "制度运行与门第、学校、地方社会相互影响。".into()] }) },
        HistoryDocument { node: node("zenghouyi-bianzhong", HistoryNodeKind::Artifact, "曾侯乙编钟", "spring-autumn", -433, None, "战国早期大型礼乐器，展示精湛铸造与音乐文化。", &["文物", "青铜器", "湖北", "礼乐"]), detail: HistoryDetail::Artifact(HistoricalArtifact { name: "曾侯乙编钟".into(), overview: "出土于湖北随州曾侯乙墓，由多件青铜钟组成，是研究先秦音乐、礼制与冶铸技术的重要实物。".into(), collection: Some("湖北省博物馆".into()) }) },
    ]
}
fn relation(from: &str, to: &str, kind: HistoryRelationKind, note: &str) -> HistoryRelation {
    HistoryRelation {
        from_id: from.into(),
        to_id: to.into(),
        kind,
        note: Some(note.into()),
    }
}
fn seed_relations() -> Vec<HistoryRelation> {
    vec![
        relation(
            "li-shimin",
            "xuanwu-gate",
            HistoryRelationKind::ParticipatedIn,
            "发动并主导政变",
        ),
        relation(
            "xuanwu-gate",
            "tang",
            HistoryRelationKind::BelongsTo,
            "唐初政治事件",
        ),
        relation(
            "xuanwu-gate",
            "changan",
            HistoryRelationKind::OccurredIn,
            "发生在长安宫城北门附近",
        ),
        relation(
            "an-lushan-rebellion",
            "tang",
            HistoryRelationKind::BelongsTo,
            "唐朝中期",
        ),
        relation(
            "an-lushan-rebellion",
            "changan",
            HistoryRelationKind::OccurredIn,
            "长安曾被叛军攻占",
        ),
        relation(
            "an-lushan-rebellion",
            "feishui-battle",
            HistoryRelationKind::RelatedTo,
            "均为理解政权转折的重要战争",
        ),
        relation(
            "feishui-battle",
            "tang",
            HistoryRelationKind::Predecessor,
            "唐朝之前的南北政治格局",
        ),
        relation(
            "qin-shihuang",
            "qin",
            HistoryRelationKind::BelongsTo,
            "秦朝建立者",
        ),
        relation(
            "qin-shihuang",
            "keju",
            HistoryRelationKind::Predecessor,
            "统一国家的官僚治理传统与后世制度演进相关",
        ),
        relation(
            "wang-anshi",
            "northern-song",
            HistoryRelationKind::BelongsTo,
            "北宋政治家",
        ),
        relation(
            "wang-anshi",
            "keju",
            HistoryRelationKind::RelatedTo,
            "变法涉及取士与教育制度调整",
        ),
        relation(
            "northern-song",
            "liao",
            HistoryRelationKind::PoliticalOpponent,
            "长期并存并存在外交与军事竞争",
        ),
        relation(
            "northern-song",
            "western-xia",
            HistoryRelationKind::PoliticalOpponent,
            "长期并存并存在边境冲突",
        ),
        relation(
            "linan",
            "northern-song",
            HistoryRelationKind::Successor,
            "北宋覆亡后南宋以临安为都",
        ),
        relation(
            "changan",
            "tang",
            HistoryRelationKind::BelongsTo,
            "唐朝都城",
        ),
        relation(
            "zenghouyi-bianzhong",
            "keju",
            HistoryRelationKind::Predecessor,
            "展现科举出现前的礼乐文化传统",
        ),
    ]
}
fn seed_facts() -> Vec<HistoryFact> {
    vec![
        HistoryFact {
            subject_id: "an-lushan-rebellion".into(),
            predicate: "开始时间".into(),
            value: "755年".into(),
            source_ids: vec!["baike".into(), "academic".into()],
        },
        HistoryFact {
            subject_id: "feishui-battle".into(),
            predicate: "发生时间".into(),
            value: "383年".into(),
            source_ids: vec!["baike".into()],
        },
        HistoryFact {
            subject_id: "changan".into(),
            predicate: "现代名称".into(),
            value: "西安市".into(),
            source_ids: vec!["museum".into()],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn open() -> (tempfile::TempDir, HistoryStore) {
        let dir = tempdir().expect("tempdir");
        let store = HistoryStore::open(dir.path().join("history.db")).expect("store");
        (dir, store)
    }
    #[test]
    fn timeline_is_ordered_and_keeps_concurrent_regimes() {
        let (_dir, store) = open();
        let periods = store.periods().expect("periods");
        assert!(
            periods
                .windows(2)
                .all(|pair| pair[0].start_year <= pair[1].start_year)
        );
        let active: Vec<_> = periods
            .iter()
            .filter(|period| period.start_year <= 1100 && period.end_year >= 1100)
            .map(|period| period.name.as_str())
            .collect();
        assert!(active.contains(&"北宋") && active.contains(&"辽") && active.contains(&"西夏"));
    }
    #[test]
    fn storage_search_favorites_and_history_work() {
        let (_dir, store) = open();
        assert!(store.search("长安").expect("search").len() >= 2);
        assert!(store.toggle_favorite("changan").expect("favorite"));
        assert_eq!(store.favorite_ids().expect("favorites"), vec!["changan"]);
        store.record_view("changan").expect("view");
        assert_eq!(store.recent_nodes(2).expect("recent")[0].id, "changan");
    }

    #[test]
    fn source_ranking_prefers_academic_over_reference() {
        let (_dir, store) = open();
        let sources = store
            .sources(&["baike".into(), "academic".into()])
            .expect("sources");
        assert_eq!(sources[0].id, "academic");
    }

    #[test]
    fn period_nodes_list_era_entries_in_time_order() {
        let (_dir, store) = open();
        let nodes = store.period_nodes("tang").expect("period nodes");
        let ids: Vec<_> = nodes.iter().map(|node| node.id.as_str()).collect();
        // 唐朝时代下的真实节点：朝代本体 + 人物 + 事件 + 地点，按时序排序。
        assert!(ids.contains(&"li-shimin"));
        assert!(ids.contains(&"xuanwu-gate"));
        assert!(ids.contains(&"an-lushan-rebellion"));
        assert!(ids.contains(&"changan"));
        assert!(ids.contains(&"tang"));
        assert!(
            nodes
                .windows(2)
                .all(|pair| pair[0].start_year.unwrap_or(0) <= pair[1].start_year.unwrap_or(0))
        );
        // 未录入专题节点的时代返回其朝代本体（不会展示在探索网格的专题分类中）。
        let few = store.period_nodes("three-kingdoms").expect("few");
        assert_eq!(few.len(), 1);
        assert_eq!(few[0].id, "three-kingdoms");
    }

    #[test]
    fn reopening_upgrades_seeded_documents_without_resetting_favorites() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("history.db");
        let store = HistoryStore::open(&path).expect("store");
        assert!(store.toggle_favorite("tang").expect("favorite"));
        store
            .connection
            .execute(
                "UPDATE history_nodes SET detail_json = ?1 WHERE id = 'tang'",
                [r#"{"detail_type":"dynasty","name":"唐朝","start_year":618,"end_year":907,"capital":"长安","regime_type":"统一王朝","overview":"旧资料"}"#],
            )
            .expect("write legacy document");
        drop(store);

        let upgraded = HistoryStore::open(&path).expect("reopen");
        let Some(HistoryDocument {
            detail: HistoryDetail::Dynasty(tang),
            ..
        }) = upgraded.document("tang").expect("document")
        else {
            panic!("唐朝应保留为朝代详情");
        };
        assert!(!tang.sections.is_empty());
        assert!(
            upgraded
                .favorite_ids()
                .expect("favorites")
                .contains(&"tang".into())
        );
    }
}
