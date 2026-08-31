//! LanguageService：搜索 / 词详情 / Today / Review / 收藏 / 统计（#48-#68 的用例层）。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use devtoolbox_core::language::{
    DatasetManifest, LanguageCode, LanguageItem, LanguageMetadata, LanguageRelation,
    LanguageSource, LearningState, LearningStateKind, Meaning, Pronunciation, ReviewOutcome,
    ReviewRating, SentenceRecord, SpeakingScore, TodayPlan, score as score_speaking,
};
use devtoolbox_infrastructure::language::{ItemDetailRows, LanguageStore, gate_license};
use devtoolbox_infrastructure::now_unix;

use crate::ApplicationError;

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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LanguageSearchHit {
    pub item: LanguageItem,
    /// 命中字段说明（exact/reading/romanization/meaning/text-like/english-index）。
    pub matched: String,
}

/// 词详情（#63 + 来源）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WordDetail {
    pub item: LanguageItem,
    pub meanings: Vec<Meaning>,
    pub pronunciations: Vec<Pronunciation>,
    pub relations: Vec<RelationView>,
    pub examples: Vec<ExampleView>,
    pub sentences: Vec<SentenceRecord>,
    pub state: Option<LearningState>,
    pub favorite: bool,
    pub source: Option<LanguageSource>,
    pub kanji: Option<KanjiView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RelationView {
    pub relation: LanguageRelation,
    pub item: LanguageItem,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExampleView {
    pub text: String,
    pub translation: Option<String>,
    pub source: String,
}

/// 汉字详情（KANJIDIC2 基础元数据）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KanjiView {
    pub stroke_count: Option<i64>,
    pub grade: Option<i64>,
    pub radical: Option<i64>,
    pub jlpt: Option<String>,
}

/// Today 视图（#61）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TodayView {
    pub language: String,
    pub plan: TodayPlan,
    pub languages: Vec<LanguageInfo>,
}

/// 复习卡片。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReviewCard {
    pub item: LanguageItem,
    pub state: LearningStateKind,
}

/// 来源视图（含条目数）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceInfo {
    pub source: LanguageSource,
    pub item_count: i64,
    pub manifest: Option<DatasetManifest>,
}

/// 数据包清单视图。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ManifestInfo {
    pub manifest: DatasetManifest,
    pub item_count: i64,
}

/// 学习进度总览。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProgressView {
    pub total: i64,
    pub mastered: i64,
    pub learning: i64,
    pub reviews: i64,
    pub favorites: i64,
}

pub struct LanguageService {
    store: Arc<Mutex<LanguageStore>>,
}

fn language_error(source: devtoolbox_infrastructure::InfrastructureError) -> ApplicationError {
    ApplicationError::Language { source }
}

impl LanguageService {
    #[must_use]
    pub fn new(store: Arc<Mutex<LanguageStore>>) -> Self {
        Self { store }
    }

