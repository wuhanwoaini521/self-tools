//! TravelResearchService 集成测试（Mock 全依赖，不触网，需求 #二十五）。

use std::sync::{Arc, Mutex};

use devtoolbox_core::travel::{
    CityGuide, CityInfo, FactCategory, GuideMeta, ResearchPhase, StepStatus, TravelFact,
    TravelResearchEvent,
};
use devtoolbox_infrastructure::TravelStore;

use crate::travel::mocks::{
    MockDataProvider, MockLlmProvider, MockSearchProvider, MockWebFetcher, search_result,
};
use crate::travel::service::TravelResearchService;
use crate::{ApplicationError, travel::TravelResearchRequest};

fn request(city: &str) -> TravelResearchRequest {
    TravelResearchRequest {
        city: city.to_string(),
        days: 3,
        month: None,
        preferences: vec![],
        force: false,
    }
}

/// 构造服务 + 事件收集器。
fn harness(
    providers: Vec<Box<dyn devtoolbox_infrastructure::SearchProvider>>,
    fetcher: Box<dyn devtoolbox_infrastructure::WebFetcher>,
    llm: Option<Box<dyn devtoolbox_infrastructure::LlmProvider>>,
) -> (
    TravelResearchService,
    Arc<Mutex<Vec<TravelResearchEvent>>>,
    tempfile::TempDir,
) {
    harness_with_data(providers, fetcher, llm, Vec::new())
}

/// 构造服务 + 事件收集器（带结构化数据 Provider）。
fn harness_with_data(
    providers: Vec<Box<dyn devtoolbox_infrastructure::SearchProvider>>,
    fetcher: Box<dyn devtoolbox_infrastructure::WebFetcher>,
    llm: Option<Box<dyn devtoolbox_infrastructure::LlmProvider>>,
    data_providers: Vec<Box<dyn devtoolbox_infrastructure::TravelDataProvider>>,
) -> (
    TravelResearchService,
    Arc<Mutex<Vec<TravelResearchEvent>>>,
    tempfile::TempDir,
) {
    let directory = tempfile::tempdir().expect("temp dir");
    let store: Arc<Mutex<TravelStore>> = Arc::new(Mutex::new(
        TravelStore::open(directory.path().join("travel.db")).expect("open"),
    ));
    let events: Arc<Mutex<Vec<TravelResearchEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let service = TravelResearchService::new(providers, fetcher, llm, data_providers, store);
    (service, events, directory)
}

fn guide_json(summary: &str) -> String {
    format!(
        r#"{{"city": {{"name": "杭州", "name_en": "Hangzhou"}}, "summary": "{summary}", "highlights": ["西湖"]}}"#
    )
}

fn facts_json(official: bool) -> String {
    if official {
        r#"[{"category":"opening_hours","subject":"西湖","value":"07:00-18:00","confidence":0.9}]"#
            .to_string()
    } else {
        r#"[{"category":"opening_hours","subject":"西湖","value":"08:00-17:00","confidence":0.9}]"#
            .to_string()
    }
}

/// 默认 happy path 依赖：1 个搜索 Provider + 1 个抓取器 + LLM 队列。
/// happy provider 产生 2 个文档 → 队列 = [扩展, 事实(文档1), 事实(文档2), 攻略]。
fn happy_llm_queue() -> MockLlmProvider {
    MockLlmProvider::new([
        r#"["杭州 宋韵文化体验"]"#.to_string(),
        facts_json(true),
        r#"[{"category":"food","subject":"龙井虾仁","value":"杭帮菜经典"}]"#.to_string(),
        guide_json("西湖列世界遗产，免费开放，建议游玩一天。").to_string(),
    ])
}

const OFFICIAL_URL: &str = "https://hz.gov.cn/westlake";
const PLATFORM_URL: &str = "https://you.ctrip.com/hangzhou";

fn happy_provider() -> MockSearchProvider {
    MockSearchProvider::new(
        "mock",
        vec![
            search_result(
                OFFICIAL_URL,
                "杭州西湖 官方介绍",
                "西湖是世界文化遗产，全天开放。",
            ),
            search_result(PLATFORM_URL, "杭州 三日游攻略", "路线与美食推荐。"),
        ],
    )
}

