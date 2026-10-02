//! LanguageService：**词典侧**用例（搜索 / 详情 / 来源 / 句子 / 统计）。
//!
//! 这里**没有**复习、掌握度、进度、收藏——那些是平台能力，由
//! [`LanguageLearningService`](super::learning::LanguageLearningService) 编排
//! 平台的 `LearningService` 完成。本模块过去在本文件里自建了
//! `today` / `review_next` / `rate` / `toggle_favorite` / `set_state` / `progress`
//! 六个方法，与平台 `learning.db` 形成两套互不同步的复习与进度；
//! 这六个方法已删除，前者改用平台。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use devtoolbox_core::language::{
    LanguageCode, LanguageItem, LanguageMetadata, LanguageRelation, LanguageSource, Meaning,
    Pronunciation, SentenceRecord, SpeakingScore, score as score_speaking,
};

use crate::error::ApplicationError;

use super::ports::{LanguageDetailRows, LanguageStorePort};

/// 语言信息（含条目统计，#90）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
    pub native_name: String,
    pub words: i64,
    pub phrases: i64,
    pub sentences: i64,
    pub total: i64,
}

/// 搜索结果命中。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageSearchHit {
    pub item: LanguageItem,
    /// 命中字段说明（让用户知道「为什么命中」）。
    pub matched: String,
}

/// 词详情（#63 + 来源）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WordDetail {
    pub item: LanguageItem,
    pub meanings: Vec<Meaning>,
    pub pronunciations: Vec<Pronunciation>,
    pub relations: Vec<RelationView>,
    pub examples: Vec<ExampleView>,
    pub sentences: Vec<SentenceRecord>,
    pub source: Option<LanguageSource>,
    pub kanji: Option<KanjiView>,
}

/// 关联词视图。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RelationView {
    pub relation: LanguageRelation,
    pub item: LanguageItem,
    pub label: String,
}

/// 例句视图。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExampleView {
    pub text: String,
    pub translation: Option<String>,
    pub source: String,
}

/// 汉字详情（KANJIDIC2 基础元数据）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KanjiView {
    pub readings: Vec<String>,
    pub meanings: Vec<String>,
    pub stroke_count: Option<u8>,
    pub grade: Option<u8>,
    pub jlpt: Option<u8>,
    pub frequency_rank: Option<u16>,
}

/// 来源视图（含条目数）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceInfo {
    pub source: LanguageSource,
    pub item_count: i64,
    pub manifest: Option<devtoolbox_core::language::DatasetManifest>,
}

pub struct LanguageService {
    store: Arc<dyn LanguageStorePort>,
}

fn language_error(message: String) -> ApplicationError {
    ApplicationError::Language { message }
}

impl LanguageService {
    #[must_use]
    pub fn new(store: Arc<dyn LanguageStorePort>) -> Self {
        Self { store }
    }

    pub fn languages(&self) -> Result<Vec<LanguageInfo>, ApplicationError> {
        let store = &*self.store;
        let counts = store.language_counts().map_err(language_error)?;
        let mut table: std::collections::HashMap<LanguageCode, (i64, i64, i64, i64)> =
            std::collections::HashMap::new();
        for count in counts {
            table.insert(
                count.language,
                (count.words, count.phrases, count.sentences, count.total),
            );
        }
        let codes = [
            LanguageCode::Eng,
            LanguageCode::Jap,
            LanguageCode::Zho,
            LanguageCode::Yue,
        ];
        Ok(codes
            .into_iter()
            .map(|code| {
                let (words, phrases, sentences, total) =
                    table.get(&code).copied().unwrap_or((0, 0, 0, 0));
                LanguageInfo {
                    code: code.code().to_string(),
                    name: code.label().to_string(),
                    native_name: code.native_label().to_string(),
                    words,
                    phrases,
                    sentences,
                    total,
                }
            })
            .collect())
    }

