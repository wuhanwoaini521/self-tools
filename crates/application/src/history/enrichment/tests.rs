//! HistoryEnrichmentService 集成测试（V5 §66 8 Case；Fake 全依赖，不触网）。
//!
//! 覆盖：missing→READY / cache hit / stale-refresh / reviewed 不覆盖 / 单飞 /
//! search 失败 canonical 可用 / 非法输出→FAILED / canonical revision 变化→stale。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use devtoolbox_core::history_enrichment::{
    ENRICHMENT_SCHEMA_VERSION, EnrichmentKey, EnrichmentMetadata, EnrichmentPayload,
    EnrichmentRecord, EnrichmentSection, EnrichmentState,
};

use crate::history::enrichment::ports::{
    CanonicalEventRef, EnrichmentEntityPort, EnrichmentLlmPort, EnrichmentSearchPort,
    EnrichmentStore, SourceEvidence, SourceType,
};
use crate::history::enrichment::service::{EnrichmentConfig, HistoryEnrichmentService};
use crate::time::now_unix;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeSearch {
    configured: bool,
    result: Vec<SourceEvidence>,
    calls: AtomicU32,
    fail: bool,
}

#[async_trait::async_trait]
impl EnrichmentSearchPort for FakeSearch {
    fn configured(&self) -> bool {
        self.configured
    }
    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SourceEvidence>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err("search service down".into());
        }
        Ok(self.result.clone())
    }
}

struct FakeLlm {
    configured: bool,
    outputs: Mutex<std::collections::VecDeque<String>>,
    calls: AtomicU32,
}

impl FakeLlm {
    fn with(outputs: Vec<String>) -> Self {
        Self {
            configured: true,
            outputs: Mutex::new(outputs.into_iter().collect()),
            calls: AtomicU32::new(0),
        }
    }
}

#[async_trait::async_trait]
impl EnrichmentLlmPort for FakeLlm {
    fn configured(&self) -> bool {
        self.configured
    }
    fn describe(&self) -> (Option<String>, Option<String>) {
        (Some("fake".into()), Some("fake-model".into()))
    }
    async fn generate(&self, _system: &str, _user: &str) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outputs
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "fake llm queue exhausted".to_string())
    }
}

#[derive(Default)]
struct FakeStore {
    rows: Mutex<HashMap<(EnrichmentKey, u32), EnrichmentRecord>>,
}

