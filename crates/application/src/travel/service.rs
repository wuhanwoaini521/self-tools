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
    AccommodationArea, Attraction, CityGuide, CityInfo, ContentState, EvidenceSummary,
    FactCategory, Food, GuideMeta, ItineraryDay, ItineraryStop, MapCoordinates, Place,
    QueryCategory, QueryTask, ResearchPhase, SearchIntentCategory, SearchResult, SourceLevel,
    SourceRankingContext, StepStatus, TravelDateRange, TravelDocument, TravelFact,
    TravelQueryInput, TravelQueryPlanner, TravelResearchEvent, TravelSource, TravelTip,
    VerifiedFact, VerifiedValue, WeatherDay, WeatherForecast, apply_quality_gate,
    dedup_entity_facts, dedup_facts, dedup_search_results, host_of, normalize_url,
    parse_facts_json, parse_guide_json, rate_source_for, verify_facts_with_states,
};
use devtoolbox_infrastructure::{
    InfrastructureError, LlmProvider, SearchOptions, SearchProvider, TravelDataProvider,
    TravelDataRequest, TravelRouteRequest, TravelStore, WebFetcher, now_unix,
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
    /// 可选的具体行程日期范围（ISO `YYYY-MM-DD`）。
    pub date_range: Option<TravelDateRange>,
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
        validate_date_range(request.date_range.as_ref())?;
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
        if !request.force && request.date_range.is_none() {
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
            date_range: request.date_range.clone(),
            preferences: request.preferences.clone(),
        };
        let mut tasks = TravelQueryPlanner::plan(&input);
        if let Some(extra) = self
            .llm_expand_queries(&city, &input, &tasks, &mut emit)
            .await
        {
            append_query_tasks(&mut tasks, extra, 12);
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
        let mut result_contexts: HashMap<String, (QueryCategory, String)> = HashMap::new();
        let mut result_counts: HashMap<QueryCategory, usize> = HashMap::new();
        for task in &tasks {
            emit(
                ResearchPhase::Search,
                StepStatus::InProgress,
                format!("搜索{}：{}", task.category.label(), task.query),
            );
            match self.search_query(&task.query, now).await {
                Ok(results) => {
                    result_counts.insert(
                        task.category,
                        result_counts
                            .get(&task.category)
                            .copied()
                            .unwrap_or_default()
                            + results.len(),
                    );
                    emit(
                        ResearchPhase::Search,
                        StepStatus::Done,
                        format!("「{}」返回 {} 条结果", task.query, results.len()),
                    );
                    for result in &results {
                        result_contexts
                            .entry(normalize_url(&result.url))
                            .or_insert((task.category, task.query.clone()));
                    }
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

        // 搜索完成后再做一次覆盖度检查：只有某个意图完全没有返回结果时才补搜，
        // 避免把 Query Expansion 变成无边界的“继续搜索”。
        let gap_tasks = post_search_gap_tasks(&tasks, &result_counts, &city, &input);
        if !gap_tasks.is_empty() {
            emit(
                ResearchPhase::PlanQueries,
                StepStatus::InProgress,
                format!("搜索后发现 {} 个信息缺口，执行有限补搜", gap_tasks.len()),
            );
            for task in gap_tasks {
                match self.search_query(&task.query, now).await {
                    Ok(results) => {
                        for result in &results {
                            result_contexts
                                .entry(normalize_url(&result.url))
                                .or_insert((task.category, task.query.clone()));
                        }
                        all_results.extend(results);
                    }
                    Err(error) => notes.push(format!("补搜「{}」失败：{error}", task.query)),
                }
            }
            emit(
                ResearchPhase::PlanQueries,
                StepStatus::Done,
                "搜索后补搜完成".to_string(),
            );
        }

        // 5. 去重 + 可信度排序（需求 #七 / #八）
        emit(
            ResearchPhase::RankSources,
            StepStatus::InProgress,
            format!("去重并排序 {} 条搜索结果", all_results.len()),
        );
        let deduped = dedup_search_results(all_results);
        let ranking_context = SourceRankingContext {
            city: &city,
            category: QueryCategory::Attractions,
            preferences: &request.preferences,
            date_range: request
                .date_range
                .as_ref()
                .map(|range| (range.start.as_str(), range.end.as_str())),
        };
        let mut rated: Vec<(SearchResult, TravelSource)> = deduped
            .into_iter()
            .map(|result| {
                let (category, query) = result_contexts
                    .get(&normalize_url(&result.url))
                    .cloned()
                    .unwrap_or((QueryCategory::Attractions, city.clone()));
                let context = SourceRankingContext {
                    category,
                    ..ranking_context.clone()
                };
                let source = rate_source_for(&context, &query, &result);
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
        let mut llm_available = self.llm.is_some();
        let mut facts: Vec<TravelFact> = Vec::new();
        let mut fact_failures = 0_usize;
        let mut llm_failure: Option<String> = None;
        if llm_available {
            for document in &documents {
                if llm_failure.is_some() {
                    emit(
                        ResearchPhase::ExtractFacts,
                        StepStatus::Skipped,
                        format!("{}：LLM 已不可用，跳过", document.title),
                    );
                    continue;
                }
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
                        if is_llm_transport_error(&error) {
                            llm_failure = Some(error.clone());
                        }
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
        if let Some(error) = llm_failure {
            llm_available = false;
            notes.push(format!(
                "LLM 事实提取连接失败，已停止本次剩余 LLM 请求并立即降级：{error}"
            ));
        }
        let mut facts = dedup_entity_facts(dedup_facts(facts));
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
        facts.extend(dedup_entity_facts(dedup_facts(data_facts)));
        facts = dedup_entity_facts(facts);

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
        let states: HashMap<String, ContentState> = sources
            .iter()
            .map(|source| (source.url.clone(), source.state))
            .collect();
        let verified = verify_facts_with_states(&facts, &levels, &states);
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
            .generate_guide(
                request,
                &city,
                &sources,
                &verified,
                &facts,
                llm_available,
                now,
                notes,
            )
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
        // 日期范围作为独立键持久化，避免覆盖同城同天数的普通攻略。
        let store = self.store.lock().expect("travel store poisoned");
        let stored = store.upsert_guide(&guide, now).map_err(travel)?;
        emit(
            ResearchPhase::SaveGuide,
            StepStatus::Done,
            format!(
                "已保存（{city}，{} 天{}）",
                request.days,
                request.date_range.as_ref().map_or(String::new(), |range| {
                    format!("，{} 至 {}", range.start, range.end)
                })
            ),
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
        let mut any_provider_responded = false;
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
                    any_provider_responded = true;
                }
                Err(error) => last_error = Some(error),
            }
        }
        if any_provider_responded {
            // 没有匹配结果是正常搜索结果，不应被前端误报为“搜索失败”。
            return Ok(Vec::new());
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
        existing: &[QueryTask],
        emit: &mut (dyn FnMut(ResearchPhase, StepStatus, String) + Send),
    ) -> Option<Vec<QueryTask>> {
        let llm = self.llm.as_deref()?;
        let raw = llm
            .complete(QUERY_SYSTEM_PROMPT, &query_user_prompt(city, input))
            .await
            .ok()?;
        let tasks = parse_search_intents(&raw, city);
        if tasks.is_empty() {
            return None;
        }
        emit(
            ResearchPhase::PlanQueries,
            StepStatus::Done,
            format!("LLM 根据缺口追加 {} 个搜索意图", tasks.len()),
        );
        let existing_queries = existing
            .iter()
            .map(|task| task.query.as_str())
            .collect::<Vec<_>>();
        Some(
            tasks
                .into_iter()
                .filter(|task| !existing_queries.contains(&task.query.as_str()))
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
        facts: &[TravelFact],
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
                date_range: request.date_range.clone(),
                llm_used,
                notes: Vec::new(),
            },
            ..CityGuide::default()
        };
        guide.sources = sources.to_vec();

        let llm_result = if llm_used {
            if let Some(llm) = self.llm.as_deref() {
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
                        parse_guide_json(&raw)
                            .map_err(|error| GuideGenError::Parse(error.to_string()))
                    })
            } else {
                Err(GuideGenError::Unavailable)
            }
        } else {
            Err(GuideGenError::Unavailable)
        };

        let mut llm_ok = false;
        match llm_result {
            Ok(parsed) => {
                let mut parsed = parsed;
                if parsed.city.name.trim().is_empty() {
                    parsed.city.name = city.to_string();
                }
                parsed.meta = guide.meta.clone();
                parsed.sources = sources.to_vec();
                guide = parsed;
                llm_ok = true;
            }
            Err(error) => {
                // 后续的外部 POI 合并需要知道这是降级路径；否则质量门禁会把
                // 所有仅有名称/坐标的结构化候选全部移出主行程。
                guide.meta.llm_used = false;
                notes.push(format!(
                    "LLM 攻略生成失败，已降级为“仅来源列表”模式：{error}"
                ));
                build_fallback_guide(&mut guide, city, sources);
                notes.append(&mut guide.meta.notes);
            }
        }
        // 程序层合并已验证事实（LLM 与降级两条路径都执行）：
        // - 高德 POI 的景点/美食/住宿条目补进攻略；
        // - 和风天气进入独立天气卡片；
        // - 开放时间/门票/预约挂到同名景点。
        let fallback_mode = !guide.meta.llm_used;
        merge_external_data(&mut guide, verified, facts, fallback_mode);
        sanitize_unverified_hard_facts(&mut guide, verified);
        let report = apply_quality_gate(&mut guide);
        if report.duplicate_attractions_removed > 0 || report.attractions_demoted > 0 {
            notes.push(format!(
                "质量门禁：合并 {} 个重复景点，{} 个景点降为备选",
                report.duplicate_attractions_removed, report.attractions_demoted
            ));
        }
        populate_decisions(&mut guide);
        plan_itinerary(&mut guide, days);
        self.enrich_route_data(&mut guide).await;
        guide.evidence = EvidenceSummary {
            source_count: sources.len(),
            verified_count: verified.iter().filter(|fact| fact.verified).count(),
            snippet_only_count: sources
                .iter()
                .filter(|source| source.state == ContentState::SnippetOnly)
                .count(),
            conflict_count: verified.iter().filter(|fact| fact.has_conflict).count(),
            quality: if sources.is_empty() {
                "有限".to_string()
            } else if sources
                .iter()
                .filter(|source| source.state == ContentState::Full)
                .count()
                * 2
                >= sources.len()
            {
                "良好".to_string()
            } else {
                "有限".to_string()
            },
        };
        guide.meta.llm_used = llm_used && llm_ok;
        guide.meta.notes = notes;
        guide
    }

    /// 用结构化路线服务补齐相邻行程点的驾车距离/时间，并把餐饮 POI 归属到最近的一天。
    /// 没有可用路线 Provider 时保持字段为空，不把坐标直线距离伪装成驾车时间。
    async fn enrich_route_data(&self, guide: &mut CityGuide) {
        let coordinates = guide_coordinates(guide);
        for day in &mut guide.itinerary_days {
            let mut previous: Option<MapCoordinates> = None;
            for stop in &mut day.stops {
                let current = coordinates
                    .iter()
                    .find(|(name, _)| contains(name, &stop.name) || contains(&stop.name, name))
                    .and_then(|(_, point)| point.clone());
                if let (Some(origin), Some(destination)) = (previous.clone(), current.clone()) {
                    for provider in &self.data_providers {
                        if let Ok(Some(route)) = provider
                            .driving_route(TravelRouteRequest {
                                origin: origin.clone(),
                                destination: destination.clone(),
                            })
                            .await
                        {
                            stop.travel_time = Some(format!(
                                "驾车约 {} 分钟 · {:.1} 公里",
                                route.duration_minutes, route.distance_km
                            ));
                            break;
                        }
                    }
                }
                if current.is_some() {
                    previous = current;
                }
            }
        }
        assign_restaurant_routes(guide);
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

fn validate_date_range(range: Option<&TravelDateRange>) -> Result<(), ApplicationError> {
    let Some(range) = range else {
        return Ok(());
    };
    if !is_iso_date(&range.start) || !is_iso_date(&range.end) || range.start > range.end {
        return Err(ApplicationError::TravelFailed(
            "行程日期必须是有效的 YYYY-MM-DD，且结束日期不能早于开始日期".to_string(),
        ));
    }
    Ok(())
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => return false,
    };
    day > 0 && day <= max_day
}

/// 只有请求/响应传输层错误才熔断。模型返回的非法 JSON 是单篇内容错误，
/// 后续文档或最终攻略仍可能成功，不能因此提前放弃。
fn is_llm_transport_error(error: &str) -> bool {
    error.contains("travel llm request failed") || error.contains("llm is not configured")
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
你是旅行研究中的信息缺口分析器和搜索意图规划师。先查看已覆盖的类别，只为明确缺失且会改变旅行决策的信息补充查询。\
最多输出 4 个 JSON 对象，不要重复已有意图，不要输出泛化的“旅游攻略/热门景点”。字段为 query、category、purpose、priority、freshness_required、structured_data_preferred。\
category 只能是 must_see、culture_history、nature、food、transport、accommodation_area、current_status、weather、local_experience、event、itinerary。\
城市名必须出现在 query 中；只输出 JSON。";

pub fn query_user_prompt(city: &str, input: &TravelQueryInput) -> String {
    let mut brief = format!("城市：{city}；旅行天数：{} 天", input.days);
    if let Some(month) = input.month {
        brief.push_str(&format!("；出行月份：{month} 月"));
    }
    if let Some(range) = &input.date_range {
        brief.push_str(&format!("；行程日期：{} 至 {}", range.start, range.end));
    }
    if !input.preferences.is_empty() {
        brief.push_str(&format!("；偏好：{}", input.preferences.join("、")));
    }
    let covered = "城市概览、核心景点、美食、住宿区域、交通、开放与预约";
    format!("请分析以下需求。已覆盖类别：{covered}。只补充仍缺失的信息：\n{brief}")
}

fn post_search_gap_tasks(
    tasks: &[QueryTask],
    result_counts: &HashMap<QueryCategory, usize>,
    city: &str,
    input: &TravelQueryInput,
) -> Vec<QueryTask> {
    let mut seen = std::collections::HashSet::new();
    let mut gaps = Vec::new();
    for task in tasks {
        if result_counts
            .get(&task.category)
            .copied()
            .unwrap_or_default()
            > 0
            || !seen.insert(task.category)
        {
            continue;
        }
        let (query, intent, purpose, priority, freshness_required, structured_data_preferred) =
            match task.category {
                QueryCategory::Food => (
                    format!("{city} 本地特色美食与餐厅"),
                    SearchIntentCategory::Food,
                    "补齐美食缺口",
                    0.9,
                    false,
                    true,
                ),
                QueryCategory::Transport => (
                    format!("{city} 景点之间交通与驾车时间"),
                    SearchIntentCategory::Transport,
                    "补齐移动成本缺口",
                    0.85,
                    true,
                    false,
                ),
                QueryCategory::Accommodation => (
                    format!("{city} 适合旅行住宿的区域"),
                    SearchIntentCategory::AccommodationArea,
                    "补齐住宿区域缺口",
                    0.8,
                    false,
                    false,
                ),
                QueryCategory::Warnings => (
                    format!("{city} 主要景区最新开放与预约"),
                    SearchIntentCategory::CurrentStatus,
                    "补齐开放状态缺口",
                    0.95,
                    true,
                    false,
                ),
                QueryCategory::Itinerary if input.days <= 3 => (
                    format!("{city} {}日游路线建议", input.days),
                    SearchIntentCategory::Itinerary,
                    "补齐短途路线缺口",
                    0.65,
                    false,
                    false,
                ),
                QueryCategory::Attractions => (
                    format!("{city} 核心景点与自然历史景区"),
                    SearchIntentCategory::MustSee,
                    "补齐景点缺口",
                    0.9,
                    false,
                    true,
                ),
                _ => continue,
            };
        gaps.push(QueryTask {
            category: task.category,
            query,
            intent,
            purpose: purpose.to_string(),
            priority,
            freshness_required,
            structured_data_preferred,
        });
        if gaps.len() >= 2 {
            break;
        }
    }
    gaps
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

#[derive(Debug, Deserialize)]
struct SearchIntentJson {
    query: Option<String>,
    category: Option<String>,
    purpose: Option<String>,
    priority: Option<f32>,
    freshness_required: Option<bool>,
    structured_data_preferred: Option<bool>,
}

/// 解析并校验 LLM 的结构化 SearchIntent。兼容旧模型返回的字符串数组，
/// 但会统一补上语义类别和预算字段。
pub fn parse_search_intents(raw: &str, city: &str) -> Vec<QueryTask> {
    let Ok(value) = devtoolbox_core::travel::extract_json(raw) else {
        return Vec::new();
    };
    let items = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(mut object) => object
            .remove("intents")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default(),
        _ => return Vec::new(),
    };
    let mut tasks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items.into_iter().take(4) {
        if let Some(query) = item.as_str() {
            let query = query.trim();
            if query.contains(city) && seen.insert(query.to_string()) {
                tasks.push(QueryTask {
                    category: QueryCategory::Activities,
                    query: query.to_string(),
                    intent: SearchIntentCategory::LocalExperience,
                    purpose: "补充本地体验缺口".to_string(),
                    priority: 0.65,
                    freshness_required: false,
                    structured_data_preferred: false,
                });
            }
            continue;
        }
        let Ok(parsed) = serde_json::from_value::<SearchIntentJson>(item) else {
            continue;
        };
        let Some(query) = parsed
            .query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value.contains(city))
        else {
            continue;
        };
        let Some(intent) = parsed
            .category
            .as_deref()
            .and_then(SearchIntentCategory::parse)
        else {
            continue;
        };
        if !seen.insert(query.clone()) {
            continue;
        }
        let category = match intent {
            SearchIntentCategory::Food => QueryCategory::Food,
            SearchIntentCategory::Transport => QueryCategory::Transport,
            SearchIntentCategory::AccommodationArea => QueryCategory::Accommodation,
            SearchIntentCategory::CurrentStatus => QueryCategory::Warnings,
            SearchIntentCategory::Itinerary => QueryCategory::Itinerary,
            SearchIntentCategory::Weather
            | SearchIntentCategory::Event
            | SearchIntentCategory::LocalExperience => QueryCategory::Activities,
            _ => QueryCategory::Attractions,
        };
        tasks.push(QueryTask {
            category,
            query,
            intent,
            purpose: parsed.purpose.unwrap_or_else(|| "补充信息缺口".to_string()),
            priority: parsed.priority.unwrap_or(0.6).clamp(0.0, 1.0),
            freshness_required: parsed.freshness_required.unwrap_or(false),
            structured_data_preferred: parsed.structured_data_preferred.unwrap_or(false),
        });
    }
    tasks
}

fn append_query_tasks(tasks: &mut Vec<QueryTask>, extras: Vec<QueryTask>, max: usize) {
    let mut seen = tasks
        .iter()
        .map(|task| task.query.to_lowercase())
        .collect::<std::collections::HashSet<_>>();
    for task in extras {
        if tasks.len() >= max {
            break;
        }
        if seen.insert(task.query.to_lowercase()) {
            tasks.push(task);
        }
    }
}

const GUIDE_SYSTEM_PROMPT: &str = "\
你不是信息整理器，而是一名旅行编辑和行程规划师。只使用给定事实，不补写资料中没有的价格、开放时间、地址或交通时间。\
SELECT > SUMMARIZE：宁可少，不要凑数；同类 POI 主动比较并合并别名；低信息量句子（如“旅游必去景点推荐”）禁止进入 why_go。\
根据请求天数做减法：主要景点最多 6 个，餐厅最多 5 个，住宿只推荐区域且最多 3 个；离主体路线明显过远的景点放 alternatives。\
行程必须按坐标/区域分组，避免让远距离景点与市区景点来回穿插。仅 SnippetOnly 的开放、门票、预约、交通事实不是已验证事实。\
输出严格 JSON，缺少可靠信息就留空，不要为了填满字段而编造。";

fn input_brief(request: &TravelResearchRequest) -> String {
    let mut brief = format!(
        "城市：{}；天数：{} 天",
        request.city.trim(),
        request.days.clamp(1, 7)
    );
    if let Some(month) = request.month {
        brief.push_str(&format!("；月份：{month} 月"));
    }
    if let Some(range) = &request.date_range {
        brief.push_str(&format!("；行程日期：{} 至 {}", range.start, range.end));
    }
    if !request.preferences.is_empty() {
        brief.push_str(&format!("；偏好：{}", request.preferences.join("、")));
    }
    brief
}

pub fn guide_user_prompt(city: &str, brief: &str, facts_block: &str, docs_block: &str) -> String {
    format!(
        "目标城市：{city}\n需求：{brief}\n\n已验证事实（verified=false 的硬事实只能作为线索）：\n{facts_block}\n\n信息源（标题/等级/抓取状态）：\n{docs_block}\n\n\
请输出 JSON，重点字段为 quick_decisions、top_picks、alternatives、itinerary_days、food_summary、stay_areas、transport_summary、evidence；同时填充兼容字段 summary、weather、districts、attractions、foods、restaurants、transport、accommodation_areas、itineraries、local_tips、warnings。\n\
每个主要景点尽量提供 name、why_go、why_for_this_trip、area、suggested_duration、best_for、recommended_day、opening_hours、ticket、reservation、source_ids。每一天包含 day、title、theme 和 stops；stop 包含 name、time、duration、area、reason、travel_time。只输出有事实支持且与本次天数匹配的内容。"
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
                "- [{category}] {subject} = {value}（confidence={confidence}, 来源数={count}, 冲突={conflict}, verified={verified}）",
                category = f.category.json_name(),
                subject = f.subject,
                value = f.value,
                confidence = f.confidence,
                count = f.verified_sources,
                conflict = f.has_conflict
                ,verified = f.verified
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
fn merge_external_data(
    guide: &mut CityGuide,
    verified: &[VerifiedFact],
    facts: &[TravelFact],
    fallback_mode: bool,
) {
    for fact in verified {
        match fact.category {
            FactCategory::OpeningHours | FactCategory::Ticket | FactCategory::Reservation => {
                // 尝试挂到同名景点；找不到则不强行造条目
                apply_to_attraction(guide, &fact.subject, fact.category, fact);
            }
            FactCategory::Attraction => {
                let coordinates = coordinates_for_verified_fact(fact, facts);
                let poi = facts.iter().find(|candidate| {
                    candidate.category == FactCategory::Attraction
                        && candidate.subject == fact.subject
                        && candidate.value == fact.value
                });
                if let Some(attraction) = guide.attractions.iter_mut().find(|item| {
                    contains(&item.name, &fact.subject)
                        || item.poi_id.as_deref() == poi.and_then(|p| p.poi_id.as_deref())
                }) {
                    if attraction.coordinates.is_none() {
                        attraction.coordinates = coordinates;
                    }
                    if attraction.area.is_none() {
                        attraction.area = poi.and_then(|p| p.area.clone());
                    }
                    if attraction.poi_id.is_none() {
                        attraction.poi_id = poi.and_then(|p| p.poi_id.clone());
                    }
                    if let Some(source_id) = poi.map(|candidate| candidate.source_id.clone()) {
                        attraction.source_ids.push(source_id);
                    }
                } else {
                    guide.attractions.push(Attraction {
                        id: poi.and_then(|p| p.poi_id.clone()),
                        name: fact.subject.clone(),
                        normalized_name: Some(devtoolbox_core::travel::normalize_entity_name(
                            &fact.subject,
                        )),
                        poi_id: poi.and_then(|p| p.poi_id.clone()),
                        area: poi.and_then(|p| p.area.clone()),
                        source_ids: poi
                            .map(|candidate| vec![candidate.source_id.clone()])
                            .unwrap_or_default(),
                        coordinates,
                        why_go: fallback_mode.then(|| {
                            "已由高德 POI 识别；缺少 AI 编辑描述，建议先核实开放状态，再决定是否纳入路线。"
                                .to_string()
                        }),
                        why_for_this_trip: fallback_mode.then(|| {
                            "已获得 POI 坐标，可结合当天路线安排；以下信息仍待进一步核实。"
                                .to_string()
                        }),
                        suggested_duration: fallback_mode.then(|| "待确认".to_string()),
                        confidence: fallback_mode.then(|| "low".to_string()),
                        ..Attraction::default()
                    });
                }
            }
            FactCategory::Food => merge_food_fact(guide, fact, facts),
            // 高德酒店/民宿 POI 只作为路线附近检索候选，不默认倾倒到攻略；
            // 住宿主内容保持“推荐区域”的粒度。
            FactCategory::Accommodation => {
                if let Some(area) = facts.iter().find_map(|candidate| candidate.area.clone())
                    && (area.contains('区') || area.contains("商圈"))
                    && !guide
                        .accommodation_areas
                        .iter()
                        .any(|item| item.name == area)
                {
                    guide.accommodation_areas.push(AccommodationArea {
                        name: area,
                        ..AccommodationArea::default()
                    });
                }
            }
            FactCategory::Weather => {
                if guide.weather.is_none() {
                    guide.weather = weather_forecast_from_fact(fact);
                }
                // 兼容非和风格式的天气事实：无法展示为卡片时，仍保留原始文字。
                if guide.weather.is_none()
                    && !guide
                        .local_tips
                        .iter()
                        .any(|tip| tip.title.contains("天气"))
                {
                    guide.local_tips.push(TravelTip {
                        title: format!("{}天气", fact.subject),
                        text: fact.value.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn merge_food_fact(guide: &mut CityGuide, fact: &VerifiedFact, facts: &[TravelFact]) {
    let source = facts.iter().find(|candidate| {
        candidate.category == FactCategory::Food && candidate.subject == fact.subject
    });
    let is_amap = source.is_some_and(|candidate| {
        candidate.source_id == devtoolbox_infrastructure::travel::data_provider::AMAP_SOURCE_URL
            && (candidate.coordinates.is_some()
                || candidate.poi_id.is_some()
                || candidate.area.is_some()
                || candidate.address.is_some())
    });
    if is_amap {
        // 高德这个查询返回的是“餐厅 POI”，value 通常是行政区 + 地址，不能作为菜品简介。
        if let Some(source) = source
            && !guide.restaurants.iter().any(|place| {
                place.poi_id == source.poi_id && source.poi_id.is_some()
                    || (place.poi_id.is_none() && place.name == fact.subject)
            })
        {
            guide.restaurants.push(Place {
                name: fact.subject.clone(),
                area: source.area.clone(),
                why_pick: Some("高德 POI 候选；建议结合当天路线和营业状态选择。".to_string()),
                confidence: Some("low".to_string()),
                poi_id: source.poi_id.clone(),
                coordinates: source.coordinates.clone(),
                ..Place::default()
            });
        }
        return;
    }
    if guide
        .foods
        .iter()
        .any(|food| contains(&food.name, &fact.subject))
    {
        return;
    }
    guide.foods.push(Food {
        name: fact.subject.clone(),
        intro: Some(fact.value.clone()),
        area: source.and_then(|item| item.area.clone()),
        source_ids: source
            .map(|candidate| vec![candidate.source_id.clone()])
            .unwrap_or_default(),
        ..Food::default()
    });
}

fn coordinates_for_verified_fact(
    verified: &VerifiedFact,
    facts: &[TravelFact],
) -> Option<MapCoordinates> {
    facts
        .iter()
        .find(|fact| {
            fact.category == verified.category
                && contains(&fact.subject, &verified.subject)
                && fact.value == verified.value
        })
        .and_then(|fact| fact.coordinates.clone())
}

/// 和风 Provider 将逐日预报作为一条稳定格式的 Weather 事实传入；这里恢复为 UI 可用的卡片数据。
fn weather_forecast_from_fact(fact: &VerifiedFact) -> Option<WeatherForecast> {
    let days = fact
        .value
        .split('；')
        .filter_map(|part| {
            let mut fields = part.split_whitespace();
            let date = fields.next()?.to_string();
            let text_day = fields.next()?.to_string();
            let temperatures = fields.next()?.trim_end_matches("°C");
            let (temp_min, temp_max) = temperatures.split_once('~')?;
            Some(WeatherDay {
                date,
                text_day,
                temp_min: temp_min.to_string(),
                temp_max: temp_max.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if days.is_empty() {
        return None;
    }
    Some(WeatherForecast {
        city: fact.subject.trim_end_matches("天气").to_string(),
        days,
    })
}

fn apply_to_attraction(
    guide: &mut CityGuide,
    subject: &str,
    category: FactCategory,
    verified: &VerifiedFact,
) {
    if !verified.verified {
        return;
    }
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
            verified: verified.verified,
        };
        match category {
            FactCategory::OpeningHours => attraction.opening_hours = Some(value),
            FactCategory::Ticket => attraction.ticket = Some(value),
            FactCategory::Reservation => attraction.reservation = Some(value),
            _ => {}
        }
    }
}

fn sanitize_unverified_hard_facts(guide: &mut CityGuide, verified: &[VerifiedFact]) {
    for attractions in [
        &mut guide.attractions,
        &mut guide.top_picks,
        &mut guide.alternatives,
    ] {
        sanitize_attraction_hard_facts(attractions, verified);
    }
}

fn sanitize_attraction_hard_facts(attractions: &mut [Attraction], verified: &[VerifiedFact]) {
    for attraction in attractions {
        let has_verified = |category: FactCategory, value: &VerifiedValue| {
            verified.iter().any(|fact| {
                fact.category == category
                    && fact.verified
                    && contains(&fact.subject, &attraction.name)
                    && contains(&fact.value, &value.value)
            })
        };
        if attraction
            .opening_hours
            .as_ref()
            .is_some_and(|value| !has_verified(FactCategory::OpeningHours, value))
        {
            attraction.opening_hours = None;
        }
        if attraction
            .ticket
            .as_ref()
            .is_some_and(|value| !has_verified(FactCategory::Ticket, value))
        {
            attraction.ticket = None;
        }
        if attraction
            .reservation
            .as_ref()
            .is_some_and(|value| !has_verified(FactCategory::Reservation, value))
        {
            attraction.reservation = None;
        }
    }
}

fn populate_decisions(guide: &mut CityGuide) {
    if guide.quick_decisions.best_area_to_stay.is_none() {
        guide.quick_decisions.best_area_to_stay = guide
            .accommodation_areas
            .first()
            .map(|area| area.name.clone());
    }
    if guide.quick_decisions.signature_food.is_none() {
        guide.quick_decisions.signature_food = guide.foods.first().map(|food| food.name.clone());
    }
    if guide.quick_decisions.must_visit.is_empty() {
        guide.quick_decisions.must_visit = guide
            .attractions
            .iter()
            .take(3)
            .map(|item| item.name.clone())
            .collect();
    }
    if guide.food_summary.is_none() {
        guide.food_summary = guide.foods.first().and_then(|food| food.intro.clone());
        if guide.food_summary.is_none() && !guide.restaurants.is_empty() {
            guide.food_summary =
                Some("已识别到本地餐饮 POI；优先选择靠近当天路线、再核实营业状态。".to_string());
        }
    }
    if guide.transport_summary.is_none() {
        guide.transport_summary = guide.transport.overview.clone();
    }
}

/// 根据坐标/区域把首选景点分配到实际请求的天数，并生成可直接阅读的 Day Card 数据。
/// 不依赖 LLM，因此即使模型失败，行程仍有稳定的 Day 1..Day N 骨架。
fn plan_itinerary(guide: &mut CityGuide, days: u8) {
    let days = days.clamp(1, 7);
    let attractions = if guide.top_picks.is_empty() {
        guide.attractions.clone()
    } else {
        guide.top_picks.clone()
    };
    let mut buckets = vec![Vec::<Attraction>::new(); days as usize];
    let mut unassigned = Vec::new();
    for attraction in attractions {
        if let Some(day) = attraction
            .recommended_day
            .filter(|day| (1..=days).contains(day))
        {
            buckets[day as usize - 1].push(attraction);
        } else {
            unassigned.push(attraction);
        }
    }
    // 有坐标时按经度/纬度排序后切成连续组，形成“市区 / 远郊”这类地理簇；
    // 无坐标时退回区域排序，不让 LLM 随机把远距离景点穿插到同一天。
    unassigned.sort_by(|left, right| {
        let left_key = left
            .coordinates
            .as_ref()
            .map_or((String::new(), 0.0, 0.0), |point| {
                (String::new(), point.longitude, point.latitude)
            });
        let right_key = right
            .coordinates
            .as_ref()
            .map_or((String::new(), 0.0, 0.0), |point| {
                (String::new(), point.longitude, point.latitude)
            });
        left.area
            .cmp(&right.area)
            .then_with(|| {
                left_key
                    .1
                    .partial_cmp(&right_key.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left_key
                    .2
                    .partial_cmp(&right_key.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let total = unassigned.len().max(1);
    for (index, attraction) in unassigned.into_iter().enumerate() {
        let target = (index * days as usize / total).min(days as usize - 1);
        buckets[target].push(attraction);
    }
    for (index, bucket) in buckets.iter().enumerate() {
        for attraction in bucket {
            let day = (index + 1) as u8;
            for item in guide
                .attractions
                .iter_mut()
                .chain(guide.top_picks.iter_mut())
            {
                if item.name == attraction.name && item.recommended_day.is_none() {
                    item.recommended_day = Some(day);
                }
            }
        }
    }
    guide.itinerary_days = buckets
        .into_iter()
        .enumerate()
        .map(|(index, stops)| {
            let theme = stops
                .iter()
                .filter_map(|item| item.area.clone())
                .collect::<Vec<_>>()
                .join(" + ");
            let day = (index + 1) as u8;
            let itinerary_stops = stops
                .into_iter()
                .enumerate()
                .map(|(stop_index, attraction)| {
                    let time = match stop_index {
                        0 => "09:00",
                        1 => "13:30",
                        2 => "16:00",
                        _ => "18:30",
                    };
                    ItineraryStop {
                        name: attraction.name,
                        note: attraction
                            .why_for_this_trip
                            .clone()
                            .or(attraction.why_go.clone()),
                        time: Some(time.to_string()),
                        duration: attraction.suggested_duration,
                        area: attraction.area,
                        reason: attraction.why_for_this_trip.or(attraction.why_go),
                        // 没有路线服务时不伪造交通时间；真实驾车时间由后置增强阶段填入。
                        travel_time: None,
                    }
                })
                .collect();
            ItineraryDay {
                day,
                title: Some(format!("第 {day} 天")),
                theme: (!theme.is_empty()).then_some(theme),
                stops: itinerary_stops,
            }
        })
        .collect();
}

fn contains(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.trim();
    let needle = needle.trim();
    !needle.is_empty() && (haystack.contains(needle) || needle.contains(haystack))
}

fn guide_coordinates(guide: &CityGuide) -> Vec<(String, Option<MapCoordinates>)> {
    guide
        .attractions
        .iter()
        .chain(guide.top_picks.iter())
        .chain(guide.alternatives.iter())
        .map(|item| (item.name.clone(), item.coordinates.clone()))
        .collect()
}

fn assign_restaurant_routes(guide: &mut CityGuide) {
    let attraction_points = guide
        .attractions
        .iter()
        .filter_map(|item| Some((item.recommended_day?, item.coordinates.clone()?)))
        .collect::<Vec<_>>();
    if attraction_points.is_empty() {
        return;
    }
    for place in &mut guide.restaurants {
        let Some(point) = place.coordinates.clone() else {
            continue;
        };
        let Some((day, distance)) = attraction_points
            .iter()
            .map(|(day, destination)| (*day, haversine_km(&point, destination)))
            .min_by(|left, right| {
                left.1
                    .partial_cmp(&right.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            continue;
        };
        place.route_day = Some(day);
        // 明确标注“直线”，避免把估算冒充驾车距离。
        place.distance_to_route = Some(format!("距 Day {day} 路线直线约 {distance:.1} 公里"));
    }
}

fn haversine_km(left: &MapCoordinates, right: &MapCoordinates) -> f64 {
    let radius_km = 6_371.0_f64;
    let lat1 = left.latitude.to_radians();
    let lat2 = right.latitude.to_radians();
    let dlat = (right.latitude - left.latitude).to_radians();
    let dlon = (right.longitude - left.longitude).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    radius_km * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

/// 降级版攻略：只使用真实搜索信息（标题 / 摘要 / 来源），缺失区留空并注明。
fn build_fallback_guide(guide: &mut CityGuide, _city: &str, sources: &[TravelSource]) {
    guide.summary = format!(
        "「{}」的资料已收集，但 AI 编辑暂不可用。当前只展示可追溯的有限结论，请以官方信息为准。",
        guide.city.name
    );
    let full_sources = sources
        .iter()
        .filter(|source| source.state == ContentState::Full)
        .count();
    guide.meta.notes.push(format!(
        "降级模式：保留 {} 个来源，其中 {} 个可抓取全文；未将来源标题当作景点或酒店推荐。",
        sources.len(),
        full_sources
    ));
    guide
        .meta
        .notes
        .push("未生成可靠的精选景点、餐厅或住宿区域，避免用搜索结果凑数。".to_string());
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}
