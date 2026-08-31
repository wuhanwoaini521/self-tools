//! Travel 用例编排（需求 #二十一）：`TravelResearchService` 主流程。
//!
//! ```text
//! research_city()
//!   → plan_queries()        （TravelQueryPlanner，LLM 可选扩展）
//!   → search()              （Provider 链 fallback + 24h 缓存）
//!   → fetch_documents()     （并发抓取，失败保留 snippet → Partial Success）
//!   → extract_facts()       （LLM 抽取；未配置 LLM 则跳过）
//!   → rank_sources()        （SourceAuthority + 综合评分排序）
//!   → validate_facts()      （多源验证 / 冲突检测）
//!   → generate_city_guide() （LLM 结构化 JSON → 合并已验证事实；失败/无 LLM → 降级版）
//!   → save_city_guide()     （SQLite 缓存 24h）
//! ```
//!
//! 原则（需求 # 核心思想）：
//! - AI 整理信息，不凭空提供信息；缺失字段保持为空并写入 notes，绝不编造；
//! - 单个 Provider / 网页 / LLM 失败不阻塞整体 —— Partial Success；
//! - 网络阶段与存储阶段分离（rusqlite Connection 非 Sync，短锁不跨 await）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use devtoolbox_core::travel::{
    AccommodationArea, Attraction, CityGuide, CityInfo, ContentState, FactCategory, Food,
    GuideMeta, QueryCategory, QueryTask, ResearchPhase, SearchResult, SourceLevel, StepStatus,
    TravelDocument, TravelFact, TravelQueryInput, TravelQueryPlanner, TravelResearchEvent,
    TravelSource, TravelTip, TravelWarning, VerifiedFact, VerifiedValue, dedup_facts,
    dedup_search_results, host_of, parse_facts_json, parse_guide_json, rate_source, verify_facts,
};
use devtoolbox_infrastructure::{
    InfrastructureError, LlmProvider, SearchOptions, SearchProvider, TravelDataProvider,
    TravelDataRequest, TravelStore, WebFetcher, now_unix,
};

use crate::ApplicationError;

/// 每次搜索期望的结果数（Provider 尽力而为）。
const RESULTS_PER_QUERY: usize = 8;
/// 参与抓取的最大结果数（控制并发与耗时）。
const MAX_FETCH_CANDIDATES: usize = 20;
/// 抓取并发上限：分组分批执行。
const FETCH_CONCURRENCY: usize = 6;

/// 旅行研究请求（来自前端）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelResearchRequest {
    pub city: String,
    pub days: u8,
    pub month: Option<u32>,
    pub preferences: Vec<String>,
    /// 跳过攻略缓存，强制重新研究。
    pub force: bool,
}

/// Travel 研究服务。依赖全部注入，便于测试替换为 Mock。
pub struct TravelResearchService {
    search_providers: Vec<Box<dyn SearchProvider>>,
    fetcher: Box<dyn WebFetcher>,
    llm: Option<Box<dyn LlmProvider>>,
    data_providers: Vec<Box<dyn TravelDataProvider>>,
    store: Arc<Mutex<TravelStore>>,
}

