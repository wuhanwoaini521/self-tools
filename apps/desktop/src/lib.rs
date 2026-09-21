//! `Tauri` command adapter。业务规则位于 workspace 的 application/core crates。
//!
//! 全局 `AppState` 持有各模块的 SQLite 存储器、Travel 会话注册表与共享 HTTP
//! 客户端；每个功能模块(文档 / RSS / Travel)的命令各自独立，互不依赖。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use devtoolbox_application::language::{
    LanguageInfo, LanguageSearchHit, LanguageService, ProgressView, ReviewCard, SourceInfo,
    TodayView, WordDetail,
};
use devtoolbox_application::travel::session::TravelSessionRegistry;
use devtoolbox_application::{
    ApplicationError, ArticleDto, DocumentDto, FeedDto, GeoEntityDetail, GeoSearchGroup,
    GeographyHome, RefreshReport, RssErrorKind, RssRepositoryPort, TravelErrorKind,
    TravelResearchRequest, commit_new_feed, commit_refresh, cycle_lines, delete_feed,
    feed_snapshots, fetch_all_feeds, fetch_new_feed, latest_articles, list_articles, list_feeds,
    load_document, load_settings, mark_article_read, save_document, save_settings, scan_workspace,
    validate_feed_url,
};
use devtoolbox_core::{
    AppSettings, ChatModelProvider, WorkspaceFile,
    geography::GeoEntityType as CoreGeoEntityType,
    language::{LearningStateKind, ReviewRating, SpeakingScore},
    travel::{CityGuide, GuideSummary, TravelDateRange, TravelResearchEvent},
};
use devtoolbox_infrastructure::language::starter::{self, StarterReport};
use devtoolbox_infrastructure::{
    FeedRepository, GeographyStore, HistoryDuckDbRepository, LanguageStore, SettingsStore,
    TravelDataProvider, TravelDataRequest, TravelStore, feed_client,
};

// V6：Personal Knowledge 组合根（适配器 + 服务装配 + 启动同步）。
mod knowledge;

// lib 已不再直接使用 serde_json（History 用例迁入 application）；保留空导入以消除 unused warning。
use serde::{Deserialize, Serialize};
use serde_json as _;
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
            ApplicationError::Geography { .. } | ApplicationError::GeographyData(_) => {
                "geography_error"
            }
            ApplicationError::History(_) => "history_error",
            ApplicationError::PersonalAi(error) => error.code(),
            ApplicationError::Travel(failure) => match failure.kind {
                TravelErrorKind::Search => "travel_search_failed",
                TravelErrorKind::Fetch => "travel_fetch_failed",
                TravelErrorKind::Llm => "travel_llm_failed",
                TravelErrorKind::Data => "travel_data_failed",
                TravelErrorKind::Store => "travel_error",
            },
            ApplicationError::Rss { kind, .. } => match kind {
                RssErrorKind::Fetch => "rss_fetch_failed",
                RssErrorKind::Parse => "rss_parse_failed",
                RssErrorKind::Repository => "infrastructure_error",
            },
            ApplicationError::Memory { .. } => "memory_error",
            ApplicationError::Documents { .. } => "documents_error",
            ApplicationError::Files { .. } => "files_error",
            ApplicationError::Knowledge { .. } => "knowledge_error",
            ApplicationError::Server { .. } => "server_error",
            ApplicationError::Infrastructure { .. } => "infrastructure_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

/// 旅行缓存命令的错误映射（存储层直出错误；code/text 与
/// `ApplicationError::Travel` 旧形态完全一致）。
fn store_command_error(source: devtoolbox_infrastructure::InfrastructureError) -> CommandError {
    let code = match &source {
        devtoolbox_infrastructure::InfrastructureError::TravelSearch(_) => "travel_search_failed",
        devtoolbox_infrastructure::InfrastructureError::TravelFetch(_) => "travel_fetch_failed",
        devtoolbox_infrastructure::InfrastructureError::TravelLlm(_) => "travel_llm_failed",
        devtoolbox_infrastructure::InfrastructureError::TravelData(_) => "travel_data_failed",
        _ => "travel_error",
    };
    CommandError {
        code,
        message: format!("travel error: {source}"),
    }
}

/// 应用级共享状态：RSS 存储 + Travel 缓存 + 会话注册表 + HTTP 客户端。
///
/// Gate 7：Travel 会话生命周期属于 application（`TravelSessionRegistry` 是应用层
/// 类型，内存态）；Tauri 不再持有/管理会话容器。
pub struct AppState {
    /// RSS：应用层端口（adapters 在组合根装配；不直接暴露 SQLite/reqwest）。
    pub rss_repository: Arc<dyn RssRepositoryPort>,
    pub rss_fetcher: composition::FeedFetcherAdapter,
    pub travel_store: Arc<Mutex<TravelStore>>,
    pub travel_registry: TravelSessionRegistry,
    pub history_duckdb: Arc<HistoryDuckDbRepository>,
    pub language_store: Arc<Mutex<LanguageStore>>,
    pub geography_store: Arc<Mutex<GeographyStore>>,
    pub client: reqwest::Client,
    /// Personal AI 注册中心（Gates 2/5：History 标准模块已注册）。
    pub ai: Arc<PersonalHub>,
    /// Personal AI 会话存储（V4 §34，内存；P1 再持久化）。
    pub ai_session: Arc<InMemorySessionStore>,
    /// History Enrichment 运行器（V5 Gate 4；命令与 agent 工具共用）。
    pub history_enrichment: Arc<dyn EnrichmentRunnerPort>,
    /// Personal Knowledge 运行时（V6：memory / documents / files + 统一检索）。
    pub knowledge: Arc<knowledge::KnowledgeRuntime>,
}

/// 轮询快照（Serialize 给前端；命令契约形状保持不变）。
#[derive(Debug, Serialize)]
pub struct TravelResearchSnapshot {
    pub session_id: String,
    pub done: bool,
    pub error: Option<String>,
    pub from_cache: bool,
    pub events: Vec<TravelResearchEvent>,
    pub guide: Option<CityGuide>,
}

