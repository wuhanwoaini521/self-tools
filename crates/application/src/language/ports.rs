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
    DatasetManifest, LanguageCode, LanguageItem, LanguageLearningItem, LanguageRelation,
    LanguageSource, Lesson, LessonPosition, Meaning, Mistake, Pronunciation, SentenceRecord,
    SentenceStudy,
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
/// 端口按**职责**划分，而非按表划分：
///
/// - **词典数据**（条目 / 释义 / 发音 / 来源 / 句子）由本端口提供 —— 这些是语言
///   模块独有的内容形态。
/// - **学习状态**（掌握度 / 复习排期 / 事件流 / 合集）**不在**本端口：那是平台
///   `LearningStorePort` 的能力，由 `LanguageLearningService` 组合调用。端口里
///   出现 `today_plan` / `review_next` / `rate_review` / `progress` / `favorites`
///   正是本模块过去自建第二套复习与进度的根源。
/// - **学习内容**（Lesson / 错题 / 句子拆解）是语言特有的组织方式，留在本端口。
pub trait LanguageStorePort: Send + Sync {
    // ---- 词典数据 ----
    fn language_counts(&self) -> Result<Vec<LanguageCount>, String>;
    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHitModel>, String>;
    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String>;
    fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, String>;
    fn sources(&self) -> Result<Vec<LanguageSource>, String>;
    fn manifests(&self) -> Result<Vec<DatasetManifest>, String>;
    fn count_by_source(&self, source_id: &str) -> Result<i64, String>;
    fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, String>;

    // ---- 学习内容 ----
    /// 适配单个学习条目（`None` = 词典中不存在）。
    fn learning_item(&self, item_id: &str) -> Result<Option<LanguageLearningItem>, String>;
    /// 批量适配（一次调用内完成，避免调用方 N+1）。
    fn learning_items(&self, item_ids: &[String]) -> Result<Vec<LanguageLearningItem>, String>;
    /// 句子学习视图（原文 / 译意 / 词典切分）。
    fn sentence_study(&self, sentence_id: &str) -> Result<Option<SentenceStudy>, String>;
    /// 未学过的词。`exclude` 是调用方从平台 `learning_progress` 取到的已学 id
    /// （单点真相在平台，语言库看不到进度）。
    fn next_new_items(
        &self,
        language: LanguageCode,
        exclude: &[String],
        limit: usize,
    ) -> Result<Vec<LanguageLearningItem>, String>;

    // ---- Lesson ----
    fn upsert_lesson(&self, lesson: &Lesson) -> Result<(), String>;
    fn lesson(&self, lesson_id: &str) -> Result<Option<Lesson>, String>;
    fn lessons(&self, language: Option<LanguageCode>, limit: usize) -> Result<Vec<Lesson>, String>;
    fn delete_lesson(&self, lesson_id: &str) -> Result<(), String>;
    fn save_lesson_position(&self, position: &LessonPosition) -> Result<(), String>;
    fn lesson_position(&self, lesson_id: &str) -> Result<Option<LessonPosition>, String>;
    fn recent_lesson_positions(&self, limit: usize) -> Result<Vec<LessonPosition>, String>;

    // ---- 错题 ----
    /// `card_id` 参与去重：同一张卡重复答错只累加 `error_count`。
    fn record_mistake(&self, mistake: &Mistake, card_id: &str) -> Result<(), String>;
    fn mistakes(&self, limit: usize) -> Result<Vec<Mistake>, String>;
    fn resolve_mistake(&self, item_id: &str, card_id: &str) -> Result<(), String>;
    fn mistake_count(&self) -> Result<i64, String>;
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
