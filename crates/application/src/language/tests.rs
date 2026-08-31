//! Language 模块集成测试：Starter Pack 全流程 + 验收搜索场景。
//!
//! 完全离线：fixtures 内置于 `tests/fixtures/language`，不访问任何第三方（#78）。

use std::sync::{Arc, Mutex};

use tempfile::tempdir;

use crate::language::{LanguageService, starter};
use devtoolbox_infrastructure::language::LanguageStore;

fn service_with_starter() -> (tempfile::TempDir, LanguageService) {
    let directory = tempdir().expect("tempdir");
    let mut store = LanguageStore::open(directory.path().join("language.db")).expect("open");
    starter::install_starter(&mut store, None).expect("starter");
    let service = LanguageService::new(Arc::new(Mutex::new(store)));
    (directory, service)
}

#[test]
fn starter_pack_imports_all_core_packs() {
    let (_dir, service) = service_with_starter();
    let languages = service.languages().expect("languages");
    let by_code: std::collections::HashMap<_, _> = languages
        .iter()
        .map(|info| (info.code.as_str(), info))
        .collect();
    // English + Japanese 完整；Mandarin + Cantonese 至少可导入搜索（#41/#80）
    assert!(by_code["eng"].total > 0, "english pack");
    assert!(by_code["jpn"].total > 0, "japanese pack");
    assert!(by_code["cmn"].total > 0, "mandarin pack");
    assert!(by_code["yue"].total > 0, "cantonese pack");
    let sources = service.sources().expect("sources");
    assert!(sources.len() >= 8, "sources registered: {sources:?}");
}

/// 验收场景（#88）：搜索必须支持 text/reading/romanization/meaning。
#[test]
fn acceptance_searches() {
    let (_dir, service) = service_with_starter();

    // Japanese：食べる（text）、たべる（reading）、taberu（romanization）
    let hits = service.search(Some("jpn"), "食べる", 5).expect("search");
    assert!(
        hits.iter()
            .any(|hit| hit.item.id.starts_with("jmdict:") && hit.item.text == "食べる"),
        "食べる from JMdict: {hits:?}"
    );
    let hits = service.search(Some("jpn"), "たべる", 5).expect("search");
    assert!(
        hits.iter().any(|hit| hit.item.text == "食べる"),
        "たべる → 食べる: {hits:?}"
    );
    let hits = service.search(Some("jpn"), "taberu", 5).expect("search");
    assert!(
        hits.iter().any(|hit| hit.item.text == "食べる"),
        "taberu → 食べる: {hits:?}"
    );

    // English：reservation 定义来自 OEWN、发音来自 CMUdict（ARPABET）
    let hits = service
        .search(Some("eng"), "reservation", 5)
        .expect("search");
    let reservation = hits
        .iter()
        .find(|hit| hit.item.id == "wn:reservation")
        .expect("reservation in OEWN");
    assert_eq!(reservation.matched, "exact");
    let detail = service
        .detail("wn:reservation")
        .expect("detail")
        .expect("found");
    assert!(!detail.meanings.is_empty(), "meanings from OEWN");
    assert!(detail.meanings[0].gloss.is_some());
    assert!(
        detail
            .pronunciations
            .iter()
            .any(|pron| pron.scheme == devtoolbox_core::language::PronunciationScheme::Arpabet),
        "CMUdict ARPABET pronunciation attached: {:?}",
        detail.pronunciations
    );

    // Mandarin：旅行 → CC-CEDICT（simplified/traditional/pinyin/meaning）
    let hits = service.search(Some("cmn"), "旅行", 5).expect("search");
    let lvxing = hits
        .iter()
        .find(|hit| hit.item.text == "旅行")
        .expect("旅行");
    let detail = service
        .detail(&lvxing.item.id)
        .expect("detail")
        .expect("found");
    let meta = detail.item.meta.clone().expect("mandarin meta");
    let devtoolbox_core::language::LanguageMetadata::Mandarin(mandarin) = meta else {
        panic!("mandarin metadata expected")
    };
    assert_eq!(mandarin.simplified.as_deref(), Some("旅行"));
    assert_eq!(mandarin.traditional.as_deref(), Some("旅行"));
    assert!(
        mandarin
            .pinyin
            .as_deref()
            .unwrap_or_default()
            .starts_with("lu:3")
    );
    assert!(mandarin.hsk.is_none(), "HSK 无明确来源，V1 不填");

    // Cantonese：食飯 → words.hk（public domain）jyutping；CC-Canto 为增补
    let hits = service.search(Some("yue"), "食飯", 5).expect("search");
    let sik = hits
        .iter()
        .find(|hit| hit.item.id.starts_with("whk:"))
        .expect("食飯");
    let detail = service
        .detail(&sik.item.id)
        .expect("detail")
        .expect("found");
    assert!(
        detail
            .pronunciations
            .iter()
            .any(|pron| pron.phonemes.contains("sik6 faan6")),
        "jyutping from words.hk: {:?}",
        detail.pronunciations
    );
    let hits = service
        .search(Some("yue"), "sik6 faan6", 5)
        .expect("search");
    assert!(
        hits.iter().any(|hit| hit.item.id == sik.item.id),
        "sik6 faan6 → 食飯: {hits:?}"
    );
    // 英文→粤语（words.hk English Index）
    let hits = service.search(Some("yue"), "food", 5).expect("search");
    assert!(!hits.is_empty(), "food → Cantonese words");

    // 句子里搜索（Tatoeba 例句）
    let hits = service.search(Some("jpn"), "食べる", 10).expect("search");
    assert!(
        hits.iter()
            .any(|hit| hit.item.item_type == devtoolbox_core::language::LanguageItemType::Sentence),
        "Tatoeba sentences include 食べる"
    );
}

#[test]
fn learning_flow_offline() {
    let (_dir, service) = service_with_starter();
    let now_ish = devtoolbox_infrastructure::now_unix();
    let card = service
        .review_next("eng")
        .expect("review")
        .expect("card available");
    let outcome = service
        .rate(&card.item.id, devtoolbox_core::language::ReviewRating::Good)
        .expect("rate");
    assert!(outcome.interval_days >= 1.0);
    assert!(card.state == devtoolbox_core::language::LearningStateKind::New);
    assert!(service.toggle_favorite(&card.item.id).expect("favorite"));
    let progress = service.progress().expect("progress");
    assert!(progress.total >= 1);
    assert!(progress.favorites >= 1);
    let _ = now_ish;
}

#[test]
fn today_plan_available_offline() {
    let (_dir, service) = service_with_starter();
    let today = service.today("jpn").expect("today");
    assert!(today.languages.iter().any(|info| info.code == "jpn"));
    assert!(today.plan.new_words > 0 || today.plan.sentences > 0);
}

#[test]
fn speaker_scoring_offline() {
    let (_dir, service) = service_with_starter();
    let score = service.speaking_feedback(
        "I would like to make a reservation",
        "I would like to make a reservation",
        4_000,
        4_000,
        &[],
    );
    assert_eq!(score.accuracy, 100);
    assert_eq!(score.completeness, 100);
}
