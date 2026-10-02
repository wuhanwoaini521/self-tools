//! 应用层测试替身。
//!
//! **刻意保持最小**：只实现 `LanguageStorePort` 里用例真正要碰的方法，其余
//! 返回空值。旧版这里把搜索排序、复习队列、Today 计划全部**重新实现**了一遍
//! （约 650 行），于是应用层测试验证的是一份平行实现而非出货代码——真正的
//! `LanguageStore::search` 与复习写入路径当时零覆盖。
//!
//! 现在：真实行为由 `crates/infrastructure` 的 SQLite 测试覆盖；应用层只测
//! **用例编排**（调用了谁、参数对不对、结果怎么组装）。
#![allow(clippy::uninlined_format_args)]

use parking_lot::Mutex;
use std::collections::HashMap;

use devtoolbox_core::language::{
    DatasetManifest, Difficulty, LanguageCode, LanguageItem, LanguageItemType,
    LanguageLearningItem, LanguageSource, Lesson, LessonPosition, Mistake, SentenceRecord,
    SentenceStudy,
};

use super::ports::{LanguageCount, LanguageDetailRows, LanguageStorePort, SearchHitModel};

/// 应用层测试用的内容存储。
#[derive(Default)]
pub struct FakeLanguageStore {
    items: Mutex<Vec<LanguageItem>>,
    details: Mutex<HashMap<String, LanguageDetailRows>>,
    sources: Mutex<Vec<LanguageSource>>,
    manifests: Mutex<Vec<DatasetManifest>>,
    counts: Mutex<Vec<LanguageCount>>,
    lessons: Mutex<HashMap<String, Lesson>>,
    positions: Mutex<HashMap<String, LessonPosition>>,
    mistakes: Mutex<Vec<Mistake>>,
    /// 记录被调用过的方法，用于断言「用例是否走了正确的能力面」。
    calls: Mutex<Vec<&'static str>>,
}

impl FakeLanguageStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注入一个词条（并同步一条最简学习条目视图）。
    pub fn insert_word(&self, id: &str, language: LanguageCode, text: &str, meaning: &str) {
        let item = LanguageItem::plain(
            language,
            LanguageItemType::Word,
            id.to_string(),
            text.to_string(),
            "jmdict".to_string(),
        );
        self.items.lock().push(item);
        self.details.lock().insert(
            id.to_string(),
            LanguageDetailRows {
                item: self.items.lock().iter().find(|item| item.id == id).cloned(),
                meanings: vec![devtoolbox_core::language::Meaning {
                    id: format!("{id}:m1"),
                    item_id: id.to_string(),
                    pos: None,
                    gloss: Some(meaning.to_string()),
                    raw: None,
                    sense_key: None,
                    lang: None,
                    rank: 0,
                    source: "jmdict".to_string(),
                }],
                ..LanguageDetailRows::default()
            },
        );
    }

    pub fn insert_sentence(&self, id: &str, language: LanguageCode, text: &str) {
        self.items.lock().push(LanguageItem::plain(
            language,
            LanguageItemType::Sentence,
            id.to_string(),
            text.to_string(),
            "tatoeba".to_string(),
        ));
    }

    /// 断言某个能力被调用过（用例编排测试用）。
    pub fn called(&self, method: &'static str) -> bool {
        self.calls.lock().contains(&method)
    }

    fn record(&self, method: &'static str) {
        self.calls.lock().push(method);
    }

    fn learning_item_for(&self, id: &str) -> Option<LanguageLearningItem> {
        let item = self
            .items
            .lock()
            .iter()
            .find(|item| item.id == id)
            .cloned()?;
        let meaning = self
            .details
            .lock()
            .get(id)
            .and_then(|rows| rows.meanings.first())
            .and_then(|meaning| meaning.gloss.clone());
        LanguageLearningItem::from_item(&item, meaning, item.reading.clone(), Difficulty::Unknown)
    }
}

impl LanguageStorePort for FakeLanguageStore {
    // ---- 词典数据 ----

    fn language_counts(&self) -> Result<Vec<LanguageCount>, String> {
        self.record("language_counts");
        Ok(self.counts.lock().clone())
    }

    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHitModel>, String> {
        self.record("search");
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let items = self.items.lock().clone();
        Ok(items
            .into_iter()
            .filter(|item| language.is_none_or(|code| code == item.language))
            .filter(|item| {
                item.text.to_lowercase().contains(&needle)
                    || item
                        .reading
                        .as_ref()
                        .is_some_and(|reading| reading.to_lowercase().contains(&needle))
                    || item
                        .romanization
                        .as_ref()
                        .is_some_and(|roman| roman.to_lowercase().contains(&needle))
            })
            .take(limit)
            .map(|item| SearchHitModel {
                matched: "text".to_string(),
                item,
            })
            .collect())
    }

    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String> {
        self.record("item_detail");
        Ok(self.details.lock().get(id).cloned().unwrap_or_default())
    }

    fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, String> {
        self.record("source_by_id");
        Ok(self
            .sources
            .lock()
            .iter()
            .find(|source| source.id == id)
            .cloned())
    }

    fn sources(&self) -> Result<Vec<LanguageSource>, String> {
        self.record("sources");
        Ok(self.sources.lock().clone())
    }

