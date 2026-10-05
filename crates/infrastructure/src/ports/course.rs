//! `CourseStorePort` 适配器：把 `LanguageStore` 的 course/dict 表组包装成
//! application 端口（保持依赖方向 infrastructure → application）。

use std::sync::Arc;

use devtoolbox_application::language::CourseStorePort;
use devtoolbox_core::language::{
    BookSummary, Course, CourseBook, CourseLesson, LearningPlan, LessonListEntry, LessonProgress,
    LessonSentence, LessonVocab, ShadowAttempt, WordEntry, WordOccurrence,
};
use parking_lot::Mutex;

use crate::language::LanguageStore;

fn map<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}

/// 课程存储端口适配器。
pub struct CourseStoreAdapter {
    store: Arc<Mutex<LanguageStore>>,
}

impl CourseStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<LanguageStore>>) -> Self {
        Self { store }
    }
}

impl CourseStorePort for CourseStoreAdapter {
    fn courses(&self) -> Result<Vec<Course>, String> {
        map(self.store.lock().courses())
    }

    fn course_books(&self, course_id: &str) -> Result<Vec<CourseBook>, String> {
        map(self.store.lock().course_books(course_id))
    }

    fn book(&self, book_id: &str) -> Result<Option<CourseBook>, String> {
        map(self.store.lock().book(book_id))
    }

    fn book_lessons(&self, book_id: &str) -> Result<Vec<LessonListEntry>, String> {
        map(self.store.lock().book_lessons(book_id))
    }

    fn course_lesson(&self, lesson_id: &str) -> Result<Option<CourseLesson>, String> {
        map(self.store.lock().course_lesson(lesson_id))
    }

    fn lesson_sentences(&self, lesson_id: &str) -> Result<Vec<LessonSentence>, String> {
        map(self.store.lock().lesson_sentences(lesson_id))
    }

    fn lesson_vocab(&self, lesson_id: &str) -> Result<Vec<LessonVocab>, String> {
        map(self.store.lock().lesson_vocab(lesson_id))
    }

    fn lesson_vocab_marks(
        &self,
        lesson_id: &str,
    ) -> Result<std::collections::HashMap<String, Option<String>>, String> {
        map(self.store.lock().lesson_vocab_marks(lesson_id))
    }

    fn set_lesson_vocab_mark(
        &self,
        lesson_id: &str,
        word: &str,
        mark: Option<&str>,
    ) -> Result<(), String> {
        map(self
            .store
            .lock()
            .set_lesson_vocab_mark(lesson_id, word, mark))
    }

    fn book_summary(&self, book_id: &str) -> Result<BookSummary, String> {
        map(self.store.lock().book_summary(book_id))
    }

    fn lesson_progress(&self, lesson_id: &str) -> Result<Option<LessonProgress>, String> {
        map(self.store.lock().lesson_progress(lesson_id))
    }

    fn save_lesson_progress(&self, progress: &LessonProgress) -> Result<(), String> {
        map(self.store.lock().save_lesson_progress(progress))
    }

    fn latest_learning_lesson(&self) -> Result<Option<LessonListEntry>, String> {
        map(self.store.lock().latest_learning_lesson())
    }

    fn recent_lesson_entries(&self, limit: usize) -> Result<Vec<LessonListEntry>, String> {
        map(self.store.lock().recent_lesson_entries(limit))
    }

    fn study_seconds_since(&self, day_start: i64) -> Result<i64, String> {
        map(self.store.lock().study_seconds_since(day_start))
    }

    fn study_streak_days(&self, now: i64) -> Result<u32, String> {
        map(self.store.lock().study_streak_days(now))
    }

    fn learning_plan(&self, language: &str) -> Result<Option<LearningPlan>, String> {
        map(self.store.lock().learning_plan(language))
    }

    fn save_learning_plan(&self, plan: &LearningPlan) -> Result<(), String> {
        map(self.store.lock().save_learning_plan(plan))
    }

    fn record_word_occurrence(&self, occurrence: &WordOccurrence) -> Result<(), String> {
        map(self.store.lock().record_word_occurrence(occurrence))
    }

    fn word_occurrences(&self, word: &str, limit: usize) -> Result<Vec<WordOccurrence>, String> {
        map(self.store.lock().word_occurrences(word, limit))
    }

    fn word_occurrence_count(&self, word: &str) -> Result<i64, String> {
        map(self.store.lock().word_occurrence_count(word))
    }

    fn dict_lookup(&self, word: &str) -> Result<Option<WordEntry>, String> {
        map(self.store.lock().dict_lookup(word))
    }

    fn dict_search(&self, query: &str, limit: usize) -> Result<Vec<WordEntry>, String> {
        map(self.store.lock().dict_search(query, limit))
    }

    fn dict_count(&self) -> Result<i64, String> {
        map(self.store.lock().dict_count())
    }

    fn lesson_title_search(&self, query: &str, limit: usize) -> Result<Vec<CourseLesson>, String> {
        map(self.store.lock().lesson_title_search(query, limit))
    }

    fn insert_shadow_attempt(&self, attempt: &ShadowAttempt) -> Result<(), String> {
        map(self.store.lock().insert_shadow_attempt(attempt))
    }

    fn shadow_attempts(
        &self,
        lesson_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ShadowAttempt>, String> {
        map(self.store.lock().shadow_attempts(lesson_id, limit))
    }
}