fn project_root_from(start: PathBuf) -> Option<PathBuf> {
    let mut current = start;
    loop {
        if current.join("Cargo.toml").is_file() || current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn project_config_directory(app: &AppHandle) -> Result<PathBuf, CommandError> {
    let current_dir = std::env::current_dir().map_err(|error| CommandError {
        code: "project_config_dir",
        message: error.to_string(),
    })?;
    let project_root = project_root_from(current_dir.clone())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| project_root_from(path.parent()?.to_path_buf()))
        })
        .unwrap_or(current_dir);
    let config_directory = project_root.join("config");
    std::fs::create_dir_all(&config_directory).map_err(|error| CommandError {
        code: "project_config_dir",
        message: error.to_string(),
    })?;

    // 首次切换时只迁移不存在的文件，不覆盖项目 config 中已有内容。
    if let Ok(legacy_directory) = app.path().app_config_dir()
        && legacy_directory != config_directory
    {
        for filename in [
            "settings.json",
            "dashboard.db",
            "travel.db",
            "geography.db",
            "language.db",
        ] {
            let source = legacy_directory.join(filename);
            let target = config_directory.join(filename);
            if source.is_file() && !target.exists() {
                let _ = std::fs::copy(source, target);
            }
        }
    }
    Ok(config_directory)
}

fn settings_store(app: &AppHandle) -> Result<SettingsStore, CommandError> {
    Ok(SettingsStore::new(project_config_directory(app)?))
}

fn semantic_history_path(_app: &AppHandle) -> Result<PathBuf, CommandError> {
    // V2：唯一事实源是 history-data-pipeline/dist/history.duckdb（build artifact）。
    // 刻意不再回退到 data/normalized/ 的 legacy 语义库；缺失时明确返回开发错误。
    const RELATIVE: &str = "history-data-pipeline/dist/history.duckdb";
    fn find_from(start: &Path) -> Option<PathBuf> {
        let mut current = start.to_path_buf();
        loop {
            let candidate = current.join(RELATIVE);
            if candidate.is_file() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    let current_dir = std::env::current_dir().map_err(|error| CommandError {
        code: "history_data_missing",
        message: error.to_string(),
    })?;
    let path = find_from(&current_dir).or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(PathBuf::from))
            .and_then(|path| find_from(&path))
    });
    path.ok_or_else(|| CommandError {
        code: "history_data_missing",
        message: format!(
            "History data artifact is missing: {} (searched from {} and the executable directory). \
             Rebuild it with the submodule build command, e.g. `uv run history-data backbone build`.",
            RELATIVE,
            current_dir.display()
        ),
    })
}

// ---------- 文档 / Markdown 模块 ----------

mod composition;
mod travel_providers;

#[tauri::command]
fn read_document(path: String) -> Result<DocumentDto, CommandError> {
    load_document(&composition::DocumentStoreAdapter, &path).map_err(CommandError::from)
}

#[tauri::command]
fn write_document(path: String, text: String) -> Result<(), CommandError> {
    save_document(&composition::DocumentStoreAdapter, &path, &text).map_err(CommandError::from)
}

#[tauri::command]
fn list_workspace(path: String) -> Result<Vec<WorkspaceFile>, CommandError> {
    scan_workspace(&composition::DocumentStoreAdapter, &path).map_err(CommandError::from)
}

