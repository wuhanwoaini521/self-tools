//! Language 数据集导入框架（#44/#45/#76）。
//!
//! 统一：Raw → Checksum → Parser(纯函数) → Normalizer → Validator → 去重 → SQLite。
//! 每个数据集实现 `LanguageDatasetImporter`；许可证 Gate：Unknown/非商业数据被拒绝进入默认包。

use std::time::Duration;

use devtoolbox_core::language::{
    LanguageCode, LanguageItemType, LanguageMetadata, LanguageRelationKind, LanguageSource,
    LicenseKind, PronunciationScheme, SourceLicense,
};
use thiserror::Error;

use super::store::LanguageStore;
use crate::error::InfrastructureError;

pub mod cc_canto;
pub mod cedict;
pub mod cmudict;
pub mod jmdict;
pub mod kanjidic2;
pub mod oewn;
pub mod tatoeba;
pub mod words_hk;

/// 解析/校验错误（解析器纯函数返回；运行时测不依赖网络）。
#[derive(Debug, Error, PartialEq)]
pub enum ImportError {
    #[error("dataset has no declared license (kind=unknown) — DO NOT IMPORT")]
    UnknownLicense,
    #[error("non-commercial source excluded from default pack: {0}")]
    NonCommercial(String),
    #[error("invalid json: {0}")]
    Json(String),
    #[error("invalid xml: {0}")]
    Xml(String),
    #[error("malformed line {line}: {detail}")]
    MalformedLine { line: usize, detail: String },
    #[error("empty dataset")]
    Empty,
}

/// 导入中间结构（Normalizer 输出层；与数据库列一一对应）。
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ImportedItem {
    pub id: String,
    pub language: LanguageCode,
    pub item_type: LanguageItemType,
    pub text: String,
    pub reading: Option<String>,
    pub romanization: Option<String>,
    pub meta: Option<LanguageMetadata>,
    pub pronunciations: Vec<ImportedPronunciation>,
    pub meanings: Vec<ImportedMeaning>,
    pub relations: Vec<ImportedRelation>,
    pub examples: Vec<ImportedExample>,
    /// item_extra 任意 JSON（句子 author/license、english index 等）。
    pub extra: Option<serde_json::Value>,
    /// 附加搜索词（word.hk English Index 等）。
    pub search_terms: Vec<String>,
}

impl ImportedItem {
    #[must_use]
    pub fn new(
        id: String,
        language: LanguageCode,
        item_type: LanguageItemType,
        text: String,
    ) -> Self {
        Self {
            id,
            language,
            item_type,
            text,
            ..ImportedItem::default()
        }
    }
}