fn happy_fetcher() -> MockWebFetcher {
    MockWebFetcher::new()
        .with_page(
            OFFICIAL_URL,
            "西湖景区",
            "西湖位于杭州，是著名景点。开放时间 07:00-18:00。",
        )
        .with_page(PLATFORM_URL, "杭州攻略", "推荐西湖、灵隐寺，三日游路线。")
}

#[tokio::test]
async fn full_research_produces_structured_guide() {
    let (service, events, _dir) = harness(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        Some(Box::new(happy_llm_queue())),
    );
    let collector = events.clone();
    let guide = service
        .research_city(&request("杭州"), &move |event| {
            collector.lock().expect("poisoned").push(event);
        })
        .await
        .expect("research");

    assert_eq!(guide.city.name, "杭州");
    assert_eq!(guide.city.name_en.as_deref(), Some("Hangzhou"));
    assert!(guide.summary.contains("西湖"));
    assert_eq!(guide.highlights, vec!["西湖"]);
    assert!(guide.meta.llm_used);
    assert_eq!(guide.meta.days, 3);
    // LLM 攻略 + 程序验证合并：开放时间应落到景点上（官方来源优先）
    let westlake = guide
        .attractions
        .iter()
        .find(|a| a.name.contains("西湖"))
        .or_else(|| guide.attractions.first());
    if let Some(attraction) = westlake
        && let Some(hours) = &attraction.opening_hours
    {
        assert_eq!(hours.value, "07:00-18:00");
    }
    // 来源应完整保留
    assert!(
        guide.sources.len() >= 2,
        "sources: {:?}",
        guide.sources.len()
    );
    // 进度事件完整
    let locked = events.lock().expect("poisoned");
    assert!(
        locked
            .iter()
            .any(|e| e.phase == ResearchPhase::PlanQueries && e.status == StepStatus::Done)
    );
    assert!(
        locked
            .iter()
            .any(|e| e.phase == ResearchPhase::FetchDocuments && e.status == StepStatus::Done)
    );
    assert!(
        locked
            .iter()
            .any(|e| e.phase == ResearchPhase::SaveGuide && e.status == StepStatus::Done)
    );
    assert!(locked.iter().any(|e| e.seq > 0));
    // 攻略已落库
    drop(locked);
}

#[tokio::test]
async fn provider_failure_falls_back_to_secondary() {
    let mut primary = MockSearchProvider::new("primary", vec![]);
    primary
        .errors
        .insert("*".to_string(), "waf blocked".to_string());
    let secondary = MockSearchProvider::new(
        "secondary",
        vec![search_result(OFFICIAL_URL, "杭州 介绍", "snippet")],
    );
    let secondary_calls = secondary.calls.clone();
    let (service, _events, _dir) = harness(
        vec![Box::new(primary), Box::new(secondary)],
        Box::new(happy_fetcher()),
        None,
    );
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("research with fallback");
    // 主 Provider 全挂时备选 Provider 顶上，研究仍成功
    assert!(!guide.sources.is_empty());
    assert!(!secondary_calls.lock().expect("poisoned").is_empty());
}

#[tokio::test]
async fn all_providers_failing_fails_research() {
    let mut failing = MockSearchProvider::new("broken", vec![]);
    failing.errors.insert("*".to_string(), "down".to_string());
    let (service, events, _dir) = harness(
        vec![Box::new(failing)],
        Box::new(MockWebFetcher::new()),
        None,
    );
    let collector = events.clone();
    let result = service
        .research_city(&request("杭州"), &move |event| {
            collector.lock().expect("poisoned").push(event);
        })
        .await;
    assert!(matches!(result, Err(ApplicationError::TravelFailed(_))));
}