#[tauri::command]
fn cycle_task_lines(lines: Vec<String>, step: isize) -> Vec<String> {
    cycle_lines(&lines, step)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettings, CommandError> {
    let store = settings_store(&app)?;
    load_settings(&composition::SettingsStoreAdapter::new(store)).map_err(CommandError::from)
}

#[tauri::command]
fn put_settings(app: AppHandle, settings: AppSettings) -> Result<(), CommandError> {
    let store = settings_store(&app)?;
    save_settings(&composition::SettingsStoreAdapter::new(store), &settings)
        .map_err(CommandError::from)
}

// ---------- RSS 模块 ----------

#[tauri::command]
async fn add_rss_feed(state: State<'_, AppState>, url: String) -> Result<FeedDto, CommandError> {
    // 两段式：先无锁抓取(可跨 await),再短锁落库。
    let normalized = validate_feed_url(&url).map_err(CommandError::from)?;
    let fetched = fetch_new_feed(&normalized, &state.rss_fetcher)
        .await
        .map_err(CommandError::from)?;
    commit_new_feed(state.rss_repository.as_ref(), &normalized, fetched).map_err(CommandError::from)
}

#[tauri::command]
async fn refresh_rss_feeds(state: State<'_, AppState>) -> Result<RefreshReport, CommandError> {
    // 快照 → 并发抓取(无锁) → 短锁落库;单个 Feed 失败不影响其他。
    let snapshots = feed_snapshots(state.rss_repository.as_ref()).map_err(CommandError::from)?;
    let results = fetch_all_feeds(&snapshots, &state.rss_fetcher).await;
    commit_refresh(state.rss_repository.as_ref(), results).map_err(CommandError::from)
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
    list_feeds(state.rss_repository.as_ref()).map_err(CommandError::from)
}

#[tauri::command]
fn list_rss_articles(
    state: State<'_, AppState>,
    feed_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ArticleDto>, CommandError> {
    list_articles(state.rss_repository.as_ref(), feed_id, limit.unwrap_or(200))
        .map_err(CommandError::from)
}

#[tauri::command]
fn latest_rss_articles(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<ArticleDto>, CommandError> {
    latest_articles(state.rss_repository.as_ref(), limit.unwrap_or(5)).map_err(CommandError::from)
}

#[tauri::command]
fn mark_rss_article_read(state: State<'_, AppState>, article_id: i64) -> Result<(), CommandError> {
    mark_article_read(state.rss_repository.as_ref(), article_id).map_err(CommandError::from)
}

#[tauri::command]
fn delete_rss_feed(state: State<'_, AppState>, feed_id: i64) -> Result<(), CommandError> {
    delete_feed(state.rss_repository.as_ref(), feed_id).map_err(CommandError::from)
}

// ---------- Travel 模块 ----------

/// 开启一次城市研究（后台任务）。立即返回 session_id，进度由 `travel_research_progress` 轮询。
///
/// Gate 7：命令保持「薄」——会话由应用层注册表分配，provider 由组合根
/// (`travel_providers`) 装配；命令只做参数校验、登记会话、调度后台任务。
#[tauri::command]
fn travel_research_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TravelResearchRequest,
) -> Result<String, CommandError> {
    if request.city.trim().is_empty() {
        return Err(ApplicationError::EmptyCity.into());
    }
    let (session_id, session) = state.travel_registry.register();

    let client = state.client.clone();
    let store = Arc::clone(&state.travel_store);
    tauri::async_runtime::spawn(async move {
        // 运行时配置：provider 装配在组合根按设置完成（未配置项自动降级）。
        let travel = settings_store(&app)
            .ok()
            .and_then(|settings_store| {
                load_settings(&composition::SettingsStoreAdapter::new(settings_store)).ok()
            })
            .map(|settings| settings.travel)
            .unwrap_or_default();
        let service = travel_providers::travel_research_service(&client, &travel, store);

        let session_handle = Arc::clone(&session);
        let progress = move |event: TravelResearchEvent| {
            session_handle
                .lock()
                .expect("travel session poisoned")
                .push_event(event);
        };
        match service.research_city(&request, &progress).await {
            Ok(outcome) => session
                .lock()
                .expect("travel session poisoned")
                .finish(outcome.guide, outcome.from_cache),
            Err(error) => session
                .lock()
                .expect("travel session poisoned")
                .fail(error.to_string()),
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
    let provider = travel_providers::llm_test_provider(
        &state.client,
        request.base_url,
        request.api_key,
        request.model,
    );
    let answer = provider
        .chat(devtoolbox_core::ChatRequest {
            messages: vec![
                devtoolbox_core::ChatMessage::system("You are a connectivity test."),
                devtoolbox_core::ChatMessage::user("Reply with OK."),
            ],
            tools: Vec::new(),
            temperature: Some(0.2),
            max_tokens: None,
        })
        .await
        .map_err(|error| travel_test_error("travel_llm_test_failed", error.message))?
        .content
        .unwrap_or_default();
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
    let provider = travel_providers::amap_test_provider(&state.client, request.api_key);
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
    let provider = travel_providers::qweather_test_provider(&state.client, request.api_key, host);
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
    let Some(session) = state.travel_registry.get(&session_id) else {
        return Ok(None);
    };
    let session = session.lock().expect("travel session poisoned");
    let view = session.view();
    Ok(Some(TravelResearchSnapshot {
        session_id,
        done: view.done,
        error: view.error,
        from_cache: view.from_cache,
        events: view.events,
        guide: view.guide,
    }))
}

/// 最近生成的攻略列表（历史）。
#[tauri::command]
fn travel_recent_guides(state: State<'_, AppState>) -> Result<Vec<GuideSummary>, CommandError> {
    let store = state.travel_store.lock().expect("travel store poisoned");
    let summaries = store.list_guides(20).map_err(store_command_error)?;
    Ok(summaries)
}

/// 按城市 + 天数读取已保存攻略（不校验缓存有效期，供历史查看）。
#[tauri::command]
fn travel_load_guide(
    state: State<'_, AppState>,
    city: String,
    days: u8,
    date_range: Option<TravelDateRange>,
) -> Result<Option<CityGuide>, CommandError> {
    let store = state.travel_store.lock().expect("travel store poisoned");
    let guide = store
        .load_guide(&city, days, date_range.as_ref())
        .map_err(store_command_error)?;
    Ok(guide)
}

// ---------- Language 模块（离线优先；数据包安装不联网） ----------

fn language_service(state: &State<'_, AppState>) -> LanguageService {
    LanguageService::new(Arc::new(composition::LanguageStoreAdapter::new(
        Arc::clone(&state.language_store),
    )))
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

/// 安装内置 Starter Pack（离线；真实数据子集 + attribution）。
#[tauri::command]
fn language_install_starter(
    state: State<'_, AppState>,
    only: Option<String>,
) -> Result<StarterReport, CommandError> {
    let mut store = state
        .language_store
        .lock()
        .expect("language store poisoned");
    starter::install_starter(&mut store, only.as_deref()).map_err(|error| match error {
        starter::StarterError::License(_) => CommandError {
            code: "language_license",
            message: "language license error: starter pack data source not permitted".into(),
        },
        starter::StarterError::Store(_) => CommandError {
            code: "language_error",
            message: "language error: starter pack installation failed".into(),
        },
    })
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

// ---------- History 模块（V2：唯一事实源 history-data-pipeline/dist） ----------
// 用例逻辑在 crates/application/src/history/（HistoryService + HistoryQueryPort）；
// 本文件只做 Tauri 命令转发，不做聚合决策。

mod history_enrichment;
mod history_query;
mod personal_ai;
mod travel_ai;

use devtoolbox_application::history::enrichment::EnrichmentRunnerPort;
use devtoolbox_application::history::{EnrichmentKey, EnrichmentSection, EnrichmentView};
use devtoolbox_application::history::{
    HistorySemanticEventDetail, HistorySemanticHome, HistorySemanticPeriodDetail,
    HistorySemanticPersonDetail, HistorySemanticSearchGroup, HistorySemanticStoryDetail,
    HistorySemanticWorkDetail, HistoryService,
};
use devtoolbox_application::personal_ai::{InMemorySessionStore, PersonalHub};
use devtoolbox_core::ToolSpec;
use devtoolbox_core::personal_ai::{AgentRequest, AgentResponse, ModuleDescriptor};

fn history_service(state: &State<'_, AppState>) -> HistoryService {
    HistoryService::new(Box::new(history_query::HistoryQueryAdapter::new(
        Arc::clone(&state.history_duckdb),
    )))
}

#[tauri::command]
fn history_semantic_home(state: State<'_, AppState>) -> Result<HistorySemanticHome, CommandError> {
    history_service(&state).home().map_err(CommandError::from)
}

#[tauri::command]
fn history_semantic_period(
    state: State<'_, AppState>,
    period_id: String,
) -> Result<Option<HistorySemanticPeriodDetail>, CommandError> {
    history_service(&state)
        .period_detail(&period_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_semantic_story(
    state: State<'_, AppState>,
    story_id: String,
) -> Result<Option<HistorySemanticStoryDetail>, CommandError> {
    history_service(&state)
        .story_detail(&story_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_semantic_event(
    state: State<'_, AppState>,
    event_id: String,
) -> Result<Option<HistorySemanticEventDetail>, CommandError> {
    history_service(&state)
        .event_detail(&event_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_semantic_person(
    state: State<'_, AppState>,
    person_id: String,
) -> Result<Option<HistorySemanticPersonDetail>, CommandError> {
    history_service(&state)
        .person_detail(&person_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_semantic_work(
    state: State<'_, AppState>,
    work_id: String,
) -> Result<Option<HistorySemanticWorkDetail>, CommandError> {
    history_service(&state)
        .work_detail(&work_id)
        .map_err(CommandError::from)
}

#[tauri::command]
fn history_semantic_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<HistorySemanticSearchGroup>, CommandError> {
    history_service(&state)
        .search(&query)
        .map_err(CommandError::from)
}

// ---------- Geography Explorer 模块（离线优先） ----------

mod geography_query;

fn geography_service(state: &State<'_, AppState>) -> devtoolbox_application::GeographyService {
    devtoolbox_application::GeographyService::new(Box::new(
        geography_query::GeographyQueryAdapter::new(Arc::clone(&state.geography_store)),
    ))
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
fn geography_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool, CommandError> {
    geography_service(&state)
        .toggle_favorite(&id)
        .map_err(CommandError::from)
}

// ---------------------------------------------------------------------------
// Personal AI（V4 Gate 6）：状态查询 + 对话（配置按每次调用读取，改设置即生效）
// ---------------------------------------------------------------------------

/// AI 面板状态（不含任何 key）。
#[derive(Debug, Serialize)]
struct PersonalAiStatus {
    configured: bool,
    provider: Option<String>,
    model: Option<String>,
    modules: Vec<ModuleDescriptor>,
    tools: Vec<ToolSpec>,
}

fn load_ai_settings(
    app: &AppHandle,
) -> Result<devtoolbox_core::settings::AiSettings, CommandError> {
    let store = settings_store(app)?;
    load_settings(&composition::SettingsStoreAdapter::new(store))
        .map(|settings| settings.ai)
        .map_err(CommandError::from)
}

#[tauri::command]
fn personal_ai_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PersonalAiStatus, CommandError> {
    let ai = load_ai_settings(&app)?;
    Ok(PersonalAiStatus {
        configured: ai.is_configured(),
        provider: if ai.is_configured() {
            Some("openai-compatible".to_string())
        } else {
            None
        },
        model: ai.model.clone(),
        modules: state.ai.modules.descriptors(),
        tools: state.ai.tools.specs(),
    })
}

#[tauri::command]
async fn personal_ai_chat(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentRequest,
) -> Result<AgentResponse, CommandError> {
    if request.message.trim().is_empty() {
        return Err(CommandError {
            code: "personal_ai_empty_message",
            message: "消息不能为空".to_string(),
        });
    }
    let ai = load_ai_settings(&app)?;
    let provider = personal_ai::build_provider(state.client.clone(), &ai);
    let agent = personal_ai::build_agent(
        provider,
        Arc::clone(&state.ai),
        Arc::clone(&state.ai_session),
    );
    agent
        .run(request)
        .await
        .map_err(ApplicationError::PersonalAi)
        .map_err(CommandError::from)
}

// ---------------------------------------------------------------------------
// Personal Knowledge（V6）：Memory / Documents / Files 管理与检索命令
// ---------------------------------------------------------------------------
//
// 边界（V6 §15/§20/§42）：
// - `memory_confirm` / `memory_save` 是**唯一**能把记忆写成 ACTIVE 的入口（用户动作）；
// - 没有任何命令可以写、移动或删除文件；索引扫描只写本地索引库。

#[derive(Debug, Serialize)]
struct MemoryStatsDto {
    total: usize,
    active: usize,
    candidates: usize,
    archived: usize,
    rejected: usize,
    expired: usize,
    categories: Vec<MemoryCategoryCountDto>,
}

#[derive(Debug, Serialize)]
struct MemoryCategoryCountDto {
    category: String,
    count: usize,
}

fn memory_stats(state: &State<'_, AppState>) -> Result<MemoryStatsDto, CommandError> {
    let stats = state
        .knowledge
        .memory
        .stats()
        .map_err(CommandError::from)?;
    let categories = state
        .knowledge
        .memory
        .category_counts()
        .map_err(CommandError::from)?
        .into_iter()
        .map(|(category, count)| MemoryCategoryCountDto {
            category: category.as_str().to_string(),
            count,
        })
        .collect();
    Ok(MemoryStatsDto {
        total: stats.total(),
        active: stats.active,
        candidates: stats.candidates,
        archived: stats.archived,
        rejected: stats.rejected,
        expired: stats.expired,
        categories,
    })
}

#[tauri::command]
fn memory_status(state: State<'_, AppState>) -> Result<MemoryStatsDto, CommandError> {
    memory_stats(&state)
}

fn memory_item_json(item: &devtoolbox_core::memory::MemoryItem) -> serde_json::Value {
    devtoolbox_application::personal_ai::memory::memory_json(item)
}

#[tauri::command]
fn memory_list(
    state: State<'_, AppState>,
    query: Option<String>,
    category: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, CommandError> {
    let spec = devtoolbox_core::memory::MemoryQuery {
        query: query.unwrap_or_default(),
        category: parse_memory_category(category.as_deref())?,
        status: parse_memory_status(status.as_deref())?,
        // 管理页是用户显式动作：敏感项可见（模型路径仍然过滤）。
        include_sensitive: true,
        limit: limit.unwrap_or(0),
    };
    let items = state
        .knowledge
        .memory
        .list(&spec)
        .map_err(CommandError::from)?;
    Ok(items.iter().map(memory_item_json).collect())
}

#[tauri::command]
fn memory_get(state: State<'_, AppState>, id: String) -> Result<serde_json::Value, CommandError> {
    let item = state
        .knowledge
        .memory
        .get(&id, true)
        .map_err(CommandError::from)?;
    Ok(memory_item_json(&item))
}

#[derive(Debug, Deserialize)]
struct MemoryDraftRequest {
    category: String,
    content: String,
    #[serde(default)]
    source_type: Option<String>,
    #[serde(default)]
    source_reference: Option<String>,
    #[serde(default)]
    sensitivity: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
}

fn memory_draft(request: MemoryDraftRequest) -> Result<devtoolbox_core::memory::MemoryDraft, CommandError> {
    let category = parse_memory_category(Some(&request.category))?
        .ok_or_else(|| CommandError {
            code: "memory_error",
            message: "记忆分类不能为空".to_string(),
        })?;
    let source_type = match request.source_type.as_deref() {
        None => devtoolbox_core::memory::MemorySourceType::ExplicitUser,
        Some(raw) => devtoolbox_core::memory::MemorySourceType::parse(raw).ok_or_else(|| {
            CommandError {
                code: "memory_error",
                message: format!("未知来源类型：{raw}"),
            }
        })?,
    };
    let sensitivity = match request.sensitivity.as_deref() {
        None => devtoolbox_core::memory::MemorySensitivity::Normal,
        Some(raw) => devtoolbox_core::memory::MemorySensitivity::parse(raw).ok_or_else(|| {
            CommandError {
                code: "memory_error",
                message: format!("未知敏感度：{raw}"),
            }
        })?,
    };
    Ok(devtoolbox_core::memory::MemoryDraft {
        category,
        content: request.content,
        source_type,
        source_reference: request.source_reference,
        sensitivity,
        confidence: request.confidence.unwrap_or(0.9),
        expires_at: None,
        metadata: serde_json::Value::Null,
    })
}

fn parse_memory_category(
    raw: Option<&str>,
) -> Result<Option<devtoolbox_core::memory::MemoryCategory>, CommandError> {
    match raw {
        None => Ok(None),
        Some(raw) => devtoolbox_core::memory::MemoryCategory::parse(raw)
            .map(Some)
            .ok_or_else(|| CommandError {
                code: "memory_error",
                message: format!("未知记忆分类：{raw}"),
            }),
    }
}

fn parse_memory_status(
    raw: Option<&str>,
) -> Result<Option<devtoolbox_core::memory::MemoryStatus>, CommandError> {
    match raw {
        None => Ok(None),
        Some(raw) => devtoolbox_core::memory::MemoryStatus::parse(raw)
            .map(Some)
            .ok_or_else(|| CommandError {
                code: "memory_error",
                message: format!("未知记忆状态：{raw}"),
            }),
    }
}

/// 用户在界面上确认保存（§15/§26）：唯一能把新记忆写成 ACTIVE 的入口之一。
#[tauri::command]
fn memory_save(
    state: State<'_, AppState>,
    request: MemoryDraftRequest,
) -> Result<serde_json::Value, CommandError> {
    let draft = memory_draft(request)?;
    let item = state
        .knowledge
        .memory
        .save_confirmed(draft)
        .map_err(CommandError::from)?;
    Ok(memory_item_json(&item))
}

/// 用户确认候选（§25：「是否记住这条信息？」→ 记住）。
#[tauri::command]
fn memory_confirm(
    state: State<'_, AppState>,
    id: String,
) -> Result<serde_json::Value, CommandError> {
    let item = state
        .knowledge
        .memory
        .confirm(&id)
        .map_err(CommandError::from)?;
    Ok(memory_item_json(&item))
}

#[tauri::command]
fn memory_reject(state: State<'_, AppState>, id: String) -> Result<serde_json::Value, CommandError> {
    let item = state
        .knowledge
        .memory
        .reject(&id)
        .map_err(CommandError::from)?;
    Ok(memory_item_json(&item))
}

#[tauri::command]
fn memory_archive(state: State<'_, AppState>, id: String) -> Result<serde_json::Value, CommandError> {
    let item = state
        .knowledge
        .memory
        .archive(&id)
        .map_err(CommandError::from)?;
    Ok(memory_item_json(&item))
}

#[derive(Debug, Deserialize)]
struct MemoryUpdateRequest {
    id: String,
    content: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    sensitivity: Option<String>,
}

#[tauri::command]
fn memory_update(
    state: State<'_, AppState>,
    request: MemoryUpdateRequest,
) -> Result<serde_json::Value, CommandError> {
    let category = parse_memory_category(request.category.as_deref())?;
    let sensitivity = match request.sensitivity.as_deref() {
        None => None,
        Some(raw) => Some(
            devtoolbox_core::memory::MemorySensitivity::parse(raw).ok_or_else(|| CommandError {
                code: "memory_error",
                message: format!("未知敏感度：{raw}"),
            })?,
        ),
    };
    let item = state
        .knowledge
        .memory
        .update(&request.id, &request.content, category, sensitivity)
        .map_err(CommandError::from)?;
    Ok(memory_item_json(&item))
}

fn knowledge_roots(state: &State<'_, AppState>) -> Vec<devtoolbox_core::files::KnowledgeRoot> {
    let settings = state.knowledge.settings();
    let mut roots = settings.file_roots.clone();
    for root in settings.effective_document_roots() {
        if !roots.iter().any(|existing| existing.id == root.id) {
            roots.push(root);
        }
    }
    roots
}

#[derive(Debug, Serialize)]
struct DocumentStatusDto {
    roots: Vec<devtoolbox_core::files::KnowledgeRoot>,
    configured: bool,
    documents: usize,
    chunks: usize,
    content_available: usize,
    metadata_only: usize,
    failed: usize,
}

#[tauri::command]
fn documents_status(state: State<'_, AppState>) -> Result<DocumentStatusDto, CommandError> {
    let settings = state.knowledge.settings();
    let stats = state
        .knowledge
        .documents
        .stats()
        .map_err(CommandError::from)?;
    Ok(DocumentStatusDto {
        configured: !settings.effective_document_roots().is_empty(),
        roots: knowledge_roots(&state),
        documents: stats.documents,
        chunks: stats.chunks,
        content_available: stats.content_available,
        metadata_only: stats.metadata_only,
        failed: stats.failed,
    })
}

fn document_hit_json(
    hit: &devtoolbox_core::documents::DocumentHit,
    score: f32,
) -> serde_json::Value {
    serde_json::json!({
        "meta": hit.meta,
        "chunk_id": hit.chunk_id,
        "location": hit.location,
        "snippet": hit.snippet,
        "matched_in_title": hit.matched_in_title,
        "score": score,
    })
}

/// 手动扫描（§89）：只写本地索引库；模型无法触发本命令。
#[tauri::command]
fn documents_scan(
    state: State<'_, AppState>,
    root_id: Option<String>,
) -> Result<Vec<devtoolbox_application::documents::IndexReport>, CommandError> {
    let settings = state.knowledge.settings();
    let started = std::time::Instant::now();
    let roots: Vec<devtoolbox_core::files::KnowledgeRoot> = settings
        .effective_document_roots()
        .into_iter()
        .filter(|root| root.enabled)
        .filter(|root| root_id.as_deref().is_none_or(|id| id == root.id))
        .collect();
    if roots.is_empty() {
        return Err(CommandError {
            code: "documents_error",
            message: "未配置文档目录：请在 设置 → 知识 中添加允许目录".to_string(),
        });
    }
    let mut reports = Vec::new();
    for root in &roots {
        reports.push(
            state
                .knowledge
                .documents
                .index_root(root, &settings)
                .map_err(CommandError::from)?,
        );
    }
    state.knowledge.metrics.record_index(
        &reports,
        &[],
        started.elapsed().as_millis() as u64,
    );
    Ok(reports)
}

#[tauri::command]
fn documents_search(
    state: State<'_, AppState>,
    query: String,
    document_type: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, CommandError> {
    let document_type = match document_type.as_deref() {
        None => None,
        Some(raw) => Some(
            devtoolbox_core::documents::DocumentType::parse(raw).ok_or_else(|| CommandError {
                code: "documents_error",
                message: format!("未知文档类型：{raw}"),
            })?,
        ),
    };
    let started = std::time::Instant::now();
    let hits = state
        .knowledge
        .documents
        .search(&query, document_type, limit.unwrap_or(0))
        .map_err(CommandError::from)?;
    state.knowledge.metrics.record_tool(
        "documents.search",
        started.elapsed().as_millis() as u64,
    );
    let results = state
        .knowledge
        .documents
        .to_knowledge_results(&hits, &query);
    Ok(hits
        .iter()
        .zip(results.iter())
        .map(|(hit, result)| document_hit_json(hit, result.score))
        .collect())
}

#[tauri::command]
fn documents_get(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<devtoolbox_core::documents::DocumentMeta, CommandError> {
    state
        .knowledge
        .documents
        .get(&document_id)
        .map_err(CommandError::from)
}

#[derive(Debug, Default, Deserialize)]
struct DocumentReadRequestDto {
    document_id: String,
    #[serde(default)]
    chunk_id: Option<String>,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    max_chars: Option<usize>,
}

#[tauri::command]
fn documents_read(
    state: State<'_, AppState>,
    request: DocumentReadRequestDto,
) -> Result<devtoolbox_core::documents::DocumentReadResult, CommandError> {
    let read = devtoolbox_core::documents::DocumentReadRequest {
        chunk_id: request.chunk_id,
        section: request.section,
        offset: request.offset.unwrap_or(0),
        max_chars: request.max_chars.unwrap_or(2_000),
    };
    state
        .knowledge
        .documents
        .read(&request.document_id, &read)
        .map_err(CommandError::from)
}

#[tauri::command]
fn documents_recent(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_core::documents::DocumentMeta>, CommandError> {
    state
        .knowledge
        .documents
        .recent(limit.unwrap_or(0))
        .map_err(CommandError::from)
}

#[derive(Debug, Serialize)]
struct FileStatusDto {
    roots: Vec<devtoolbox_core::files::KnowledgeRoot>,
    configured: bool,
    files: usize,
    text_files: usize,
    binary_files: usize,
    restricted: usize,
    failed: usize,
}

#[tauri::command]
fn files_status(state: State<'_, AppState>) -> Result<FileStatusDto, CommandError> {
    let settings = state.knowledge.settings();
    let stats = state.knowledge.files.stats().map_err(CommandError::from)?;
    Ok(FileStatusDto {
        configured: settings.file_policy().is_configured(),
        roots: knowledge_roots(&state),
        files: stats.files,
        text_files: stats.text_files,
        binary_files: stats.binary_files,
        restricted: stats.restricted,
        failed: stats.failed,
    })
}

#[tauri::command]
fn files_scan(
    state: State<'_, AppState>,
    root_id: Option<String>,
) -> Result<Vec<devtoolbox_application::files::FileIndexReport>, CommandError> {
    let settings = state.knowledge.settings();
    if !settings.file_policy().is_configured() {
        return Err(CommandError {
            code: "files_error",
            message: "未配置允许目录：请在 设置 → 知识 中添加允许目录".to_string(),
        });
    }
    let started = std::time::Instant::now();
    let policy = settings.file_policy();
    let roots: Vec<devtoolbox_core::files::KnowledgeRoot> = policy
        .enabled_roots()
        .into_iter()
        .filter(|root| root_id.as_deref().is_none_or(|id| id == root.id))
        .cloned()
        .collect();
    let mut reports = Vec::new();
    for root in &roots {
        let mut single = settings.clone();
        single.file_roots = vec![root.clone()];
        reports.extend(
            state
                .knowledge
                .files
                .index_configured_roots(&single)
                .map_err(CommandError::from)?,
        );
    }
    state
        .knowledge
        .metrics
        .record_index(&[], &reports, started.elapsed().as_millis() as u64);
    Ok(reports)
}

#[tauri::command]
fn files_search(
    state: State<'_, AppState>,
    query: Option<String>,
    extension: Option<String>,
    root_id: Option<String>,
    modified_after: Option<i64>,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_core::files::FileMetadata>, CommandError> {
    let settings = state.knowledge.settings();
    let spec = devtoolbox_core::files::FileQuery {
        query: query.unwrap_or_default(),
        extension,
        root_id,
        modified_after,
        limit: limit.unwrap_or(0),
        include_restricted: false,
    };
    let started = std::time::Instant::now();
    let entries = state
        .knowledge
        .files
        .search(&settings, &spec)
        .map_err(CommandError::from)?;
    state
        .knowledge
        .metrics
        .record_tool("files.search", started.elapsed().as_millis() as u64);
    Ok(entries)
}

#[tauri::command]
fn files_metadata(
    state: State<'_, AppState>,
    target: String,
) -> Result<devtoolbox_core::files::FileMetadata, CommandError> {
    let settings = state.knowledge.settings();
    state
        .knowledge
        .files
        .metadata(&settings, &target)
        .map_err(CommandError::from)
}

#[tauri::command]
fn files_read_text(
    state: State<'_, AppState>,
    target: String,
    max_chars: Option<usize>,
) -> Result<devtoolbox_application::files::FileReadResult, CommandError> {
    let settings = state.knowledge.settings();
    state
        .knowledge
        .files
        .read_text(&settings, &target, max_chars.unwrap_or(0))
        .map_err(CommandError::from)
}

#[derive(Debug, Serialize)]
struct FileOpenDto {
    file: devtoolbox_core::files::FileMetadata,
    action: devtoolbox_core::personal_ai::Action,
}

/// 返回 `OpenFile` Action（§46）：**backend 不执行 shell**，由前端决定如何打开。
#[tauri::command]
fn files_open(state: State<'_, AppState>, target: String) -> Result<FileOpenDto, CommandError> {
    let settings = state.knowledge.settings();
    let (file, action) = state
        .knowledge
        .files
        .open_action(&settings, &target)
        .map_err(CommandError::from)?;
    Ok(FileOpenDto { file, action })
}

#[tauri::command]
fn files_recent(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_core::files::FileMetadata>, CommandError> {
    state
        .knowledge
        .files
        .recent(limit.unwrap_or(0))
        .map_err(CommandError::from)
}

#[derive(Debug, Serialize)]
struct KnowledgeStatusDto {
    sources: Vec<String>,
    budget: devtoolbox_core::knowledge::KnowledgeBudget,
    metrics: devtoolbox_application::knowledge::KnowledgeMetricsSnapshot,
    configured_roots: Vec<devtoolbox_core::files::KnowledgeRoot>,
}

#[tauri::command]
fn knowledge_status(state: State<'_, AppState>) -> Result<KnowledgeStatusDto, CommandError> {
    Ok(KnowledgeStatusDto {
        sources: state
            .knowledge
            .retrieval
            .kinds()
            .into_iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        budget: state.knowledge.retrieval.budget(),
        metrics: state.knowledge.metrics.snapshot(),
        configured_roots: knowledge_roots(&state),
    })
}

// ---------------------------------------------------------------------------
// History Enrichment（V5 Gate 4）：按需 AI 富化命令
// ---------------------------------------------------------------------------

/// 单个 section 状态（前端「AI 解读」区。
#[derive(Debug, Serialize)]
struct EnrichmentSectionState {
    section: String,
    state: String,
}

fn enrichment_error(message: String) -> CommandError {
    CommandError {
        code: "history_enrichment_error",
        message,
    }
}

/// 解析 section 字符串为域枚举（非法 → 受控错误）。
fn parse_section(section: &str) -> Result<EnrichmentSection, CommandError> {
    match section {
        "overview" => Ok(EnrichmentSection::Overview),
        "background" => Ok(EnrichmentSection::Background),
        "impact" => Ok(EnrichmentSection::Impact),
        other => Err(CommandError {
            code: "history_enrichment_invalid_section",
            message: format!("未知富化 section: {other}"),
        }),
    }
}

fn parse_enrichment_key(
    entity_type: &str,
    entity_id: &str,
    section: &str,
    locale: &str,
) -> Result<EnrichmentKey, CommandError> {
    if entity_type != "event" {
        return Err(CommandError {
            code: "history_enrichment_invalid_entity",
            message: format!("富化目前仅支持 event 实体，收到: {entity_type}"),
        });
    }
    if entity_id.trim().is_empty() {
        return Err(CommandError {
            code: "history_enrichment_invalid_entity",
            message: "entity_id 为空".to_string(),
        });
    }
    Ok(EnrichmentKey::new(
        "event",
        entity_id,
        parse_section(section)?,
        locale,
    ))
}

/// 各 section 状态（首次渲染只读；不触发搜索/生成）。
#[tauri::command]
fn history_enrichment_state(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    locale: String,
) -> Result<Vec<EnrichmentSectionState>, CommandError> {
    state
        .history_enrichment
        .sections(&entity_type, &entity_id, &locale)
        .map(|sections| {
            sections
                .into_iter()
                .map(|(section, state)| EnrichmentSectionState {
                    section: section.as_str().to_string(),
                    state: enrichment_state_text(state).to_string(),
                })
                .collect()
        })
        .map_err(enrichment_error)
}

fn enrichment_state_text(
    state: devtoolbox_core::history_enrichment::EnrichmentState,
) -> &'static str {
    use devtoolbox_core::history_enrichment::EnrichmentState as S;
    match state {
        S::Missing => "MISSING",
        S::Generating => "GENERATING",
        S::Ready => "READY",
        S::Stale => "STALE",
        S::Failed => "FAILED",
        S::Reviewed => "REVIEWED",
    }
}

/// 单 section 视图（只读：含 payload/metadata/error；不触发生成）。
#[tauri::command]
fn history_enrichment_view(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    section: String,
    locale: String,
) -> Result<EnrichmentView, CommandError> {
    let key = parse_enrichment_key(&entity_type, &entity_id, &section, &locale)?;
    state.history_enrichment.get(&key).map_err(enrichment_error)
}

/// 按需生成（含跨调用单飞；已在生成 → GENERATING）。
#[tauri::command]
async fn history_enrichment_ensure(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    section: String,
    locale: String,
) -> Result<EnrichmentView, CommandError> {
    let key = parse_enrichment_key(&entity_type, &entity_id, &section, &locale)?;
    state
        .history_enrichment
        .ensure(&key)
        .await
        .map_err(enrichment_error)
}

/// 手动重新整理（Reviewed 也允许 → 新 revision 候选，不覆盖审定内容）。
#[tauri::command]
async fn history_enrichment_refresh(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    section: String,
    locale: String,
) -> Result<EnrichmentView, CommandError> {
    let key = parse_enrichment_key(&entity_type, &entity_id, &section, &locale)?;
    state
        .history_enrichment
        .refresh(&key)
        .await
        .map_err(enrichment_error)
}

/// 人工审定（automatic refresh 之后跳过该 section）。
#[tauri::command]
fn history_enrichment_review(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    section: String,
    locale: String,
) -> Result<(), CommandError> {
    let key = parse_enrichment_key(&entity_type, &entity_id, &section, &locale)?;
    state
        .history_enrichment
        .mark_reviewed(&key)
        .map_err(enrichment_error)
}

/// 应用入口。前端需要的权限被限制在文件选择器、command API 与打开原文链接；
/// 不暴露任意 shell 执行能力。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_directory = project_config_directory(app.handle())
                .map_err(|error| std::io::Error::other(error.message))?;
            let store = FeedRepository::open(config_directory.join("dashboard.db"))
                .expect("open rss database");
            let travel_store = TravelStore::open(config_directory.join("travel.db"))
                .expect("open travel database");
            let travel_store_shared = Arc::new(Mutex::new(travel_store));
            let history_duckdb = HistoryDuckDbRepository::open(
                semantic_history_path(app.handle())
                    .map_err(|error| std::io::Error::other(error.message))?,
            )
            .expect("open history semantic database");
            let language_store = LanguageStore::open(config_directory.join("language.db"))
                .expect("open language database");
            let geography_store = GeographyStore::open(config_directory.join("geography.db"))
                .expect("open geography database");
            let language_store_shared = Arc::new(Mutex::new(language_store));
            let geography_store_shared = Arc::new(Mutex::new(geography_store));
            let client = feed_client().expect("build http client");
            let rss_repository: Arc<dyn RssRepositoryPort> = Arc::new(
                composition::RssRepositoryAdapter::new(Arc::new(Mutex::new(store))),
            );
            let history_repo = Arc::new(history_duckdb);
            let history_port: Arc<dyn devtoolbox_application::history::HistoryQueryPort> = Arc::new(
                history_query::HistoryQueryAdapter::new(Arc::clone(&history_repo)),
            );
            // 富化设置读取器：每次调用读最新 settings.json（同 travel 模式）。
            let enrichment_settings: history_enrichment::SettingsLoader = {
                let handle = app.handle().clone();
                Arc::new(move || -> Result<AppSettings, String> {
                    let store = settings_store(&handle).map_err(|error| error.message.clone())?;
                    load_settings(&composition::SettingsStoreAdapter::new(store))
                        .map_err(|error| error.to_string())
                })
            };
            let history_enrichment = history_enrichment::build_runner(
                client.clone(),
                enrichment_settings.clone(),
                &config_directory,
                Arc::clone(&history_port),
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            let travel_ai: Arc<dyn devtoolbox_application::travel::TravelAiPort> =
                Arc::new(travel_ai::TravelAiAdapter::new(
                    client.clone(),
                    enrichment_settings.clone(),
                    Arc::clone(&travel_store_shared),
                ));
            // Geography / Language 模块端口（V5 Gate 6/7）：复用既有 store 适配器。
            let geography_port: Arc<
                dyn devtoolbox_application::geography::GeographyQueryPort + Send + Sync,
            > = Arc::new(geography_query::GeographyQueryAdapter::new(Arc::clone(
                &geography_store_shared,
            )));
            let language_port: Arc<dyn devtoolbox_application::language::LanguageStorePort> =
                Arc::new(composition::LanguageStoreAdapter::new(Arc::clone(
                    &language_store_shared,
                )));
            // V6 Personal Knowledge：三个索引库 + 三个域服务 + 统一检索服务。
            let knowledge = knowledge::KnowledgeRuntime::build(
                &config_directory,
                Arc::clone(&enrichment_settings),
                devtoolbox_core::knowledge::KnowledgeBudget::default(),
            )
            .map_err(std::io::Error::other)?;
            // 启动轻量同步（§89）：未配置允许根 → 空操作；失败只记录，不阻塞启动。
            let sync_notes = knowledge.startup_sync();
            for note in &sync_notes {
                eprintln!("[knowledge] startup sync: {note}");
            }
            let knowledge = Arc::new(knowledge);

            // language.explain 的可选 LLM 增强（setup 时读取一次配置；未配置 → None 确定性降级）。
            let language_llm_plug: Option<
                Arc<dyn devtoolbox_core::personal_ai::ChatModelProvider>,
            > = (enrichment_settings)()
                .ok()
                .map(|settings| settings.ai)
                .filter(|ai| ai.is_configured())
                .map(|ai| personal_ai::build_provider(client.clone(), &ai));
            app.manage(AppState {
                rss_repository,
                rss_fetcher: composition::FeedFetcherAdapter::new(client.clone()),
                travel_store: Arc::clone(&travel_store_shared),
                travel_registry: TravelSessionRegistry::new(),
                history_duckdb: Arc::clone(&history_repo),
                language_store: Arc::clone(&language_store_shared),
                geography_store: Arc::clone(&geography_store_shared),
                client,
                ai: personal_ai::build_hub(
                    history_repo,
                    Some(Arc::clone(&history_enrichment)),
                    travel_ai,
                    geography_port,
                    language_port,
                    language_llm_plug,
                    &knowledge,
                ),
                ai_session: Arc::new(InMemorySessionStore::new()),
                history_enrichment,
                knowledge,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_document,
            write_document,
            list_workspace,
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
            history_semantic_home,
            history_semantic_period,
            history_semantic_story,
            history_semantic_event,
            history_semantic_person,
            history_semantic_work,
            history_semantic_search,
            geography_home,
            geography_search,
            geography_detail,
            geography_toggle_favorite,
            personal_ai_status,
            personal_ai_chat,
            memory_status,
            memory_list,
            memory_get,
            memory_save,
            memory_confirm,
            memory_reject,
            memory_archive,
            memory_update,
            documents_status,
            documents_scan,
            documents_search,
            documents_get,
            documents_read,
            documents_recent,
            files_status,
            files_scan,
            files_search,
            files_metadata,
            files_read_text,
            files_open,
            files_recent,
            knowledge_status,
            history_enrichment_state,
            history_enrichment_view,
            history_enrichment_ensure,
            history_enrichment_refresh,
            history_enrichment_review,
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
            language_install_starter,
            language_speaking_feedback
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application event loop failed");
}
