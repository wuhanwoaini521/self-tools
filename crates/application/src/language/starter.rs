//! 内置 Starter Pack 安装（任务 #44/#73/#88）。
//!
//! Starter Pack = `tests/fixtures/language/**`（真实数据集的小型子集，均保留 attribution），
//! 编译期 `include_str!` 嵌入，运行期经**真实 importer 走完整管线**（Parser → Normalizer →
//! Validator → 去重 → SQLite）。首次使用离线即可安装；CI 与 App 共用同一入口。

use serde::{Deserialize, Serialize};

use devtoolbox_infrastructure::language::LanguageStore;
use devtoolbox_infrastructure::language::import::{
    ImportReport, ImportedItem, cc_canto, cedict, cmudict, import_into, jmdict, kanjidic2, oewn,
    sources, tatoeba, words_hk,
};
use devtoolbox_infrastructure::now_unix;

use crate::ApplicationError;

const EN_WORDNET_ENTRIES: &str =
    include_str!("../../../../tests/fixtures/language/en/wordnet-entries.json");
const EN_WORDNET_SYNSETS: &str =
    include_str!("../../../../tests/fixtures/language/en/wordnet-synsets.json");
const EN_CMUDICT: &str = include_str!("../../../../tests/fixtures/language/en/cmudict.dict");
const JP_JMDICT: &str = include_str!("../../../../tests/fixtures/language/jp/jmdict.xml");
const JP_KANJIDIC2: &str = include_str!("../../../../tests/fixtures/language/jp/kanjidic2.xml");
const ZH_CEDICT: &str = include_str!("../../../../tests/fixtures/language/zh/cedict.txt");
const YUE_WORDS: &str =
    include_str!("../../../../tests/fixtures/language/yue/words_hk_wordlist.json");
const YUE_CHARS: &str =
    include_str!("../../../../tests/fixtures/language/yue/words_hk_charlist.json");
const YUE_ENGLISH_INDEX: &str =
    include_str!("../../../../tests/fixtures/language/yue/words_hk_english_index.json");
const YUE_CCCANTO: &str = include_str!("../../../../tests/fixtures/language/yue/cccanto.txt");
const SENT_CC0: &str =
    include_str!("../../../../tests/fixtures/language/sentences/tatoeba-cc0.csv");
const SENT_JPN: &str =
    include_str!("../../../../tests/fixtures/language/sentences/tatoeba-jpn.tsv");
const SENT_CMN: &str =
    include_str!("../../../../tests/fixtures/language/sentences/tatoeba-cmn.tsv");
const SENT_YUE: &str =
    include_str!("../../../../tests/fixtures/language/sentences/tatoeba-yue.tsv");
const SENT_ENG: &str =
    include_str!("../../../../tests/fixtures/language/sentences/tatoeba-eng.tsv");

/// Starter 安装报告（Settings → Language Data 展示）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StarterReport {
    pub datasets: Vec<DatasetReport>,
    pub total_inserted: i64,
    pub total_updated: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DatasetReport {
    pub id: String,
    pub name: String,
    pub inserted: i64,
    pub updated: i64,
}

fn dataset_report(
    id: &str,
    name: &str,
    report: ImportReport,
    accumulated: &mut StarterReport,
) -> DatasetReport {
    accumulated.total_inserted += report.inserted;
    accumulated.total_updated += report.updated;
    DatasetReport {
        id: id.to_string(),
        name: name.to_string(),
        inserted: report.inserted,
        updated: report.updated,
    }
}

