//! HistoryEnrichmentService（V5 §14/§27/§30）：按需富化协调器。
//!
//! 流程：状态检查 → 单飞去重 → 搜索 → 排序 → 证据包 → 结构化生成 → 校验 → 持久化。
//! Canonical 只读；富化写入独立缓存。无网络/无模型时 Canonical 照常（get 路径零依赖）。

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

use crate::history::enrichment::generation::{parse_generation, system_prompt, user_prompt};
use crate::history::enrichment::ports::{
    EnrichmentEntityPort, EnrichmentLlmPort, EnrichmentSearchPort, EnrichmentStore,
};
use crate::history::enrichment::ranking::{rank_sources, sources_to_prompt_block};
use crate::history::enrichment::validation::validate;
use crate::time::now_unix;
use devtoolbox_core::history_enrichment::{
    ENRICHMENT_SCHEMA_VERSION, EnrichmentKey, EnrichmentMetadata, EnrichmentRecord,
    EnrichmentSection, EnrichmentState, EnrichmentView,
};

/// 富化配置（组合根可调）。
#[derive(Clone, Debug)]
pub struct EnrichmentConfig {
    /// 新鲜度 TTL（秒）；超过 → STALE（§19 之一，非唯一判据）。
    pub ttl_secs: i64,
    /// 提示词版本（缓存元数据；变 → STALE）。
    pub prompt_version: String,
    /// 每次搜索取回数量（排序后再截断）。
    pub search_limit: usize,
    /// 进入 prompt 的来源上限（§24 不保存整页）。
    pub max_sources: usize,
    /// canonical 摘要最大字符。
    pub canonical_chars: usize,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            ttl_secs: 30 * 24 * 3600, // 30 天
            prompt_version: "v5-enrich-1".to_string(),
            search_limit: 10,
            max_sources: 6,
            canonical_chars: 2000,
        }
    }
}

/// 把 LLM 生成的来源 id 白名单。

/// History 富化服务。`ensure` 单飞（同一 key 并发只 1 次 search + 1 次 generation，§30）。
pub struct HistoryEnrichmentService {
    store: Arc<dyn EnrichmentStore>,
    search: Arc<dyn EnrichmentSearchPort>,
    llm: Arc<dyn EnrichmentLlmPort>,
    entity: Arc<dyn EnrichmentEntityPort>,
    config: EnrichmentConfig,
    in_flight: Mutex<HashSet<EnrichmentKey>>,
}

impl HistoryEnrichmentService {
    #[must_use]
    pub fn new(
        store: Arc<dyn EnrichmentStore>,
        search: Arc<dyn EnrichmentSearchPort>,
        llm: Arc<dyn EnrichmentLlmPort>,
        entity: Arc<dyn EnrichmentEntityPort>,
        config: EnrichmentConfig,
    ) -> Self {
        Self {
            store,
            search,
            llm,
            entity,
            config,
            in_flight: Mutex::new(HashSet::new()),
        }
    }

    // ------------------------------------------------------------------
    // 读取（零外部依赖；Canonical / 已有富化照常，§28/§63/§64）
    // ------------------------------------------------------------------

    /// 读取最佳记录（派生 STALE / Reviewed）。不触发任何 search/LLM。
    pub fn get(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        let record = self.store.load_best(key)?;
        let Some(record) = record else {
            return Ok(EnrichmentView {
                key: key.clone(),
                state: EnrichmentState::Missing,
                payload: None,
                metadata: None,
                error: None,
            });
        };
        let revision = self.canonical_revision(key)?;
        Ok(to_view(record, &self.config, revision))
    }