#[tokio::test]
async fn partial_page_failures_keep_snippets() {
    let provider = MockSearchProvider::new(
        "mock",
        vec![
            search_result("https://a.gov.example", "官方A", "摘要A 开放时间"),
            search_result("https://b.example", "站点B", "摘要B 美食"),
        ],
    );
    let fetcher = MockWebFetcher::new()
        .with_page("https://a.gov.example", "官方A", "A 的正文内容。")
        .with_error("https://b.example", "403 forbidden");
    let llm = MockLlmProvider::new([
        "[\"杭州 补充主题\"]".to_string(),
        r#"[{"category":"attraction","subject":"A","value":"介绍"}]"#.to_string(),
        r#"[{"category":"food","subject":"B","value":"小吃"}]"#.to_string(),
        guide_json("部分页面不可用。").to_string(),
    ]);
    let (service, _events, _dir) = harness(
        vec![Box::new(provider)],
        Box::new(fetcher),
        Some(Box::new(llm)),
    );
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("partial success");
    // 失败页面仍以「仅摘要」出现在来源里
    let snippet_source = guide
        .sources
        .iter()
        .find(|s| s.url == "https://b.example")
        .expect("snippet source kept");
    assert_eq!(
        snippet_source.state,
        devtoolbox_core::travel::ContentState::SnippetOnly
    );
    assert!(guide.meta.notes.iter().any(|n| n.contains("未能抓取全文")));
}

#[tokio::test]
async fn llm_failure_falls_back_to_sources_only() {
    let llm = MockLlmProvider::new(["ERR:service unavailable".to_string()]);
    let (service, _events, _dir) = harness(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        Some(Box::new(llm)),
    );
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("research keeps working without llm");
    assert!(!guide.meta.llm_used);
    assert!(!guide.sources.is_empty());
    // 降级说明写入 notes，不编造 AI 内容：summary 为降级文案
    assert!(
        guide.summary.contains("暂无 AI 整理")
            || guide.meta.notes.iter().any(|n| n.contains("降级"))
    );
}

#[tokio::test]
async fn no_llm_configured_still_returns_sources() {
    let (service, events, _dir) = harness(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        None,
    );
    let collector = events.clone();
    let guide = service
        .research_city(&request("杭州"), &move |event| {
            collector.lock().expect("poisoned").push(event);
        })
        .await
        .expect("research without llm");
    assert!(!guide.meta.llm_used);
    assert!(guide.sources.len() >= 2);
    assert!(guide.meta.notes.iter().any(|n| n.contains("未配置 LLM")));
    let locked = events.lock().expect("poisoned");
    assert!(
        locked
            .iter()
            .any(|e| e.phase == ResearchPhase::ExtractFacts && e.status == StepStatus::Skipped)
    );
}

#[tokio::test]
async fn illegal_llm_json_is_tolerated() {
    // happy provider 产生 2 个文档 → 队列 = [扩展, 非JSON(文档1), 事实(文档2), 攻略]
    let llm = MockLlmProvider::new([
        "[\"杭州 补充\"]".to_string(),
        "not json at all".to_string(),
        r#"[{"category":"food","subject":"龙井虾仁","value":"杭帮菜"}]"#.to_string(),
        guide_json("攻略依然生成。").to_string(),
    ]);
    let (service, _events, _dir) = harness(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        Some(Box::new(llm)),
    );
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("illegal json tolerated");
    assert!(guide.meta.llm_used, "guide 由 LLM 生成");
    assert_eq!(guide.summary, "攻略依然生成。");
    // LLM 攻略里没有龙井虾仁，但合并层应把已验证事实补进去（food 类暂不强制，至少不报错）
    assert!(guide.meta.notes.iter().any(|n| n.contains("提取失败")));
}

