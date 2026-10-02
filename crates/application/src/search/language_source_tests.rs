//! Language 检索源接入全局搜索的用例测试。
//!
//! 覆盖此前缺失的能力：`SearchSource::Language` 早已在 core 定义，却没有任何端口
//! 注册它，全局搜索搜不到语言内容（只能在 Language 页面用模块私有的
//! `language_search` 搜——第二套搜索入口）。

use std::sync::Arc;

use devtoolbox_core::language::LanguageCode;
use devtoolbox_core::search::{GlobalSearchQuery, SearchSource};

use crate::language::LanguageService;
use crate::language::mocks::FakeLanguageStore;
use crate::search::{GlobalSearchPort, GlobalSearchService, LanguageSearchPort};

fn port_with_content() -> LanguageSearchPort {
    let store = Arc::new(FakeLanguageStore::new());
    store.insert_word("jmdict:1002990", LanguageCode::Jap, "食べる", "to eat");
    store.insert_sentence("tatoeba:1", LanguageCode::Jap, "ご飯を食べる。");
    LanguageSearchPort::new(Arc::new(LanguageService::new(store)))
}

#[test]
fn language_port_declares_the_language_source() {
    assert_eq!(port_with_content().source(), SearchSource::Language);
}

#[test]
fn blank_query_yields_no_hits_instead_of_everything() {
    let hits = port_with_content()
        .search(&GlobalSearchQuery::new("   "))
        .expect("search");
    assert!(hits.is_empty(), "空白查询应返回空结果，而不是全量或错误");
}

#[test]
fn matching_word_becomes_a_navigable_global_hit() {
    let hits = port_with_content()
        .search(&GlobalSearchQuery::new("食べる"))
        .expect("search");
    // 「ご飯を食べる。」这句也包含该子串，因此词条 + 句子共两条命中。
    assert_eq!(hits.len(), 2, "词条与包含该子串的句子都应命中");

    let word = hits
        .iter()
        .find(|hit| hit.action_target["itemId"] == "jmdict:1002990")
        .expect("词条命中");
    assert_eq!(word.source, SearchSource::Language);
    assert_eq!(word.title, "食べる");
    // action_target 决定前端能否跳转，必须带 module 与条目 id
    assert_eq!(word.action_target["module"], "language");
    assert!(
        word.snippet.contains("食べる"),
        "片段应包含命中文本，实际 {}",
        word.snippet
    );
}

#[test]
fn unmatched_query_returns_empty_rather_than_error() {
    let hits = port_with_content()
        .search(&GlobalSearchQuery::new("zzzz-not-present"))
        .expect("search");
    assert!(hits.is_empty());
}

#[test]
fn language_hits_are_aggregated_alongside_other_sources() {
    let search = GlobalSearchService::new(vec![Arc::new(port_with_content())]);
    let result = search.search(&GlobalSearchQuery::new("食べる"));

    assert_eq!(result.total, 2, "语言命中应进入全局搜索结果");
    assert_eq!(result.hits[0].source, SearchSource::Language);
    assert!(
        result.degraded_sources.is_empty(),
        "语言源未失败，不应进降级列表"
    );
}

#[test]
fn search_respects_the_per_source_limit() {
    let search = GlobalSearchService::new(vec![Arc::new(port_with_content())]);
    let mut query = GlobalSearchQuery::new("食べる");
    query.limit_per_source = 0; // 0 表示「用默认值」，而非「不要结果」
    let result = search.search(&query);
    assert_eq!(result.total, 2, "limit=0 应回落到默认上限，而不是返回空");
}
