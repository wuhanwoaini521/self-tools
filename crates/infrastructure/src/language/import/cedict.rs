//! CC-CEDICT 解析（#17/#19）：`TRAD SIMP [pinyin] /meaning/meaning/`。
//!
//! 例：`旅行 旅行 [lu:3 xing2] /to travel/journey; trip/CL:趟[tang4],次[ci4]/`
//! - pinyin 中 `u:` 表示 ü（保留原样，另存规范化形式供搜索）；
//! - 同一 (trad, simp) 多条行（异读）合并为一个条目，读音与释义累积。

use std::collections::BTreeMap;

use devtoolbox_core::language::{
    LanguageCode, LanguageItemType, LanguageMetadata, MandarinMetadata, PronunciationScheme,
};

use super::{ImportError, ImportedItem, ImportedMeaning, ImportedPronunciation};

/// CEDICT 行解析结果为合并前的单条记录。
struct CedictLine {
    trad: String,
    simp: String,
    pinyin: String,
    meanings: Vec<String>,
}

/// 解析 CC-CEDICT 内容（`#` 注释跳过）。
pub fn parse(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let mut merged: BTreeMap<String, CedictLine> = BTreeMap::new();
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
            Some(entry) => {
                entry.pinyin.push(' ');
                entry.pinyin.push_str(&parsed.pinyin);
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
        let item_id = format!("cedict:{key}");
        let tones = devtoolbox_core::language::tones_from_syllables(&line.pinyin);
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Zho,
            LanguageItemType::Word,
            line.trad.clone(),
        );
        item.reading = Some(line.pinyin.clone());
        item.romanization = Some(line.pinyin.clone());
        item.meta = Some(LanguageMetadata::Mandarin(MandarinMetadata {
            simplified: Some(line.simp.clone()),
            traditional: Some(line.trad.clone()),
            pinyin: Some(line.pinyin.clone()),
            tones: tones.clone(),
            hsk: None, // #20：HSK 无明确开放来源，V1 不填
        }));
        item.pronunciations.push(ImportedPronunciation {
            id: format!("{item_id}:pinyin"),
            scheme: PronunciationScheme::Pinyin,
            phonemes: line.pinyin.clone(),
            tone: tones.first().copied(),
            variant: None,
            source: "".to_string(),
        });
        for (rank, meaning) in line.meanings.iter().enumerate() {
            item.meanings.push(ImportedMeaning {
                id: format!("{item_id}:m{rank}"),
                pos: None,
                gloss: Some(strip_cl_prefix(meaning)),
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

/// 解析单行；无法解析返回 None（跳过不致命）。
fn parse_line(line: &str, line_number: usize) -> Option<CedictLine> {
    let _ = line_number;
    // 行首 `TRAD SIMP [..]`
    if line.starts_with('#') {
        return None;
    }
    let (trad, rest) = line.split_once(' ')?;
    let (simp, rest) = rest.split_once(' ')?;
    if trad.is_empty() || simp.is_empty() {
        return None;
    }
    let rest = rest.strip_prefix('[')?;
    let (pinyin, rest) = rest.split_once(']')?;
    if pinyin.is_empty() {
        return None;
    }
    let meanings: Vec<String> = rest
        .trim_start()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    Some(CedictLine {
        trad: trad.to_string(),
        simp: simp.to_string(),
        pinyin: pinyin.to_string(),
        meanings,
    })
}

/// 把 `CL:` 等 measure-word 前缀的绝对值放入 raw 但 gloss 保留全文（直接可读）。
fn strip_cl_prefix(meaning: &str) -> String {
    meaning.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_travel_line() {
        let raw = "旅行 旅行 [lu:3 xing2] /to travel/journey; trip/CL:趟[tang4],次[ci4]/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.text, "旅行");
        assert_eq!(item.reading.as_deref(), Some("lu:3 xing2"));
        assert_eq!(item.meanings.len(), 3);
        assert_eq!(item.meanings[0].gloss.as_deref(), Some("to travel"));
        let devtoolbox_core::language::LanguageMetadata::Mandarin(meta) =
            item.meta.clone().expect("meta")
        else {
            panic!("mandarin meta")
        };
        assert_eq!(meta.tones, vec![3, 2]);
        assert_eq!(meta.hsk, None);
    }

    #[test]
    fn merges_duplicate_trad_simp() {
        let raw = "行 行 [hang2] /row/\n行 行 [xing2] /to walk/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
        assert!(
            items[0]
                .reading
                .as_deref()
                .unwrap_or_default()
                .contains("hang2")
        );
        assert!(
            items[0]
                .reading
                .as_deref()
                .unwrap_or_default()
                .contains("xing2")
        );
        assert_eq!(items[0].meanings.len(), 2);
    }

    #[test]
    fn comments_are_ignored() {
        let raw = "# CC-CEDICT\n# http://creativecommons.org/licenses/by-sa/4.0/\n你好 你好 [ni3 hao3] /hello; hi/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn malformed_line_is_skipped() {
        let raw = "not a cedict line\n你好 你好 [ni3 hao3] /hello; hi/\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn empty_input_errors() {
        assert_eq!(parse("# only comments\n"), Err(ImportError::Empty));
    }
}
