//! Tatoeba 句子解析（#27-#31）。
//!
//! 支持两种文件：
//! - CC0：`id\tlang\ttext\tdate_modified`（sentences_CC0.csv）；
//! - 官方分语言：`id\tlang\ttext`（per_language/*_sentences.tsv）。
//!
//! 每句保存 license（CC0 / CC BY 2.0 FR）与 source（tatoeba id），CC BY 句来自身分语言文件（author 由 detailed 文件另行补充）。

use devtoolbox_core::language::{LanguageCode, LanguageItemType};

use super::{ImportError, ImportedItem};

/// 解析 Tatoeba 内容。`default_license`：调用方注入（`CC0 1.0` / `CC BY 2.0 FR`）。
/// 出现非本语言代码的句子（多语言合并文件）按行内 lang 归属。
pub fn parse(raw: &str, default_license: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let mut items = Vec::new();
    let mut skipped = 0usize;
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<&str> = trimmed.split('\t').collect();
        if fields.len() < 3 {
            skipped += 1;
            continue;
        }
        let sentence_id = fields[0].trim();
        let lang = fields[1].trim();
        let text = fields[2].trim();
        if !sentence_id.chars().all(|ch| ch.is_ascii_digit()) {
            skipped += 1;
            continue;
        }
        let Some(language) = LanguageCode::from_code(lang) else {
            // 多语言合集的其它语言（fixtures/完整导入都可能含）——跳过并计数
            skipped += 1;
            continue;
        };
        if text.is_empty() {
            skipped += 1;
            continue;
        }
        let item_id = format!("tatoeba:{sentence_id}");
        let mut item = ImportedItem::new(
            item_id.clone(),
            language,
            LanguageItemType::Sentence,
            text.to_string(),
        );
        item.extra = Some(serde_json::json!({
            "sentence_id": sentence_id,
            "author": null,
            "license": default_license,
            "lang": language.code(),
        }));
        items.push(item);
        // 防御：超大文件保护
        if index > 5_000_000 {
            break;
        }
    }
    if items.is_empty() && skipped == 0 {
        return Err(ImportError::Empty);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cc0_rows() {
        let raw = "330998\teng\tChildren who spend more time outdoors have a lower risk of myopia.\t2019-01-12 19:39:42\n\
                   5080\tjpn\t私のチョコレートを食べることを考えさえしないで。\t2020-05-01 10:00:00\n";
        let items = parse(raw, "CC0 1.0").expect("parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "tatoeba:330998");
        assert_eq!(items[0].language, LanguageCode::Eng);
        assert_eq!(items[1].language, LanguageCode::Jap);
        let extra = items[1].extra.clone().expect("extra");
        assert_eq!(extra["license"], "CC0 1.0");
    }

    #[test]
    fn ignores_unknown_languages() {
        let raw = "1\tfra\tBonjour le monde.\n2\teng\tHello world.\n";
        let items = parse(raw, "CC BY 2.0 FR").expect("parse");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Hello world.");
    }

    #[test]
    fn malformed_rows_are_skipped() {
        let raw = "not-a-number\teng\thello\n\n3\teng\tvalid sentence here\n";
        let items = parse(raw, "CC BY 2.0 FR").expect("parse");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn empty_input_errors() {
        assert_eq!(parse("", "CC0 1.0"), Err(ImportError::Empty));
    }
}