impl Default for ImportedItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            language: LanguageCode::Eng,
            item_type: LanguageItemType::Word,
            text: String::new(),
            reading: None,
            romanization: None,
            meta: None,
            pronunciations: Vec::new(),
            meanings: Vec::new(),
            relations: Vec::new(),
            examples: Vec::new(),
            extra: None,
            search_terms: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ImportedPronunciation {
    pub id: String,
    pub scheme: PronunciationScheme,
    pub phonemes: String,
    pub tone: Option<u8>,
    pub variant: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ImportedMeaning {
    pub id: String,
    pub pos: Option<String>,
    pub gloss: Option<String>,
    pub raw: Option<String>,
    pub sense_key: Option<String>,
    pub lang: Option<String>,
    pub rank: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ImportedRelation {
    pub id: String,
    pub from_item_id: String,
    pub to_item_id: String,
    pub kind: LanguageRelationKind,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ImportedExample {
    pub id: String,
    pub item_id: String,
    pub text: String,
    pub translation: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct ImportReport {
    pub inserted: i64,
    pub updated: i64,
    pub skipped: i64,
}

/// 数据集导入器接口（#45）。
pub trait LanguageDatasetImporter {
    /// 数据集来源（含许可证）。
    fn source(&self) -> &'static LanguageSource;
    fn importer_version(&self) -> i64;
    /// 解析原始内容为标准化条目（纯函数；无网络）。
    fn parse(&self, raw: &str) -> Result<Vec<ImportedItem>, ImportError>;
}

/// 许可证 Gate（#76）：未知/非商业许可 → 拒绝。
pub fn gate_license(source: &LanguageSource) -> Result<(), ImportError> {
    let license = &source.license;
    if license.is_unknown() {
        return Err(ImportError::UnknownLicense);
    }
    if !license.is_commercial_safe() {
        return Err(ImportError::NonCommercial(source.name.clone()));
    }
    Ok(())
}

/// 把解析结果写入 store：先登记来源与清单，再导数据。
/// `source` 的 `id` 作为 `language_items.source` 与 manifest.source_id 落库。
pub fn import_into(
    store: &mut LanguageStore,
    source: &LanguageSource,
    manifest_id: &str,
    manifest_name: &str,
    items: &[ImportedItem],
    raw_file: Option<&str>,
    now: i64,
) -> Result<ImportReport, InfrastructureError> {
    store.upsert_source(source)?;
    store.insert_manifest(&devtoolbox_core::language::DatasetManifest {
        id: manifest_id.to_string(),
        name: manifest_name.to_string(),
        language: source.id.split(':').next().unwrap_or("").to_string(),
        version: source.dataset_version.clone(),
        downloaded_at: source.downloaded_at,
        source_id: source.id.clone(),
        checksum: None,
        raw_file: raw_file.map(str::to_string),
        record_count: items.len() as i64,
        importer_version: 1,
        imported_at: now,
    })?;
    store.import_items(items, &source.id, now)
}

/// 从失败导入中恢复：报告级别错误（供 CLI/应用复用）。
pub fn import_failed(report: &ImportReport) -> bool {
    report.inserted == 0 && report.updated == 0 && report.skipped > 0
}

/// 简易校验和（非加密；用于清单记录）。
pub fn simple_sha256(bytes: &[u8]) -> String {
    use std::fmt::Write;
    // 不引入外部 crate：用固定 FNV-1a 双通做档案标记即可（非安全用途）。
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x84222325cbf29ce4;
    for byte in bytes {
        h1 ^= u64::from(*byte);
        h1 = h1.wrapping_mul(0x100000001b3);
        h2 ^= u64::from(*byte).rotate_left(1);
        h2 = h2.wrapping_mul(0x100000001b3 ^ 0x9e3779b1);
    }
    let mut out = String::with_capacity(32);
    let _ = write!(out, "{h1:016x}{h2:016x}");
    out
}

// 兼容辅助：解析超时估算（CLI 统计用）。
pub fn estimated_duration(count: usize) -> Duration {
    Duration::from_millis(((count / 1_000).clamp(1, 120) * 250) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_license_rejected() {
        let source = LanguageSource {
            id: "t:x".into(),
            name: "no license".into(),
            license: SourceLicense::default(),
            ..dummy_source_fields()
        };
        assert_eq!(gate_license(&source), Err(ImportError::UnknownLicense));
    }

    #[test]
    fn nc_license_rejected() {
        let source = LanguageSource {
            id: "t:y".into(),
            name: "nc".into(),
            license: SourceLicense::cc_by_nc(),
            ..dummy_source_fields()
        };
        assert_eq!(
            gate_license(&source),
            Err(ImportError::NonCommercial("nc".into()))
        );
    }

    #[test]
    fn commercial_sources_accepted() {
        for license in [
            SourceLicense::public_domain(),
            SourceLicense::cc0(),
            SourceLicense::cc_by(),
            SourceLicense::cc_by_sa(),
            SourceLicense::custom(),
        ] {
            let source = LanguageSource {
                license,
                ..dummy_source_fields()
            };
            assert!(gate_license(&source).is_ok(), "{:?}", license.kind);
        }
    }

    fn dummy_source_fields() -> LanguageSource {
        LanguageSource {
            id: "t:z".into(),
            name: String::new(),
            homepage: String::new(),
            download_source: String::new(),
            dataset_version: String::new(),
            downloaded_at: None,
            license: SourceLicense::default(),
            license_url: None,
            attribution: String::new(),
            commercial_use: false,
            redistribution: false,
            notes: None,
        }
    }
}

/// 许可证常量集中定义（与 docs/language/DATA_SOURCES.md 逐项对应）。
/// 各数据集来源常量（`'static` 字面量；每次导入时调用以生成 LanguageSource 值）。
pub mod sources {
    use super::*;
    use devtoolbox_core::language::LanguageSource;

    /// Open English WordNet（CC BY 4.0，#7）。
    pub fn open_english_wordnet() -> LanguageSource {
        source_static(
            "oewn",
            "Open English WordNet",
            "https://en-word.net/",
            "https://en-word.net/static/english-wordnet-2025-json.zip",
            "2025 Edition (2025-12-31)",
            LicenseKind::CcBy,
            "https://creativecommons.org/licenses/by/4.0/",
            "The Open English WordNet Team (globalwordnet/english-wordnet)",
            "Definition/sense/synonym/relation 主源；2025 版为词项版（专名移入 Namenet）。",
        )
    }

    /// CMUdict（#9）。
    pub fn cmudict() -> LanguageSource {
        source_static(
            "cmudict",
            "CMUdict (CMU Pronouncing Dictionary)",
            "http://www.speech.cs.cmu.edu/cgi-bin/cmudict",
            "https://raw.githubusercontent.com/cmusphinx/cmudict/master/cmudict.dict",
            "cmusphinx master (0.7b 系)",
            LicenseKind::Custom,
            "https://github.com/cmusphinx/cmudict",
            "Copyright (C) 1993-2015 Carnegie Mellon University",
            "ARPABET 发音；research/commercial 免费，使用或再分发需注明来源。",
        )
    }

    /// JMdict（CC BY-SA 4.0，#12/#13）。
    pub fn jmdict() -> LanguageSource {
        source_static(
            "jmdict",
            "JMdict (EDRDG)",
            "https://www.edrdg.org/wiki/JMdict-EDICT_Dictionary_Project.html",
            "https://www.edrdg.org/pub/Nihongo/JMdict_e.gz",
            "daily (downloaded 2026-08-31)",
            LicenseKind::CcBySa,
            "https://creativecommons.org/licenses/by-sa/4.0/",
            "JMDict (Japanese-Multilingual Dictionary) © Electronic Dictionary Research and Development Group",
            "使用 JMdict_e.gz（English Gloss）。应用内 Settings → Language Data 展示此 attribution。",
        )
    }

    /// KANJIDIC2（CC BY-SA 4.0 + 专项条件，#14/#15）。
    pub fn kanjidic2() -> LanguageSource {
        source_static(
            "kanjidic2",
            "KANJIDIC2 (EDRDG)",
            "https://www.edrdg.org/wiki/KANJIDIC_Project.html",
            "https://www.edrdg.org/kanjidic/kanjidic2.xml.gz",
            "daily (downloaded 2026-08-31, 13,108 kanji)",
            LicenseKind::CcBySa,
            "https://creativecommons.org/licenses/by-sa/4.0/",
            "KANJIDIC2 © Electronic Dictionary Research and Development Group",
            "仅导入许可明确的学习字段（character/reading/meaning/stroke_count/grade/radical/kanjidic2_jlpt）；SKIP 等字段因第三方许可不导入。",
        )
    }

    /// CC-CEDICT（CC BY-SA 4.0，#17/#18）。
    pub fn cc_cedict() -> LanguageSource {
        source_static(
            "cc-cedict",
            "CC-CEDICT (MDBG)",
            "https://www.mdbg.net/chinese/dictionary?page=cc-cedict",
            "https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz",
            "release (downloaded 2026-08-31, ~124,935 entries)",
            LicenseKind::CcBySa,
            "https://creativecommons.org/licenses/by-sa/4.0/",
            "CC-CEDICT © 2026 MDBG; 前身 CEDICT © Paul Andrew Denisowski (1997-98)",
            "HSK 非本数据集字段 → V1 不导入。",
        )
    }

    /// words.hk word list（Public Domain，#21/#22）。
    pub fn words_hk_word_list() -> LanguageSource {
        source_static(
            "words-hk",
            "words.hk 粵典詞表 (word list)",
            "https://words.hk/faiman/analysis/",
            "https://words.hk/faiman/analysis/wordslist.json",
            "last updated 2026-03-30 (62,274 words)",
            LicenseKind::PublicDomain,
            "",
            "words.hk (粵典) — word list, public domain",
            "仅 Public Domain 列表；释义/例句属 Non-Commercial Open Data License，不导入。",
        )
    }

    /// words.hk char list（Public Domain）。
    pub fn words_hk_char_list() -> LanguageSource {
        source_static(
            "words-hk-chars",
            "words.hk 粵典字表 (character list)",
            "https://words.hk/faiman/analysis/",
            "https://words.hk/faiman/analysis/charlist.json",
            "2026 (5,875 characters)",
            LicenseKind::PublicDomain,
            "",
            "words.hk (粵典) — character list, public domain",
            "",
        )
    }

    /// words.hk English Index（Public Domain）。
    pub fn words_hk_english_index() -> LanguageSource {
        source_static(
            "words-hk-english",
            "words.hk 英粵對照表 (English index)",
            "https://words.hk/faiman/analysis/",
            "https://words.hk/faiman/analysis/englishindex.json",
            "2026 (40,839 entries)",
            LicenseKind::PublicDomain,
            "",
            "words.hk (粵典) — English index, public domain",
            "用于英文 → 粤语词搜索。",
        )
    }

    /// CC-Canto（CC BY-SA 3.0，#24）。
    pub fn cc_canto() -> LanguageSource {
        source_static(
            "cc-canto",
            "CC-Canto (Pleco)",
            "https://cantonese.org/",
            "https://cantonese.org/cccanto-170202.zip",
            "2017-02-02 (webdist, ~22k entries)",
            LicenseKind::CcBySa,
            "https://creativecommons.org/licenses/by-sa/3.0/",
            "CC-Canto © 2015-17 Pleco Inc.",
            "粤语释义/粤拼增补源。",
        )
    }

    /// Tatoeba（CC BY 2.0 FR 默认 / CC0 子集，#27-#31）。
    pub fn tatoeba() -> LanguageSource {
        source_static(
            "tatoeba",
            "Tatoeba",
            "https://tatoeba.org/en/downloads",
            "https://downloads.tatoeba.org/exports/",
            "daily (downloaded 2026-08-31)",
            LicenseKind::CcBy,
            "https://creativecommons.org/licenses/by/2.0/fr/",
            "Tatoeba Project (contributors)",
            "默认句子 CC BY 2.0 FR（逐句 attribution）；CC0 子集另行注明。",
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn source_static(
        id: &'static str,
        name: &'static str,
        homepage: &'static str,
        download_source: &'static str,
        version: &'static str,
        kind: LicenseKind,
        license_url: &'static str,
        attribution: &'static str,
        notes: &'static str,
    ) -> LanguageSource {
        let license = match kind {
            LicenseKind::PublicDomain => SourceLicense::public_domain(),
            LicenseKind::Cc0 => SourceLicense::cc0(),
            LicenseKind::CcBy => SourceLicense::cc_by(),
            LicenseKind::CcBySa => SourceLicense::cc_by_sa(),
            LicenseKind::CcByNc => SourceLicense::cc_by_nc(),
            _ => SourceLicense::custom(),
        };
        LanguageSource {
            id: id.into(),
            name: name.into(),
            homepage: homepage.into(),
            download_source: download_source.into(),
            dataset_version: version.into(),
            downloaded_at: None,
            license,
            license_url: (!license_url.is_empty()).then(|| license_url.into()),
            attribution: attribution.into(),
            commercial_use: license.commercial_use_allowed,
            redistribution: license.redistribution_allowed,
            notes: (!notes.is_empty()).then(|| notes.into()),
        }
    }
}