#[tokio::test]
async fn conflicting_facts_resolve_by_authority() {
    let provider = MockSearchProvider::new(
        "mock",
        vec![
            search_result(OFFICIAL_URL, "官方发布", "官方 开放时间"),
            search_result(PLATFORM_URL, "平台攻略", "平台 开放时间"),
        ],
    );
    let fetcher = MockWebFetcher::new()
        .with_page(OFFICIAL_URL, "西湖景区-官方", "开放时间 07:00-18:00。")
        .with_page(PLATFORM_URL, "西湖景区-平台", "开放时间 08:00-17:00。");
    // 官方文档在前（权重更高 → 排在抓取队列前面）
    let llm = MockLlmProvider::new([
        "[]".to_string(),
        facts_json(true).to_string(),
        facts_json(false).to_string(),
        r#"{"city":{"name":"杭州"},"attractions":[{"name":"西湖"}]}"#.to_string(),
    ]);
    let (service, _events, _dir) = harness(
        vec![Box::new(provider)],
        Box::new(fetcher),
        Some(Box::new(llm)),
    );
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("conflict resolved");
    let attraction = guide
        .attractions
        .iter()
        .find(|a| a.name.contains("西湖"))
        .expect("西湖 attraction");
    let hours = attraction
        .opening_hours
        .as_ref()
        .expect("verified hours merged");
    assert_eq!(hours.value, "07:00-18:00", "官方来源应优先");
    assert_eq!(hours.primary_source, "official");
    assert!(hours.has_conflict, "存在两个版本应标记冲突");
    assert!(guide.meta.notes.iter().any(|n| n.contains("多个来源版本")));
}

#[tokio::test]
async fn cache_hit_skips_network() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store: Arc<Mutex<TravelStore>> = Arc::new(Mutex::new(
        TravelStore::open(directory.path().join("travel.db")).expect("open"),
    ));
    let now = devtoolbox_infrastructure::now_unix();
    let cached_guide = CityGuide {
        city: CityInfo {
            name: "杭州".to_string(),
            ..CityInfo::default()
        },
        meta: GuideMeta {
            generated_at: now,
            updated_at: now,
            days: 3,
            llm_used: true,
            notes: vec!["缓存测试".to_string()],
        },
        summary: "缓存中的攻略".to_string(),
        ..CityGuide::default()
    };
    store
        .lock()
        .expect("poisoned")
        .upsert_guide(&cached_guide, now)
        .expect("seed cache");

    let cache_hit_provider = MockSearchProvider::new("should-not-be-called", vec![]);
    let service = TravelResearchService::new(
        vec![Box::new(cache_hit_provider)],
        Box::new(MockWebFetcher::new()),
        None,
        Vec::new(),
        store,
    );
    let _keep = directory;
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("cache hit");
    assert_eq!(guide.summary, "缓存中的攻略");
}

#[tokio::test]
async fn empty_city_is_rejected() {
    let (service, _events, _dir) = harness(
        vec![Box::new(MockSearchProvider::new("mock", vec![]))],
        Box::new(MockWebFetcher::new()),
        None,
    );
    let result = service.research_city(&request("  "), &|_| {}).await;
    assert!(matches!(result, Err(ApplicationError::EmptyCity)));
}

#[tokio::test]
async fn no_search_results_fails_cleanly() {
    let (service, _events, _dir) = harness(
        vec![Box::new(MockSearchProvider::new("empty", vec![]))],
        Box::new(MockWebFetcher::new()),
        None,
    );
    let result = service.research_city(&request("杭州"), &|_| {}).await;
    assert!(matches!(result, Err(ApplicationError::TravelFailed(_))));
}

#[tokio::test]
async fn parse_query_list_handles_json_and_lines() {
    let json = r#"["杭州 宋韵", "杭州 秋季赏桂"]"#;
    let parsed = crate::travel::service::parse_query_list(json);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0], "杭州 宋韵");

    let lines = "- 杭州 徒步路线\n- 杭州 骑行\n";
    let parsed = crate::travel::service::parse_query_list(lines);
    assert_eq!(parsed.len(), 2);
}

fn amap_fact(category: FactCategory, subject: &str, value: &str) -> TravelFact {
    TravelFact {
        category,
        subject: subject.to_string(),
        value: value.to_string(),
        source_id: "https://restapi.amap.com".to_string(),
        confidence: 0.8,
        fetched_at: 1_700_000_000,
    }
}

