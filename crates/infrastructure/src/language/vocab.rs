//! 课时生词提取：从课文真实句子出发，经词典还原 lemma、过滤功能词后得到词表。
//!
//! 原则（诚实数据）：
//! - 词表来自**本课真实课文**，不是预置词单；
//! - 词形还原只信词典 exchange 表（`0:` lemma），不自造词形规则；
//! - 词典缺失时仍然收录（word = 小写词形，无释义）——导入词典后可重导入补齐；
//! - 功能词（the/and/don't…）由 core 的 `is_stopword` 过滤。

use std::collections::HashMap;

use devtoolbox_core::language::{
    LessonSentence, LessonVocab, importance_from_frequency, is_stopword, tokenize_english,
};

use super::store::LanguageStore;

/// 从一课句子提取生词（按 importance 排序由 SQL 完成；此处保证内容正确）。
pub fn extract_lesson_vocab(
    store: &LanguageStore,
    lesson_id: &str,
    sentences: &[LessonSentence],
) -> Vec<LessonVocab> {
    // surface（小写）→ (lemma, 首现句子, 首现原句快照)
    let mut first_seen: HashMap<String, (String, String, String)> = HashMap::new();
    // lemma → 词典命中
    let mut entries: HashMap<String, Option<devtoolbox_core::language::WordEntry>> = HashMap::new();

    for sentence in sentences {
        for token in tokenize_english(&sentence.english) {
            if is_stopword(&token) || token == "lesson" {
                continue;
            }
            if first_seen.contains_key(&token) {
                continue;
            }
            let entry = entries
                .entry(token.clone())
                .or_insert_with(|| store.dict_lookup(&token).ok().flatten())
                .clone();
            let lemma = entry
                .as_ref()
                .map(|entry| entry.lemma.clone())
                .unwrap_or_else(|| token.clone());
            if is_stopword(&lemma) {
                continue;
            }
            first_seen.insert(
                token,
                (lemma, sentence.id.clone(), sentence.english.clone()),
            );
        }
    }

    // 聚合：lemma → (首现 surface / 句子 / 快照)。同一课不同时态只出现一次。
    let mut by_lemma: HashMap<String, (String, String, String)> = HashMap::new();
    for (surface, (lemma, sentence_id, context)) in first_seen {
        by_lemma
            .entry(lemma)
            .and_modify(|existing| {
                // 保留首现更早的（句子 id 形如 `nce:2:17#7`，字典序即可比）。
                if sentence_id < existing.1 {
                    *existing = (surface.clone(), sentence_id.clone(), context.clone());
                }
            })
            .or_insert((surface, sentence_id, context));
    }

    // 最终词条：释义/音标一律取 **lemma 本身** 的词典条目（不是碰巧先出现的词形）。
    let mut lemma_entries: HashMap<String, Option<devtoolbox_core::language::WordEntry>> =
        HashMap::new();
    by_lemma
        .into_iter()
        .map(|(lemma, (surface, sentence_id, context))| {
            let entry = lemma_entries
                .entry(lemma.clone())
                .or_insert_with(|| store.dict_lookup(&lemma).ok().flatten())
                .clone();
            let frequency = entry.as_ref().map(|e| e.frequency).unwrap_or(0);
            LessonVocab {
                lesson_id: lesson_id.to_string(),
                word: lemma,
                surface: Some(surface),
                sentence_id: Some(sentence_id),
                context: Some(context),
                phonetic: entry.as_ref().and_then(|e| e.phonetic.clone()),
                pos: entry.as_ref().and_then(|e| e.pos.clone()),
                translation_zh: entry.as_ref().and_then(|e| e.translation_zh.clone()),
                definition_en: entry.as_ref().and_then(|e| e.definition_en.clone()),
                frequency,
                tags: entry.as_ref().map(|e| e.tags.clone()).unwrap_or_default(),
                importance: importance_from_frequency(frequency),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(lesson: &str, sequence: u32, english: &str) -> LessonSentence {
        LessonSentence {
            id: format!("{lesson}#{sequence}"),
            lesson_id: lesson.to_string(),
            sequence,
            start_ms: 0,
            end_ms: 1000,
            english: english.to_string(),
            chinese: None,
        }
    }

    #[test]
    fn extracts_content_words_and_dedups_inflections() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        store
            .conn()
            .execute(
                "INSERT INTO dict_entries
                    (word, phonetic, definition_en, translation_zh, pos, collins, oxford,
                     tag, bnc, frq, exchange)
                 VALUES
                    ('hesitated', '', '', '犹豫', 'v.', 0, 0, 'cet4', 0, 9000,
                     'p:hesitated/0:hesitate'),
                    ('hesitate', 'hɪˈzɪteɪt', '', '犹豫；迟疑', 'v.', 3, 0, 'cet4 cet6',
                     0, 8000, 'p:hesitated')",
                [],
            )
            .expect("insert dict");
        let sentences = vec![
            sentence("nce:2:17", 0, "Don't hesitate to ask questions."),
            sentence("nce:2:17", 1, "She hesitated for a moment."),
        ];
        let vocab = extract_lesson_vocab(&store, "nce:2:17", &sentences);
        let lemmas: Vec<&str> = vocab.iter().map(|word| word.word.as_str()).collect();
        // stopwords 被过滤；hesitate/hesitated 合一（以先出现的为准）。
        assert!(lemmas.contains(&"hesitate"));
        assert!(!lemmas.contains(&"hesitated"));
        assert!(lemmas.contains(&"ask"));
        assert!(lemmas.contains(&"questions"));
        assert!(lemmas.contains(&"moment"));
        assert!(!lemmas.contains(&"don't"));
        assert!(!lemmas.contains(&"the"));
        let hesitate = vocab
            .iter()
            .find(|word| word.word == "hesitate")
            .expect("hesitate");
        assert_eq!(hesitate.translation_zh.as_deref(), Some("犹豫；迟疑"));
        assert_eq!(hesitate.importance, 80); // frq 8000 → 稀有度高
    }

    #[test]
    fn works_without_dictionary() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        let sentences = vec![sentence("nce:1:1", 0, "Excuse me, is this your handbag?")];
        let vocab = extract_lesson_vocab(&store, "nce:1:1", &sentences);
        let words: Vec<&str> = vocab.iter().map(|word| word.word.as_str()).collect();
        assert!(words.contains(&"excuse"));
        assert!(words.contains(&"handbag"));
        assert!(!words.contains(&"your"));
        assert!(vocab.iter().all(|word| word.translation_zh.is_none()));
    }
}