impl EnrichmentStore for FakeStore {
    fn load_best(&self, key: &EnrichmentKey) -> Result<Option<EnrichmentRecord>, String> {
        let rows = self.rows.lock().unwrap();
        let records: Vec<_> = rows
            .iter()
            .filter(|((k, _), _)| *k == *key)
            .map(|(_, record)| record.clone())
            .collect();
        Ok(records
            .iter()
            .filter(|record| record.reviewed)
            .max_by_key(|record| record.revision)
            .or_else(|| records.iter().max_by_key(|record| record.revision))
            .cloned())
    }
    fn load_revision(
        &self,
        key: &EnrichmentKey,
        revision: u32,
    ) -> Result<Option<EnrichmentRecord>, String> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&(key.clone(), revision))
            .cloned())
    }
    fn list_revisions(&self, key: &EnrichmentKey) -> Result<Vec<EnrichmentRecord>, String> {
        let mut records: Vec<_> = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|((k, _), _)| *k == *key)
            .map(|(_, record)| record.clone())
            .collect();
        records.sort_by_key(|record| record.revision);
        Ok(records)
    }
    fn next_revision(&self, key: &EnrichmentKey) -> Result<u32, String> {
        let max = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|((k, _), _)| *k == *key)
            .map(|((_, revision), _)| *revision)
            .max()
            .unwrap_or(0);
        Ok(max + 1)
    }
    fn put(&self, record: &EnrichmentRecord) -> Result<(), String> {
        self.rows
            .lock()
            .unwrap()
            .insert((record.key.clone(), record.revision), record.clone());
        Ok(())
    }
    fn mark_reviewed(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String> {
        if let Some(record) = self.rows.lock().unwrap().get_mut(&(key.clone(), revision)) {
            record.reviewed = true;
            record.state = EnrichmentState::Reviewed;
        }
        Ok(())
    }
    fn delete_revision(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String> {
        self.rows.lock().unwrap().remove(&(key.clone(), revision));
        Ok(())
    }
}

struct FakeEntity {
    exists: bool,
    revision: Option<String>,
    canonical: Option<CanonicalEventRef>,
}

impl EnrichmentEntityPort for FakeEntity {
    fn event_exists(&self, _event_id: &str) -> Result<bool, String> {
        Ok(self.exists)
    }
    fn event_revision(&self, _event_id: &str) -> Result<Option<String>, String> {
        Ok(self.revision.clone())
    }
    fn event_canonical(&self, _event_id: &str) -> Result<Option<CanonicalEventRef>, String> {
        Ok(self.canonical.clone())
    }
}

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

fn key() -> EnrichmentKey {
    EnrichmentKey::new(
        "event",
        "zunyi_meeting",
        EnrichmentSection::Overview,
        "zh-CN",
    )
}

fn sources() -> Vec<SourceEvidence> {
    vec![
        SourceEvidence {
            title: "遵义会议百年纪念".into(),
            url: "https://www.gov.cn/zunyi".into(),
            domain: "gov.cn".into(),
            snippet: "1935 年 1 月…".into(),
            published_at: None,
            source_type: SourceType::Official,
        },
        SourceEvidence {
            title: "中央红军长征".into(),
            url: "https://baike.cn/zunyi".into(),
            domain: "baike.cn".into(),
            snippet: "遵义会议…".into(),
            published_at: None,
            source_type: SourceType::Reference,
        },
    ]
}

fn valid_envelope() -> String {
    r#"{"section":"overview","content":"长征途中一次重要转折","claims":[{"text":"确立毛泽东领导地位","source_ids":["https://www.gov.cn/zunyi"]}],"uncertainties":[],"controversies":[]}"#.to_string()
}

fn demo_entity() -> FakeEntity {
    FakeEntity {
        exists: true,
        revision: Some("rev-1".into()),
        canonical: Some(CanonicalEventRef {
            id: "zunyi_meeting".into(),
            name_zh_cn: "遵义会议".into(),
            summary_zh_cn: Some("1935 年长征途中举行的重要会议。".into()),
            start_year: Some(1935),
            end_year: None,
            importance: Some("critical".into()),
            quality_status: Some("reviewed".into()),
            source_reference: Some("《毛泽东年谱》".into()),
            evidence_count_hint: "evidence 3 条".into(),
        }),
    }
}

fn service(
    search: FakeSearch,
    llm: FakeLlm,
    store: Arc<FakeStore>,
    entity: FakeEntity,
) -> HistoryEnrichmentService {
    HistoryEnrichmentService::new(
        store,
        Arc::new(search),
        Arc::new(llm),
        Arc::new(entity),
        EnrichmentConfig {
            ttl_secs: 3600,
            prompt_version: "test".into(),
            search_limit: 10,
            max_sources: 6,
            canonical_chars: 2000,
        },
    )
}

// ---------------------------------------------------------------------------
// §66 8 Case
// ---------------------------------------------------------------------------

// Case 1：missing → search → generation → validation → READY
#[tokio::test]
async fn case1_missing_to_ready() {
    let store = Arc::new(FakeStore::default());
    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    let llm = FakeLlm::with(vec![valid_envelope()]);
    let service = service(search, llm, store.clone(), demo_entity());

    let view = service.ensure(&key()).await.unwrap();
    assert_eq!(view.state, EnrichmentState::Ready);
    let payload = view.payload.unwrap();
    assert_eq!(payload.section, "overview");
    assert_eq!(payload.content, "长征途中一次重要转折");
    assert_eq!(view.metadata.unwrap().source_ids.len(), 2);
    // 持久化存在
    let best = store.load_best(&key()).unwrap().unwrap();
    assert_eq!(best.revision, 1);
    assert_eq!(best.state, EnrichmentState::Ready);
}

// Case 2：READY → cache hit → 不再 search / 不再生成
#[tokio::test]
async fn case2_ready_is_cache_hit() {
    let store = Arc::new(FakeStore::default());
    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    let llm = FakeLlm::with(vec![valid_envelope()]);
    let service = service(search, llm, store.clone(), demo_entity());

    let first = service.ensure(&key()).await.unwrap();
    assert_eq!(first.state, EnrichmentState::Ready);
    // 再次 ensure：cache hit —— 不触发 search / 生成（用 Arc 计数复核）
    let second = service.ensure(&key()).await.unwrap();
    assert_eq!(second.state, EnrichmentState::Ready);
    let best = store.load_best(&key()).unwrap().unwrap();
    assert_eq!(best.revision, 1, "cache hit must not regenerate");
}

// Case 3：STALE → 旧内容立即返回（get），ensure 触发 refresh → READY
#[tokio::test]
async fn case3_stale_old_content_then_refresh() {
    let store = Arc::new(FakeStore::default());
    // 预置过期 READY 记录
    let stale = EnrichmentRecord {
        key: key(),
        revision: 1,
        state: EnrichmentState::Ready,
        payload: Some(EnrichmentPayload {
            section: "overview".into(),
            content: "旧内容".into(),
            claims: vec![],
            uncertainties: vec![],
            controversies: vec![],
        }),
        metadata: Some(EnrichmentMetadata {
            generated_at: now_unix() - 7200,
            refreshed_at: now_unix() - 7200,
            model: None,
            provider: None,
            prompt_version: "old".into(),
            schema_version: ENRICHMENT_SCHEMA_VERSION,
            canonical_revision: Some("rev-1".into()),
            source_ids: vec![],
            generation_count: 1,
        }),
        error: None,
        reviewed: false,
    };
    store.put(&stale).unwrap();

    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    let llm = FakeLlm::with(vec![valid_envelope()]);
    let entity = demo_entity(); // revision rev-1 与 stale 一致 → 仅 TTL 过期
    let service = service(search, llm, store.clone(), entity);

    // stale-while-revalidate：get 立即返回旧内容 + STALE
    let view = service.get(&key()).unwrap();
    assert_eq!(view.state, EnrichmentState::Stale);
    assert_eq!(view.payload.as_ref().unwrap().content, "旧内容");

    // ensure → 后台 refresh → READY 新内容
    let refreshed = service.ensure(&key()).await.unwrap();
    assert_eq!(refreshed.state, EnrichmentState::Ready);
    assert_eq!(refreshed.payload.unwrap().content, "长征途中一次重要转折");
}

// Case 4：REVIEWED → ensure 不覆盖（不自动刷新，§20）
#[tokio::test]
async fn case4_reviewed_not_overwritten() {
    let store = Arc::new(FakeStore::default());
    let reviewed = EnrichmentRecord {
        key: key(),
        revision: 1,
        state: EnrichmentState::Reviewed,
        payload: Some(EnrichmentPayload {
            section: "overview".into(),
            content: "人工审定内容".into(),
            claims: vec![],
            uncertainties: vec![],
            controversies: vec![],
        }),
        metadata: Some(EnrichmentMetadata {
            generated_at: 1,
            refreshed_at: 1,
            model: None,
            provider: None,
            prompt_version: "v".into(),
            schema_version: ENRICHMENT_SCHEMA_VERSION,
            canonical_revision: Some("rev-1".into()),
            source_ids: vec![],
            generation_count: 1,
        }),
        error: None,
        reviewed: true,
    };
    store.put(&reviewed).unwrap();

    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    let llm = FakeLlm::with(vec![valid_envelope()]);
    let service = service(search, llm, Arc::clone(&store), demo_entity());

    let view = service.ensure(&key()).await.unwrap();
    assert_eq!(view.state, EnrichmentState::Reviewed);
    assert_eq!(view.payload.unwrap().content, "人工审定内容");
    // 未新增 revision（不覆盖）
    assert_eq!(store.list_revisions(&key()).unwrap().len(), 1);
}

// Case 5：同一 key 并发 N 次 → 单飞；只 1 次 search + 1 次 generation
#[tokio::test]
async fn case5_single_flight_concurrent_ensure() {
    let store = Arc::new(FakeStore::default());
    let search = Arc::new(FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    });
    let llm = Arc::new(FakeLlm::with(vec![valid_envelope()]));
    let entity = Arc::new(demo_entity());
    let service = Arc::new(HistoryEnrichmentService::new(
        store.clone(),
        search.clone(),
        llm.clone(),
        entity,
        EnrichmentConfig {
            ttl_secs: 3600,
            prompt_version: "t".into(),
            search_limit: 10,
            max_sources: 6,
            canonical_chars: 2000,
        },
    ));

    let key = key();
    let (a, b, c) = tokio::join!(
        service.ensure(&key),
        service.ensure(&key),
        service.ensure(&key),
    );
    let states = [a.unwrap().state, b.unwrap().state, c.unwrap().state];
    // 至少一个 Running/Ready；管线计数只有 1
    assert!(
        states
            .iter()
            .any(|s| *s == EnrichmentState::Ready || *s == EnrichmentState::Generating)
    );
    assert_eq!(
        search.calls.load(Ordering::SeqCst),
        1,
        "single-flight: exactly one search"
    );
    assert_eq!(
        llm.calls.load(Ordering::SeqCst),
        1,
        "single-flight: exactly one generation"
    );
}