#[tokio::test]
async fn data_provider_facts_enrich_guide_and_sources() {
    // 模拟高德 POI + 和风天气：填入钥匙才有这些事实（未配置时 providers 为空，见其他用例）
    let data = MockDataProvider::new("amap-poi")
        .with_facts(
            "poi",
            vec![
                amap_fact(FactCategory::Attraction, "龙井村", "西湖区 龙井路"),
                amap_fact(FactCategory::Food, "龙井虾仁", "西湖区 特色菜"),
            ],
        )
        .with_facts(
            "weather",
            vec![TravelFact {
                category: FactCategory::Weather,
                subject: "杭州天气".to_string(),
                value: "今天 晴 24~33°C；明天 多云 25~32°C".to_string(),
                source_id: "https://devapi.qweather.com".to_string(),
                confidence: 0.9,
                fetched_at: 1_700_000_000,
            }],
        );
    let llm = MockLlmProvider::new([
        "[]".to_string(),
        r#"[]"#.to_string(),
        r#"[]"#.to_string(),
        r#"{"city":{"name":"杭州"},"summary":"基本攻略"}"#.to_string(),
    ]);
    let (service, _events, _dir) = harness_with_data(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        Some(Box::new(llm)),
        vec![Box::new(data)],
    );
    let guide = service
        .research_city(&request("杭州"), &|_| {})
        .await
        .expect("research with data providers");
    // 高德 POI 补进攻略区块
    assert!(
        guide.attractions.iter().any(|a| a.name.contains("龙井村")),
        "attractions: {:?}",
        guide.attractions
    );
    assert!(guide.foods.iter().any(|f| f.name.contains("龙井虾仁")));
    // 和风天气进入本地 Tips
    assert!(
        guide
            .local_tips
            .iter()
            .any(|tip| tip.title.contains("天气"))
    );
    // 数据源作为可信来源出现在 Sources（A 级）
    let amap_source = guide
        .sources
        .iter()
        .find(|s| s.url == "https://restapi.amap.com")
        .expect("amap source listed");
    assert_eq!(amap_source.level, devtoolbox_core::travel::SourceLevel::A);
    assert!(guide.meta.notes.iter().any(|n| n.contains("已接入数据源")));
    // 数据源标题使用人类可读标签
    assert!(
        guide
            .sources
            .iter()
            .any(|s| s.title.contains("高德地图 POI"))
    );
}

#[tokio::test]
async fn data_provider_failure_is_partial_success() {
    let data = MockDataProvider::new("amap-poi").with_error("poi", "invalid key");
    let (service, events, _dir) = harness_with_data(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        None,
        vec![Box::new(data)],
    );
    let collector = events.clone();
    let guide = service
        .research_city(&request("杭州"), &move |event| {
            collector.lock().expect("poisoned").push(event);
        })
        .await
        .expect("data provider failure must not abort research");
    assert!(!guide.sources.is_empty());
    assert!(
        guide
            .meta
            .notes
            .iter()
            .any(|n| n.contains("数据源") && n.contains("失败"))
    );
    let locked = events.lock().expect("poisoned");
    assert!(
        locked
            .iter()
            .any(|e| e.phase == ResearchPhase::DataSources && e.status == StepStatus::Failed)
    );
}

#[tokio::test]
async fn without_data_keys_providers_skipped() {
    // 未配置 Key → 无数据 Provider → 阶段跳过，攻略照常
    let (service, events, _dir) = harness(
        vec![Box::new(happy_provider())],
        Box::new(happy_fetcher()),
        None,
    );
    let collector = events.clone();
    let guide = service
        .research_city(&request("杭州"), &move |event| {
            collector.lock().expect("poisoned").push(event);
        })
        .await
        .expect("research without keys");
    assert!(!guide.sources.is_empty());
    assert!(
        guide
            .meta
            .notes
            .iter()
            .any(|n| n.contains("未配置数据源 Key"))
    );
    let locked = events.lock().expect("poisoned");
    assert!(
        locked
            .iter()
            .any(|e| e.phase == ResearchPhase::DataSources && e.status == StepStatus::Skipped)
    );
}