/// 安装全部 Starter 数据集（幂等：同一 item id 将更新而非重复）。
pub fn install_starter(
    store: &mut LanguageStore,
    only_language: Option<&str>,
) -> Result<StarterReport, ApplicationError> {
    let now = now_unix();
    let only = only_language.map(str::to_ascii_lowercase);
    let mut report = StarterReport::default();
    let want = |lang: &str| only.as_deref().is_none_or(|filter| filter == lang);

    // ---- English Core Pack ----
    if want("eng") {
        let source = sources::open_english_wordnet();
        let items = oewn::parse(
            &[("entries.json", EN_WORDNET_ENTRIES.to_string())],
            &[("synsets.json", EN_WORDNET_SYNSETS.to_string())],
        )
        .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-oewn",
            "English Core — Open English WordNet (starter)",
            &items,
            Some("tests/fixtures/language/en/*.json"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-oewn", "Open English WordNet", result, &mut report);
        report.datasets.push(dataset);

        // CMUdict：优先挂到 OEWN 同名词条（enrichment），其余独立成词
        let cmu_items = cmudict::parse(EN_CMUDICT)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let cmu_source = sources::cmudict();
        let mut standalone: Vec<ImportedItem> = Vec::new();
        let mut attached = 0i64;
        for item in cmu_items.into_iter() {
            let base = item
                .extra
                .as_ref()
                .and_then(|extra| extra.get("enrich_text"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&item.text);
            let wn_id = format!("wn:{}", base.to_lowercase());
            let exists = store.item(&wn_id).map_err(ApplicationError::from)?;
            let mut pronunciation = item.pronunciations[0].clone();
            pronunciation.source = cmu_source.id.clone();
            if exists.is_some() {
                if store
                    .attach_pronunciation(&wn_id, &pronunciation, &cmu_source.id)
                    .map_err(ApplicationError::from)?
                {
                    attached += 1;
                }
            } else {
                standalone.push(item);
            }
        }
        let result = import_into(
            store,
            &cmu_source,
            "starter-cmudict",
            "English Core — CMUdict (starter)",
            &standalone,
            Some("tests/fixtures/language/en/cmudict.dict"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let mut with_attached = result.clone();
        if attached > 0 {
            with_attached.inserted += attached;
        }
        let dataset = dataset_report("starter-cmudict", "CMUdict", with_attached, &mut report);
        report.datasets.push(dataset);
    }

    // ---- Japanese Core Pack ----
    if want("jpn") {
        let source = sources::jmdict();
        let items = jmdict::parse(JP_JMDICT)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-jmdict",
            "Japanese Core — JMdict (starter)",
            &items,
            Some("tests/fixtures/language/jp/jmdict.xml"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-jmdict", "JMdict", result, &mut report);
        report.datasets.push(dataset);

        let source = sources::kanjidic2();
        let items = kanjidic2::parse(JP_KANJIDIC2)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-kanjidic2",
            "Japanese Core — KANJIDIC2 (starter)",
            &items,
            Some("tests/fixtures/language/jp/kanjidic2.xml"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-kanjidic2", "KANJIDIC2", result, &mut report);
        report.datasets.push(dataset);
    }

    // ---- Mandarin Core Pack ----
    if want("cmn") {
        let source = sources::cc_cedict();
        let items = cedict::parse(ZH_CEDICT)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-cedict",
            "Mandarin Core — CC-CEDICT (starter)",
            &items,
            Some("tests/fixtures/language/zh/cedict.txt"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-cedict", "CC-CEDICT", result, &mut report);
        report.datasets.push(dataset);
    }

    // ---- Cantonese Core Pack ----
    if want("yue") {
        let source = sources::words_hk_word_list();
        let items = words_hk::parse_word_list(YUE_WORDS)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-words-hk",
            "Cantonese Core — words.hk word list (starter)",
            &items,
            Some("tests/fixtures/language/yue/words_hk_wordlist.json"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-words-hk", "words.hk 詞表", result, &mut report);
        report.datasets.push(dataset);

        let source = sources::words_hk_char_list();
        let items = words_hk::parse_char_list(YUE_CHARS)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-words-hk-chars",
            "Cantonese Core — words.hk 字表 (starter)",
            &items,
            Some("tests/fixtures/language/yue/words_hk_charlist.json"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report(
            "starter-words-hk-chars",
            "words.hk 字表",
            result,
            &mut report,
        );
        report.datasets.push(dataset);

        let pairs = words_hk::parse_english_index(YUE_ENGLISH_INDEX, &items)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let _attached = store
            .attach_search_terms(&pairs)
            .map_err(ApplicationError::from)?;
        let source = sources::words_hk_english_index();
        store
            .upsert_source(&source)
            .map_err(ApplicationError::from)?;
        store
            .insert_manifest(&devtoolbox_core::language::DatasetManifest {
                id: "starter-words-hk-english".to_string(),
                name: "Cantonese Core — words.hk English index (starter)".to_string(),
                language: "yue".to_string(),
                version: source.dataset_version.clone(),
                downloaded_at: None,
                source_id: source.id.clone(),
                checksum: None,
                raw_file: Some(
                    "tests/fixtures/language/yue/words_hk_english_index.json".to_string(),
                ),
                record_count: pairs.len() as i64,
                importer_version: 1,
                imported_at: now,
            })
            .map_err(ApplicationError::from)?;
        report.total_inserted += pairs.len() as i64;
        report.datasets.push(DatasetReport {
            id: "starter-words-hk-english".to_string(),
            name: "words.hk 英粵對照".to_string(),
            inserted: pairs.len() as i64,
            updated: 0,
        });

        let source = sources::cc_canto();
        let items = cc_canto::parse(YUE_CCCANTO)
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        let result = import_into(
            store,
            &source,
            "starter-cc-canto",
            "Cantonese Core — CC-Canto (starter)",
            &items,
            Some("tests/fixtures/language/yue/cccanto.txt"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-cc-canto", "CC-Canto", result, &mut report);
        report.datasets.push(dataset);
    }

    // ---- Sentences（Tatoeba；CC0 子集 + CC BY 2.0 FR 分语言样例）----
    {
        let source = sources::tatoeba();
        let mut all_items = Vec::new();
        let cc0 = tatoeba::parse(SENT_CC0, "CC0 1.0")
            .map_err(|error| ApplicationError::License(error.to_string()))?;
        all_items.extend(cc0);
        let ccby = [
            ("jpn", SENT_JPN),
            ("cmn", SENT_CMN),
            ("yue", SENT_YUE),
            ("eng", SENT_ENG),
        ]
        .into_iter()
        .filter(|(lang, _)| only.as_deref().is_none_or(|filter| filter == *lang))
        .map(|(_, content)| {
            tatoeba::parse(content, "CC BY 2.0 FR")
                .map_err(|error| ApplicationError::License(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        all_items.extend(ccby);
        let result = import_into(
            store,
            &source,
            "starter-tatoeba",
            "Sentences — Tatoeba (starter)",
            &all_items,
            Some("tests/fixtures/language/sentences/*"),
            now,
        )
        .map_err(ApplicationError::from)?;
        let dataset = dataset_report("starter-tatoeba", "Tatoeba", result, &mut report);
        report.datasets.push(dataset);
    }

    Ok(report)
}