impl TravelResearchService {
    /// 构造服务。`llm` 与 `data_providers` 可为空（未配置时模块照常工作）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        search_providers: Vec<Box<dyn SearchProvider>>,
        fetcher: Box<dyn WebFetcher>,
        llm: Option<Box<dyn LlmProvider>>,
        data_providers: Vec<Box<dyn TravelDataProvider>>,
        store: Arc<Mutex<TravelStore>>,
    ) -> Self {
        Self {
            search_providers,
            fetcher,
            llm,
            data_providers,
            store,
        }
    }

    /// 主流程：研究一个城市，产出结构化攻略。
    /// `progress` 接收研究进度事件（由调用方收集，例如写入会话供前端轮询）。
    pub async fn research_city(
        &self,
        request: &TravelResearchRequest,
        progress: &(dyn Fn(TravelResearchEvent) + Sync),
    ) -> Result<CityGuide, ApplicationError> {
        let now = now_unix();
        let mut seq = 0_u64;
        let mut notes: Vec<String> = Vec::new();
        let city = request.city.trim().to_string();
        if city.is_empty() {
            return Err(ApplicationError::EmptyCity);
        }
        let mut emit = |phase, status, message: String| {
            seq += 1;
            progress(TravelResearchEvent {
                phase,
                status,
                message,
                seq,
            });
        };

        // 1. 识别城市
        emit(
            ResearchPhase::IdentifyCity,
            StepStatus::InProgress,
            format!("识别城市：{city}"),
        );

        // 2. 攻略缓存（24h；force 跳过）。缓存损坏视为 miss，不阻塞研究。
        if !request.force {
            let cached = {
                let store = self.store.lock().expect("travel store poisoned");
                store.get_guide(&city, request.days, now).ok().flatten()
            };
            if let Some(guide) = cached {
                emit(
                    ResearchPhase::IdentifyCity,
                    StepStatus::Done,
                    format!("命中缓存攻略（{city}，{days} 天）", days = request.days),
                );
                emit(
                    ResearchPhase::SaveGuide,
                    StepStatus::Done,
                    "直接返回缓存结果".to_string(),
                );
                return Ok(guide);
            }
        }
        emit(
            ResearchPhase::IdentifyCity,
            StepStatus::Done,
            format!("{city} 已确认"),
        );

        // 3. 查询规划
        emit(
            ResearchPhase::PlanQueries,
            StepStatus::InProgress,
            format!("规划搜索任务（{city}）"),
        );
        let input = TravelQueryInput {
            city: city.clone(),
            days: request.days.clamp(1, 7),
            month: request.month,
            preferences: request.preferences.clone(),
        };
        let mut tasks = TravelQueryPlanner::plan(&input);
        if let Some(extra) = self.llm_expand_queries(&city, &input, &mut emit).await {
            tasks.extend(extra);
        }
        if tasks.is_empty() {
            return Err(ApplicationError::EmptyCity);
        }
        emit(
            ResearchPhase::PlanQueries,
            StepStatus::Done,
            format!("已生成 {} 个搜索任务", tasks.len()),
        );

        // 4. 搜索（Provider 链 fallback + 24h 缓存；单个任务失败不阻塞）
        emit(ResearchPhase::Search, StepStatus::InProgress, String::new());
        let mut all_results: Vec<SearchResult> = Vec::new();
        for task in &tasks {
            emit(
                ResearchPhase::Search,
                StepStatus::InProgress,
                format!("搜索{}：{}", task.category.label(), task.query),
            );
            match self.search_query(&task.query, now).await {
                Ok(results) => {
                    emit(
                        ResearchPhase::Search,
                        StepStatus::Done,
                        format!("「{}」返回 {} 条结果", task.query, results.len()),
                    );
                    all_results.extend(results);
                }
                Err(error) => {
                    emit(
                        ResearchPhase::Search,
                        StepStatus::Failed,
                        format!("「{}」搜索失败：{error}", task.query),
                    );
                    notes.push(format!("搜索「{}」失败：{error}", task.query));
                }
            }
        }
        if all_results.is_empty() {
            return Err(ApplicationError::TravelFailed(format!(
                "所有搜索请求均未返回结果（{city}）"
            )));
        }

        // 5. 去重 + 可信度排序（需求 #七 / #八）
        emit(
            ResearchPhase::RankSources,
            StepStatus::InProgress,
            format!("去重并排序 {} 条搜索结果", all_results.len()),
        );
        let deduped = dedup_search_results(all_results);
        let mut rated: Vec<(SearchResult, TravelSource)> = deduped
            .into_iter()
            .map(|result| {
                let source = rate_source(&city, &result);
                (result, source)
            })
            .collect();
        rated.sort_by(|a, b| {
            b.1.score
                .partial_cmp(&a.1.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rated.truncate(MAX_FETCH_CANDIDATES);
        emit(
            ResearchPhase::RankSources,
            StepStatus::Done,
            format!("保留 {} 个高可信来源", rated.len()),
        );

        // 6. 抓取文档（并发分组；失败保留 snippet）
        emit(
            ResearchPhase::FetchDocuments,
            StepStatus::InProgress,
            format!("抓取 {} 个页面", rated.len()),
        );
        let documents = self.fetch_documents(&rated, now).await;
        let fetched_full = documents
            .iter()
            .filter(|doc| doc.state == ContentState::Full)
            .count();
        let snippet_only = documents
            .iter()
            .filter(|doc| doc.state == ContentState::SnippetOnly)
            .count();
        emit(
            ResearchPhase::FetchDocuments,
            StepStatus::Done,
            format!("抓取完成：全文 {fetched_full} 个，仅摘要 {snippet_only} 个"),
        );
        if snippet_only > 0 {
            notes.push(format!(
                "{snippet_only} 个来源未能抓取全文，仅采用搜索摘要（低可信）"
            ));
        }

        // 7. 事实提取（LLM；未配置 / 失败 → 跳过，绝不编造）
        emit(
            ResearchPhase::ExtractFacts,
            StepStatus::InProgress,
            String::new(),
        );
        let llm_used = self.llm.is_some();
        let mut facts: Vec<TravelFact> = Vec::new();
        let mut fact_failures = 0_usize;
        if self.llm.is_some() {
            for document in &documents {
                emit(
                    ResearchPhase::ExtractFacts,
                    StepStatus::InProgress,
                    format!("提取事实：{}", document.title),
                );
                match self.extract_facts_from_document(document, now).await {
                    Ok(extracted) if !extracted.is_empty() => {
                        let count = extracted.len();
                        facts.extend(extracted);
                        emit(
                            ResearchPhase::ExtractFacts,
                            StepStatus::Done,
                            format!("{} 提取到 {count} 条事实", document.title),
                        );
                    }
                    Ok(_) => {
                        emit(
                            ResearchPhase::ExtractFacts,
                            StepStatus::Skipped,
                            format!("{} 未提取到事实", document.title),
                        );
                    }
                    Err(error) => {
                        fact_failures += 1;
                        emit(
                            ResearchPhase::ExtractFacts,
                            StepStatus::Failed,
                            format!("{} 事实提取失败：{error}", document.title),
                        );
                    }
                }
            }
        } else {
            emit(
                ResearchPhase::ExtractFacts,
                StepStatus::Skipped,
                "未配置 LLM，跳过事实提取（仍展示来源与基础信息）".to_string(),
            );
            notes.push("未配置 LLM：跳过事实提取与摘要生成".to_string());
        }
        if fact_failures > 0 {
            notes.push(format!("{fact_failures} 个文档的事实提取失败，已跳过"));
        }
        let mut facts = dedup_facts(facts);
        emit(
            ResearchPhase::ExtractFacts,
            StepStatus::Done,
            format!("去重后共 {} 条事实", facts.len()),
        );

        // 8. 结构化数据 Provider（高德 POI / 和风天气；Key 未配置时为空并跳过，需求 #十二）
        emit(
            ResearchPhase::DataSources,
            StepStatus::InProgress,
            String::new(),
        );
        let mut data_facts: Vec<TravelFact> = Vec::new();
        let mut data_sources: Vec<TravelSource> = Vec::new();
        if self.data_providers.is_empty() {
            emit(
                ResearchPhase::DataSources,
                StepStatus::Skipped,
                "未配置数据源 Key（高德 / 和风），跳过".to_string(),
            );
            notes.push(
                "未配置数据源 Key：只使用网页搜索信息（高德 POI / 和风天气 可增强）".to_string(),
            );
        }
        for kind in ["poi", "weather"] {
            for provider in &self.data_providers {
                match provider
                    .fetch(TravelDataRequest {
                        city: city.clone(),
                        kind,
                    })
                    .await
                {
                    Ok(items) if !items.is_empty() => {
                        let count = items.len();
                        emit(
                            ResearchPhase::DataSources,
                            StepStatus::Done,
                            format!("{} {}：获取到 {count} 条数据", provider.name(), kind),
                        );
                        notes.push(format!("已接入数据源「{}」（{}）", provider.name(), kind));
                        data_facts.extend(items);
                        // 数据源作为可信来源进入 Sources（供权威权重匹配与展示）
                        data_sources.push(TravelSource {
                            url: data_source_url(provider.name()),
                            title: format!("{}（{}）", data_source_label(provider.name()), city),
                            host: host_of(&data_source_url(provider.name())),
                            level: SourceLevel::A,
                            state: ContentState::Full,
                            published_at: None,
                            fetched_at: now,
                            score: 0.85,
                        });
                    }
                    Ok(_) => {}
                    Err(error) => {
                        emit(
                            ResearchPhase::DataSources,
                            StepStatus::Failed,
                            format!("数据源「{}」失败：{error}", provider.name()),
                        );
                        notes.push(format!("数据源「{}」失败：{error}", provider.name()));
                    }
                }
            }
        }
        emit(
            ResearchPhase::DataSources,
            StepStatus::Done,
            format!("结构化数据共 {} 条", data_facts.len()),
        );
        facts.extend(dedup_facts(data_facts));

        // 9. 来源评级（合并抓取状态 + 数据源 → TravelSource 列表）
        emit(
            ResearchPhase::RankSources,
            StepStatus::InProgress,
            String::new(),
        );
        let mut sources = self.build_sources(&rated, &documents);
        sources.extend(data_sources);
        emit(
            ResearchPhase::RankSources,
            StepStatus::Done,
            format!("共 {} 个信息源", sources.len()),
        );

        // 10. 多源验证 / 冲突检测
        emit(
            ResearchPhase::ValidateFacts,
            StepStatus::InProgress,
            format!("验证 {} 条事实", facts.len()),
        );
        let levels: HashMap<String, SourceLevel> = sources
            .iter()
            .map(|source| (source.url.clone(), source.level))
            .collect();
        let verified = verify_facts(&facts, &levels);
        let conflict_count = verified.iter().filter(|v| v.has_conflict).count();
        emit(
            ResearchPhase::ValidateFacts,
            StepStatus::Done,
            format!("验证完成（{} 项存在多版本信息）", conflict_count),
        );
        if conflict_count > 0 {
            notes.push(format!(
                "{conflict_count} 项信息存在多个来源版本，已按权威优先处理"
            ));
        }

        // 11. 生成结构化攻略
        emit(
            ResearchPhase::GenerateGuide,
            StepStatus::InProgress,
            format!("整理攻略（{city}）"),
        );
        let guide = self
            .generate_guide(request, &city, &sources, &verified, llm_used, now, notes)
            .await;
        emit(
            ResearchPhase::GenerateGuide,
            StepStatus::Done,
            "攻略整理完成".to_string(),
        );

        // 12. 保存（24h 缓存）
        emit(
            ResearchPhase::SaveGuide,
            StepStatus::InProgress,
            "保存到本地缓存".to_string(),
        );
        let stored = {
            let store = self.store.lock().expect("travel store poisoned");
            store.upsert_guide(&guide, now).map_err(travel)?
        };
        emit(
            ResearchPhase::SaveGuide,
            StepStatus::Done,
            format!("已保存（{city}，{} 天）", request.days),
        );
        Ok(stored)
    }

    /// 单查询搜索：先查 24h 缓存（损坏视为 miss），再按 Provider 链顺序 fallback。
    async fn search_query(
        &self,
        query: &str,
        now: i64,
    ) -> Result<Vec<SearchResult>, ApplicationError> {
        if let Some(cached) = {
            let store = self.store.lock().expect("travel store poisoned");
            store.get_search_results(query, now).ok().flatten()
        } {
            return Ok(cached);
        }
        if self.search_providers.is_empty() {
            return Err(travel(InfrastructureError::TravelSearch(
                "no search provider configured".to_string(),
            )));
        }
        let options = SearchOptions {
            count: RESULTS_PER_QUERY,
        };
        let mut last_error: Option<InfrastructureError> = None;
        for provider in &self.search_providers {
            match provider.search(query, options).await {
                Ok(results) if !results.is_empty() => {
                    let store = self.store.lock().expect("travel store poisoned");
                    store
                        .put_search_results(query, &results, now)
                        .map_err(travel)?;
                    return Ok(results);
                }
                Ok(_) => {
                    last_error = Some(InfrastructureError::TravelSearch(format!(
                        "provider {} returned no results",
                        provider.name()
                    )));
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(travel(last_error.unwrap_or_else(|| {
            InfrastructureError::TravelSearch("all providers failed".to_string())
        })))
    }

    /// 并发抓取文档（分组限流）。失败 → SnippetOnly（保留搜索摘要），不中断整体。
    async fn fetch_documents(
        &self,
        rated: &[(SearchResult, TravelSource)],
        now: i64,
    ) -> Vec<TravelDocument> {
        let mut documents = Vec::with_capacity(rated.len());
        for chunk in rated.chunks(FETCH_CONCURRENCY) {
            let fetched = join_all(chunk.iter().map(|(result, _)| self.fetch_one(result, now)))
                .await
                .into_iter()
                .collect::<Vec<_>>();
            documents.extend(fetched);
        }
        documents
    }

    async fn fetch_one(&self, result: &SearchResult, now: i64) -> TravelDocument {
        if let Some(cached) = {
            let store = self.store.lock().expect("travel store poisoned");
            store
                .get_document(&result.url, now)
                .map_err(travel)
                .ok()
                .flatten()
        } {
            return cached;
        }
        match self.fetcher.fetch(&result.url).await {
            Ok(mut document) => {
                if let Some(published) = result.published_at {
                    document.published_at = Some(published);
                }
                let store = self.store.lock().expect("travel store poisoned");
                let _ = store.put_document(&document, now);
                document
            }
            Err(_) => TravelDocument {
                url: result.url.clone(),
                title: result.title.clone(),
                state: ContentState::SnippetOnly,
                content: None,
                snippet: Some(result.snippet.clone()),
                published_at: result.published_at,
                fetched_at: now,
                provider: Some(result.provider.clone()),
            },
        }
    }

    /// LLM 抽取单个文档的事实。解析失败 / 调用失败 → Err（上层按文档跳过）。
    async fn extract_facts_from_document(
        &self,
        document: &TravelDocument,
        now: i64,
    ) -> Result<Vec<TravelFact>, String> {
        let llm = self
            .llm
            .as_deref()
            .ok_or_else(|| "llm not configured".to_string())?;
        let text = document.readable_text();
        let text = truncate(&text, 3000);
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let raw = llm
            .complete(FACT_SYSTEM_PROMPT, &fact_user_prompt(&text))
            .await
            .map_err(|error| error.to_string())?;
        parse_facts_json(&raw, &document.url, now).map_err(|error| error.to_string())
    }

    /// LLM 扩展查询（可选；失败静默，不影响主流程）。
    async fn llm_expand_queries(
        &self,
        city: &str,
        input: &TravelQueryInput,
        emit: &mut (dyn FnMut(ResearchPhase, StepStatus, String) + Send),
    ) -> Option<Vec<QueryTask>> {
        let llm = self.llm.as_deref()?;
        let raw = llm
            .complete(QUERY_SYSTEM_PROMPT, &query_user_prompt(city, input))
            .await
            .ok()?;
        let raw_queries = parse_query_list(&raw);
        if raw_queries.is_empty() {
            return None;
        }
        emit(
            ResearchPhase::PlanQueries,
            StepStatus::Done,
            format!("LLM 追加 {} 个搜索主题", raw_queries.len()),
        );
        Some(
            raw_queries
                .into_iter()
                .map(|query| QueryTask {
                    category: QueryCategory::Activities,
                    query,
                })
                .collect(),
        )
    }

    /// 组装信息源列表（评级 + 抓取状态）。
    fn build_sources(
        &self,
        rated: &[(SearchResult, TravelSource)],
        documents: &[TravelDocument],
    ) -> Vec<TravelSource> {
        let state_by_url: HashMap<&str, ContentState> = documents
            .iter()
            .map(|doc| (doc.url.as_str(), doc.state))
            .collect();
        rated
            .iter()
            .map(|(_, source)| {
                let mut source = source.clone();
                if let Some(state) = state_by_url.get(source.url.as_str()) {
                    source.state = *state;
                }
                source
            })
            .collect()
    }

    /// 生成攻略：LLM 结构化为优先；失败 / 未配置 → 降级版（仅真实搜索信息）。
    #[allow(clippy::too_many_arguments)]
    async fn generate_guide(
        &self,
        request: &TravelResearchRequest,
        city: &str,
        sources: &[TravelSource],
        verified: &[VerifiedFact],
        llm_used: bool,
        generated_at: i64,
        mut notes: Vec<String>,
    ) -> CityGuide {
        let days = request.days.clamp(1, 7);
        let mut guide = CityGuide {
            city: CityInfo {
                name: city.to_string(),
                ..CityInfo::default()
            },
            meta: GuideMeta {
                generated_at,
                updated_at: generated_at,
                days,
                llm_used,
                notes: Vec::new(),
            },
            ..CityGuide::default()
        };
        guide.sources = sources.to_vec();

        let llm_result = if let Some(llm) = self.llm.as_deref() {
            let facts_block = format_verified_facts(verified);
            let docs_block = format_documents(sources);
            let raw = llm
                .complete(
                    GUIDE_SYSTEM_PROMPT,
                    &guide_user_prompt(city, &input_brief(request), &facts_block, &docs_block),
                )
                .await;
            raw.map_err(|error| GuideGenError::Llm(error.to_string()))
                .and_then(|raw| {
                    parse_guide_json(&raw).map_err(|error| GuideGenError::Parse(error.to_string()))
                })
        } else {
            Err(GuideGenError::Unavailable)
        };

        let mut llm_ok = false;
        match llm_result {
            Ok(parsed) => {
                guide.city.name_en = parsed.city.name_en;
                guide.city.province = parsed.city.province;
                guide.city.country = parsed.city.country;
                guide.summary = parsed.summary;
                guide.highlights = parsed.highlights;
                guide.best_time = parsed.best_time;
                guide.districts = parsed.districts;
                guide.attractions = parsed.attractions;
                guide.foods = parsed.foods;
                guide.restaurants = parsed.restaurants;
                guide.transport = parsed.transport;
                guide.accommodation_areas = parsed.accommodation_areas;
                guide.itineraries = parsed.itineraries;
                guide.local_tips = parsed.local_tips;
                guide.warnings = parsed.warnings;
                llm_ok = true;
            }
            Err(error) => {
                notes.push(format!(
                    "LLM 攻略生成失败，已降级为“仅来源列表”模式：{error}"
                ));
                build_fallback_guide(&mut guide, city, sources);
            }
        }
        // 程序层合并已验证事实（LLM 与降级两条路径都执行）：
        // - 高德 POI 的景点/美食/住宿条目补进攻略；
        // - 和风天气进入「本地 Tips」；
        // - 开放时间/门票/预约挂到同名景点。
        merge_external_data(&mut guide, verified);
        guide.meta.llm_used = llm_used && llm_ok;
        guide.meta.notes = notes;
        guide
    }
}

/// 攻略生成的降级哨兵 / 错误（统一 LLM 失败与解析失败，让主流程走降级分支）。
#[derive(thiserror::Error, Debug)]
enum GuideGenError {
    #[error("llm error: {0}")]
    Llm(String),
    #[error("llm json parse error: {0}")]
    Parse(String),
    #[error("llm not configured")]
    Unavailable,
}

fn travel(source: InfrastructureError) -> ApplicationError {
    ApplicationError::Travel { source }
}

/// 数据源 Provider 名 → 其 Sources 展示地址（同时是事实 source_id，保证权威权重可匹配）。
fn data_source_url(provider_name: &str) -> String {
    match provider_name {
        "amap-poi" => devtoolbox_infrastructure::travel::data_provider::AMAP_SOURCE_URL.to_string(),
        "qweather" => {
            devtoolbox_infrastructure::travel::data_provider::QWEATHER_SOURCE_URL.to_string()
        }
        other => format!("https://data.{other}"),
    }
}

/// 数据源 Provider 名 → 人类可读标签。
fn data_source_label(provider_name: &str) -> &'static str {
    match provider_name {
        "amap-poi" => "高德地图 POI",
        "qweather" => "和风天气",
        _ => "结构化数据源",
    }
}

// ---------- Prompt 与纯函数（可单测） ----------

const FACT_SYSTEM_PROMPT: &str = "\
你是旅行信息事实抽取助手。你只能从给定的网页文本中抽取明确陈述的事实，\
不得推测、不得编造。将每条事实输出为一个 JSON 对象，整体输出 JSON 数组：\
[{\"category\": \"attraction|food|transport|accommodation|weather|opening_hours|ticket|reservation|activity|travel_tip|warning\", \
\"subject\": \"主体名（如：西湖）\", \"value\": \"事实内容（如：07:00-18:00）\", \"confidence\": 0.0-1.0}]。\
信息不确定时 confidence 降低；文本中没有明确事实时输出空数组 []。只输出 JSON。";

pub fn fact_user_prompt(text: &str) -> String {
    format!("请从下面网页文本中抽取旅行相关事实：\n\n{text}")
}

const QUERY_SYSTEM_PROMPT: &str = "\
你是中文旅行研究助手。根据城市与偏好，列出 3-6 个补充搜索主题（城市名必须拼入查询词）。\
只输出 JSON 字符串数组，如 [\"杭州 宋韵文化体验\", \"杭州 秋季桂花景点\"]。不要解释。";

pub fn query_user_prompt(city: &str, input: &TravelQueryInput) -> String {
    let mut brief = format!("城市：{city}；旅行天数：{} 天", input.days);
    if let Some(month) = input.month {
        brief.push_str(&format!("；出行月份：{month} 月"));
    }
    if !input.preferences.is_empty() {
        brief.push_str(&format!("；偏好：{}", input.preferences.join("、")));
    }
    format!("请为以下需求生成补充搜索主题：\n{brief}")
}

/// 解析 LLM 返回的查询列表（容错：剥围栏、取 JSON 数组或逐行读取）。
pub fn parse_query_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[')
        && let Ok(serde_json::Value::Array(items)) = serde_json::from_str(trimmed)
    {
        return items
            .into_iter()
            .filter_map(|item| item.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
    }
    trimmed
        .lines()
        .map(|line| {
            line.trim()
                .trim_matches(|c| matches!(c, '-' | '*' | ' ' | '\t' | '"' | '，' | ','))
                .to_string()
        })
        .filter(|line| !line.is_empty() && line.chars().count() >= 4)
        .collect()
}

const GUIDE_SYSTEM_PROMPT: &str = "\
你是中文旅行攻略整理助手。请仅基于给定的「已验证事实」与「信息源列表」整理结构化攻略，\
不得编造任何未出现在资料中的信息（包括具体价格、开放时间、地址）。信息不足的字段留空或省略。\
输出 JSON（字段名与结构见用户消息中的 schema）。“注意事项”只能来自 source 类型为 Warning 的事实。";

fn input_brief(request: &TravelResearchRequest) -> String {
    let mut brief = format!(
        "城市：{}；天数：{} 天",
        request.city.trim(),
        request.days.clamp(1, 7)
    );
    if let Some(month) = request.month {
        brief.push_str(&format!("；月份：{month} 月"));
    }
    if !request.preferences.is_empty() {
        brief.push_str(&format!("；偏好：{}", request.preferences.join("、")));
    }
    brief
}

pub fn guide_user_prompt(city: &str, brief: &str, facts_block: &str, docs_block: &str) -> String {
    format!(
        "目标城市：{city}\n需求：{brief}\n\n已验证事实：\n{facts_block}\n\n信息源（标题/等级/地址/抓取状态）：\n{docs_block}\n\n\
请输出如下 JSON 结构（缺失字段省略或为空数组）：\n\
{{\"summary\":\"120字以内概览\",\"highlights\":[\"亮点\"],\"best_time\":\"最佳时间\",\n\
\"districts\":[{{\"name\":\"区域\",\"note\":\"说明\",\"landmarks\":[\"地标\"]}}],\n\
\"attractions\":[{{\"name\":\"景点\",\"intro\":\"介绍\",\"area\":\"区域\",\"suggested_duration\":\"建议时长\",\
\"opening_hours\":{{\"value\":\"时间\",\"confidence\":\"high|medium|low\",\"verified_sources\":1,\"primary_source\":\"类型\",\"has_conflict\":false}},\
\"ticket\":{{\"value\":\"票价\",\"confidence\":\"high\",\"verified_sources\":1,\"primary_source\":\"类型\",\"has_conflict\":false}},\
\"tips\":[\"贴士\"],\"source_ids\":[\"来源URL\"]}}],\n\
\"foods\":[{{\"name\":\"美食\",\"dish_type\":\"类别\",\"intro\":\"介绍\",\"source_ids\":[]}}],\n\
\"restaurants\":[{{\"name\":\"店名\",\"area\":\"区域\",\"note\":\"说明\"}}],\n\
\"transport\":{{\"overview\":\"总览\",\"airport\":\"机场\",\"train_station\":\"高铁站\",\"metro\":\"地铁\",\"bus_taxi\":\"公交打车\",\"tips\":[]}},\n\
\"accommodation_areas\":[{{\"name\":\"区域\",\"area\":\"位置\",\"note\":\"适合谁\",\"budget\":\"价位\"}}],\n\
\"itineraries\":{{\"one_day\":{{\"day\":1,\"title\":\"主题\",\"stops\":[{{\"name\":\"地点\",\"note\":\"安排\"}}]}},\
\"two_days\":{{\"day\":2,...}},\"three_days\":{{\"day\":3,...}}}}（按需求天数提供对应条目）,\n\
\"local_tips\":[{{\"title\":\"标题\",\"text\":\"内容\"}}],\n\
\"warnings\":[{{\"title\":\"标题\",\"text\":\"内容\"}}]}}"
    )
}

/// 格式化已验证事实（供 LLM 参考）。
fn format_verified_facts(verified: &[VerifiedFact]) -> String {
    if verified.is_empty() {
        return "（无已验证事实）".to_string();
    }
    verified
        .iter()
        .map(|f| {
            format!(
                "- [{category}] {subject} = {value}（confidence={confidence}, 来源数={count}, 冲突={conflict}）",
                category = f.category.json_name(),
                subject = f.subject,
                value = f.value,
                confidence = f.confidence,
                count = f.verified_sources,
                conflict = f.has_conflict
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_documents(sources: &[TravelSource]) -> String {
    if sources.is_empty() {
        return "（无信息源）".to_string();
    }
    sources
        .iter()
        .map(|source| {
            format!(
                "- {title}（{level}/{state}，{host}，{url}）",
                title = source.title,
                level = source.level,
                state = source.state,
                host = source.host,
                url = source.url
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 把已验证事实合并回攻略（LLM 输出可能遗漏，程序层保证验证结果落地）。
/// 覆盖：高德 POI（景点/美食/住宿）、和风天气、开放时间/门票/预约挂景点。
fn merge_external_data(guide: &mut CityGuide, verified: &[VerifiedFact]) {
    for fact in verified {
        match fact.category {
            FactCategory::OpeningHours | FactCategory::Ticket | FactCategory::Reservation => {
                // 尝试挂到同名景点；找不到则不强行造条目
                apply_to_attraction(guide, &fact.subject, fact.category, fact);
            }
            FactCategory::Attraction
                if !guide
                    .attractions
                    .iter()
                    .any(|a| contains(&a.name, &fact.subject)) =>
            {
                guide.attractions.push(Attraction {
                    name: fact.subject.clone(),
                    intro: Some(fact.value.clone()),
                    ..Attraction::default()
                });
            }
            FactCategory::Food if !guide.foods.iter().any(|f| contains(&f.name, &fact.subject)) => {
                guide.foods.push(Food {
                    name: fact.subject.clone(),
                    intro: Some(fact.value.clone()),
                    ..Food::default()
                });
            }
            FactCategory::Accommodation
                if !guide
                    .accommodation_areas
                    .iter()
                    .any(|a| contains(&a.name, &fact.subject)) =>
            {
                guide.accommodation_areas.push(AccommodationArea {
                    name: fact.subject.clone(),
                    note: Some(fact.value.clone()),
                    ..AccommodationArea::default()
                });
            }
            FactCategory::Weather
                if !guide
                    .local_tips
                    .iter()
                    .any(|tip| tip.title.contains("天气")) =>
            {
                guide.local_tips.push(TravelTip {
                    title: format!("{}天气", fact.subject),
                    text: fact.value.clone(),
                });
            }
            _ => {}
        }
    }
}

fn apply_to_attraction(
    guide: &mut CityGuide,
    subject: &str,
    category: FactCategory,
    verified: &VerifiedFact,
) {
    let mut target: Option<&mut Attraction> = guide
        .attractions
        .iter_mut()
        .find(|a| contains(&a.name, subject));
    if target.is_none() {
        // 主体不是已知景点（如「西湖」「灵隐寺」开放时间）→ 尝试按关键词配对
        target = guide
            .attractions
            .iter_mut()
            .find(|a| contains(subject, &a.name));
    }
    if let Some(attraction) = target {
        let value = VerifiedValue {
            value: verified.value.clone(),
            confidence: verified.confidence.clone(),
            verified_sources: verified.verified_sources,
            primary_source: verified.primary_source.clone(),
            has_conflict: verified.has_conflict,
        };
        match category {
            FactCategory::OpeningHours => attraction.opening_hours = Some(value),
            FactCategory::Ticket => attraction.ticket = Some(value),
            FactCategory::Reservation => attraction.reservation = Some(value),
            _ => {}
        }
    }
}

fn contains(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.trim();
    let needle = needle.trim();
    !needle.is_empty() && (haystack.contains(needle) || needle.contains(haystack))
}

/// 降级版攻略：只使用真实搜索信息（标题 / 摘要 / 来源），缺失区留空并注明。
fn build_fallback_guide(guide: &mut CityGuide, _city: &str, sources: &[TravelSource]) {
    guide.summary = format!(
        "「{}」暂无 AI 整理；以下为基于搜索结果的原始信息来源（请以官方信息为准）。",
        guide.city.name
    );
    // 来源标题按域名归类为“候选条目”，标注为低可信参考
    for source in sources {
        let name = &source.title;
        let note = || {
            Some(format!(
                "来自 {}（{} 级来源，仅作参考）",
                source.host, source.level
            ))
        };
        if name.contains("美食") || name.contains("小吃") || name.contains("餐厅") {
            guide.foods.push(Food {
                name: name.clone(),
                intro: note(),
                ..Food::default()
            });
        } else if name.contains("酒店") || name.contains("住宿") || name.contains("民宿") {
            guide.accommodation_areas.push(AccommodationArea {
                name: name.clone(),
                note: note(),
                ..AccommodationArea::default()
            });
        } else if name.contains("交通")
            || name.contains("地铁")
            || name.contains("机场")
            || name.contains("车站")
        {
            guide
                .transport
                .tips
                .push(format!("{name} — 来源：{}", source.host));
        } else if name.contains("避坑") || name.contains("注意") {
            guide.warnings.push(TravelWarning {
                title: name.clone(),
                text: format!("来源：{}（请以官方公告为准）", source.host),
            });
        } else {
            guide.attractions.push(Attraction {
                name: name.clone(),
                intro: note(),
                ..Attraction::default()
            });
        }
        if guide.attractions.len() > 12 {
            break;
        }
    }
    if guide.foods.is_empty() {
        guide
            .meta
            .notes
            .push("美食信息不足，暂无可靠数据".to_string());
    }
    if guide.attractions.is_empty() {
        guide
            .meta
            .notes
            .push("景点信息不足，暂无可靠数据".to_string());
    }
    guide
        .meta
        .notes
        .push("当前为降级模式：LLM 未配置或不可用，仅展示原始来源".to_string());
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}
