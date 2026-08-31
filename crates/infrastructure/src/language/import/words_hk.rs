//! words.hk 公有领域数据集解析（#21-#23/#25）。
//!
//! 三个列表（均为 **Public Domain**，页面标注；完整释义/例句属 Non-Commercial 许可，**不导入**）：
//! - 词表 `{word: [jyutping…]}` → Word 条目 + Jyutping 发音；
//! - 字表 `{char: {jyutping: count}}` → 单字条目；
//! - 英粵對照 `{english: [[word:jyutping, score]…]}` → 附加英文搜索词（`item_search_index`）。

use std::collections::HashMap;

use devtoolbox_core::language::{
    CantoneseMetadata, LanguageCode, LanguageItemType, LanguageMetadata, PronunciationScheme,
};
use serde_json::Value;

use super::{ImportError, ImportedItem, ImportedPronunciation};

/// 解析词表 JSON。
pub fn parse_word_list(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| ImportError::Json(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(ImportError::Json("word list must be an object".to_string()));
    };
    let mut items = Vec::with_capacity(object.len());
    for (word, pronunciations) in object {
        let Some(list) = pronunciations.as_array() else {
            continue;
        };
        let mut jyutpings: Vec<String> = list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        jyutpings.dedup();
        if jyutpings.is_empty() {
            continue;
        }
        let item_id = format!("whk:{}", *word);
        let jyutping = jyutpings.join(" / ");
        let tones = devtoolbox_core::language::tones_from_syllables(&jyutping);
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Yue,
            LanguageItemType::Word,
            word.clone(),
        );
        item.romanization = Some(jyutping.clone());
        item.meta = Some(LanguageMetadata::Cantonese(CantoneseMetadata {
            traditional: Some(word.clone()),
            simplified: None,
            jyutping: Some(jyutping.clone()),
            tones: tones.clone(),
        }));
        for (index, single) in jyutpings.iter().enumerate() {
            let single_tones = devtoolbox_core::language::tones_from_syllables(single);
            item.pronunciations.push(ImportedPronunciation {
                id: format!("{item_id}:jyut:{index}"),
                scheme: PronunciationScheme::Jyutping,
                phonemes: single.clone(),
                tone: single_tones.first().copied(),
                variant: None,
                source: "".to_string(),
            });
        }
        items.push(item);
    }
    if items.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(items)
}

/// 解析字表 JSON：`{char: {jyutping: count}}`。
pub fn parse_char_list(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| ImportError::Json(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(ImportError::Json("char list must be an object".to_string()));
    };
    let mut items = Vec::with_capacity(object.len());
    for (character, readings) in object {
        let Some(readings) = readings.as_object() else {
            continue;
        };
        // 异体字（带 *）仍保留原字符
        let clean = character.trim_end_matches('*');
        let mut jyutpings: Vec<String> = readings.keys().cloned().collect();
        jyutpings.sort();
        if jyutpings.is_empty() {
            continue;
        }
        let item_id = format!("whk-char:{}", clean);
        let jyutping = jyutpings.join(" / ");
        let tones = devtoolbox_core::language::tones_from_syllables(&jyutping);
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Yue,
            LanguageItemType::Pronunciation,
            clean.to_string(),
        );
        item.romanization = Some(jyutping.clone());
        item.meta = Some(LanguageMetadata::Cantonese(CantoneseMetadata {
            traditional: Some(clean.to_string()),
            simplified: None,
            jyutping: Some(jyutping.clone()),
            tones: tones.clone(),
        }));
        for (index, single) in jyutpings.iter().enumerate() {
            let single_tones = devtoolbox_core::language::tones_from_syllables(single);
            item.pronunciations.push(ImportedPronunciation {
                id: format!("{item_id}:jyut:{index}"),
                scheme: PronunciationScheme::Jyutping,
                phonemes: single.clone(),
                tone: single_tones.first().copied(),
                variant: None,
                source: "".to_string(),
            });
        }
        items.push(item);
    }
    if items.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(items)
}

