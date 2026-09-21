//! 测试用 Fake 存储（Gate 7.5：应用层测试只依赖 `LanguageStorePort`，无 SQLite）。
//!
//! 覆盖验收搜索 / 详情 / Today / Review / 收藏 / 进度 / 来源 / 句子等真实使用面，
//! 数据完全内置，不访问任何文件或网络。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use devtoolbox_core::language::{
    DatasetManifest, LanguageCode, LanguageItem, LanguageItemType, LanguageMetadata,
    LanguageRelation, LanguageRelationKind, LanguageSource, LearningState, LearningStateKind,
    MandarinMetadata, Meaning, Pronunciation, PronunciationScheme, ReviewOutcome, ReviewRating,
    ReviewScheduler, SentenceRecord, SourceLicense, TodayPlan,
};

use super::ports::{
    LanguageCount, LanguageDetailRows, LanguageExample, LanguageStorePort, SearchHitModel,
};

/// 可变状态（收藏 / 学习记录 / 复习队列）。
#[derive(Default)]
pub struct FakeState {
    pub favorites: HashSet<String>,
    pub states: HashMap<String, LearningState>,
    pub review_queue: Vec<String>,
}

pub struct FakeLanguageStore {
    pub items: Vec<LanguageItem>,
    pub details: HashMap<String, LanguageDetailRows>,
    pub sources: Vec<LanguageSource>,
    pub manifests: Vec<DatasetManifest>,
    pub counts: Vec<LanguageCount>,
    pub sentences: Vec<SentenceRecord>,
    pub state: Mutex<FakeState>,
}