    /// 统一搜索（#49：text/reading/romanization/meaning + 英语索引）。
    pub fn search(
        &self,
        language: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LanguageSearchHit>, ApplicationError> {
        let store = &*self.store;
        let lang = language.and_then(LanguageCode::from_code);
        let hits = store.search(lang, query, limit).map_err(language_error)?;
        Ok(hits
            .into_iter()
            .map(|hit| LanguageSearchHit {
                item: hit.item,
                matched: hit.matched,
            })
            .collect())
    }

    /// 词详情（#63）。
    pub fn detail(&self, id: &str) -> Result<Option<WordDetail>, ApplicationError> {
        let store = &*self.store;
        let rows: LanguageDetailRows = store.item_detail(id).map_err(language_error)?;
        let Some(item) = rows.item.clone() else {
            return Ok(None);
        };
        let source = store
            .source_by_id(&item.source)
            .map_err(language_error)
            .ok()
            .flatten();
        // 逐条按 relation.to_item_id 取关联词：旧实现用
        // `relations.iter().zip(related_items.iter())`，一旦有悬空关系就会
        // 让**之后所有** relation 与词错位（标签贴到别的词上）。
        let mut relations = Vec::with_capacity(rows.relations.len());
        for relation in &rows.relations {
            let Some(related) = rows
                .related_items
                .iter()
                .find(|candidate| candidate.id == relation.to_item_id)
            else {
                continue;
            };
            relations.push(RelationView {
                relation: relation.clone(),
                item: related.clone(),
                label: relation.kind.label().to_string(),
            });
        }
        let examples = rows
            .examples
            .into_iter()
            .map(|example| ExampleView {
                text: example.text,
                translation: example.translation,
                source: example.source,
            })
            .collect();
        let kanji = read_kanji(&rows.item.clone().and_then(|item| item.meta), &rows.extra);
        Ok(Some(WordDetail {
            item,
            meanings: rows.meanings,
            pronunciations: rows.pronunciations,
            relations,
            examples,
            sentences: rows.sentences,
            source,
            kanji,
        }))
    }

    /// 按语言取句子（听力/口语/Daily Expression，#61/#79）。
    pub fn sentences(
        &self,
        language: &str,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, ApplicationError> {
        let store = &*self.store;
        let code = LanguageCode::from_code(language).unwrap_or(LanguageCode::Eng);
        store
            .sentences_by_language(code, limit)
            .map_err(language_error)
    }

    /// Settings → Language Data（#90）。
    pub fn sources(&self) -> Result<Vec<SourceInfo>, ApplicationError> {
        let store = &*self.store;
        let sources = store.sources().map_err(language_error)?;
        let manifests = store.manifests().map_err(language_error)?;
        let mut result = Vec::new();
        for source in sources {
            let item_count = store.count_by_source(&source.id).map_err(language_error)?;
            let manifest = manifests
                .iter()
                .find(|manifest| manifest.source_id == source.id)
                .cloned();
            result.push(SourceInfo {
                source,
                item_count,
                manifest,
            });
        }
        Ok(result)
    }

    /// 口语反馈（#68，纯函数经命令层调用）。
    #[must_use]
    pub fn speaking_feedback(
        &self,
        target: &str,
        transcript: &str,
        duration_ms: u64,
        target_ms: u64,
        long_pauses_ms: &[u64],
    ) -> SpeakingScore {
        score_speaking(target, transcript, duration_ms, target_ms, long_pauses_ms)
    }
}

fn read_kanji(
    meta: &Option<LanguageMetadata>,
    extra: &Option<serde_json::Value>,
) -> Option<KanjiView> {
    let extra = extra.as_ref()?;
    let grade = extra.get("grade").and_then(serde_json::Value::as_u64);
    let jlpt = extra.get("jlpt").and_then(serde_json::Value::as_u64);
    let stroke_count = extra.get("strokes").and_then(serde_json::Value::as_u64);
    let frequency_rank = extra.get("freq_rank").and_then(serde_json::Value::as_u64);
    let readings: Vec<String> = extra
        .get("readings")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let meanings: Vec<String> = extra
        .get("meanings")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    if grade.is_none() && jlpt.is_none() && readings.is_empty() && meanings.is_empty() {
        let _ = meta;
        return None;
    }
    Some(KanjiView {
        readings,
        meanings,
        stroke_count: stroke_count.map(|value| value.min(u64::from(u8::MAX)) as u8),
        grade: grade.map(|value| value.min(u64::from(u8::MAX)) as u8),
        jlpt: jlpt.map(|value| value.min(u64::from(u8::MAX)) as u8),
        frequency_rank: frequency_rank.map(|value| value.min(u64::from(u16::MAX)) as u16),
    })
}