/// 解析英粵對照表 JSON：`{english: [[word:jyutping, score]…]}`。
/// 返回 `(item_id, english_terms)` 列表，由 store 的 `attach_search_terms` 落库（仅供搜索）。
pub fn parse_english_index(
    raw: &str,
    word_items: &[ImportedItem],
) -> Result<Vec<(String, Vec<String>)>, ImportError> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| ImportError::Json(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(ImportError::Json(
            "english index must be an object".to_string(),
        ));
    };
    // word → item_id（词表已导入时为 whk:word）
    let by_text: HashMap<&str, &ImportedItem> = word_items
        .iter()
        .map(|item| (item.text.as_str(), item))
        .collect();
    // english 可能带 `!` 前缀（表示该映射并非严格释义匹配）→ 仅作搜索提示
    let mut english_terms: HashMap<String, Vec<String>> = HashMap::new();
    for (english, entries) in object {
        let clean = english.trim_start_matches('!');
        let Some(entries) = entries.as_array() else {
            continue;
        };
        for entry in entries {
            let Some(entry) = entry.as_array() else {
                continue;
            };
            if entry.is_empty() {
                continue;
            }
            let Some(combined) = entry.first().and_then(Value::as_str) else {
                continue;
            };
            // `word:jyutping`（或逗号分隔多音节 `word:jyutping,jyutping`）→ 取 word 部分
            let word = combined.split(':').next().unwrap_or(combined).to_string();
            if by_text.contains_key(word.as_str()) || word.chars().any(is_cjk) {
                english_terms
                    .entry(clean.to_lowercase())
                    .or_default()
                    .push(word.clone());
            }
        }
    }
    // 聚合：word → 所有英文搜索词
    let mut terms_by_word: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (english, words) in english_terms {
        for word in words {
            let item_id = match word_items.iter().find(|item| item.text == word) {
                Some(matched) => matched.id.clone(),
                None => continue,
            };
            let terms = terms_by_word.entry(item_id).or_default();
            if !terms.contains(&english) {
                terms.push(english.clone());
            }
        }
    }
    let pairs: Vec<(String, Vec<String>)> = terms_by_word.into_iter().collect();
    if pairs.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(pairs)
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x3040..=0x30FF | 0x31F0..=0x31FF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_word_list() {
        let raw = r#"{"食飯": ["sik6 faan6", "sik6 faan6"], "你好": ["nei5 hou2"]}"#;
        let items = parse_word_list(raw).expect("parse");
        assert_eq!(items.len(), 2);
        let sik = items.iter().find(|item| item.text == "食飯").expect("食飯");
        assert_eq!(sik.romanization.as_deref(), Some("sik6 faan6"));
        assert_eq!(sik.pronunciations.len(), 1);
        assert_eq!(sik.pronunciations[0].tone, Some(6));
        let devtoolbox_core::language::LanguageMetadata::Cantonese(meta) =
            sik.meta.clone().expect("meta")
        else {
            panic!()
        };
        assert_eq!(meta.tones, vec![6, 6]);
    }

    #[test]
    fn parses_char_list() {
        let raw = r#"{"食": {"sik4": 1, "sik6": 396}, "飯": {"faan6": 100}}"#;
        let items = parse_char_list(raw).expect("parse");
        assert_eq!(items.len(), 2);
        let sik = items.iter().find(|item| item.text == "食").expect("食");
        assert_eq!(sik.pronunciations.len(), 2);
        assert_eq!(sik.item_type, LanguageItemType::Pronunciation);
    }

    #[test]
    fn english_index_attaches_terms() {
        let word_items = parse_word_list(r#"{"食飯": ["sik6 faan6"]}"#).expect("words");
        let raw = r#"{"food": [["食飯:sik6 faan6", 50]], "!tasty": [["辣:laat6", 70]]}"#;
        let pairs = parse_english_index(raw, &word_items).expect("index");
        assert_eq!(pairs.len(), 1);
        let (item_id, terms) = &pairs[0];
        assert!(item_id.starts_with("whk:食飯"));
        assert!(terms.contains(&"food".to_string()));
    }

    #[test]
    fn invalid_json_errors() {
        assert!(parse_word_list("{broken").is_err());
        assert_eq!(parse_word_list("{}"), Err(ImportError::Empty));
    }
}
