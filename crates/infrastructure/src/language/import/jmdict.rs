//! JMdict 解析（#12/#13）：EDRDG XML。
//!
//! 结构：`<JMdict><entry><ent_seq>..<k_ele><keb>..<r_ele><reb>..<sense><pos>..<gloss>..`.
//! JMdict 的 DTD 定义了大量自定义实体（`&exp;`、`&v1;`…），quick-xml 不展开 DTD 实体，
//! 因此先做「实体松弛」：标准实体按 XML 规则还原，未知实体保留为纯文本名（`&exp;` → `exp`），
//! 使 POS/field/misc 标注可读且解析不会崩溃。

use quick_xml::Reader;
use quick_xml::events::Event;

use devtoolbox_core::language::{
    JapaneseMetadata, LanguageCode, LanguageItemType, LanguageMetadata, kana_to_romaji,
};

use super::{ImportError, ImportedItem, ImportedMeaning};

/// 解析 JMdict XML 文本。
pub fn parse(raw: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut current: Option<EntryBuilder> = None;
    // 当前容器：k_ele / r_ele / sense（嵌套层级追踪）
    let mut container_stack: Vec<String> = Vec::new();
    let mut in_leaf: Option<LeafSlot> = None;
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Err(_) => break,
            Ok(Event::Eof) => break,
            Ok(Event::Start(element)) => {
                let name = String::from_utf8_lossy(element.name().as_ref()).to_string();
                match name.as_str() {
                    "entry" => {
                        current = Some(EntryBuilder::default());
                        container_stack.clear();
                    }
                    "k_ele" | "r_ele" => container_stack.push(name),
                    "sense" => {
                        container_stack.push(name);
                        if let Some(entry) = current.as_mut() {
                            entry.senses.push(SenseBuilder::default());
                        }
                    }
                    "keb" | "reb" | "ent_seq" | "gloss" | "pos" | "misc" | "field" | "lsource" => {
                        in_leaf = Some(LeafSlot::from_tag(&name));
                    }
                    _ => {}
                }
            }
            Ok(Event::End(element)) => {
                let name = String::from_utf8_lossy(element.name().as_ref()).to_string();
                match name.as_str() {
                    "entry" => {
                        if let Some(item) = current.take().and_then(EntryBuilder::into_item) {
                            items.push(item);
                        }
                        container_stack.clear();
                    }
                    "k_ele" | "r_ele" | "sense" => {
                        container_stack.pop();
                    }
                    "keb" | "reb" | "ent_seq" | "gloss" | "pos" | "misc" | "field" | "lsource" => {
                        in_leaf = None;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                let unescaped = relax_entities(&String::from_utf8_lossy(&text));
                if unescaped.trim().is_empty() {
                    continue;
                }
                let Some(entry) = current.as_mut() else {
                    continue;
                };
                match in_leaf.as_ref().map(|leaf| leaf.tag.as_str()) {
                    Some("keb") if container_stack.last().map(String::as_str) == Some("k_ele") => {
                        entry.kebs.push(unescaped.trim().to_string());
                    }
                    Some("reb") if container_stack.last().map(String::as_str) == Some("r_ele") => {
                        entry.rebs.push(unescaped.trim().to_string());
                    }
                    Some("ent_seq") => entry.ent_seq = unescaped.trim().to_string(),
                    Some("gloss")
                        if container_stack.last().map(String::as_str) == Some("sense") =>
                    {
                        if let Some(sense) = entry.senses.last_mut() {
                            sense.gloss.push(unescaped.trim().to_string());
                        }
                    }
                    Some("pos") if container_stack.last().map(String::as_str) == Some("sense") => {
                        if let Some(sense) = entry.senses.last_mut() {
                            sense.pos.push(unescaped.trim().to_string());
                        }
                    }
                    Some("misc") if container_stack.last().map(String::as_str) == Some("sense") => {
                        if let Some(sense) = entry.senses.last_mut() {
                            sense.misc.push(unescaped.trim().to_string());
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::CData(cdata)) => {
                let text = String::from_utf8_lossy(&cdata).to_string();
                let Some(entry) = current.as_mut() else {
                    continue;
                };
                if let (Some(sense), Some("sense")) = (
                    entry.senses.last_mut(),
                    container_stack.last().map(String::as_str),
                ) {
                    sense.gloss.push(text);
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

/// 叶子槽位跟踪（keb/reb/gloss/pos/… 只关心直接文本）。
struct LeafSlot {
    tag: String,
}

impl LeafSlot {
    fn from_tag(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
        }
    }
}

#[derive(Default)]
struct EntryBuilder {
    ent_seq: String,
    kebs: Vec<String>,
    rebs: Vec<String>,
    senses: Vec<SenseBuilder>,
}

#[derive(Default)]
struct SenseBuilder {
    pos: Vec<String>,
    gloss: Vec<String>,
    misc: Vec<String>,
}

impl EntryBuilder {
    fn into_item(self) -> Option<ImportedItem> {
        if self.ent_seq.is_empty() {
            return None;
        }
        let text = self
            .kebs
            .first()
            .or_else(|| self.rebs.first())
            .cloned()
            .unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        let reading = self.rebs.first().cloned();
        let romaji = reading.as_deref().map(kana_to_romaji);
        let item_id = format!("jmdict:{}", self.ent_seq);
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Jap,
            LanguageItemType::Word,
            text,
        );
        item.reading = reading.clone();
        item.romanization = romaji.clone();
        item.meta = Some(LanguageMetadata::Japanese(JapaneseMetadata {
            kana: reading.clone(),
            romaji,
            kanji: (!self.kebs.is_empty()).then(|| self.kebs.join(" / ")),
            jlpt: None, // #16：JLPT 无明确开放词表源，V1 不填（KANJIDIC2 仅提供汉字级标记）
        }));
        for (rank, sense) in self.senses.iter().enumerate() {
            let gloss = sense.gloss.join("; ");
            if gloss.is_empty() && sense.pos.is_empty() {
                continue;
            }
            item.meanings.push(ImportedMeaning {
                id: format!("{item_id}:m{rank}"),
                pos: (!sense.pos.is_empty()).then(|| sense.pos.join("; ")),
                gloss: (!gloss.is_empty()).then_some(gloss),
                raw: None,
                sense_key: None,
                lang: Some("en".to_string()),
                rank: rank as i64,
            });
        }
        Some(item)
    }
}

/// 实体松弛：标准实体还原；未知 `&name;` → `name`（JMdict POS 标注可读）。
fn relax_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            out.push_str(after);
            break;
        };
        let name = &after[..end];
        match name {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            "nbsp" => out.push(' '),
            "mdash" => out.push('—'),
            "ndash" => out.push('–'),
            "lsquo" => out.push('‘'),
            "rsquo" => out.push('’'),
            "" => out.push('&'),
            other if other.starts_with('#') => out.push_str(&decode_numeric_entity(other)),
            other => out.push_str(other), // 未知实体 → 裸名称
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn decode_numeric_entity(entity: &str) -> String {
    let digits = entity.trim_start_matches('#');
    let parsed = if let Some(hex) = digits.strip_prefix('x') {
        u32::from_str_radix(hex, 16)
    } else {
        digits.parse::<u32>()
    };
    parsed
        .ok()
        .and_then(char::from_u32)
        .map(|ch| ch.to_string())
        .unwrap_or_else(|| entity.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>1002990</ent_seq>
<k_ele><keb>食べる</keb></k_ele>
<r_ele><reb>たべる</reb></r_ele>
<sense>
<pos>&v1;</pos>
<pos>&vt;</pos>
<gloss>to eat</gloss>
<gloss>to live on (e.g. a salary)</gloss>
</sense>
</entry>
<entry>
<ent_seq>1000000</ent_seq>
<k_ele><keb>行く</keb></k_ele>
<r_ele><reb>いく</reb></r_ele>
<sense>
<pos>&v5k-s;</pos>
<gloss>to go</gloss>
</sense>
</entry>
</JMdict>"#;

    #[test]
    fn parses_entries() {
        let items = parse(SAMPLE).expect("parse");
        assert_eq!(items.len(), 2);
        let taberu = items
            .iter()
            .find(|item| item.text == "食べる")
            .expect("食べる");
        assert_eq!(taberu.id, "jmdict:1002990");
        assert_eq!(taberu.reading.as_deref(), Some("たべる"));
        assert_eq!(taberu.romanization.as_deref(), Some("taberu"));
        assert_eq!(taberu.meanings.len(), 1);
        assert_eq!(
            taberu.meanings[0].gloss.as_deref(),
            Some("to eat; to live on (e.g. a salary)")
        );
        // 实体松弛：&v1; &vt; → "v1; vt"
        assert!(
            taberu.meanings[0]
                .pos
                .as_deref()
                .unwrap_or_default()
                .contains("v1")
        );
    }

    #[test]
    fn handles_missing_keb_with_reb() {
        let raw = r#"<JMdict><entry><ent_seq>101</ent_seq><r_ele><reb>ぼろい</reb></r_ele>
        <sense><pos>&adj-i;</pos><gloss>lucrative</gloss></sense></entry></JMdict>"#;
        let items = parse(raw).expect("parse");
        assert_eq!(items[0].text, "ぼろい");
        assert_eq!(items[0].reading.as_deref(), Some("ぼろい"));
    }

    #[test]
    fn broken_xml_does_not_panic() {
        let raw = "<JMdict><entry><ent_seq>1</ent_seq><k_ele><keb>test</keb></k_ele></entry>";
        let _ = parse(raw);
    }

    #[test]
    fn empty_input_errors() {
        assert!(parse("").is_err());
    }
}
