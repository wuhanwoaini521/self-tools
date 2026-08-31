//! CC-Canto 解析（#24/#25）：扩展 CC-CEDICT 行 + `{jyutping}` 字段。
//!
//! 两种官方格式：
//! - webdist（主词典，**CC BY-SA 3.0**）：`TRAD SIMP [pinyin] {jyutping} /meaning/`；
//! - readings（Cantonese readings for CC-CEDICT，2015）：`TRAD SIMP [pinyin] {jyutping}`（无释义）。
//!
//! 同一 (trad, simp) 异读行合并；释义累积。

use std::collections::BTreeMap;

use devtoolbox_core::language::{
    CantoneseMetadata, LanguageCode, LanguageItemType, LanguageMetadata, PronunciationScheme,
};

use super::{ImportError, ImportedItem, ImportedMeaning, ImportedPronunciation};

struct CcCantoLine {
    trad: String,
    simp: String,
    jyutping: String,
    meanings: Vec<String>,
}

/// 解析 CC-Canto 内容（`#` 注释跳过；无 `{jyutping}` 的行视为 CEDICT 行跳过——CC-Canto 必有 jyutping）。
pub fn parse(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let mut merged: BTreeMap<String, CcCantoLine> = BTreeMap::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(parsed) = parse_line(trimmed, index) else {
            continue;
        };
        let key = format!("{}::{}", parsed.trad, parsed.simp);
        match merged.get_mut(&key) {
            Some(entry) if entry.jyutping != parsed.jyutping => {
                entry.jyutping.push(' ');
                entry.jyutping.push_str(&parsed.jyutping);
                for meaning in parsed.meanings {
                    if !entry.meanings.contains(&meaning) {
                        entry.meanings.push(meaning);
                    }
                }
            }
            Some(entry) => {
                for meaning in parsed.meanings {
                    if !entry.meanings.contains(&meaning) {
                        entry.meanings.push(meaning);
                    }
                }
            }
            None => {
                merged.insert(key, parsed);
            }
        }
    }
    if merged.is_empty() {
        return Err(ImportError::Empty);
    }
    let mut items = Vec::with_capacity(merged.len());
    for (key, line) in merged {
        let item_id = format!("cccanto:{key}");
        let tones = devtoolbox_core::language::tones_from_syllables(&line.jyutping);
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Yue,
            LanguageItemType::Word,
            line.trad.clone(),
        );
        item.reading = Some(line.jyutping.clone());
        item.romanization = Some(line.jyutping.clone());
        item.meta = Some(LanguageMetadata::Cantonese(CantoneseMetadata {
            traditional: Some(line.trad.clone()),
            simplified: Some(line.simp.clone()),
            jyutping: Some(line.jyutping.clone()),
            tones: tones.clone(),
        }));
        item.pronunciations.push(ImportedPronunciation {
            id: format!("{item_id}:jyutping"),
            scheme: PronunciationScheme::Jyutping,
            phonemes: line.jyutping.clone(),
            tone: tones.first().copied(),
            variant: None,
            source: "".to_string(),
        });
        for (rank, meaning) in line.meanings.iter().enumerate() {
            item.meanings.push(ImportedMeaning {
                id: format!("{item_id}:m{rank}"),
                pos: None,
                gloss: Some(meaning.clone()),
                raw: Some(meaning.clone()),
                sense_key: None,
                lang: Some("en".to_string()),
                rank: rank as i64,
            });
        }
        items.push(item);
    }
    Ok(items)
}

/// 解析单行：`TRAD SIMP [pinyin] {jyutping} /meaning/…`。
fn parse_line(line: &str, line_number: usize) -> Option<CcCantoLine> {
    let (trad, rest) = line.split_once(' ')?;
    if trad.starts_with('#') || trad.is_empty() {
        return None;
    }
    let (simp, rest) = rest.split_once(' ')?;
    if simp.is_empty() {
        return None;
    }
    let rest = rest.strip_prefix('[')?;
    let (_pinyin, rest) = rest.split_once(']')?;
    let rest = rest.trim_start();
    // {jyutping} 为 CC-Canto 特征字段
    let rest = rest.strip_prefix('{')?;
    let (jyutping, rest) = rest.split_once('}')?;
    if jyutping.is_empty() {
        let _ = line_number;
        return None;
    }
    let meanings: Vec<String> = rest
        .trim_start()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    Some(CcCantoLine {
        trad: trad.to_string(),
        simp: simp.to_string(),
        jyutping: jyutping.to_string(),
        meanings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_webdist_line() {
        let raw = "食粥食飯 食粥食饭 [shi2 zhou1 shi2 fan4] {sik6 zuk1 zi6 faan6} /whether good or bad/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.text, "食粥食飯");
        assert_eq!(item.romanization.as_deref(), Some("sik6 zuk1 zi6 faan6"));
        assert_eq!(item.pronunciations[0].scheme, PronunciationScheme::Jyutping);
        assert_eq!(
            item.meanings[0].gloss.as_deref(),
            Some("whether good or bad")
        );
        let devtoolbox_core::language::LanguageMetadata::Cantonese(meta) =
            item.meta.clone().expect("meta")
        else {
            panic!("cantonese meta")
        };
        assert_eq!(meta.jyutping.as_deref(), Some("sik6 zuk1 zi6 faan6"));
    }

    #[test]
    fn parses_readings_only_line() {
        let raw = "旅行 旅行 [lu:3 xing2] {leoi5 hang4}\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
        assert!(items[0].meanings.is_empty());
        assert_eq!(items[0].romanization.as_deref(), Some("leoi5 hang4"));
    }

    #[test]
    fn comments_and_malformed_lines_skipped() {
        let raw = "# CC-Canto\n# Copyright (c) 2015-17 Pleco Inc.\nnot-a-line\n你好嗎 你好吗 [ni3 hao3 ma5] {nei5 hou2 maa1} /how are you?/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "你好嗎");
    }

    #[test]
    fn merges_variant_readings() {
        let raw = "多謝 多谢 [duo1 xie4] {do1 ze6} /thanks/\n多謝 多谢 [duo1 xie4] {do1 ze6} /thank you/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].meanings.len(), 2);
    }

    #[test]
    fn empty_input_errors() {
        assert_eq!(parse("# only comments"), Err(ImportError::Empty));
    }
}
