//! 共享端口适配器：把 infrastructure 的 SQLite store 包装成 application 的 port trait。
//!
//! 此前这些适配器只存在于 `apps/desktop`，服务端（`apps/server`）够不着——于是网页端
//! 拿不到语言等模块的数据，只能退回硬编码假数据。下沉到本模块后，桌面端与网页端
//! 共用同一份装配语义，避免两边漂移。
//!
//! 依赖方向：`infrastructure → application`（application 不依赖 infrastructure，无环）。

use std::sync::Arc;

use devtoolbox_application::language::{
    LanguageCount, LanguageDetailRows, LanguageExample, LanguageStorePort, SearchHitModel,
};
use devtoolbox_core::language::{
    DatasetManifest, LanguageCode, LanguageLearningItem, LanguageSource, Lesson, LessonPosition,
    Mistake, SentenceRecord, SentenceStudy,
};
use parking_lot::Mutex;

use crate::language::LanguageStore;

/// 把 `devtoolbox_infrastructure::language::LanguageStore`（SQLite）包装成
/// application 的语言存储端口，存储语义与升级前一致（短锁、不跨 await）。
pub struct LanguageStoreAdapter {
    store: Arc<Mutex<LanguageStore>>,
}

impl LanguageStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<LanguageStore>>) -> Self {
        Self { store }
    }
}

impl LanguageStorePort for LanguageStoreAdapter {
    fn language_counts(&self) -> Result<Vec<LanguageCount>, String> {
        let store = self.store.lock();
        store
            .language_counts()
            .map(|counts| counts.into_iter().map(map_count).collect())
            .map_err(err_text)
    }

    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHitModel>, String> {
        let store = self.store.lock();
        store
            .search(language, query, limit)
            .map(|hits| {
                hits.into_iter()
                    .map(|hit| SearchHitModel {
                        item: hit.item,
                        matched: hit.matched,
                    })
                    .collect()
            })
            .map_err(err_text)
    }

    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String> {
        let store = self.store.lock();
        store
            .item_detail(id)
            .map(|rows| LanguageDetailRows {
                item: rows.item,
                meanings: rows.meanings,
                pronunciations: rows.pronunciations,
                relations: rows.relations,
                related_items: rows.related_items,
                examples: rows
                    .examples
                    .into_iter()
                    .map(|example| LanguageExample {
                        text: example.text,
                        translation: example.translation,
                        source: example.source,
                    })
                    .collect(),
                sentences: rows.sentences,
                extra: rows.extra,
            })
            .map_err(err_text)
    }

    fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, String> {
        self.store.lock().source_by_id(id).map_err(err_text)
    }

    fn sources(&self) -> Result<Vec<LanguageSource>, String> {
        self.store.lock().sources().map_err(err_text)
    }

    fn manifests(&self) -> Result<Vec<DatasetManifest>, String> {
        self.store.lock().manifests().map_err(err_text)
    }

    fn count_by_source(&self, source_id: &str) -> Result<i64, String> {
        self.store
            .lock()
            .count_by_source(source_id)
            .map_err(err_text)
    }

    fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, String> {
        self.store
            .lock()
            .sentences_by_language(language, limit)
            .map_err(err_text)
    }

    // ---- 学习内容 ----

    fn learning_item(&self, item_id: &str) -> Result<Option<LanguageLearningItem>, String> {
        self.store.lock().learning_item(item_id).map_err(err_text)
    }

    fn next_new_items(
        &self,
        language: LanguageCode,
        exclude: &[String],
        limit: usize,
    ) -> Result<Vec<LanguageLearningItem>, String> {
        self.store
            .lock()
            .next_new_items(language, exclude, limit)
            .map_err(err_text)
    }

    fn learning_items(&self, item_ids: &[String]) -> Result<Vec<LanguageLearningItem>, String> {
        self.store.lock().learning_items(item_ids).map_err(err_text)
    }

    fn sentence_study(&self, sentence_id: &str) -> Result<Option<SentenceStudy>, String> {
        self.store
            .lock()
            .sentence_study(sentence_id)
            .map_err(err_text)
    }

    // ---- Lesson ----

    fn upsert_lesson(&self, lesson: &Lesson) -> Result<(), String> {
        self.store.lock().upsert_lesson(lesson).map_err(err_text)
    }

    fn lesson(&self, lesson_id: &str) -> Result<Option<Lesson>, String> {
        self.store.lock().lesson(lesson_id).map_err(err_text)
    }

    fn lessons(&self, language: Option<LanguageCode>, limit: usize) -> Result<Vec<Lesson>, String> {
        self.store.lock().lessons(language, limit).map_err(err_text)
    }

    fn delete_lesson(&self, lesson_id: &str) -> Result<(), String> {
        self.store.lock().delete_lesson(lesson_id).map_err(err_text)
    }

    fn save_lesson_position(&self, position: &LessonPosition) -> Result<(), String> {
        self.store
            .lock()
            .save_lesson_position(position)
            .map_err(err_text)
    }

    fn lesson_position(&self, lesson_id: &str) -> Result<Option<LessonPosition>, String> {
        self.store
            .lock()
            .lesson_position(lesson_id)
            .map_err(err_text)
    }

    fn recent_lesson_positions(&self, limit: usize) -> Result<Vec<LessonPosition>, String> {
        self.store
            .lock()
            .recent_lesson_positions(limit)
            .map_err(err_text)
    }

    // ---- 错题 ----

    fn record_mistake(&self, mistake: &Mistake, card_id: &str) -> Result<(), String> {
        self.store
            .lock()
            .record_mistake(mistake, card_id)
            .map_err(err_text)
    }

    fn mistakes(&self, limit: usize) -> Result<Vec<Mistake>, String> {
        self.store.lock().mistakes(limit).map_err(err_text)
    }

    fn resolve_mistake(&self, item_id: &str, card_id: &str) -> Result<(), String> {
        self.store
            .lock()
            .resolve_mistake(item_id, card_id)
            .map_err(err_text)
    }

    fn mistake_count(&self) -> Result<i64, String> {
        self.store.lock().mistake_count().map_err(err_text)
    }
}

fn map_count(count: devtoolbox_core::language::LanguageCount) -> LanguageCount {
    LanguageCount {
        language: count.language,
        words: count.words,
        phrases: count.phrases,
        sentences: count.sentences,
        total: count.total,
    }
}

fn err_text(error: crate::error::InfrastructureError) -> String {
    error.to_string()
}