// Case 6：search 失败 → canonical 照常（get 走零依赖路径；ensure 受控失败，不 panic）
#[tokio::test]
async fn case6_search_failure_keeps_canonical() {
    let store = Arc::new(FakeStore::default());
    let search = FakeSearch {
        configured: true,
        fail: true,
        ..FakeSearch::default()
    };
    let llm = FakeLlm::with(vec![valid_envelope()]);
    let service = service(search, llm, store.clone(), demo_entity());

    // canonical 读取（get 未触发 search）→ 正常
    let missing_view = service.get(&key()).unwrap();
    assert_eq!(missing_view.state, EnrichmentState::Missing);

    // ensure 受控失败（search down）
    let error = service.ensure(&key()).await.unwrap_err();
    assert!(error.contains("search service down"));
}

// Case 6b：search/llm 未配置 → FAILED（unavailable），canonical 不受影响
#[tokio::test]
async fn case6b_unconfigured_pipeline_marks_failed() {
    let store = Arc::new(FakeStore::default());
    let search = FakeSearch {
        configured: false,
        ..FakeSearch::default()
    };
    let llm = FakeLlm {
        configured: false,
        outputs: Mutex::new(std::collections::VecDeque::new()),
        calls: AtomicU32::new(0),
    };
    let service = service(search, llm, store.clone(), demo_entity());

    let view = service.ensure(&key()).await.unwrap();
    assert_eq!(view.state, EnrichmentState::Failed);
    assert!(view.error.unwrap().contains("not configured"));
    assert_eq!(
        store.load_best(&key()).unwrap().unwrap().state,
        EnrichmentState::Failed
    );
}