    /// 该 entity 下各 section 的状态（UI 展示「AI 解读」区，§33）。
    pub fn sections(
        &self,
        entity_type: &str,
        entity_id: &str,
        locale: &str,
    ) -> Result<Vec<(EnrichmentSection, EnrichmentState)>, String> {
        let mut out = Vec::new();
        for section in [
            EnrichmentSection::Overview,
            EnrichmentSection::Background,
            EnrichmentSection::Impact,
        ] {
            let key = EnrichmentKey::new(entity_type, entity_id, section.clone(), locale);
            let view = self.get(&key)?;
            out.push((section, view.state));
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    // 按需生成（§27）
    // ------------------------------------------------------------------

    /// ensure_enrichment(entity, section, locale)：
    /// READY(新鲜) / Reviewed → 直接返回；GENERATING → 返回生成中；
    /// 否则触发完整管线（单飞）。
    pub async fn ensure(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        // 校验门 #1：entity 存在（不存在 → Err，不入缓存）
        if !self.entity_exists(key)? {
            return Err(format!(
                "entity `{}` ({}) does not exist in canonical",
                key.entity_id, key.entity_type
            ));
        }

        // 快速路径：已有可展示结果（不 search / 不生成）
        if let Some(record) = self.store.load_best(key)? {
            let state = effective_state(&record, &self.config, self.canonical_revision(key)?);
            if matches!(state, EnrichmentState::Ready | EnrichmentState::Reviewed) {
                return Ok(to_view(record, &self.config, self.canonical_revision(key)?));
            }
        }

        // 单飞去重（§30）：并发 ensure 同 key → 返回 Generating（调用方轮询）
        {
            let mut guard = self.in_flight.lock().expect("in-flight poisoned");
            if !guard.insert(key.clone()) {
                return Ok(generating_view(key));
            }
        }
        let result = self.generate_inner(key).await;
        self.in_flight
            .lock()
            .expect("in-flight poisoned")
            .remove(key);
        result
    }

    /// 手动刷新（UI「重新整理」；Reviewed 也允许 → 新 revision 候选，§20）。
    pub async fn refresh(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        {
            let mut guard = self.in_flight.lock().expect("in-flight poisoned");
            if !guard.insert(key.clone()) {
                return Ok(generating_view(key));
            }
        }
        let result = self.generate_inner(key).await;
        self.in_flight
            .lock()
            .expect("in-flight poisoned")
            .remove(key);
        result
    }

    /// 人工审定（UI / 工具）：automatic refresh 之后跳过该行（§20）。
    pub fn mark_reviewed(&self, key: &EnrichmentKey) -> Result<(), String> {
        if let Some(record) = self.store.load_best(key)? {
            self.store.mark_reviewed(key, record.revision)
        } else {
            Ok(())
        }
    }

    // ------------------------------------------------------------------
    // 内部
    // ------------------------------------------------------------------

    fn entity_exists(&self, key: &EnrichmentKey) -> Result<bool, String> {
        match key.entity_type.as_str() {
            "event" => self.entity.event_exists(&key.entity_id),
            other => Err(format!("unsupported entity type for enrichment: {other}")),
        }
    }

    fn canonical_revision(&self, key: &EnrichmentKey) -> Result<Option<String>, String> {
        match key.entity_type.as_str() {
            "event" => self.entity.event_revision(&key.entity_id),
            _ => Ok(None),
        }
    }

    /// 完整管线（调用方已持有单飞令牌）。
    async fn generate_inner(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        let revision = self.store.next_revision(key)?;
        let now = now_unix();

        // 1) 未配置搜索 / 生成 → 受控 Unavailable 失败（Canonical 不受影响，§63/§64）。
        //    已存在 Failed 记录则直接复用（避免重复 ensure 让缓存表逐行膨胀，reviewer note）。
        if !self.search.configured() || !self.llm.configured() {
            if let Some(existing) = self.store.load_best(key)?
                && existing.state == EnrichmentState::Failed
            {
                return Ok(to_view(
                    existing,
                    &self.config,
                    self.canonical_revision(key)?,
                ));
            }
            let record = EnrichmentRecord {
                key: key.clone(),
                revision,
                state: EnrichmentState::Failed,
                payload: None,
                metadata: Some(EnrichmentMetadata {
                    generated_at: now,
                    refreshed_at: now,
                    model: None,
                    provider: None,
                    prompt_version: self.config.prompt_version.clone(),
                    schema_version: ENRICHMENT_SCHEMA_VERSION,
                    canonical_revision: self.canonical_revision(key)?,
                    source_ids: Vec::new(),
                    generation_count: revision,
                }),
                error: Some(
                    "enrichment unavailable: search or model is not configured".to_string(),
                ),
                reviewed: false,
            };
            self.store.put(&record)?;
            return Ok(to_view(record, &self.config, self.canonical_revision(key)?));
        }

        // 2) 搜索 + 排序（§21-§23）
        let raw = self
            .search
            .search(&self.search_query(key), self.config.search_limit)
            .await
            .map_err(|error| error.to_string())?;
        let ranked = rank_sources(raw, self.config.max_sources);
        let allowed_ids: Vec<String> = ranked.iter().map(|source| source.url.clone()).collect();

        // 3) canonical 摘要 + sources 证据包（§24：元数据 + excerpt，不存整页）
        let canonical_block = self.canonical_block(key).unwrap_or_default();
        let sources_block = sources_to_prompt_block(&ranked);

        // 4) 结构化生成（§25）
        let (provider, model) = self.llm.describe();
        let raw_output = self
            .llm
            .generate(
                system_prompt(),
                &user_prompt(
                    &key.entity_type,
                    &key.entity_id,
                    &self
                        .entity_label(key)
                        .unwrap_or_else(|_| key.entity_id.clone()),
                    key.section.as_str(),
                    &canonical_block,
                    &sources_block,
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        let payload = match parse_generation(&raw_output, key.section.as_str()) {
            Ok(payload) => payload,
            Err(error) => {
                self.store.put(&failed_record(
                    key,
                    revision,
                    now,
                    &format!("generation parse: {error}"),
                    self.canonical_revision(key)?,
                    self.config.prompt_version.clone(),
                ))?;
                return self.get(key);
            }
        };

        // 5) 校验门（§26）：失败 → FAILED，不存 READY
        let report = validate(key, &payload, &allowed_ids);
        if !report.valid {
            self.store.put(&failed_record(
                key,
                revision,
                now,
                &format!("validation failed: {}", report.errors.join("; ")),
                self.canonical_revision(key)?,
                self.config.prompt_version.clone(),
            ))?;
            return self.get(key);
        }

        // 6) 持久化 READY（§17/§18 元数据）
        let record = EnrichmentRecord {
            key: key.clone(),
            revision,
            state: EnrichmentState::Ready,
            payload: Some(payload),
            metadata: Some(EnrichmentMetadata {
                generated_at: now,
                refreshed_at: now,
                model,
                provider,
                prompt_version: self.config.prompt_version.clone(),
                schema_version: ENRICHMENT_SCHEMA_VERSION,
                canonical_revision: self.canonical_revision(key)?,
                source_ids: allowed_ids,
                generation_count: revision,
            }),
            error: None,
            reviewed: false,
        };
        self.store.put(&record)?;
        self.get(key)
    }

    fn search_query(&self, key: &EnrichmentKey) -> String {
        let label = self
            .entity_label(key)
            .unwrap_or_else(|_| key.entity_id.clone());
        match key.section {
            EnrichmentSection::Overview => {
                format!("{label} 历史 事件 概述")
            }
            EnrichmentSection::Background => {
                format!("{label} 历史 背景 起因")
            }
            EnrichmentSection::Impact => {
                format!("{label} 历史 影响 意义")
            }
        }
    }

    fn entity_label(&self, key: &EnrichmentKey) -> Result<String, String> {
        match key.entity_type.as_str() {
            "event" => Ok(self
                .entity
                .event_canonical(&key.entity_id)?
                .map(|event| event.name_zh_cn)
                .unwrap_or_else(|| key.entity_id.clone())),
            _ => Ok(key.entity_id.clone()),
        }
    }

    fn canonical_block(&self, key: &EnrichmentKey) -> Result<String, String> {
        match key.entity_type.as_str() {
            "event" => {
                let Some(event) = self.entity.event_canonical(&key.entity_id)? else {
                    return Ok(String::new());
                };
                let mut text = format!(
                    "canonical event `{}`（{}）\n- 时间: {}–{}\n- 重要性: {}\n- 质量状态: {}\n- 来源: {}\n- {}",
                    event.id,
                    event.name_zh_cn,
                    event
                        .start_year
                        .map(|y| y.to_string())
                        .unwrap_or_else(|| "?".into()),
                    event
                        .end_year
                        .map(|y| y.to_string())
                        .unwrap_or_else(|| "?".into()),
                    event.importance.unwrap_or_default(),
                    event.quality_status.unwrap_or_default(),
                    event.source_reference.unwrap_or_default(),
                    event.evidence_count_hint,
                );
                if let Some(summary) = event.summary_zh_cn {
                    text.push_str("\n- 摘要: ");
                    text.push_str(
                        &summary
                            .chars()
                            .take(self.config.canonical_chars)
                            .collect::<String>(),
                    );
                }
                Ok(text)
            }
            _ => Ok(String::new()),
        }
    }
}

fn generating_view(key: &EnrichmentKey) -> EnrichmentView {
    EnrichmentView {
        key: key.clone(),
        state: EnrichmentState::Generating,
        payload: None,
        metadata: None,
        error: None,
    }
}

fn failed_record(
    key: &EnrichmentKey,
    revision: u32,
    now: i64,
    error: &str,
    canonical_revision: Option<String>,
    prompt_version: String,
) -> EnrichmentRecord {
    EnrichmentRecord {
        key: key.clone(),
        revision,
        state: EnrichmentState::Failed,
        payload: None,
        metadata: Some(EnrichmentMetadata {
            generated_at: now,
            refreshed_at: now,
            model: None,
            provider: None,
            prompt_version,
            schema_version: ENRICHMENT_SCHEMA_VERSION,
            canonical_revision,
            source_ids: Vec::new(),
            generation_count: revision,
        }),
        error: Some(error.to_string()),
        reviewed: false,
    }
}

/// 派生状态：Ready 新鲜度 = TTL 内 AND canonical_revision 一致 AND schema_version 一致（§19）。
fn effective_state(
    record: &EnrichmentRecord,
    config: &EnrichmentConfig,
    current_canonical_revision: Option<String>,
) -> EnrichmentState {
    if record.reviewed {
        return EnrichmentState::Reviewed;
    }
    match record.state {
        EnrichmentState::Ready => {
            let Some(metadata) = &record.metadata else {
                return EnrichmentState::Stale;
            };
            let fresh = now_unix() - metadata.refreshed_at <= config.ttl_secs
                && metadata.schema_version == ENRICHMENT_SCHEMA_VERSION
                && metadata.canonical_revision == current_canonical_revision;
            if fresh {
                EnrichmentState::Ready
            } else {
                EnrichmentState::Stale
            }
        }
        other => other,
    }
}

fn to_view(
    record: EnrichmentRecord,
    config: &EnrichmentConfig,
    revision: Option<String>,
) -> EnrichmentView {
    let state = effective_state(&record, config, revision);
    let key = record.key.clone();
    EnrichmentView {
        key,
        state,
        payload: record.payload,
        metadata: record.metadata,
        error: record.error,
    }
}
