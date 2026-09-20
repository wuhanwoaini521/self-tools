//! History 知识库的只读查询结果记录。
//!
//! 这些类型是 application 的 `HistoryQueryPort`、infrastructure 的
//! DuckDB 行映射与 frontend JSON 契约共用的纯数据载体（无 IO、无 SQL），
//! 因此归 core 持有：infrastructure 允许直接依赖 core，application 也不
//! 需要为了这些记录回指 infrastructure。

use serde::Serialize;

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
    pub process_zh_cn: Option<String>,
    pub result_zh_cn: Option<String>,
    pub impact_zh_cn: Option<String>,
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

/// Backbone V2 的 `event_evidence` 章节级史料出处行。
///
/// V2 `dist/history.duckdb` 不再内嵌 HistoricalText 全文，而是用
/// “作品 + 篇目（term）+ 术语提示”承载史料证据；`link_status` 反映原文
/// 关联状态（`needs_linking` / `pending_knowledge`）。
#[derive(Debug, Clone, Serialize)]
pub struct EventEvidenceResult {
    pub id: String,
    pub event_id: String,
    pub historical_text_id: Option<String>,
    pub work: Option<String>,
    pub term: Option<String>,
    pub chapter_hint: Option<String>,
    pub context_keywords: Option<String>,
    pub evidence_role: Option<String>,
    pub link_status: Option<String>,
    pub link_quality_status: Option<String>,
    pub link_confidence: Option<f64>,
    pub review_note: Option<String>,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub quality_status: Option<String>,
    pub rejected_text_ids: Option<String>,
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
    pub events: i64,
    pub periods: i64,
    pub regimes: i64,
    pub stories: i64,
    pub event_relations: i64,
    pub event_evidences: i64,
}

/// 时期页事件列表行：事件要点 + 关联人物/关系/证据计数。
#[derive(Debug, Clone, Serialize)]
pub struct PeriodEventItem {
    pub id: String,
    pub name_zh_cn: String,
    pub event_type: Option<String>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub importance: Option<String>,
    pub summary_zh_cn: Option<String>,
    pub result_zh_cn: Option<String>,
    pub people_count: i64,
    pub relation_count: i64,
    pub evidence_count: i64,
}

/// 时期页的核心人物（按参与事件数倒序）。
///
/// `birth_year` / `death_year` / `intro_zh_cn` 为 2026-09 Period Detail 重构新增
/// （原型列来自 `people` 表，供「核心人物」模块展示身份/活跃年代，不改变既有计数逻辑）。
#[derive(Debug, Clone, Serialize)]
pub struct PeriodPersonItem {
    pub person_id: String,
    pub canonical_name_zh_cn: String,
    pub event_count: i64,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub intro_zh_cn: Option<String>,
}
