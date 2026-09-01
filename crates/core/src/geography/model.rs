use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEntityType {
    World,
    Country,
    Region,
    Province,
    City,
    River,
    Mountain,
    MountainRange,
    Plateau,
    Plain,
    Basin,
    Desert,
    Lake,
    Ocean,
    Sea,
    Island,
    Archipelago,
    ClimateZone,
    TectonicPlate,
}

impl GeoEntityType {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::World => "世界",
            Self::Country => "国家",
            Self::Region => "区域",
            Self::Province => "省级行政区",
            Self::City => "城市",
            Self::River => "河流",
            Self::Mountain => "山体",
            Self::MountainRange => "山脉",
            Self::Plateau => "高原",
            Self::Plain => "平原",
            Self::Basin => "盆地",
            Self::Desert => "沙漠",
            Self::Lake => "湖泊",
            Self::Ocean => "海洋",
            Self::Sea => "海域",
            Self::Island => "岛屿",
            Self::Archipelago => "群岛",
            Self::ClimateZone => "气候区",
            Self::TectonicPlate => "构造板块",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CoordinateSystem {
    WGS84,
    GCJ02,
    BD09,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoCoordinate {
    pub system: CoordinateSystem,
    pub latitude: f64,
    pub longitude: f64,
}

impl GeoCoordinate {
    #[must_use]
    pub fn is_valid(self) -> bool {
        (-90.0..=90.0).contains(&self.latitude) && (-180.0..=180.0).contains(&self.longitude)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryKind {
    Point,
    Line,
    Polygon,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoGeometry {
    pub kind: GeometryKind,
    pub coordinates: Vec<GeoCoordinate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GeoProperty {
    pub key: String,
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoEntity {
    pub id: String,
    pub entity_type: GeoEntityType,
    pub name: String,
    pub name_en: Option<String>,
    pub aliases: Vec<String>,
    pub coordinates: Option<GeoCoordinate>,
    pub geometry: Option<GeoGeometry>,
    pub parent_id: Option<String>,
    pub properties: Vec<GeoProperty>,
    pub summary: String,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRelationKind {
    LocatedIn,
    PartOf,
    Borders,
    FlowsThrough,
    SourceOf,
    FlowsInto,
    ConnectedTo,
    Near,
    Crosses,
    Surrounds,
    Affects,
    Influences,
    FormedBy,
    Causes,
    BelongsToClimateZone,
}

impl GeoRelationKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LocatedIn => "位于",
            Self::PartOf => "属于",
            Self::Borders => "接壤",
            Self::FlowsThrough => "流经",
            Self::SourceOf => "发源于",
            Self::FlowsInto => "汇入",
            Self::ConnectedTo => "连接",
            Self::Near => "邻近",
            Self::Crosses => "穿过",
            Self::Surrounds => "环绕",
            Self::Affects => "影响",
            Self::Influences => "塑造",
            Self::FormedBy => "形成于",
            Self::Causes => "导致",
            Self::BelongsToClimateZone => "属于气候区",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GeoRelation {
    pub from_id: String,
    pub to_id: String,
    pub kind: GeoRelationKind,
    pub note: Option<String>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GeoSource {
    pub id: String,
    pub dataset: String,
    pub version: String,
    pub url: String,
    pub license: String,
    pub updated_at: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoRelationView {
    pub relation: GeoRelation,
    pub entity: GeoEntity,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoEntityDetail {
    pub entity: GeoEntity,
    pub relations: Vec<GeoRelationView>,
    pub sources: Vec<GeoSource>,
    pub favorite: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoSearchGroup {
    pub entity_type: GeoEntityType,
    pub items: Vec<GeoEntity>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationKind {
    Entity,
    Question,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GeoRecommendation {
    pub kind: RecommendationKind,
    pub title: String,
    pub question: String,
    pub entity_id: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoMapPoint {
    pub entity_id: String,
    pub name: String,
    pub entity_type: GeoEntityType,
    pub coordinate: GeoCoordinate,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoMapLine {
    pub from_id: String,
    pub to_id: String,
    pub kind: GeoRelationKind,
    pub from: GeoCoordinate,
    pub to: GeoCoordinate,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeographyHome {
    pub recommendation: GeoRecommendation,
    pub featured: Vec<GeoEntity>,
    pub recent: Vec<GeoEntity>,
    pub favorite_ids: Vec<String>,
    pub map_points: Vec<GeoMapPoint>,
    pub map_lines: Vec<GeoMapLine>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompareMetric {
    pub label: String,
    pub key: String,
    pub left: Option<String>,
    pub right: Option<String>,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoCompareView {
    pub left: GeoEntity,
    pub right: GeoEntity,
    pub metrics: Vec<CompareMetric>,
    pub explanation: String,
}

#[cfg(test)]
mod tests {
    use super::{CoordinateSystem, GeoCoordinate, GeoEntityType, GeoRelationKind};

    #[test]
    fn coordinates_keep_their_reference_system_when_serialized() {
        let coordinate = GeoCoordinate {
            system: CoordinateSystem::GCJ02,
            latitude: 30.57,
            longitude: 104.07,
        };
        assert!(coordinate.is_valid());
        let json = serde_json::to_string(&coordinate).expect("coordinate json");
        assert!(json.contains("GCJ02"));
        assert_eq!(
            serde_json::from_str::<GeoCoordinate>(&json)
                .expect("coordinate")
                .system,
            CoordinateSystem::GCJ02
        );
    }

    #[test]
    fn entity_and_relation_labels_are_stable() {
        assert_eq!(GeoEntityType::River.label(), "河流");
        assert_eq!(GeoRelationKind::FlowsThrough.label(), "流经");
    }
}
