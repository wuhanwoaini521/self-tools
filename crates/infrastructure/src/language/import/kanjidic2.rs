//! KANJIDIC2 解析（#14/#15）：EDRDG XML。
//!
//! 导入范围（仅许可明确的学习字段）：`literal` / `reading`(on,kun) / `meaning`(en) /
//! `stroke_count` / `grade` / `radical` / `jlpt`（`kanjidic2_jlpt`，EDRDG 文档注明传承自已发布的级别表）。
//! 明确 **SKIP**（第三方许可不清晰）：`dic_number/*_ref`、`skkip`(SKIP 码)、`query_code`、`misc.freq` 等。

use quick_xml::Reader;
use quick_xml::events::Event;

use devtoolbox_core::language::{
    JapaneseMetadata, LanguageCode, LanguageItemType, LanguageMetadata,
};

use super::{ImportError, ImportedItem, ImportedMeaning};

/// 解析 KANJIDIC2 XML 文本。
pub fn parse(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut current: Option<CharacterBuilder> = None;
    // 叶子跟踪：<reading r_type="on|kun|..."> / <meaning m_lang="en"> / <stroke_count> / <grade> / <rad_value> / <jlpt>
    let mut reading_type: Option<String> = None;
    let mut meaning_lang: Option<String> = None;
    let mut element_stack: Vec<String> = Vec::new();
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Err(_) => break,
            Ok(Event::Eof) => break,
            Ok(Event::Start(element)) => {
                let name = String::from_utf8_lossy(element.name().as_ref()).to_string();
                match name.as_str() {
                    "character" => current = Some(CharacterBuilder::default()),
                    "reading" => {
                        reading_type = element
                            .attributes()
                            .filter_map(|attr| attr.ok())
                            .find(|attr| attr.key.as_ref() == b"r_type")
                            .map(|attr| String::from_utf8_lossy(&attr.value).to_string());
                    }
                    "meaning" => {
                        // 无 m_lang 属性的默认英语释义
                        meaning_lang = element
                            .attributes()
                            .filter_map(|attr| attr.ok())
                            .find(|attr| attr.key.as_ref() == b"m_lang")
                            .map(|attr| String::from_utf8_lossy(&attr.value).to_string())
                            .or_else(|| Some("en".to_string()));
                    }
                    _ => {}
                }
                element_stack.push(name);
            }
            Ok(Event::End(element)) => {
                let name = String::from_utf8_lossy(element.name().as_ref()).to_string();
                match name.as_str() {
                    "character" => {
                        if let Some(item) = current.take().and_then(CharacterBuilder::into_item) {
                            items.push(item);
                        }
                    }
                    "reading" => reading_type = None,
                    "meaning" => meaning_lang = None,
                    _ => {}
                }
                if name == "character" {
                    current = None;
                }
                if let Some(position) = element_stack.iter().rposition(|slot| slot == &name) {
                    element_stack.truncate(position);
                }
            }
            Ok(Event::Text(text)) => {
                let content = String::from_utf8_lossy(&text).to_string();
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Some(character) = current.as_mut() else {
                    continue;
                };
                let parent = element_stack.last().cloned().unwrap_or_default();
                // 需要知道父标签：reading/meaning 由 attribute 驱动，其余用元素栈
                if parent == "literal" {
                    character.literal = trimmed.to_string();
                }
                match reading_type.as_deref() {
                    Some("on") => character.on.push(trimmed.to_string()),
                    Some("kun") => character.kun.push(trimmed.to_string()),
                    Some(_) => {}
                    None => match meaning_lang.as_deref() {
                        Some("en") => character.meaning_en.push(trimmed.to_string()),
                        Some(_) => {} // 非英语释义不导入（来源许可边界内谨慎处理）
                        None => match parent.as_str() {
                            "stroke_count" => character.stroke_count = parse_number(trimmed),
                            "grade" => character.grade = parse_number(trimmed),
                            "jlpt" => character.jlpt = Some(trimmed.to_string()),
                            "rad_value" => {
                                // <radical><rad_value rad_type="classical">
                                character.radical = parse_number(trimmed);
                            }
                            _ => {}
                        },
                    },
                }
            }
            _ => {}
        }
    }
    if items.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(items)
}

#[derive(Default)]
struct CharacterBuilder {
    literal: String,
    on: Vec<String>,
    kun: Vec<String>,
    meaning_en: Vec<String>,
    stroke_count: Option<i64>,
    grade: Option<i64>,
    jlpt: Option<String>,
    radical: Option<i64>,
}