impl FakeLanguageStore {
    #[must_use]
    pub fn new() -> Self {
        let items = vec![
            LanguageItem {
                id: "jmdict:taberu".into(),
                language: LanguageCode::Jap,
                item_type: LanguageItemType::Word,
                text: "食べる".into(),
                reading: Some("たべる".into()),
                romanization: Some("taberu".into()),
                meta: None,
                source: "jmdict".into(),
            },
            LanguageItem {
                id: "wn:reservation".into(),
                language: LanguageCode::Eng,
                item_type: LanguageItemType::Word,
                text: "reservation".into(),
                reading: None,
                romanization: None,
                meta: None,
                source: "oewn".into(),
            },
            LanguageItem {
                id: "cedict:lvxing".into(),
                language: LanguageCode::Zho,
                item_type: LanguageItemType::Word,
                text: "旅行".into(),
                reading: None,
                romanization: Some("lu:3 xing2".into()),
                meta: Some(LanguageMetadata::Mandarin(MandarinMetadata {
                    simplified: Some("旅行".into()),
                    traditional: Some("旅行".into()),
                    pinyin: Some("lu:3 xing2".into()),
                    tones: vec![3, 2],
                    hsk: None,
                })),
                source: "cc_cedict".into(),
            },
            LanguageItem {
                id: "whk:sik6faan6".into(),
                language: LanguageCode::Yue,
                item_type: LanguageItemType::Word,
                text: "食飯".into(),
                reading: None,
                romanization: Some("sik6 faan6".into()),
                meta: None,
                source: "words_hk".into(),
            },
            LanguageItem {
                id: "tatoeba:taberu-sentence".into(),
                language: LanguageCode::Jap,
                item_type: LanguageItemType::Sentence,
                text: "私はご飯を食べる。".into(),
                reading: None,
                romanization: None,
                meta: None,
                source: "tatoeba".into(),
            },
        ];

        let mut details = HashMap::new();
        details.insert(
            "jmdict:taberu".into(),
            LanguageDetailRows {
                item: items
                    .iter()
                    .find(|item| item.id == "jmdict:taberu")
                    .cloned(),
                meanings: vec![Meaning {
                    id: "m1".into(),
                    item_id: "jmdict:taberu".into(),
                    pos: Some("verb".into()),
                    gloss: Some("to eat".into()),
                    raw: None,
                    sense_key: None,
                    lang: None,
                    rank: 0,
                    source: "jmdict".into(),
                }],
                pronunciations: vec![Pronunciation {
                    id: "p1".into(),
                    item_id: "jmdict:taberu".into(),
                    scheme: PronunciationScheme::Kana,
                    phonemes: "たべる".into(),
                    tone: None,
                    variant: None,
                    source: "jmdict".into(),
                }],
                relations: vec![LanguageRelation {
                    id: "r1".into(),
                    from_item_id: "jmdict:taberu".into(),
                    to_item_id: "whk:sik6faan6".into(),
                    kind: LanguageRelationKind::TranslationOf,
                    note: None,
                    source: "test".into(),
                }],
                related_items: vec![items[3].clone()],
                examples: vec![LanguageExample {
                    text: "ご飯を食べる。".into(),
                    translation: Some("I eat rice.".into()),
                    source: "test".into(),
                }],
                sentences: vec![SentenceRecord {
                    sentence_id: "s1".into(),
                    language: LanguageCode::Jap,
                    text: "ご飯を食べる。".into(),
                    author: None,
                    license: "CC BY 2.0 FR".into(),
                    source: "tatoeba_common".into(),
                }],
                state: None,
                favorite: false,
                extra: None,
            },
        );
        details.insert(
            "wn:reservation".into(),
            LanguageDetailRows {
                item: items
                    .iter()
                    .find(|item| item.id == "wn:reservation")
                    .cloned(),
                meanings: vec![Meaning {
                    id: "m2".into(),
                    item_id: "wn:reservation".into(),
                    pos: Some("noun".into()),
                    gloss: Some("an arrangement made in advance".into()),
                    raw: None,
                    sense_key: Some("reservation%1:10:00::".into()),
                    lang: None,
                    rank: 1,
                    source: "oewn".into(),
                }],
                pronunciations: vec![Pronunciation {
                    id: "p2".into(),
                    item_id: "wn:reservation".into(),
                    scheme: PronunciationScheme::Arpabet,
                    phonemes: "R EH Z ER V EY SH AH N".into(),
                    tone: None,
                    variant: None,
                    source: "cmudict".into(),
                }],
                relations: vec![],
                related_items: vec![],
                examples: Vec::new(),
                sentences: Vec::new(),
                state: None,
                favorite: false,
                extra: Some(serde_json::json!({
                    "stroke_count": null,
                    "grade": null,
                    "radical": null,
                    "kanjidic2_jlpt": null,
                })),
            },
        );
        details.insert(
            "ced:lvxing".into(),
            LanguageDetailRows {
                item: items.iter().find(|item| item.id == "ced:lvxing").cloned(),
                meanings: vec![Meaning {
                    id: "m3".into(),
                    item_id: "ced:lvxing".into(),
                    pos: None,
                    gloss: Some("to travel; to tour".into()),
                    raw: Some("旅行 /ly:3 xing2/".into()),
                    sense_key: None,
                    lang: None,
                    rank: 0,
                    source: "cc_cedict".into(),
                }],
                pronunciations: vec![Pronunciation {
                    id: "p3".into(),
                    item_id: "ced:lvxing".into(),
                    scheme: PronunciationScheme::Pinyin,
                    phonemes: "lu:3 xing2".into(),
                    tone: None,
                    variant: None,
                    source: "cc_cedict".into(),
                }],
                relations: vec![],
                related_items: vec![],
                examples: Vec::new(),
                sentences: Vec::new(),
                state: None,
                favorite: false,
                extra: None,
            },
        );
        details.insert(
            "whk:sik6faan6".into(),
            LanguageDetailRows {
                item: items
                    .iter()
                    .find(|item| item.id == "whk:sik6faan6")
                    .cloned(),
                meanings: vec![Meaning {
                    id: "m4".into(),
                    item_id: "whk:sik6faan6".into(),
                    pos: None,
                    gloss: Some("rice; food; meal".into()),
                    raw: None,
                    sense_key: None,
                    lang: None,
                    rank: 0,
                    source: "words_hk".into(),
                }],
                pronunciations: vec![Pronunciation {
                    id: "p4".into(),
                    item_id: "whk:sik6faan6".into(),
                    scheme: PronunciationScheme::Jyutping,
                    phonemes: "sik6 faan6".into(),
                    tone: None,
                    variant: None,
                    source: "words_hk".into(),
                }],
                relations: vec![],
                related_items: vec![],
                examples: Vec::new(),
                sentences: Vec::new(),
                state: None,
                favorite: false,
                extra: None,
            },
        );

        let sources: Vec<LanguageSource> = [
            (
                "jmdict",
                "JMdict",
                "https://www.edrdg.org/jmdict/",
                SourceLicense::cc_by_sa(),
            ),
            (
                "tatoeba",
                "Tatoeba",
                "https://tatoeba.org",
                SourceLicense::cc_by(),
            ),
            (
                "tatoeba_common",
                "Tatoeba Common",
                "https://tatoeba.org",
                SourceLicense::cc_by(),
            ),
            (
                "oewn",
                "OEWN",
                "https://en-word.net",
                SourceLicense::public_domain(),
            ),
            (
                "cmudict",
                "CMUdict",
                "http://www.speech.cs.cmu.edu",
                SourceLicense::public_domain(),
            ),
            (
                "words_hk",
                "words.hk",
                "https://words.hk",
                SourceLicense::public_domain(),
            ),
            (
                "cc_cedict",
                "CC-CEDICT (CC-Canto)",
                "https://cc-cedict.org",
                SourceLicense::cc_by_sa(),
            ),
            (
                "kanjidic2",
                "KANJIDIC2",
                "https://www.edrdg.org/kanjidic/",
                SourceLicense::cc_by_sa(),
            ),
            (
                "jmdict",
                "Default pack",
                "https://github.com/HansenWuuuu/self-tools",
                SourceLicense::cc_by(),
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(_index, (id, name, homepage, license))| LanguageSource {
            id: id.to_string(),
            name: name.to_string(),
            homepage: homepage.to_string(),
            download_source: homepage.to_string(),
            dataset_version: "2024-01".to_string(),
            downloaded_at: None,
            license,
            license_url: None,
            attribution: "test fixture".to_string(),
            commercial_use: license.commercial_use_allowed,
            redistribution: license.redistribution_allowed,
            notes: None,
        })
        .collect();

        let counts = vec![
            LanguageCount {
                language: LanguageCode::Eng,
                words: 10,
                phrases: 1,
                sentences: 1,
                total: 12,
            },
            LanguageCount {
                language: LanguageCode::Jap,
                words: 8,
                phrases: 1,
                sentences: 1,
                total: 10,
            },
            LanguageCount {
                language: LanguageCode::Zho,
                words: 6,
                phrases: 1,
                sentences: 0,
                total: 7,
            },
            LanguageCount {
                language: LanguageCode::Yue,
                words: 6,
                phrases: 1,
                sentences: 0,
                total: 7,
            },
        ];

        let sentences = vec![SentenceRecord {
            sentence_id: "s1".into(),
            language: LanguageCode::Jap,
            text: "ご飯を食べる。".into(),
            author: None,
            license: "CC BY 2.0 FR".into(),
            source: "tatoeba_common".into(),
        }];

        let store = Self {
            items,
            details,
            sources,
            manifests: vec![DatasetManifest {
                id: "pack-default".into(),
                name: "default".into(),
                language: "multi".into(),
                version: "1.0".into(),
                source_id: "jit".into(),
                downloaded_at: None,
                checksum: None,
                raw_file: None,
                record_count: 10,
                importer_version: 1,
                imported_at: 1,
            }],
            counts,
            sentences,
            state: Mutex::new(FakeState::default()),
        };
        store
            .state
            .lock()
            .expect("fake state poisoned")
            .review_queue
            .push("wn:reservation".into());
        store
    }

    pub fn item_by_id(&self, id: &str) -> Option<LanguageItem> {
        self.items.iter().find(|item| item.id == id).cloned()
    }
}

impl Default for FakeLanguageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageStorePort for FakeLanguageStore {
    fn language_counts(&self) -> Result<Vec<LanguageCount>, String> {
        Ok(self.counts.clone())
    }

    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHitModel>, String> {
        let lower = query.to_lowercase();

        let mut matched: Vec<SearchHitModel> = Vec::new();
        for item in &self.items {
            if let Some(code) = language {
                if item.language != code {
                    continue;
                }
            }
            let kind_matched = if item.text == query {
                Some("exact")
            } else if item
                .reading
                .as_deref()
                .map(|value| value.contains(&lower))
                .unwrap_or(false)
            {
                Some("reading")
            } else if item
                .romanization
                .as_deref()
                .map(|value| value == lower)
                .unwrap_or(false)
            {
                Some("romanization")
            } else if item.item_type == LanguageItemType::Sentence && item.text.contains(query) {
                Some("text-like")
            } else {
                None
            };
            if let Some(kind) = kind_matched {
                matched.push(SearchHitModel {
                    item: item.clone(),
                    matched: kind.to_string(),
                });
            }
        }
        // 兜底：按释义收录命中（英文索引 https://words.hk 用例）
        if matched.is_empty() && !lower.is_empty() {
            for item in &self.items {
                let Some(detail) = self.details.get(&item.id) else {
                    continue;
                };
                if detail.meanings.iter().any(|meaning| {
                    meaning
                        .gloss
                        .as_deref()
                        .map(|g| g.to_lowercase().contains(&lower))
                        .unwrap_or(false)
                }) {
                    matched.push(SearchHitModel {
                        item: item.clone(),
                        matched: "meaning".to_string(),
                    });
                }
            }
        }
        matched.truncate(limit);
        Ok(matched)
    }

    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String> {
        Ok(self.details.get(id).cloned().unwrap_or_else(|| {
            let mut rows = LanguageDetailRows::default();
            rows.item = self.item_by_id(id);
            rows
        }))
    }

    fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, String> {
        Ok(self.sources.iter().find(|source| source.id == id).cloned())
    }

    fn today_plan(&self, language: LanguageCode, _now: i64) -> Result<TodayPlan, String> {
        Ok(TodayPlan {
            due_reviews: 0,
            new_words: if language == LanguageCode::Jap { 10 } else { 5 },
            sentences: 5,
            listening: 3,
            speaking: 2,
            total: 20,
        })
    }

    fn review_next(
        &self,
        language_code: LanguageCode,
        _now: i64,
    ) -> Result<Option<LanguageItem>, String> {
        let state = self.state.lock().expect("fake state poisoned");
        let id = state.review_queue.iter().find(|id| {
            self.item_by_id(id)
                .map(|item| item.language == language_code)
                .unwrap_or(false)
        });
        Ok(id.and_then(|id| self.item_by_id(id)))
    }

    fn learning_state(&self, item_id: &str) -> Result<Option<LearningState>, String> {
        let state = self.state.lock().expect("fake state poisoned");
        Ok(state.states.get(item_id).cloned())
    }

    fn rate_review(
        &self,
        item_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewOutcome, String> {
        let mut state = self.state.lock().expect("fake state poisoned");
        let current = state
            .states
            .get(item_id)
            .cloned()
            .unwrap_or_else(|| LearningState::new(item_id, now));
        let outcome = ReviewScheduler::schedule(&current, rating, now);
        let updated = LearningState {
            state: outcome.state,
            interval_days: outcome.interval_days,
            ease: outcome.ease,
            due_at: outcome.due_at,
            review_count: current.review_count + 1,
            lapses: outcome.lapses,
            ..current
        };
        state.states.insert(item_id.to_string(), updated);
        Ok(outcome)
    }

    fn toggle_favorite(&self, item_id: &str, _now: i64) -> Result<bool, String> {
        let mut state = self.state.lock().expect("fake state poisoned");
        if state.favorites.remove(item_id) {
            Ok(false)
        } else {
            state.favorites.insert(item_id.to_string());
            Ok(true)
        }
    }

    fn favorites(&self, limit: usize) -> Result<Vec<LanguageItem>, String> {
        let state = self.state.lock().expect("fake state poisoned");
        Ok(self
            .items
            .iter()
            .filter(|item| state.favorites.contains(&item.id))
            .take(limit)
            .cloned()
            .collect())
    }

    fn set_learning_state(
        &self,
        item_id: &str,
        state: LearningStateKind,
        now: i64,
    ) -> Result<(), String> {
        let mut states = self.state.lock().expect("fake state poisoned");
        let current = states
            .states
            .get(item_id)
            .cloned()
            .unwrap_or_else(|| LearningState::new(item_id, now));
        let updated = LearningState { state, ..current };
        states.states.insert(item_id.to_string(), updated);
        Ok(())
    }

    fn progress(&self) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "total": self.items.len(),
            "mastered": 0,
            "learning": 0,
            "reviews": 0,
        }))
    }

    fn favorites_count(&self) -> Result<i64, String> {
        let state = self.state.lock().expect("fake state poisoned");
        Ok(state.favorites.len() as i64)
    }

    fn sources(&self) -> Result<Vec<LanguageSource>, String> {
        Ok(self.sources.clone())
    }

    fn manifests(&self) -> Result<Vec<DatasetManifest>, String> {
        Ok(self.manifests.clone())
    }

    fn count_by_source(&self, source_id: &str) -> Result<i64, String> {
        Ok(self
            .items
            .iter()
            .filter(|item| item.source == source_id)
            .count() as i64)
    }

    fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, String> {
        Ok(self
            .sentences
            .iter()
            .filter(|sentence| sentence.language == language)
            .take(limit)
            .cloned()
            .collect())
    }
}
