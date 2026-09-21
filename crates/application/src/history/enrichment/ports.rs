//! History Enrichment 端口（V5 Gate 2/3）。
//!
//! application 只依赖端口；Search / LLM / 持久化实现由组合根（desktop）按设置装配；
//! Canonical 读取复用 `HistoryQueryPort`（V3 只读）。

use std::sync::Arc;

use async_trait::async_trait;
use devtoolbox_core::history_enrichment::{
    EnrichmentKey, EnrichmentRecord, EnrichmentSection, EnrichmentState, EnrichmentView,
};
use devtoolbox_core::history_records::EventResult;

use crate::history::HistoryQueryPort;

/// 来源类型分类（§23 权威加权：official > museum > archive > university > academic >
/// reference > general web）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Official,
    Museum,
    Archive,
    University,
    Academic,
    Reference,
    General,
}

/// 一份证据来源（§22/§24：元数据 + 短 excerpt，不保存整页）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEvidence {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    pub published_at: Option<i64>,
    pub source_type: SourceType,
}

/// 搜索端口（复用既有搜索生态；desktop 适配器由组合根按设置装配）。
#[async_trait]
pub trait EnrichmentSearchPort: Send + Sync {
    fn configured(&self) -> bool;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SourceEvidence>, String>;
}

/// 结构化生成端口（底层为统一 ChatModelProvider）。
#[async_trait]
pub trait EnrichmentLlmPort: Send + Sync {
    fn configured(&self) -> bool;
    /// (provider 名, model 名)，写入缓存元数据（§18）。
    fn describe(&self) -> (Option<String>, Option<String>);
    async fn generate(&self, system: &str, user: &str) -> Result<String, String>;
}

/// Canonical 事件摘要（供生成 prompt；紧凑字段，不去读整页长文）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CanonicalEventRef {
    pub id: String,
    pub name_zh_cn: String,
    pub summary_zh_cn: Option<String>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub importance: Option<String>,
    pub quality_status: Option<String>,
    pub source_reference: Option<String>,
    pub evidence_count_hint: String,
}

/// Canonical 读取端口：entity 存在性与修订指纹（只读，绝不写回）。
pub trait EnrichmentEntityPort: Send + Sync {
    /// 事件 canonical 是否存在。
    fn event_exists(&self, event_id: &str) -> Result<bool, String>;
    /// 事件 canonical 修订指纹（quality_status + source_reference + source_ids 摘要）。
    fn event_revision(&self, event_id: &str) -> Result<Option<String>, String>;
    /// 事件 compact canonical（供生成 prompt；无则 None）。
    fn event_canonical(&self, event_id: &str) -> Result<Option<CanonicalEventRef>, String>;
}

/// 富化存储端口（持久化实现见 infrastructure）。
pub trait EnrichmentStore: Send + Sync {
    /// 读最佳记录（已审定行优先，否则最高 revision）。
    fn load_best(&self, key: &EnrichmentKey) -> Result<Option<EnrichmentRecord>, String>;
    /// 读指定 revision。
    fn load_revision(
        &self,
        key: &EnrichmentKey,
        revision: u32,
    ) -> Result<Option<EnrichmentRecord>, String>;
    /// 列出该 entity+section 的所有 revision（供 UI / 审计）。
    fn list_revisions(&self, key: &EnrichmentKey) -> Result<Vec<EnrichmentRecord>, String>;
    /// 下一可用 revision（已审定行重生成 = 新 candidate，§20）。
    fn next_revision(&self, key: &EnrichmentKey) -> Result<u32, String>;
    /// 写入/更新一个 revision。
    fn put(&self, record: &EnrichmentRecord) -> Result<(), String>;
    /// 标记人工审定（automatic refresh 跳过）。
    fn mark_reviewed(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String>;
    /// 删除指定 revision（用户手动清理）。
    fn delete_revision(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Canonical 端口默认实现（复用 HistoryQueryPort，V3 只读数据面）
// ---------------------------------------------------------------------------

/// `EventResult` 的 revision 指纹：canonical 内容变化 → 指纹变化 → 富化转 STALE。
#[must_use]
pub fn event_revision_fingerprint(event: &EventResult) -> String {
    let mut acc = Vec::new();
    acc.push(event.id.clone());
    acc.push(event.quality_status.clone().unwrap_or_default());
    acc.push(event.source_reference.clone().unwrap_or_default());
    acc.push(event.source_ids.clone().unwrap_or_default());
    acc.push(event.summary_zh_cn.clone().unwrap_or_default());
    // 常规字符串哈希（不引入外部依赖）。
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    acc.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 基于 `Arc<dyn HistoryQueryPort>` 的 Canonical 端口（application 内实现，仅读 get_event）。
pub struct HistoryEntityPort {
    port: Arc<dyn HistoryQueryPort>,
}

impl HistoryEntityPort {
    #[must_use]
    pub fn new(port: Arc<dyn HistoryQueryPort>) -> Self {
        Self { port }
    }

    fn event(&self, event_id: &str) -> Result<Option<EventResult>, String> {
        self.port
            .get_event(event_id)
            .map_err(|error| error.to_string())
    }
}

impl EnrichmentEntityPort for HistoryEntityPort {
    fn event_exists(&self, event_id: &str) -> Result<bool, String> {
        Ok(self.event(event_id)?.is_some())
    }

    fn event_revision(&self, event_id: &str) -> Result<Option<String>, String> {
        Ok(self
            .event(event_id)?
            .map(|event| event_revision_fingerprint(&event)))
    }

    fn event_canonical(&self, event_id: &str) -> Result<Option<CanonicalEventRef>, String> {
        Ok(self.event(event_id)?.map(|event| {
            let evidence_count = self
                .port
                .get_event_evidences(event_id)
                .map(|rows| rows.len())
                .unwrap_or(0);
            CanonicalEventRef {
                id: event.id.clone(),
                name_zh_cn: event.name_zh_cn,
                summary_zh_cn: event.summary_zh_cn,
                start_year: event.start_year,
                end_year: event.end_year,
                importance: event.importance,
                quality_status: event.quality_status,
                source_reference: event.source_reference,
                evidence_count_hint: format!("evidence {evidence_count} 条"),
            }
        }))
    }
}

/// 富化运行器端口：组合根（desktop）实现的跨调用单飞入口；命令与 agent 工具共用。
#[async_trait]
pub trait EnrichmentRunnerPort: Send + Sync {
    /// 读取最佳记录（无外部调用）。
    fn get(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String>;
    /// 按需生成（含跨调用单飞；已在 Generate → Generating 状态）。
    async fn ensure(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String>;
    /// 手动刷新（Reviewed 也允许 → 新 revision 候选）。
    async fn refresh(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String>;
    /// 各 section 状态（UI「AI 解读」区）。
    fn sections(
        &self,
        entity_type: &str,
        entity_id: &str,
        locale: &str,
    ) -> Result<Vec<(EnrichmentSection, EnrichmentState)>, String>;
    /// 人工审定（automatic refresh 跳过）。
    fn mark_reviewed(&self, key: &EnrichmentKey) -> Result<(), String>;
}

/// 便捷构造（组合根 / 测试共用）。
#[must_use]
pub fn entity_port_from_history(port: Arc<dyn HistoryQueryPort>) -> Arc<dyn EnrichmentEntityPort> {
    Arc::new(HistoryEntityPort::new(port))
}

// 让 serde derive 可用（SourceType 需要 Serialize）。
use serde::Serialize;
