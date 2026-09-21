//! History V3.1 On-demand Enrichment — 域模型（V5 Gate 2）。
//!
//! 原则：Canonical = Truth Layer。富化是 **derived 用户数据**（独立 SQLite 缓存），
//! 绝不会写回 Canonical；AI 只允许 read canonical → search → write enrichment。

use serde::{Deserialize, Serialize};

/// 富化 schema 版本（缓存 key 与格式演进时递增；旧记录因此变 STALE）。
pub const ENRICHMENT_SCHEMA_VERSION: u16 = 1;

/// 富化 section（第一版 Mandatory：overview / background / impact；Optional 后续）。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentSection {
    Overview,
    Background,
    Impact,
}

impl EnrichmentSection {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Background => "background",
            Self::Impact => "impact",
        }
    }
}

/// 富化状态机（§16）。`Stale` 为**派生**状态（读取时计算），不落库。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrichmentState {
    Missing,
    Generating,
    Ready,
    Stale,
    Failed,
    Reviewed,
}

/// 富化缓存 key（§17）。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct EnrichmentKey {
    pub entity_type: String,
    pub entity_id: String,
    pub section: EnrichmentSection,
    pub locale: String,
    /// 生成时使用的富化 schema 版本（与现版本不一致 → STALE）。
    pub schema_version: u16,
}

impl EnrichmentKey {
    #[must_use]
    pub fn new(entity_type: &str, entity_id: &str, section: EnrichmentSection, locale: &str) -> Self {
        Self {
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            section,
            locale: locale.to_string(),
            schema_version: ENRICHMENT_SCHEMA_VERSION,
        }
    }
}

/// 一条可溯源的主张（§25）：文本 + 引用 source_ids。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnrichmentClaim {
    pub text: String,
    #[serde(default)]
    pub source_ids: Vec<String>,
}

/// 结构化富化负载（禁止 LLM → 大段 Markdown → 直接 DB）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnrichmentPayload {
    pub section: String,
    pub content: String,
    #[serde(default)]
    pub claims: Vec<EnrichmentClaim>,
    #[serde(default)]
    pub uncertainties: Vec<String>,
    #[serde(default)]
    pub controversies: Vec<String>,
}

/// 缓存元数据（§18）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnrichmentMetadata {
    pub generated_at: i64,
    pub refreshed_at: i64,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub prompt_version: String,
    pub schema_version: u16,
    /// 生成时 Canonical 的指纹（quality_status/source_reference/source_ids 摘要）；
    /// 与当前指纹不一致 → STALE（§19）。
    pub canonical_revision: Option<String>,
    #[serde(default)]
    pub source_ids: Vec<String>,
    /// 生成次数（审计 / 成本）。
    pub generation_count: u32,
}

/// 一条富化记录（含 revision 版本：REVIEWED 保护采用「新候选版本」而非覆盖，§20）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnrichmentRecord {
    pub key: EnrichmentKey,
    /// revision 1 起；reviewed 行重生成 = 新 revision（并列存储，不覆盖旧审定内容）。
    pub revision: u32,
    pub state: EnrichmentState,
    pub payload: Option<EnrichmentPayload>,
    pub metadata: Option<EnrichmentMetadata>,
    pub error: Option<String>,
    /// 人工审定标记（true 时 automatic refresh 跳过）。
    pub reviewed: bool,
}

/// 对外视图（UI / 工具消费）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnrichmentView {
    pub key: EnrichmentKey,
    pub state: EnrichmentState,
    pub payload: Option<EnrichmentPayload>,
    pub metadata: Option<EnrichmentMetadata>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_round_trip_and_defaults() {
        let key = EnrichmentKey::new("event", "zunyi_meeting", EnrichmentSection::Overview, "zh-CN");
        let json = serde_json::to_string(&key).unwrap();
        let back: EnrichmentKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, key);
        assert_eq!(key.schema_version, ENRICHMENT_SCHEMA_VERSION);
        assert_eq!(key.section.as_str(), "overview");
    }

    #[test]
    fn payload_requires_section_and_content_fields() {
        let payload = EnrichmentPayload {
            section: "overview".into(),
            content: "内容".into(),
            claims: vec![EnrichmentClaim {
                text: "主张".into(),
                source_ids: vec!["https://example.com/a".into()],
            }],
            ..EnrichmentPayload::default()
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"section\""));
        assert!(json.contains("\"claims\""));
    }
}