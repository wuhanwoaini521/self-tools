//! CMUdict 解析（#9/#10）：`WORD ARPABET…` 纯文本行。
//!
//! 例：`NATURAL N AE1 CH ER0 AH0 L`（数字后缀 = stress）。
//! 解析为 pronunciation-only 条目；由导入工作流把发音挂到 OEWN 同名词条（enrichment）。

use devtoolbox_core::language::{LanguageCode, LanguageItemType, PronunciationScheme};

use super::{ImportError, ImportedItem, ImportedPronunciation};

/// 解析 CMUdict 内容（注释行 `;;;` 跳过；多发音 `_variant` 保留为独立条目）。
pub fn parse(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let mut items = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with(";;;") {
            continue;
        }
        let Some((word, phonemes)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        if !word.chars().all(|ch| {
            ch.is_ascii_alphabetic()
                || ch.is_ascii_digit()
                || matches!(ch, '\'' | '"' | '-' | '_' | '.' | '(' | ')')
        }) {
            // 遇到非词行（如头部说明）安全跳过
            continue;
        }
        let phonemes = phonemes.split_whitespace().collect::<Vec<_>>().join(" ");
        if phonemes.is_empty() {
            continue;
        }
        // 音素必须包含字母（拒绝 `hello 123` 这类噪声行）
        if phonemes
            .split_whitespace()
            .any(|phoneme| !phoneme.chars().any(|ch| ch.is_ascii_alphabetic()))
        {
            continue;
        }
        let (base, variant) = split_variant(word);
        let id = format!("cmudict:{word}");
        let mut item = ImportedItem::new(
            id.clone(),
            LanguageCode::Eng,
            LanguageItemType::Pronunciation,
            base.to_string(),
        );
        item.romanization = Some(base.to_string());
        item.pronunciations.push(ImportedPronunciation {
            id: format!("{id}:pron"),
            scheme: PronunciationScheme::Arpabet,
            phonemes: phonemes.clone(),
            tone: None,
            variant: variant.map(str::to_string),
            source: "cmudict".to_string(),
        });
        item.extra = Some(serde_json::json!({
            "enrich_text": base.to_string(),
            "phonemes": phonemes,
        }));
        items.push(item);
        if index > 10_000_000 {
            break;
        }
    }
    if items.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(items)
}

/// `word(1)` / `word(2)` → (base, variant)。
fn split_variant(word: &str) -> (&str, Option<&str>) {
    if let Some(open) = word.find('(') {
        let (base, rest) = word.split_at(open);
        let variant = rest.trim_matches(|ch: char| ch == '(' || ch == ')' || ch.is_whitespace());
        (base, Some(variant))
    } else {
        (word, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_arpabet_line() {
        let raw = "NATURAL N AE1 CH ER0 AH0 L\nRESERVATION R EH2 Z ER0 V EY1 SH AH0 N\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "NATURAL");
        assert_eq!(items[0].pronunciations[0].phonemes, "N AE1 CH ER0 AH0 L");
        assert_eq!(
            items[0].pronunciations[0].scheme,
            PronunciationScheme::Arpabet
        );
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let raw = ";;; # CMUdict -- Major Version: 0.07\n\nHELLO HH AH0 L OW0\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn keeps_variant_suffix() {
        let raw = "TOMATO T AH0 M EY1 T OW0\nTOMATO(1) T AH0 M AA1 T OW0\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].pronunciations[0].variant.as_deref(), Some("1"));
    }

    #[test]
    fn invalid_lines_are_skipped_not_fatal() {
        let raw = "hello 123\nWORLD W ER0 L D\n";
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn empty_input_errors() {
        assert_eq!(parse(""), Err(ImportError::Empty));
    }
}