impl CharacterBuilder {
    fn into_item(self) -> Option<ImportedItem> {
        if self.literal.is_empty() {
            return None;
        }
        let reading = [self.on.clone(), self.kun.clone()].concat().join(" / ");
        let item_id = format!("kanjidic2:{}", self.literal);
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Jap,
            LanguageItemType::Word,
            self.literal.clone(),
        );
        item.reading = (!reading.is_empty()).then_some(reading);
        if self.jlpt.is_some() {
            // jlpt 有 EDRDG 文档来源；UI 展示必须标注 source=KANJIDIC2
        }
        let mut extra = serde_json::Map::new();
        if let Some(stroke_count) = self.stroke_count {
            extra.insert(
                "stroke_count".to_string(),
                serde_json::Value::from(stroke_count),
            );
        }
        if let Some(grade) = self.grade {
            extra.insert("grade".to_string(), serde_json::Value::from(grade));
        }
        if let Some(radical) = self.radical {
            extra.insert("radical".to_string(), serde_json::Value::from(radical));
        }
        if let Some(jlpt) = self.jlpt.as_deref() {
            extra.insert(
                "kanjidic2_jlpt".to_string(),
                serde_json::Value::String(jlpt.to_string()),
            );
        }
        if !extra.is_empty() {
            item.extra = Some(serde_json::Value::Object(extra));
        }
        item.meta = Some(LanguageMetadata::Japanese(JapaneseMetadata {
            kana: None,
            romaji: None,
            kanji: Some(self.literal.clone()),
            jlpt: self.jlpt.clone(),
        }));
        for (rank, meaning) in self.meaning_en.iter().enumerate() {
            item.meanings.push(ImportedMeaning {
                id: format!("{item_id}:m{rank}"),
                pos: None,
                gloss: Some(meaning.clone()),
                raw: None,
                sense_key: None,
                lang: Some("en".to_string()),
                rank: rank as i64,
            });
        }
        Some(item)
    }
}

fn parse_number(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kanjidic2>
<header />
<character>
<literal>食</literal>
<codepoint><cp_value cp_type="ucs">98df</cp_value></codepoint>
<radical><rad_value rad_type="classical">184</rad_value></radical>
<misc>
<grade>2</grade>
<stroke_count>9</stroke_count>
<jlpt>4</jlpt>
</misc>
<dic_number><dic_ref dr_type="nelson_c">5154</dic_ref></dic_number>
<reading_meaning>
<rmgroup>
<reading r_type="on">ショク</reading>
<reading r_type="on">ジキ</reading>
<reading r_type="kun">く.う</reading>
<reading r_type="kun">く.らう</reading>
</rmgroup>
<meaning>to eat; food</meaning>
<meaning m_lang="fr">manger</meaning>
</reading_meaning>
</character>
</kanjidic2>"#;

    #[test]
    fn parses_character() {
        let items = parse(SAMPLE).expect("parse");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.id, "kanjidic2:食");
        assert_eq!(item.text, "食");
        assert!(
            item.reading
                .as_deref()
                .unwrap_or_default()
                .contains("ショク")
        );
        assert!(
            item.reading
                .as_deref()
                .unwrap_or_default()
                .contains("く.う")
        );
        assert_eq!(item.meanings.len(), 1);
        assert_eq!(item.meanings[0].gloss.as_deref(), Some("to eat; food"));
        let extra = item.extra.clone().expect("extra");
        assert_eq!(extra["stroke_count"], 9);
        assert_eq!(extra["grade"], 2);
        assert_eq!(extra["kanjidic2_jlpt"], "4");
        // SKIP：dic_ref 不进入任何字段
        assert!(extra.get("nelson_c").is_none());
        // 非英语释义不导入
        assert!(
            item.meanings
                .iter()
                .all(|m| m.gloss.as_deref() != Some("manger"))
        );
    }

    #[test]
    fn multiple_characters_parsed() {
        let raw = r#"<kanjidic2><character><literal>食</literal><misc><stroke_count>9</stroke_count>
            <jlpt>4</jlpt></misc><reading_meaning><rmgroup><reading r_type="on">ショク</reading></rmgroup>
            <meaning>to eat; food</meaning></reading_meaning></character>
            <character><literal>飯</literal><misc><stroke_count>12</stroke_count></misc>
            <reading_meaning><rmgroup><reading r_type="kun">めし</reading></rmgroup>
            <meaning>meal; cooked rice</meaning></reading_meaning></character></kanjidic2>"#;
        let items = parse(raw).expect("parse");
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.text == "食"));
        assert!(items.iter().any(|item| item.text == "飯"));
    }

    #[test]
    fn empty_input_errors() {
        assert!(parse("").is_err());
    }
}