    pub fn languages(&self) -> Result<Vec<LanguageInfo>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
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
        let store = self.store.lock().expect("language store poisoned");
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
        let store = self.store.lock().expect("language store poisoned");
        let rows: ItemDetailRows = store.item_detail(id).map_err(language_error)?;
        let Some(item) = rows.item.clone() else {
            return Ok(None);
        };
        let source = rows.item.as_ref().and_then(|item| {
            store
                .source_by_id(&item.source)
                .map_err(language_error)
                .ok()
                .flatten()
        });
        let relations = rows
            .relations
            .iter()
            .zip(rows.related_items.iter())
            .map(|(relation, related)| RelationView {
                relation: relation.clone(),
                item: related.clone(),
                label: relation.kind.label().to_string(),
            })
            .collect();
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
            state: rows.state,
            favorite: rows.favorite,
            source,
            kanji,
        }))
    }

    /// Today（#61）。
    pub fn today(&self, language: &str) -> Result<TodayView, ApplicationError> {
        let languages = self.languages()?;
        let store = self.store.lock().expect("language store poisoned");
        let code = LanguageCode::from_code(language).unwrap_or(LanguageCode::Eng);
        let plan = store.today_plan(code, now_unix()).map_err(language_error)?;
        Ok(TodayView {
            language: code.code().to_string(),
            plan,
            languages,
        })
    }

    /// 下一张复习卡（到期优先，其次新词）。
    pub fn review_next(&self, language: &str) -> Result<Option<ReviewCard>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        let code = LanguageCode::from_code(language).unwrap_or(LanguageCode::Eng);
        let item = store
            .review_next(code, now_unix())
            .map_err(language_error)?;
        let Some(item) = item else { return Ok(None) };
        let state = store
            .learning_state(&item.id)
            .map_err(language_error)?
            .map(|state| state.state)
            .unwrap_or(LearningStateKind::New);
        Ok(Some(ReviewCard { item, state }))
    }

    /// 评分一次复习。
    pub fn rate(
        &self,
        item_id: &str,
        rating: ReviewRating,
    ) -> Result<ReviewOutcome, ApplicationError> {
        let mut store = self.store.lock().expect("language store poisoned");
        store
            .rate_review(item_id, rating, now_unix())
            .map_err(language_error)
    }

    pub fn toggle_favorite(&self, item_id: &str) -> Result<bool, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        store
            .toggle_favorite(item_id, now_unix())
            .map_err(language_error)
    }

    pub fn favorites(&self, limit: usize) -> Result<Vec<LanguageItem>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        store.favorites(limit).map_err(language_error)
    }

    pub fn set_state(
        &self,
        item_id: &str,
        state: LearningStateKind,
    ) -> Result<(), ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        store
            .set_learning_state(item_id, state, now_unix())
            .map_err(language_error)
    }

    pub fn progress(&self) -> Result<ProgressView, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        let value = store.progress().map_err(language_error)?;
        let favorites = store.favorites_count().map_err(language_error)?;
        Ok(ProgressView {
            total: value["total"].as_i64().unwrap_or(0),
            mastered: value["mastered"].as_i64().unwrap_or(0),
            learning: value["learning"].as_i64().unwrap_or(0),
            reviews: value["reviews"].as_i64().unwrap_or(0),
            favorites,
        })
    }

    /// Settings → Language Data（#90）。
    pub fn sources(&self) -> Result<Vec<SourceInfo>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
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

    pub fn manifests(&self) -> Result<Vec<ManifestInfo>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        let manifests = store.manifests().map_err(language_error)?;
        let mut result = Vec::new();
        for manifest in manifests {
            let item_count = store
                .count_by_source(&manifest.source_id)
                .map_err(language_error)?;
            result.push(ManifestInfo {
                manifest,
                item_count,
            });
        }
        Ok(result)
    }

    /// 口语评分（#68，纯函数经命令层调用）。
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

    /// 按语言取句子（听力/口语/Daily Expression，#61/#79）。
    pub fn sentences(
        &self,
        language: &str,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        let code = LanguageCode::from_code(language).unwrap_or(LanguageCode::Eng);
        store
            .sentences_by_language(code, limit)
            .map_err(language_error)
    }

    /// 许可证 Gate 转发（供 Tauri/CLI 使用）。
    pub fn verify_source(&self, source: &LanguageSource) -> Result<(), ApplicationError> {
        gate_license(source).map_err(|error| ApplicationError::License(error.to_string()))
    }

    /// 全部词条语言（帮助前端构建语言下拉）。
    pub fn available_languages(&self) -> Result<Vec<String>, ApplicationError> {
        let store = self.store.lock().expect("language store poisoned");
        let mut languages: Vec<String> = store
            .language_counts()
            .map_err(language_error)?
            .into_iter()
            .map(|count| count.language.code().to_string())
            .collect();
        languages.sort();
        Ok(languages)
    }
}

fn read_kanji(
    meta: &Option<LanguageMetadata>,
    extra: &Option<serde_json::Value>,
) -> Option<KanjiView> {
    let extra = extra.as_ref()?;
    let object = extra.as_object()?;
    let has_kanji = object.contains_key("stroke_count")
        || object.contains_key("grade")
        || object.contains_key("radical")
        || object.contains_key("kanjidic2_jlpt");
    if !has_kanji {
        let _ = meta;
        return None;
    }
    Some(KanjiView {
        stroke_count: object
            .get("stroke_count")
            .and_then(serde_json::Value::as_i64),
        grade: object.get("grade").and_then(serde_json::Value::as_i64),
        radical: object.get("radical").and_then(serde_json::Value::as_i64),
        jlpt: object
            .get("kanjidic2_jlpt")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}
