//! Language 存储端口（Gate 7.5：语言依赖方向反转）。
//!
//! `LanguageService` 只依赖本端口，不直接 import 基础设施的 `LanguageStore` /
//! `now_unix` / 导入工具。端口按**真实使用面**定义（搜索 / 详情 / Today /
//! Review / 进度 / 来源 / 句子），不是 Store 完整 API 的复制。
//! runtime（desktop/server）在组合根实现本端口（adapter）。
//!
//! 许可证 Gate（`verify_source_license`）是纯规则（core 的 `SourceLicense`
//! 判定），随端口放在应用层——不再经由基础设施导入工具转发。

use devtoolbox_core::language::{
    DatasetManifest, LanguageCode, LanguageItem, LanguageRelation, LanguageSource, LearningState,
    LearningStateKind, Meaning, Pronunciation, ReviewOutcome, ReviewRating, SentenceRecord,
    TodayPlan,
};
use serde::Serialize;

/// 语言条目计数（对应前端 Languages 列表）。
#[derive(Clone, Debug, Serialize)]
pub struct LanguageCount {
    pub language: LanguageCode,
    pub words: i64,
    pub phrases: i64,
    pub sentences: i64,
    pub total: i64,
}

/// 搜索结果命中（含命中字段说明）。
#[derive(Clone, Debug, Serialize)]
pub struct LanguageSearchHitModel {
    pub item: LanguageItem,
    pub matched: String,
}

/// 短别名（兼容 search 端口签名多年来的命名习惯）。
pub type SearchHitModel = LanguageSearchHitModel;

/// 词详情所需关联集合（端口返回形状；与基础设施 `ItemDetailRows` 一致）。
#[derive(Clone, Debug, Default)]
pub struct LanguageDetailRows {
    pub item: Option<LanguageItem>,
    pub meanings: Vec<Meaning>,
    pub pronunciations: Vec<Pronunciation>,
    pub relations: Vec<LanguageRelation>,
    pub related_items: Vec<LanguageItem>,
    pub examples: Vec<LanguageExample>,
    pub sentences: Vec<SentenceRecord>,
    pub state: Option<LearningState>,
    pub favorite: bool,
    /// item_extra JSON（kanji 元数据等）。
    pub extra: Option<serde_json::Value>,
}

/// 例句（详情页展示）。
#[derive(Clone, Debug, Serialize)]
pub struct LanguageExample {
    pub text: String,
    pub translation: Option<String>,
    pub source: String,
}

/// 语言存储端口：应用层消费的最小 capability 集。
///
/// 错误统一为可展示文本（适配层负责把基础设施错误转换为用户可见文案，
/// 与升级前 `ApplicationError::Language { source }` 的语义保持一致）。
pub trait LanguageStorePort: Send + Sync {
    fn language_counts(&self) -> Result<Vec<LanguageCount>, String>;
    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHitModel>, String>;
    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String>;
    fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, String>;
    fn today_plan(&self, language: LanguageCode, now: i64) -> Result<TodayPlan, String>;
    fn review_next(&self, language: LanguageCode, now: i64) -> Result<Option<LanguageItem>, String>;
    fn learning_state(&self, item_id: &str) -> Result<Option<LearningState>, String>;
    fn rate_review(
        &self,
        item_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewOutcome, String>;
    fn toggle_favorite(&self, item_id: &str, now: i64) -> Result<bool, String>;
    fn favorites(&self, limit: usize) -> Result<Vec<LanguageItem>, String>;
    fn set_learning_state(
        &self,
        item_id: &str,
        state: LearningStateKind,
        now: i64,
    ) -> Result<(), String>;
    fn progress(&self) -> Result<serde_json::Value, String>;
    fn favorites_count(&self) -> Result<i64, String>;
    fn sources(&self) -> Result<Vec<LanguageSource>, String>;
    fn manifests(&self) -> Result<Vec<DatasetManifest>, String>;
    fn count_by_source(&self, source_id: &str) -> Result<i64, String>;
    fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, String>;
}

/// 许可证 Gate（纯函数；文本与 `import::gate_license` 一致）。
pub fn verify_source_license(source: &LanguageSource) -> Result<(), String> {
    let license = &source.license;
    if license.is_unknown() {
        return Err("dataset has no declared license (kind=unknown) — DO NOT IMPORT".to_string());
    }
    if !license.is_commercial_safe() {
        return Err(format!(
            "non-commercial source excluded from default pack: {}",
            source.name
        ));
    }
    Ok(())
}