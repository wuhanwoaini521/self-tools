//! History V1 的离线优先 SQLite 存储。

use std::path::Path;

use devtoolbox_core::history::{
    Dynasty, HistoricalArtifact, HistoricalEvent, HistoricalInstitution, HistoricalPerson,
    HistoricalPlace, HistoryDetail, HistoryDocument, HistoryFact, HistoryNode, HistoryNodeKind,
    HistoryPeriod, HistoryRelation, HistoryRelationKind, HistorySource, SourceAuthority,
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
        store.seed_if_empty()?;
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

    fn seed_if_empty(&mut self) -> Result<(), InfrastructureError> {
        // 种子按稳定 id 幂等补齐，升级时新增时期可进入已有资料库，
        // 同时不会覆盖用户的收藏和浏览历史。
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
            "INSERT OR IGNORE INTO history_nodes VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
        HistoryDocument { node: node("qin", HistoryNodeKind::Dynasty, "秦朝", "qin", -221, Some(-206), "中国首个完成大一统的中央集权王朝。", &["秦始皇", "郡县制", "统一六国"]), detail: HistoryDetail::Dynasty(Dynasty { name: "秦朝".into(), start_year: -221, end_year: -206, capital: Some("咸阳".into()), regime_type: "统一王朝".into(), overview: "秦以郡县制、统一文字度量衡等措施奠定大一统治理的制度基础。".into() }) },
        era_topic("western-han", "西汉", -202, 8, "汉初在秦制基础上调整，逐步形成稳定的大一统治理。", &["郡国并行是早期重要制度特征。", "与西域联系的开拓影响深远。"]),
        era_topic("eastern-han", "东汉", 25, 220, "东汉后期地方势力与政治矛盾加深，最终走向分裂。", &["外戚、宦官与豪强影响中央政治。", "黄巾起义后各地军阀力量扩张。"]),
        era_topic("three-kingdoms", "三国", 220, 280, "魏、蜀、吴并立的时期，不能简化为单一朝代。", &["政权并存与战争塑造了区域格局。", "赤壁之战是理解三方形成的重要节点。"]),
        era_topic("western-jin", "西晋", 266, 316, "短暂实现统一后，迅速因内乱与外部压力而瓦解。", &["八王之乱严重削弱统治基础。", "北方政治与人口格局发生显著变化。"]),
        era_topic("eastern-jin", "东晋", 317, 420, "以建康为中心的南方政权，与北方多个政权长期对峙。", &["江南地区持续开发。", "淝水之战巩固了短期稳定。"]),
        era_topic("northern-southern", "南北朝", 420, 589, "多个南北政权并存、民族与文化交流频繁的时期。", &["政权更替频繁，制度与文化发展并未中断。", "为隋唐统一积累条件。"]),
        era_topic("sui", "隋", 581, 618, "结束长期分裂并重新统一的短命王朝。", &["大运河影响南北交通与经济联系。", "制度建设对唐朝具有承接意义。"]),
        HistoryDocument { node: node("tang", HistoryNodeKind::Dynasty, "唐朝", "tang", 618, Some(907), "一个文化开放、制度成熟而后期经历深刻转折的王朝。", &["长安", "安史之乱", "科举"]), detail: HistoryDetail::Dynasty(Dynasty { name: "唐朝".into(), start_year: 618, end_year: 907, capital: Some("长安".into()), regime_type: "统一王朝".into(), overview: "唐朝前期形成较强中央政府，安史之乱后藩镇、宦官等问题加剧。".into() }) },
        era_topic("five-dynasties", "五代十国", 907, 979, "北方多朝更替、南方多政权并存的过渡时期。", &["不能用单一王朝线解释全国局势。", "区域经济与文化持续发展。"]),
        HistoryDocument { node: node("northern-song", HistoryNodeKind::Dynasty, "北宋", "northern-song", 960, Some(1127), "与辽、西夏长期并存的中原王朝。", &["辽", "西夏", "燕云十六州", "王安石"]), detail: HistoryDetail::Dynasty(Dynasty { name: "北宋".into(), start_year: 960, end_year: 1127, capital: Some("东京开封府".into()), regime_type: "并存政权".into(), overview: "北宋与辽、西夏同时存在，政治、财政与边防问题彼此交织。".into() }) },
        HistoryDocument { node: node("liao", HistoryNodeKind::Dynasty, "辽", "liao", 916, Some(1125), "契丹建立的政权，与北宋长期并存。", &["契丹", "北宋", "澶渊之盟"]), detail: HistoryDetail::Dynasty(Dynasty { name: "辽".into(), start_year: 916, end_year: 1125, capital: Some("上京临潢府".into()), regime_type: "并存政权".into(), overview: "辽的存在说明中国历史时间线并非单一王朝接续。".into() }) },
        HistoryDocument { node: node("western-xia", HistoryNodeKind::Dynasty, "西夏", "western-xia", 1038, Some(1227), "党项建立的政权，先后与北宋、辽、金、南宋并存。", &["党项", "北宋", "南宋", "金"]), detail: HistoryDetail::Dynasty(Dynasty { name: "西夏".into(), start_year: 1038, end_year: 1227, capital: Some("兴庆府".into()), regime_type: "并存政权".into(), overview: "西夏是理解宋辽金夏并存格局的重要节点。".into() }) },
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
}
