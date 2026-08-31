//! 从原始数据文件导入（CLI 与扩展包用）：读文件（支持 .gz / .zip）→ 真实 importer → 落库。
//!
//! 与 Starter（内置 fixture）共用同一套解析器与 `import_into` 管线；仅输入来源不同。
//! 完整数据的官方下载地址见 `docs/language/DATA_SOURCES.md`。

use std::io::Read;
use std::path::Path;

use devtoolbox_infrastructure::language::LanguageStore;
use devtoolbox_infrastructure::language::import::{
    ImportError, ImportReport, cc_canto, cedict, cmudict, import_into, jmdict, kanjidic2, oewn,
    sources, tatoeba, words_hk,
};

use crate::ApplicationError;

/// 读取文件内容；`.gz` 自动解压（flate2），其余按 UTF-8 原样读。
pub fn read_raw(path: &Path) -> Result<String, ApplicationError> {
    let bytes =
        std::fs::read(path).map_err(|error| ApplicationError::License(error.to_string()))?;
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
    {
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut text = String::new();
        decoder
            .read_to_string(&mut text)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        Ok(text)
    } else {
        String::from_utf8(bytes).map_err(|error| ApplicationError::License(error.to_string()))
    }
}

/// (文件名, 内容) 分块列表（条目文件 / 义项文件）。
type JsonChunks = Vec<(&'static str, String)>;

/// 读取 OEWN zip 内的 JSON 文件（entries-* 与义项文件）。
fn read_oewn_zip(zip_path: &Path) -> Result<(JsonChunks, JsonChunks), ApplicationError> {
    let file = std::fs::File::open(zip_path)
        .map_err(|error| ApplicationError::License(error.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ApplicationError::License(error.to_string()))?;
    let mut entries = Vec::new();
    let mut synsets = Vec::new();
    for index in 0..archive.len() {
        let mut member = archive
            .by_index(index)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let name = member.name().to_string();
        if !name.ends_with(".json") || name.ends_with("frames.json") {
            continue;
        }
        let mut content = String::new();
        member
            .read_to_string(&mut content)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        if name.starts_with("entries-") {
            entries.push((leak_name(name.as_str()), content));
        } else {
            synsets.push((leak_name(name.as_str()), content));
        }
    }
    if entries.is_empty() || synsets.is_empty() {
        return Err(ApplicationError::License(
            "oewn zip 缺少 entries-*.json 或义项文件".to_string(),
        ));
    }
    Ok((entries, synsets))
}

/// 文件名为 'static 常量（CLI 一次性导入，泄漏可接受）。
fn leak_name(name: &str) -> &'static str {
    Box::leak(name.to_string().into_boxed_str())
}

fn license_error(error: ImportError) -> ApplicationError {
    ApplicationError::License(error.to_string())
}

/// English Core Pack（OEWN + CMUdict）。
pub fn import_english(
    store: &mut LanguageStore,
    oewn_path: &Path,
    cmudict_path: &Path,
    manifest_id: &str,
) -> Result<(ImportReport, ImportReport), ApplicationError> {
    let now = devtoolbox_infrastructure::now_unix();
    // OEWN
    let source = sources::open_english_wordnet();
    let (entry_chunks, synset_chunks) = if oewn_path.is_dir() {
        let mut entries = Vec::new();
        let mut synsets = Vec::new();
        let mut names: Vec<_> = std::fs::read_dir(oewn_path)
            .map_err(|error| ApplicationError::License(error.to_string()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        names.sort();
        for path in names {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();
            if !name.ends_with(".json") || name.ends_with("frames.json") {
                continue;
            }
            let content = read_raw(&path)?;
            if name.starts_with("entries-") {
                entries.push((leak_name(&name), content));
            } else {
                synsets.push((leak_name(&name), content));
            }
        }
        (entries, synsets)
    } else {
        read_oewn_zip(oewn_path)?
    };
    if entry_chunks.is_empty() || synset_chunks.is_empty() {
        return Err(license_error(ImportError::Empty));
    }
    let items = oewn::parse(&entry_chunks, &synset_chunks).map_err(license_error)?;
    let oewn_report = import_into(
        store,
        &source,
        manifest_id,
        "English Core — Open English WordNet",
        &items,
        Some(oewn_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)?;
    // CMUdict（挂 OEWN 词条 / 独立成词）
    let cmu_content = read_raw(cmudict_path)?;
    let cmu_source = sources::cmudict();
    let cmu_items = cmudict::parse(&cmu_content).map_err(license_error)?;
    let mut standalone = Vec::new();
    for item in cmu_items.into_iter() {
        let base = item
            .extra
            .as_ref()
            .and_then(|extra| extra.get("enrich_text"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&item.text);
        let wn_id = format!("wn:{}", base.to_lowercase());
        if store
            .item(&wn_id)
            .map_err(ApplicationError::from)?
            .is_some()
        {
            let mut pronunciation = item.pronunciations[0].clone();
            pronunciation.source = cmu_source.id.clone();
            store
                .attach_pronunciation(&wn_id, &pronunciation, &cmu_source.id)
                .map_err(ApplicationError::from)?;
        } else {
            standalone.push(item);
        }
    }
    let cmu_report = import_into(
        store,
        &cmu_source,
        &format!("{manifest_id}-cmudict"),
        "English Core — CMUdict",
        &standalone,
        Some(cmudict_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)?;
    Ok((oewn_report, cmu_report))
}

pub fn import_japanese(
    store: &mut LanguageStore,
    jmdict_path: &Path,
    manifest_id: &str,
) -> Result<ImportReport, ApplicationError> {
    let now = devtoolbox_infrastructure::now_unix();
    let source = sources::jmdict();
    let content = read_raw(jmdict_path)?;
    let items = jmdict::parse(&content).map_err(license_error)?;
    import_into(
        store,
        &source,
        manifest_id,
        "Japanese Core — JMdict",
        &items,
        Some(jmdict_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)
}

pub fn import_kanji(
    store: &mut LanguageStore,
    kanjidic2_path: &Path,
    manifest_id: &str,
) -> Result<ImportReport, ApplicationError> {
    let now = devtoolbox_infrastructure::now_unix();
    let source = sources::kanjidic2();
    let content = read_raw(kanjidic2_path)?;
    let items = kanjidic2::parse(&content).map_err(license_error)?;
    import_into(
        store,
        &source,
        manifest_id,
        "Japanese Core — KANJIDIC2",
        &items,
        Some(kanjidic2_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)
}

pub fn import_mandarin(
    store: &mut LanguageStore,
    cedict_path: &Path,
    manifest_id: &str,
) -> Result<ImportReport, ApplicationError> {
    let now = devtoolbox_infrastructure::now_unix();
    let source = sources::cc_cedict();
    let content = read_raw(cedict_path)?;
    let items = cedict::parse(&content).map_err(license_error)?;
    import_into(
        store,
        &source,
        manifest_id,
        "Mandarin Core — CC-CEDICT",
        &items,
        Some(cedict_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)
}

/// Cantonese Core Pack：words.hk word list + (可选) char list / english index / CC-Canto。
pub fn import_cantonese(
    store: &mut LanguageStore,
    words_hk_path: &Path,
    char_list: Option<&Path>,
    english_index: Option<&Path>,
    cc_canto: Option<&Path>,
    manifest_id: &str,
) -> Result<ImportReport, ApplicationError> {
    let now = devtoolbox_infrastructure::now_unix();
    let mut total = ImportReport::default();
    let source = sources::words_hk_word_list();
    let content = read_raw(words_hk_path)?;
    let items = words_hk::parse_word_list(&content).map_err(license_error)?;
    let report = import_into(
        store,
        &source,
        manifest_id,
        "Cantonese Core — words.hk word list",
        &items,
        Some(words_hk_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)?;
    accumulate(&mut total, &report);

    if let Some(chars_path) = char_list {
        let source = sources::words_hk_char_list();
        let content = read_raw(chars_path)?;
        let items = words_hk::parse_char_list(&content).map_err(license_error)?;
        let report = import_into(
            store,
            &source,
            &format!("{manifest_id}-chars"),
            "Cantonese Core — words.hk 字表",
            &items,
            Some(chars_path.to_string_lossy().as_ref()),
            now,
        )
        .map_err(ApplicationError::from)?;
        accumulate(&mut total, &report);
    }
    if let Some(index_path) = english_index {
        // 英文索引需要词表条目做关联（直接用刚解析的内存词表）
        let content = read_raw(index_path)?;
        let pairs = words_hk::parse_english_index(&content, &items).map_err(license_error)?;
        if !pairs.is_empty() {
            store
                .attach_search_terms(&pairs)
                .map_err(ApplicationError::from)?;
            let source = sources::words_hk_english_index();
            store
                .upsert_source(&source)
                .map_err(ApplicationError::from)?;
            store
                .insert_manifest(&devtoolbox_core::language::DatasetManifest {
                    id: format!("{manifest_id}-english"),
                    name: "Cantonese Core — words.hk English index".to_string(),
                    language: "yue".to_string(),
                    version: source.dataset_version.clone(),
                    downloaded_at: None,
                    source_id: source.id.clone(),
                    checksum: None,
                    raw_file: Some(index_path.to_string_lossy().as_ref().to_string()),
                    record_count: pairs.len() as i64,
                    importer_version: 1,
                    imported_at: now,
                })
                .map_err(ApplicationError::from)?;
            total.inserted += pairs.len() as i64;
        }
    }
    if let Some(cc_canto_path) = cc_canto {
        let source = sources::cc_canto();
        let content = read_raw(cc_canto_path)?;
        let items = cc_canto::parse(&content).map_err(license_error)?;
        let report = import_into(
            store,
            &source,
            &format!("{manifest_id}-cc-canto"),
            "Cantonese Core — CC-Canto",
            &items,
            Some(cc_canto_path.to_string_lossy().as_ref()),
            now,
        )
        .map_err(ApplicationError::from)?;
        accumulate(&mut total, &report);
    }
    Ok(total)
}

/// 句子导入（Tatoeba）：`license` 传 "CC0 1.0" 或 "CC BY 2.0 FR"。
pub fn import_sentences(
    store: &mut LanguageStore,
    sentences_path: &Path,
    license: &str,
    manifest_id: &str,
) -> Result<ImportReport, ApplicationError> {
    let now = devtoolbox_infrastructure::now_unix();
    let source = sources::tatoeba();
    let content = read_raw(sentences_path)?;
    let items = tatoeba::parse(&content, license).map_err(license_error)?;
    import_into(
        store,
        &source,
        manifest_id,
        "Sentences — Tatoeba",
        &items,
        Some(sentences_path.to_string_lossy().as_ref()),
        now,
    )
    .map_err(ApplicationError::from)
}

fn accumulate(total: &mut ImportReport, report: &ImportReport) {
    total.inserted += report.inserted;
    total.updated += report.updated;
    total.skipped += report.skipped;
}
