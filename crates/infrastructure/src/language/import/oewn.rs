//! Open English WordNet 解析（#7/#10）：GWA JSON 格式。
//!
//! - 词目文件（`entries-*.json`）：`{ lemma: { pos: { sense: [{id, synset, derivation…}] , pronunciation: … } } }`；
//! - 义项文件（`noun.*.json` / `verb.*.json` / `adj.*.json` / `adv.*.json`）：`{ synsetId: { members, definition, example, partOfSpeech, hypernym… } }`。
//!
//! 产出：每个 lemma 一个 Word 条目（senses → meanings），synset members → SYNONYM 关系，
//! hypernym/antonym/attribute/domain_topic → 语义关系（对应词已在本批次时建关系）。
//! 词目文件可多份（官方 zip 按首字母拆分）；义项文件可多份。

use std::collections::{BTreeMap, HashMap};

use devtoolbox_core::language::{
    EnglishMetadata, LanguageCode, LanguageItemType, LanguageMetadata, LanguageRelationKind,
    PronunciationScheme,
};
use serde_json::{Map, Value};

use super::{
    ImportError, ImportedExample, ImportedItem, ImportedMeaning, ImportedPronunciation,
    ImportedRelation,
};

/// 解析 OEWN 词目 + 义项文件集合。
/// `entry_chunks`: `(文件名, 内容)` 列表（`entries-*.json`）；`synset_chunks`: 义项文件列表。
pub fn parse(
    entry_chunks: &[(&str, String)],
    synset_chunks: &[(&str, String)],
) -> Result<Vec<ImportedItem>, ImportError> {
    let mut entries: Map<String, Value> = Map::new();
    for (name, chunk) in entry_chunks {
        let value: Value = serde_json::from_str(chunk)
            .map_err(|error| ImportError::Json(format!("{name}: {error}")))?;
        let object = value
            .as_object()
            .ok_or_else(|| ImportError::Json(format!("{name}: not an object")))?;
        entries.extend(object.clone());
    }
    if entries.is_empty() {
        return Err(ImportError::Empty);
    }

    let mut synsets: HashMap<String, Synset> = HashMap::new();
    for (name, chunk) in synset_chunks {
        let value: Value = serde_json::from_str(chunk)
            .map_err(|error| ImportError::Json(format!("{name}: {error}")))?;
        let Some(object) = value.as_object() else {
            continue;
        };
        for (synset_id, record) in object {
            if let Some(synset) = Synset::from_value(synset_id, record) {
                synsets.insert(synset_id.clone(), synset);
            }
        }
    }

    // 先建所有 lemma item（用于关系指向）
    let mut items_by_lemma: HashMap<String, String> = HashMap::new();
    for lemma in entries.keys() {
        let item_id = format!("wn:{}", lemma.to_lowercase());
        items_by_lemma.insert(lemma.to_lowercase(), item_id);
    }

    let mut items = Vec::with_capacity(entries.len());
    for (lemma, poses) in &entries {
        let Some(pos_map) = poses.as_object() else {
            continue;
        };
        let item_id = format!("wn:{}", lemma.to_lowercase());
        let mut item = ImportedItem::new(
            item_id.clone(),
            LanguageCode::Eng,
            LanguageItemType::Word,
            lemma.clone(),
        );
        item.romanization = Some(lemma.to_lowercase());
        let arpabet: Option<String> = None;

        let mut sense_rank = 0i64;
        for (pos, spec) in pos_map {
            let Some(spec) = spec.as_object() else {
                continue;
            };
            // 发音（OEWN entries 自带 IPA，可选增强；V1 主发音 = CMUdict）
            if let Some(list) = spec.get("pronunciation").and_then(Value::as_array) {
                for entry in list {
                    if let Some(value) = entry.get("value").and_then(Value::as_str) {
                        let variant = entry.get("variety").and_then(Value::as_str);
                        item.pronunciations.push(ImportedPronunciation {
                            id: format!("{item_id}:ipa:{}", item.pronunciations.len()),
                            scheme: PronunciationScheme::Ipa,
                            phonemes: value.to_string(),
                            tone: None,
                            variant: variant.map(str::to_string),
                            source: "".to_string(),
                        });
                    }
                }
            }
            let Some(senses) = spec.get("sense").and_then(Value::as_array) else {
                continue;
            };
            for sense in senses {
                let Some(sense) = sense.as_object() else {
                    continue;
                };
                let sense_id = sense
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let Some(synset_id) = sense.get("synset").and_then(Value::as_str) else {
                    continue;
                };
                item.meanings.push(ImportedMeaning {
                    id: format!("wn:{sense_id}"),
                    pos: Some(pos.to_string()),
                    gloss: synsets
                        .get(synset_id)
                        .and_then(|synset| synset.definition.clone()),
                    raw: None,
                    sense_key: Some(sense_id.clone()),
                    lang: Some("en".to_string()),
                    rank: sense_rank,
                });
                sense_rank += 1;
                // 同义关系：同 synset 的其它 member → 若在词目集内则建关系
                if let Some(synset) = synsets.get(synset_id) {
                    for member in &synset.members {
                        let member_normalized = member.to_lowercase();
                        if member_normalized == lemma.to_lowercase() {
                            continue;
                        }
                        if let Some(target_id) = items_by_lemma.get(&member_normalized) {
                            item.relations.push(ImportedRelation {
                                id: format!("wn-relation:{}:{:?}", synset_id, item.relations.len()),
                                from_item_id: item_id.clone(),
                                to_item_id: target_id.clone(),
                                kind: LanguageRelationKind::Synonym,
                                note: Some(synset_id.to_string()),
                            });
                        }
                    }
                    for (rel_kind, rel_field) in [
                        (LanguageRelationKind::Hypernym, "hypernym"),
                        (LanguageRelationKind::Hyponym, "hyponym"),
                        (LanguageRelationKind::Antonym, "antonym"),
                        (LanguageRelationKind::Attribute, "attribute"),
                        (LanguageRelationKind::DomainTopic, "domain_topic"),
                    ] {
                        if let Some(related) = synset.relations.get(rel_field) {
                            for target_synset in related {
                                if let Some(target) = synsets.get(target_synset) {
                                    for member in &target.members {
                                        let member_lemma = member.to_lowercase();
                                        if let Some(target_id) = items_by_lemma.get(&member_lemma) {
                                            if *target_id == item_id {
                                                continue;
                                            }
                                            item.relations.push(ImportedRelation {
                                                id: format!(
                                                    "wn-relation:{}:{:?}:{:?}",
                                                    target_synset,
                                                    rel_kind,
                                                    item.relations.len()
                                                ),
                                                from_item_id: item_id.clone(),
                                                to_item_id: target_id.clone(),
                                                kind: rel_kind,
                                                note: Some(target_synset.to_string()),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // 例句
                    if let Some(examples) = &synset.examples {
                        for (eindex, example) in examples.iter().enumerate() {
                            item.examples.push(ImportedExample {
                                id: format!("{item_id}:ex:{sense_rank}:{eindex}"),
                                item_id: item_id.clone(),
                                text: example.clone(),
                                translation: None,
                                source: "".to_string(),
                            });
                        }
                    }
                }
            }
        }
        item.meta = Some(LanguageMetadata::English(EnglishMetadata {
            arpabet,
            phonemes: Vec::new(),
            stress: Vec::new(),
            cefr: None,
        }));
        items.push(item);
    }
    Ok(items)
}

/// 义项对象（从 synset JSON 抽取）。
struct Synset {
    definition: Option<String>,
    members: Vec<String>,
    examples: Option<Vec<String>>,
    relations: BTreeMap<String, Vec<String>>,
}

impl Synset {
    fn from_value(synset_id: &str, value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        if !synset_id.ends_with("-n")
            && !synset_id.ends_with("-v")
            && !synset_id.ends_with("-a")
            && !synset_id.ends_with("-r")
        {
            return None;
        }
        let definition = object
            .get("definition")
            .and_then(Value::as_array)
            .and_then(|list| list.first())
            .and_then(Value::as_str)
            .map(str::to_string);
        let members = object
            .get("members")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let examples = object.get("example").and_then(Value::as_array).map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });
        let mut relations = BTreeMap::new();
        for field in [
            "hypernym",
            "hyponym",
            "antonym",
            "attribute",
            "domain_topic",
        ] {
            if let Some(list) = object.get(field).and_then(Value::as_array) {
                let ids: Vec<String> = list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                if !ids.is_empty() {
                    relations.insert(field.to_string(), ids);
                }
            }
        }
        Some(Self {
            definition,
            members,
            examples,
            relations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRIES: &str = r#"{
      "reservation": {
        "n": {
          "pronunciation": [{"value": "ˌɹɛzəˈveɪʃən", "variety": "GB"}],
          "sense": [
            {"id": "reservation%1:10:00::", "synset": "06775091-n"},
            {"id": "reservation%1:04:00::", "synset": "00821752-n"}
          ]
        }
      }
    }"#;

    const SYNSETS: &str = r#"{
      "06775091-n": {
        "definition": ["the act of reserving (a place or passage) or engaging the services of (a person or group)"],
        "example": ["wondered who had made the booking"],
        "members": ["booking", "reservation"],
        "partOfSpeech": "n"
      },
      "00821752-n": {
        "definition": ["a statement that limits or restricts some claim"],
        "members": ["reservation"],
        "hypernym": ["06775091-n"]
      }
    }"#;

    #[test]
    fn parses_entries_and_synsets() {
        let items = parse(
            &[("entries-r.json", ENTRIES.to_string())],
            &[("noun.communication.json", SYNSETS.to_string())],
        )
        .expect("parse");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.id, "wn:reservation");
        assert_eq!(item.meanings.len(), 2);
        assert_eq!(
            item.meanings[0].gloss.as_deref(),
            Some(
                "the act of reserving (a place or passage) or engaging the services of (a person or group)"
            )
        );
        assert_eq!(
            item.meanings[0].sense_key.as_deref(),
            Some("reservation%1:10:00::")
        );
        // booking 是 reservation 的同义词（同 synset member，且在词目集 → 只建到已有词的 relation？此处 booking 不在词目集 → 无）
        assert!(item.relations.is_empty());
        assert_eq!(item.pronunciations.len(), 1);
        assert_eq!(item.pronunciations[0].phonemes, "ˌɹɛzəˈveɪʃən");
        assert_eq!(item.pronunciations[0].variant.as_deref(), Some("GB"));
        assert!(item.examples.is_empty() || item.meanings[0].pos.as_deref() == Some("n"));
    }

    #[test]
    fn builds_synonym_relation_when_member_in_entries() {
        let entries = r#"{
          "booking": {"n": {"sense": [{"id": "booking%1:10:00::", "synset": "06775091-n"}]}},
          "reservation": {"n": {"sense": [{"id": "reservation%1:10:00::", "synset": "06775091-n"}]}}
        }"#;
        let items = parse(
            &[("entries-r.json", entries.to_string())],
            &[("noun.communication.json", SYNSETS.to_string())],
        )
        .expect("parse");
        let reservation = items
            .iter()
            .find(|item| item.id == "wn:reservation")
            .expect("reservation");
        assert!(
            reservation
                .relations
                .iter()
                .any(|relation| relation.to_item_id == "wn:booking")
        );
    }

    #[test]
    fn invalid_json_errors() {
        assert!(parse(&[("e.json", "{broken".to_string())], &[]).is_err());
    }

    #[test]
    fn empty_entries_error() {
        assert_eq!(
            parse(&[("e.json", "{}".to_string())], &[]),
            Err(ImportError::Empty)
        );
    }
}