    fn manifests(&self) -> Result<Vec<DatasetManifest>, String> {
        self.record("manifests");
        Ok(self.manifests.lock().clone())
    }

    fn count_by_source(&self, _source_id: &str) -> Result<i64, String> {
        self.record("count_by_source");
        Ok(0)
    }

    fn sentences_by_language(
        &self,
        _language: LanguageCode,
        _limit: usize,
    ) -> Result<Vec<SentenceRecord>, String> {
        self.record("sentences_by_language");
        Ok(Vec::new())
    }

    // ---- 学习内容 ----

    fn learning_item(&self, item_id: &str) -> Result<Option<LanguageLearningItem>, String> {
        self.record("learning_item");
        Ok(self.learning_item_for(item_id))
    }

    fn learning_items(&self, item_ids: &[String]) -> Result<Vec<LanguageLearningItem>, String> {
        self.record("learning_items");
        Ok(item_ids
            .iter()
            .filter_map(|id| self.learning_item_for(id))
            .collect())
    }

    /// 未学过的词：夹具里的全部词条，排除 `exclude`。
    fn next_new_items(
        &self,
        language: LanguageCode,
        exclude: &[String],
        limit: usize,
    ) -> Result<Vec<LanguageLearningItem>, String> {
        self.record("next_new_items");
        let items = self.items.lock().clone();
        Ok(items
            .into_iter()
            .filter(|item| item.language == language)
            .filter(|item| item.item_type == devtoolbox_core::language::LanguageItemType::Word)
            .filter(|item| !exclude.contains(&item.id))
            .filter_map(|item| self.learning_item_for(&item.id))
            .take(limit)
            .collect())
    }

    fn sentence_study(&self, sentence_id: &str) -> Result<Option<SentenceStudy>, String> {
        self.record("sentence_study");
        let found = self
            .items
            .lock()
            .iter()
            .find(|item| item.id == sentence_id)
            .cloned();
        Ok(found.map(|item| SentenceStudy::new(item.id, item.language, item.text)))
    }

    // ---- Lesson ----

    fn upsert_lesson(&self, lesson: &Lesson) -> Result<(), String> {
        self.record("upsert_lesson");
        self.lessons
            .lock()
            .insert(lesson.id.clone(), lesson.clone());
        Ok(())
    }

    fn lesson(&self, lesson_id: &str) -> Result<Option<Lesson>, String> {
        self.record("lesson");
        Ok(self.lessons.lock().get(lesson_id).cloned())
    }

    fn lessons(
        &self,
        _language: Option<LanguageCode>,
        _limit: usize,
    ) -> Result<Vec<Lesson>, String> {
        self.record("lessons");
        Ok(self.lessons.lock().values().cloned().collect())
    }

    fn delete_lesson(&self, lesson_id: &str) -> Result<(), String> {
        self.record("delete_lesson");
        self.lessons.lock().remove(lesson_id);
        Ok(())
    }

    fn save_lesson_position(&self, position: &LessonPosition) -> Result<(), String> {
        self.record("save_lesson_position");
        self.positions
            .lock()
            .insert(position.lesson_id.clone(), position.clone());
        Ok(())
    }

    fn lesson_position(&self, lesson_id: &str) -> Result<Option<LessonPosition>, String> {
        self.record("lesson_position");
        Ok(self.positions.lock().get(lesson_id).cloned())
    }

    fn recent_lesson_positions(&self, _limit: usize) -> Result<Vec<LessonPosition>, String> {
        self.record("recent_lesson_positions");
        let mut positions: Vec<LessonPosition> = self.positions.lock().values().cloned().collect();
        positions.sort_by_key(|entry| std::cmp::Reverse(entry.updated_at));
        Ok(positions)
    }

    // ---- 错题 ----

    fn record_mistake(&self, mistake: &Mistake, card_id: &str) -> Result<(), String> {
        self.record("record_mistake");
        let mut mistakes = self.mistakes.lock();
        // 与真实实现同语义：同 (item_id, card_id) 累加而非重复插入。
        // 行的主键是 `card_id`（写入时存入 `Mistake.id`），因此比对的是
        // `entry.id == card_id`，而不是与参数自身比较。
        if let Some(existing) = mistakes
            .iter_mut()
            .find(|entry| entry.item_id == mistake.item_id && entry.id == card_id)
        {
            existing.error_count += 1;
            existing.user_answer = mistake.user_answer.clone();
            existing.last_missed_at = mistake.last_missed_at;
        } else {
            let mut stored = mistake.clone();
            stored.id = card_id.to_string();
            mistakes.push(stored);
        }
        Ok(())
    }

    fn mistakes(&self, limit: usize) -> Result<Vec<Mistake>, String> {
        self.record("mistakes");
        let mut all = self.mistakes.lock().clone();
        all.sort_by_key(|entry| std::cmp::Reverse(entry.last_missed_at));
        all.truncate(limit);
        Ok(all)
    }

    fn resolve_mistake(&self, item_id: &str, card_id: &str) -> Result<(), String> {
        self.record("resolve_mistake");
        self.mistakes
            .lock()
            .retain(|entry| !(entry.item_id == item_id && entry.id == card_id));
        Ok(())
    }

    fn mistake_count(&self) -> Result<i64, String> {
        self.record("mistake_count");
        Ok(self.mistakes.lock().len() as i64)
    }
}