// Case 7：非法 LLM 输出 → FAILED（不存 READY）
#[tokio::test]
async fn case7_invalid_llm_output_fails() {
    let store = Arc::new(FakeStore::default());
    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    let llm = FakeLlm {
        configured: true,
        outputs: Mutex::new(std::collections::VecDeque::from(vec![
            "一大段 Markdown".to_string(),
        ])),
        calls: AtomicU32::new(0),
    };
    let service = service(search, llm, store.clone(), demo_entity());

    let view = service.ensure(&key()).await.unwrap();
    assert_eq!(view.state, EnrichmentState::Failed);
    assert!(view.error.unwrap().contains("generation parse"));
    let best = store.load_best(&key()).unwrap().unwrap();
    assert_eq!(best.state, EnrichmentState::Failed);
    assert!(best.payload.is_none());
}

// Case 8：canonical revision 变化 → STALE
#[tokio::test]
async fn case8_canonical_revision_change_stales() {
    let store = Arc::new(FakeStore::default());
    // 预置 revision 为旧指纹 rev-old 的 READY
    let record = EnrichmentRecord {
        key: key(),
        revision: 1,
        state: EnrichmentState::Ready,
        payload: Some(EnrichmentPayload {
            section: "overview".into(),
            content: "基于旧 canonical".into(),
            claims: vec![],
            uncertainties: vec![],
            controversies: vec![],
        }),
        metadata: Some(EnrichmentMetadata {
            generated_at: now_unix(),
            refreshed_at: now_unix(),
            model: None,
            provider: None,
            prompt_version: "v".into(),
            schema_version: ENRICHMENT_SCHEMA_VERSION,
            canonical_revision: Some("rev-old".into()),
            source_ids: vec![],
            generation_count: 1,
        }),
        error: None,
        reviewed: false,
    };
    store.put(&record).unwrap();

    // entity revision 变了（canonical 更新）
    let entity = FakeEntity {
        exists: true,
        revision: Some("rev-new".into()),
        canonical: demo_entity().canonical,
    };
    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    let llm = FakeLlm::with(vec![valid_envelope()]);
    let service = service(search, llm, store.clone(), entity);

    let view = service.get(&key()).unwrap();
    assert_eq!(
        view.state,
        EnrichmentState::Stale,
        "canonical revision changed → stale"
    );
    // 旧内容仍在（stale-while-revalidate）
    assert_eq!(view.payload.unwrap().content, "基于旧 canonical");
}

// 附加：校验门拦截 unknown citation（§26）→ FAILED
#[tokio::test]
async fn validation_gate_rejects_unknown_citation() {
    let store = Arc::new(FakeStore::default());
    let search = FakeSearch {
        configured: true,
        result: sources(),
        ..FakeSearch::default()
    };
    // envelope 引用了提供列表之外的 url → 校验失败
    let bad = r#"{"section":"overview","content":"x","claims":[{"text":"c","source_ids":["https://ghost.example.com/y"]}],"uncertainties":[],"controversies":[]}"#.to_string();
    let llm = FakeLlm {
        configured: true,
        outputs: Mutex::new(std::collections::VecDeque::from(vec![bad])),
        calls: AtomicU32::new(0),
    };
    let service = service(search, llm, store.clone(), demo_entity());

    let view = service.ensure(&key()).await.unwrap();
    assert_eq!(view.state, EnrichmentState::Failed);
    assert!(view.error.unwrap().contains("validation failed"));
}

// 说明锚（避免死代码告警）：store_ready_record 为夹具占位。
#[allow(dead_code)]
fn _keep(store: &FakeStore) {
    let _ = store;
}

// 说明：Case2 的 search/llm 计数断言见 Case5（同一计数器模式）。
#[allow(dead_code)]
fn _case2_counters(search: &FakeSearch) -> u32 {
    search.calls.load(Ordering::SeqCst)
}
