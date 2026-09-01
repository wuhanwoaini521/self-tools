//! `Tauri` command adapter。业务规则位于 workspace 的 application/core crates。
//!
//! 全局 `AppState` 持有各模块的 SQLite 存储器、Travel 的缓存与共享 HTTP 客户端；
//! 每个功能模块(文档 / RSS / Travel)的命令各自独立，互不依赖。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use devtoolbox_application::language::{
    LanguageInfo, LanguageSearchHit, LanguageService, ManifestInfo, ProgressView, ReviewCard,
    SourceInfo, TodayView, WordDetail, starter,
};
use devtoolbox_application::{
    ApplicationError, ArticleDto, DocumentDto, FeedDto, GeoCompareView, GeoEntityDetail,
    GeoSearchGroup, GeographyHome, HistoryHome, HistoryService, RefreshReport,
    TravelResearchRequest, TravelResearchService, commit_new_feed, commit_refresh,
    convert_lines_to_tasks, cycle_lines, delete_feed, feed_snapshots, fetch_all_feeds,
    fetch_new_feed, latest_articles, list_articles, list_feeds, load_document, load_settings,
    mark_article_read, save_document, save_settings, scan_workspace, validate_feed_url,
};
use devtoolbox_core::{
    geography::GeoEntityType as CoreGeoEntityType,
    history::{HistoryDetailView, HistorySearchGroup},
    language::{LearningStateKind, ReviewRating, SpeakingScore},
    travel::{CityGuide, GuideSummary, TravelResearchEvent},
};
use devtoolbox_infrastructure::{
    AmapPoiProvider, AppSettings, FeedRepository, GeographyStore, HistoryStore, HttpWebFetcher,
    LanguageStore, LlmConfig, LlmProvider, OpenAiCompatibleLlmProvider, QWeatherProvider,
    SettingsStore, TravelDataProvider, TravelDataRequest, TravelStore, WorkspaceFile,
    build_providers, feed_client, providers_for,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        let code = match &error {
            ApplicationError::EmptyDocumentPath => "empty_document_path",
            ApplicationError::EmptyWorkspacePath => "empty_workspace_path",
            ApplicationError::InvalidFeedUrl(_) => "rss_invalid_url",
            ApplicationError::DuplicateFeed(_) => "rss_duplicate_feed",
            ApplicationError::FeedNotFound(_) => "rss_feed_not_found",
            ApplicationError::EmptyCity => "travel_empty_city",
            ApplicationError::TravelFailed(_) => "travel_failed",
            ApplicationError::Language { .. } => "language_error",
            ApplicationError::License(_) => "language_license",
            ApplicationError::History { .. } | ApplicationError::HistoryData(_) => "history_error",
            ApplicationError::Geography { .. } | ApplicationError::GeographyData(_) => {
                "geography_error"
            }
            ApplicationError::GeographyCompare(_) => "geography_compare_error",
            ApplicationError::Travel { source } => match source {
                devtoolbox_infrastructure::InfrastructureError::TravelSearch(_) => {
                    "travel_search_failed"
                }
                devtoolbox_infrastructure::InfrastructureError::TravelFetch(_) => {
                    "travel_fetch_failed"
                }
                devtoolbox_infrastructure::InfrastructureError::TravelLlm(_) => "travel_llm_failed",
                devtoolbox_infrastructure::InfrastructureError::TravelData(_) => {
                    "travel_data_failed"
                }
                _ => "travel_error",
            },
            ApplicationError::Rss { source } => match source {
                devtoolbox_infrastructure::InfrastructureError::FeedFetch(_) => "rss_fetch_failed",
                devtoolbox_infrastructure::InfrastructureError::FeedParse(_) => "rss_parse_failed",
                _ => "infrastructure_error",
            },
            ApplicationError::Infrastructure { .. } => "infrastructure_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

/// 应用级共享状态：RSS 存储 + Travel 缓存 + 研究会话 + HTTP 客户端。
pub struct AppState {
    pub store: Mutex<FeedRepository>,
    pub travel_store: Arc<Mutex<TravelStore>>,
    pub travel_sessions: Arc<Mutex<HashMap<String, Arc<Mutex<TravelSession>>>>>,
    pub history_store: Arc<Mutex<HistoryStore>>,
    pub language_store: Arc<Mutex<LanguageStore>>,
    pub geography_store: Arc<Mutex<GeographyStore>>,
    pub client: reqwest::Client,
}

/// 一次旅行研究的后台会话（前端按 session 轮询进度）。
pub struct TravelSession {
    pub events: Vec<TravelResearchEvent>,
    pub done: bool,
    pub error: Option<String>,
    pub guide: Option<CityGuide>,
    pub from_cache: bool,
}

/// 轮询快照（Serialize 给前端）。
#[derive(Debug, Serialize)]
pub struct TravelResearchSnapshot {
    pub session_id: String,
    pub done: bool,
    pub error: Option<String>,
    pub from_cache: bool,
    pub events: Vec<TravelResearchEvent>,
    pub guide: Option<CityGuide>,
}

fn settings_store(app: &AppHandle) -> Result<SettingsStore, CommandError> {
    let config_directory = app.path().app_config_dir().map_err(|error| CommandError {
        code: "app_config_dir",
        message: error.to_string(),
    })?;
    Ok(SettingsStore::new(config_directory))
}

// ---------- 文档 / Markdown 模块 ----------

#[tauri::command]
fn read_document(path: String) -> Result<DocumentDto, CommandError> {
    load_document(&path).map_err(CommandError::from)
}

#[tauri::command]
fn write_document(path: String, text: String) -> Result<(), CommandError> {
    save_document(&path, &text).map_err(CommandError::from)
}

#[tauri::command]
fn list_workspace(path: String) -> Result<Vec<WorkspaceFile>, CommandError> {
    scan_workspace(&path).map_err(CommandError::from)
}

#[tauri::command]
fn convert_task_lines(lines: Vec<String>) -> Vec<String> {
    convert_lines_to_tasks(&lines)
}

#[tauri::command]
fn cycle_task_lines(lines: Vec<String>, step: isize) -> Vec<String> {
    cycle_lines(&lines, step)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettings, CommandError> {
    let store = settings_store(&app)?;
    load_settings(&store).map_err(CommandError::from)
}

#[tauri::command]
fn put_settings(app: AppHandle, settings: AppSettings) -> Result<(), CommandError> {
    let store = settings_store(&app)?;
    save_settings(&store, &settings).map_err(CommandError::from)
}

// ---------- RSS 模块 ----------

#[tauri::command]
async fn add_rss_feed(state: State<'_, AppState>, url: String) -> Result<FeedDto, CommandError> {
    // 两段式：先无锁抓取(可跨 await),再短锁落库。
    let normalized = validate_feed_url(&url).map_err(CommandError::from)?;
    let fetched = fetch_new_feed(&normalized, &state.client)
        .await
        .map_err(CommandError::from)?;
    let store = state.store.lock().expect("rss store poisoned");
    commit_new_feed(&store, &normalized, fetched).map_err(CommandError::from)
}

#[tauri::command]
async fn refresh_rss_feeds(state: State<'_, AppState>) -> Result<RefreshReport, CommandError> {
    // 快照 → 并发抓取(无锁) → 短锁落库;单个 Feed 失败不影响其他。
    let snapshots = {
        let store = state.store.lock().expect("rss store poisoned");
        feed_snapshots(&store).map_err(CommandError::from)?
    };
    let results = fetch_all_feeds(&snapshots, &state.client).await;
    let store = state.store.lock().expect("rss store poisoned");
    commit_refresh(&store, results).map_err(CommandError::from)
}

/// 按需抓取单篇文章页面原始 HTML(由前端抽取并净化正文)。
/// 仅用于 RSS 正文被源站截断的"查看全文"场景,点击才抓取,不自动、不批量。
#[tauri::command]
async fn fetch_article_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<String, CommandError> {
    let trimmed = url.trim().to_string();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(CommandError {
            code: "rss_invalid_url",
            message: "invalid article url".to_string(),
        });
    }
    let response = state
        .client
        .get(&trimmed)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|source| CommandError {
            code: "rss_fetch_failed",
            message: source.to_string(),
        })?;
    if !response.status().is_success() {
        return Err(CommandError {
            code: "rss_fetch_failed",
            message: format!("server returned {}", response.status()),
        });
    }
    let bytes = response.bytes().await.map_err(|source| CommandError {
        code: "rss_fetch_failed",
        message: source.to_string(),
    })?;
    // 限制单个页面大小,避免极端情况拖慢。
    let bytes = if bytes.len() > 5 * 1024 * 1024 {
        &bytes[..5 * 1024 * 1024]
    } else {
        &bytes[..]
    };
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

#[tauri::command]
fn list_rss_feeds(state: State<'_, AppState>) -> Result<Vec<FeedDto>, CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    list_feeds(&store).map_err(CommandError::from)
}

#[tauri::command]
fn list_rss_articles(
    state: State<'_, AppState>,
    feed_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ArticleDto>, CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    list_articles(&store, feed_id, limit.unwrap_or(200)).map_err(CommandError::from)
}

#[tauri::command]
fn latest_rss_articles(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<ArticleDto>, CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    latest_articles(&store, limit.unwrap_or(5)).map_err(CommandError::from)
}

#[tauri::command]
fn mark_rss_article_read(state: State<'_, AppState>, article_id: i64) -> Result<(), CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    mark_article_read(&store, article_id).map_err(CommandError::from)
}

#[tauri::command]
fn delete_rss_feed(state: State<'_, AppState>, feed_id: i64) -> Result<(), CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    delete_feed(&store, feed_id).map_err(CommandError::from)
}

// ---------- Travel 模块 ----------

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_session_id() -> String {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("t{}", n + 1)
}

/// 开启一次城市研究（后台任务）。立即返回 session_id，进度由 `travel_research_progress` 轮询。
#[tauri::command]
fn travel_research_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TravelResearchRequest,
) -> Result<String, CommandError> {
    if request.city.trim().is_empty() {
        return Err(ApplicationError::EmptyCity.into());
    }
    let session_id = next_session_id();
    // 会话登记（克隆进后台任务）
    let session = Arc::new(Mutex::new(TravelSession {
        events: Vec::new(),
        done: false,
        error: None,
        guide: None,
        from_cache: false,
    }));
    state
        .travel_sessions
        .lock()
        .expect("travel sessions poisoned")
        .insert(session_id.clone(), Arc::clone(&session));

    let client = state.client.clone();
    let store = Arc::clone(&state.travel_store);
    let from_cache = Arc::new(AtomicBool::new(false));
    let from_cache_flag = Arc::clone(&from_cache);

    tauri::async_runtime::spawn(async move {
        // 依赖全部来自设置（未配置项自动降级，保持模块可用）
        let settings = settings_store(&app)
            .and_then(|store| load_settings(&store).map_err(CommandError::from));
        let travel = settings.map_or_else(|_| Default::default(), |settings| settings.travel);
        let providers = build_providers(
            travel.search_backend.clone(),
            travel.searxng_url.clone(),
            &client,
        );
        let fetcher = HttpWebFetcher::new(client.clone());
        let llm = if travel.llm_base_url.is_some() || travel.llm_model.is_some() {
            Some(Box::new(OpenAiCompatibleLlmProvider::new(
                client.clone(),
                LlmConfig {
                    base_url: travel.llm_base_url.clone(),
                    api_key: travel.llm_api_key.clone(),
                    model: travel.llm_model.clone(),
                },
            ))
                as Box<dyn devtoolbox_infrastructure::LlmProvider>)
        } else {
            None
        };
        let data_providers = providers_for(
            travel.amap_api_key.clone(),
            travel.qweather_api_key.clone(),
            travel.qweather_api_host.clone(),
            travel.baidu_map_api_key.clone(),
            &client,
        );
        let service =
            TravelResearchService::new(providers, Box::new(fetcher), llm, data_providers, store);

        let session_events = Arc::clone(&session);
        let progress = move |event: TravelResearchEvent| {
            if event.message.contains("命中缓存攻略") {
                from_cache_flag.store(true, Ordering::Relaxed);
            }
            let mut session = session_events.lock().expect("travel session poisoned");
            session.events.push(event);
        };
        let result = service.research_city(&request, &progress).await;

        let mut session = session.lock().expect("travel session poisoned");
        session.from_cache = from_cache.load(Ordering::Relaxed);
        session.done = true;
        match result {
            Ok(guide) => session.guide = Some(guide),
            Err(error) => session.error = Some(error.to_string()),
        }
    });
    Ok(session_id)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TravelLlmTestRequest {
    base_url: String,
    api_key: Option<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TravelKeyTestRequest {
    api_key: String,
    api_host: Option<String>,
}

fn travel_test_error(code: &'static str, error: impl std::fmt::Display) -> CommandError {
    CommandError {
        code,
        message: error.to_string(),
    }
}

/// 用当前输入值测试 OpenAI Compatible 连通性；不读取或写入 settings.json。
#[tauri::command]
async fn test_travel_llm(
    state: State<'_, AppState>,
    request: TravelLlmTestRequest,
) -> Result<String, CommandError> {
    let provider = OpenAiCompatibleLlmProvider::new(
        state.client.clone(),
        LlmConfig {
            base_url: Some(request.base_url),
            api_key: request.api_key,
            model: Some(request.model),
        },
    );
    let answer = provider
        .complete("You are a connectivity test.", "Reply with OK.")
        .await
        .map_err(|error| travel_test_error("travel_llm_test_failed", error))?;
    Ok(format!(
        "LLM 连接成功（收到 {} 个字符的响应）",
        answer.trim().chars().count()
    ))
}

/// 用当前高德 Key 执行一条北京 POI 查询；不写入 settings.json。
#[tauri::command]
async fn test_travel_amap(
    state: State<'_, AppState>,
    request: TravelKeyTestRequest,
) -> Result<String, CommandError> {
    let provider = AmapPoiProvider::new(state.client.clone(), request.api_key);
    let facts = provider
        .fetch(TravelDataRequest {
            city: "北京".to_string(),
            kind: "poi",
        })
        .await
        .map_err(|error| travel_test_error("travel_amap_test_failed", error))?;
    Ok(format!("高德连接成功（北京 POI 返回 {} 条）", facts.len()))
}

/// 用当前和风 API Host 与 Key 查询北京三日天气；不写入 settings.json。
#[tauri::command]
async fn test_travel_qweather(
    state: State<'_, AppState>,
    request: TravelKeyTestRequest,
) -> Result<String, CommandError> {
    let host = request
        .api_host
        .filter(|host| !host.trim().is_empty())
        .ok_or_else(|| CommandError {
            code: "travel_qweather_test_failed",
            message: "请先填写和风天气 API Host".to_string(),
        })?;
    let provider = QWeatherProvider::new(state.client.clone(), request.api_key, host);
    let facts = provider
        .fetch(TravelDataRequest {
            city: "北京".to_string(),
            kind: "weather",
        })
        .await
        .map_err(|error| travel_test_error("travel_qweather_test_failed", error))?;
    Ok(format!(
        "和风天气连接成功（{}）",
        facts
            .first()
            .map_or("已返回天气数据", |fact| fact.value.as_str())
    ))
}

/// 轮询一次研究进度。
#[tauri::command]
fn travel_research_progress(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<TravelResearchSnapshot>, CommandError> {
    let sessions = state
        .travel_sessions
        .lock()
        .expect("travel sessions poisoned");
    let Some(session) = sessions.get(&session_id) else {
        return Ok(None);
    };
    let session = session.lock().expect("travel session poisoned");
    Ok(Some(TravelResearchSnapshot {
        session_id,
        done: session.done,
        error: session.error.clone(),
        from_cache: session.from_cache,
        events: session.events.clone(),
        guide: session.guide.clone(),
    }))
}

/// 最近生成的攻略列表（历史）。
#[tauri::command]
fn travel_recent_guides(state: State<'_, AppState>) -> Result<Vec<GuideSummary>, CommandError> {
    let store = state.travel_store.lock().expect("travel store poisoned");
    let summaries = store
        .list_guides(20)
        .map_err(|source| CommandError::from(ApplicationError::Travel { source }))?;
    Ok(summaries)
}

/// 按城市 + 天数读取已保存攻略（不校验缓存有效期，供历史查看）。
#[tauri::command]
fn travel_load_guide(
    state: State<'_, AppState>,
    city: String,
    days: u8,
) -> Result<Option<CityGuide>, CommandError> {
    let store = state.travel_store.lock().expect("travel store poisoned");
    let guide = store
        .load_guide(&city, days)
        .map_err(|source| CommandError::from(ApplicationError::Travel { source }))?;
    Ok(guide)
}

// ---------- Language 模块（离线优先；数据包安装不联网） ----------

fn language_service(state: &State<'_, AppState>) -> LanguageService {
    LanguageService::new(Arc::clone(&state.language_store))
}

#[tauri::command]
fn language_languages(state: State<'_, AppState>) -> Result<Vec<LanguageInfo>, CommandError> {
    language_service(&state)
        .languages()
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_search(
    state: State<'_, AppState>,
    language: Option<String>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<LanguageSearchHit>, CommandError> {
    language_service(&state)
        .search(language.as_deref(), &query, limit.unwrap_or(30))
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<WordDetail>, CommandError> {
    language_service(&state)
        .detail(&id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_sentences(
    state: State<'_, AppState>,
    language: String,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_core::language::SentenceRecord>, CommandError> {
    language_service(&state)
        .sentences(&language, limit.unwrap_or(20))
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_today(state: State<'_, AppState>, language: String) -> Result<TodayView, CommandError> {
    language_service(&state)
        .today(&language)
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_review_next(
    state: State<'_, AppState>,
    language: String,
) -> Result<Option<ReviewCard>, CommandError> {
    language_service(&state)
        .review_next(&language)
        .map_err(CommandError::from)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct RateRequest {
    item_id: String,
    rating: ReviewRating,
}

#[tauri::command]
fn language_review_rate(
    state: State<'_, AppState>,
    request: RateRequest,
) -> Result<devtoolbox_core::language::ReviewOutcome, CommandError> {
    language_service(&state)
        .rate(&request.item_id, request.rating)
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_toggle_favorite(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<bool, CommandError> {
    language_service(&state)
        .toggle_favorite(&item_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_favorites(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_core::language::LanguageItem>, CommandError> {
    language_service(&state)
        .favorites(limit.unwrap_or(200))
        .map_err(CommandError::from)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct SetStateRequest {
    item_id: String,
    state: LearningStateKind,
}

#[tauri::command]
fn language_set_state(
    state: State<'_, AppState>,
    request: SetStateRequest,
) -> Result<(), CommandError> {
    language_service(&state)
        .set_state(&request.item_id, request.state)
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_progress(state: State<'_, AppState>) -> Result<ProgressView, CommandError> {
    language_service(&state)
        .progress()
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_sources(state: State<'_, AppState>) -> Result<Vec<SourceInfo>, CommandError> {
    language_service(&state)
        .sources()
        .map_err(CommandError::from)
}

#[tauri::command]
fn language_manifests(state: State<'_, AppState>) -> Result<Vec<ManifestInfo>, CommandError> {
    language_service(&state)
        .manifests()
        .map_err(CommandError::from)
}

/// 安装内置 Starter Pack（离线；真实数据子集 + attribution）。
#[tauri::command]
fn language_install_starter(
    state: State<'_, AppState>,
    only: Option<String>,
) -> Result<starter::StarterReport, CommandError> {
    let mut store = state
        .language_store
        .lock()
        .expect("language store poisoned");
    starter::install_starter(&mut store, only.as_deref()).map_err(CommandError::from)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpeakingScoreRequest {
    target: String,
    transcript: String,
    duration_ms: u64,
    target_ms: u64,
    long_pauses_ms: Vec<u64>,
}

#[tauri::command]
fn language_speaking_feedback(
    state: State<'_, AppState>,
    request: SpeakingScoreRequest,
) -> Result<SpeakingScore, CommandError> {
    let service = language_service(&state);
    Ok(service.speaking_feedback(
        &request.target,
        &request.transcript,
        request.duration_ms,
        request.target_ms,
        &request.long_pauses_ms,
    ))
}

// ---------- History 模块（离线优先） ----------

#[tauri::command]
fn history_home(state: State<'_, AppState>, cursor: u64) -> Result<HistoryHome, CommandError> {
    HistoryService::new(Arc::clone(&state.history_store))
        .home(cursor)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<HistorySearchGroup>, CommandError> {
    HistoryService::new(Arc::clone(&state.history_store))
        .search(&query)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_detail(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<HistoryDetailView>, CommandError> {
    HistoryService::new(Arc::clone(&state.history_store))
        .detail(&id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool, CommandError> {
    HistoryService::new(Arc::clone(&state.history_store))
        .toggle_favorite(&id)
        .map_err(CommandError::from)
}

// ---------- Geography Explorer 模块（离线优先） ----------

fn geography_service(state: &State<'_, AppState>) -> devtoolbox_application::GeographyService {
    devtoolbox_application::GeographyService::new(Arc::clone(&state.geography_store))
}

#[tauri::command]
fn geography_home(
    state: State<'_, AppState>,
    cursor: Option<u64>,
) -> Result<GeographyHome, CommandError> {
    geography_service(&state)
        .home(cursor.unwrap_or_default())
        .map_err(CommandError::from)
}

#[tauri::command]
fn geography_search(
    state: State<'_, AppState>,
    query: String,
    entity_type: Option<CoreGeoEntityType>,
    limit: Option<usize>,
) -> Result<Vec<GeoSearchGroup>, CommandError> {
    geography_service(&state)
        .search(&query, entity_type, limit.unwrap_or(30).min(100))
        .map_err(CommandError::from)
}

#[tauri::command]
fn geography_detail(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<GeoEntityDetail>, CommandError> {
    geography_service(&state)
        .detail(&id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn geography_map(
    state: State<'_, AppState>,
) -> Result<
    (
        Vec<devtoolbox_core::geography::GeoMapPoint>,
        Vec<devtoolbox_core::geography::GeoMapLine>,
    ),
    CommandError,
> {
    geography_service(&state).map().map_err(CommandError::from)
}

#[tauri::command]
fn geography_compare(
    state: State<'_, AppState>,
    left_id: String,
    right_id: String,
) -> Result<GeoCompareView, CommandError> {
    geography_service(&state)
        .compare(&left_id, &right_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn geography_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool, CommandError> {
    geography_service(&state)
        .toggle_favorite(&id)
        .map_err(CommandError::from)
}

/// 应用入口。前端需要的权限被限制在文件选择器、command API 与打开原文链接；
/// 不暴露任意 shell 执行能力。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_directory = app.path().app_config_dir()?;
            let store = FeedRepository::open(config_directory.join("dashboard.db"))
                .expect("open rss database");
            let travel_store = TravelStore::open(config_directory.join("travel.db"))
                .expect("open travel database");
            let history_store = HistoryStore::open(config_directory.join("history.db"))
                .expect("open history database");
            let language_store = LanguageStore::open(config_directory.join("language.db"))
                .expect("open language database");
            let geography_store = GeographyStore::open(config_directory.join("geography.db"))
                .expect("open geography database");
            let client = feed_client().expect("build http client");
            app.manage(AppState {
                store: Mutex::new(store),
                travel_store: Arc::new(Mutex::new(travel_store)),
                travel_sessions: Arc::new(Mutex::new(HashMap::new())),
                history_store: Arc::new(Mutex::new(history_store)),
                language_store: Arc::new(Mutex::new(language_store)),
                geography_store: Arc::new(Mutex::new(geography_store)),
                client,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_document,
            write_document,
            list_workspace,
            convert_task_lines,
            cycle_task_lines,
            get_settings,
            put_settings,
            add_rss_feed,
            refresh_rss_feeds,
            fetch_article_url,
            list_rss_feeds,
            list_rss_articles,
            latest_rss_articles,
            mark_rss_article_read,
            delete_rss_feed,
            travel_research_start,
            travel_research_progress,
            travel_recent_guides,
            travel_load_guide,
            test_travel_llm,
            test_travel_amap,
            test_travel_qweather,
            history_home,
            history_search,
            history_detail,
            history_toggle_favorite,
            geography_home,
            geography_search,
            geography_detail,
            geography_map,
            geography_compare,
            geography_toggle_favorite,
            language_languages,
            language_search,
            language_item,
            language_today,
            language_sentences,
            language_review_next,
            language_review_rate,
            language_toggle_favorite,
            language_favorites,
            language_set_state,
            language_progress,
            language_sources,
            language_manifests,
            language_install_starter,
            language_speaking_feedback
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application event loop failed");
}
