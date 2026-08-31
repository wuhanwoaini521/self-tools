//! 中国历史探索的纯领域模型。
//!
//! 领域对象按语义拆分；`HistoryNode` 只是用于统一导航与关系索引的轻量骨架，
//! 不能替代人物、事件、地点等各自的详情模型。

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryNodeKind {
    Person,
    Event,
    Dynasty,
    Place,
    War,
    Institution,
    Artifact,
    Culture,
}

impl HistoryNodeKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Person => "人物",
            Self::Event => "事件",
            Self::Dynasty => "朝代",
            Self::Place => "地点",
            Self::War => "战争",
            Self::Institution => "制度",
            Self::Artifact => "文物",
            Self::Culture => "文化",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryPeriod {
    pub id: String,
    pub name: String,
    pub start_year: i32,
    pub end_year: i32,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryNode {
    pub id: String,
    pub kind: HistoryNodeKind,
    pub title: String,
    pub period_id: Option<String>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub summary: String,
    pub tags: Vec<String>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dynasty {
    pub name: String,
    pub start_year: i32,
    pub end_year: i32,
    pub capital: Option<String>,
    pub regime_type: String,
    pub overview: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoricalPerson {
    pub name: String,
    pub born_year: Option<i32>,
    pub died_year: Option<i32>,
    pub identities: Vec<String>,
    pub biography: Vec<String>,
    pub achievements: Vec<String>,
    pub controversies: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoricalEvent {
    pub name: String,
    pub start_year: i32,
    pub end_year: Option<i32>,
    pub overview: String,
    pub background: Vec<String>,
    pub trigger: Option<String>,
    pub course: Vec<String>,
    pub results: Vec<String>,
    pub impacts: Vec<String>,
    pub debates: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistoricalPlace {
    pub name: String,
    pub modern_name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub overview: String,
    pub historical_names: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoricalInstitution {
    pub name: String,
    pub overview: String,
    pub key_points: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoricalArtifact {
    pub name: String,
    pub overview: String,
    pub collection: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "detail_type", rename_all = "snake_case")]
pub enum HistoryDetail {
    Dynasty(Dynasty),
    Person(HistoricalPerson),
    Event(HistoricalEvent),
    Place(HistoricalPlace),
    Institution(HistoricalInstitution),
    Artifact(HistoricalArtifact),
    Topic {
        overview: String,
        key_points: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistoryDocument {
    pub node: HistoryNode,
    pub detail: HistoryDetail,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRelationKind {
    OccurredIn,
    ParticipatedIn,
    BelongsTo,
    Cause,
    Consequence,
    RelatedTo,
    Family,
    PoliticalAlly,
    PoliticalOpponent,
    MonarchMinister,
    MilitaryOpponent,
    Predecessor,
    Successor,
}

impl HistoryRelationKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OccurredIn => "发生于",
            Self::ParticipatedIn => "参与",
            Self::BelongsTo => "属于",
            Self::Cause => "前因",
            Self::Consequence => "后果",
            Self::RelatedTo => "相关",
            Self::Family => "亲属",
            Self::PoliticalAlly => "政治盟友",
            Self::PoliticalOpponent => "政治对手",
            Self::MonarchMinister => "君臣",
            Self::MilitaryOpponent => "军事对手",
            Self::Predecessor => "前任",
            Self::Successor => "继任",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryRelation {
    pub from_id: String,
    pub to_id: String,
    pub kind: HistoryRelationKind,
    pub note: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    Official,
    Museum,
    Academic,
    Reference,
    General,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistorySource {
    pub id: String,
    pub title: String,
    pub url: String,
    pub source_type: String,
    pub authority: SourceAuthority,
    pub published_at: Option<i64>,
    pub fetched_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryFact {
    pub subject_id: String,
    pub predicate: String,
    pub value: String,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistorySearchGroup {
    pub kind: HistoryNodeKind,
    pub items: Vec<HistoryNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistoryDetailView {
    pub document: HistoryDocument,
    pub relations: Vec<HistoryRelationView>,
    pub sources: Vec<HistorySource>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryRelationView {
    pub relation: HistoryRelation,
    pub node: HistoryNode,
}
