//! Personal Memory 用例（V6 Track A，§8-§26）。
//!
//! 这一层负责**写入 gate** 与生命周期，不负责持久化细节：
//! - 模型路径（`propose`）永远只产生 `CANDIDATE`；
//! - 只有用户确认（`confirm` / `save_confirmed`）能产生 `ACTIVE`；
//! - 归档可恢复；V6 不提供物理删除；
//! - 检索默认只返回 `ACTIVE` 且非 `SENSITIVE`。

use std::sync::Arc;

use devtoolbox_core::knowledge::{KnowledgeResult, KnowledgeSourceKind, snippet};
use devtoolbox_core::memory::{
    MemoryCategory, MemoryDraft, MemoryItem, MemoryQuery, MemorySensitivity, MemorySourceType,
    MemoryStatus, WriteIntent, resolve_status, source_type_for, validate_draft,
};

use crate::error::ApplicationError;
use crate::memory::ports::{MemoryStoreError, MemoryStorePort};
use crate::time::now_unix;

/// 服务配置（上限集中在配置，不散落）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryConfig {
    /// 管理页列表默认上限。
    pub list_limit: usize,
    /// 检索默认上限。
    pub search_limit: usize,
    /// 单条记忆进入检索结果时的片段上限（Memory 本身很短）。
    pub snippet_chars: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            list_limit: 100,
            search_limit: 20,
            snippet_chars: 300,
        }
    }
}

/// 各状态计数（管理页 / 观测；不含正文）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStats {
    pub active: usize,
    pub candidates: usize,
    pub archived: usize,
    pub rejected: usize,
    pub expired: usize,
}

impl MemoryStats {
    #[must_use]
    pub fn total(&self) -> usize {
        self.active + self.candidates + self.archived + self.rejected + self.expired
    }
}

/// Personal Memory 服务。
pub struct MemoryService {
    store: Arc<dyn MemoryStorePort>,
    config: MemoryConfig,
}

impl MemoryService {
    #[must_use]
    pub fn new(store: Arc<dyn MemoryStorePort>) -> Self {
        Self {
            store,
            config: MemoryConfig::default(),
        }
    }

    #[must_use]
    pub fn with_config(store: Arc<dyn MemoryStorePort>, config: MemoryConfig) -> Self {
        Self { store, config }
    }

    #[must_use]
    pub fn config(&self) -> &MemoryConfig {
        &self.config
    }

    // -----------------------------------------------------------------------
    // 写入路径
    // -----------------------------------------------------------------------

    /// 模型工具路径（`memory.save`）：**只产生 CANDIDATE**（V6 §14/§113）。
    pub fn propose(&self, draft: MemoryDraft) -> Result<MemoryItem, ApplicationError> {
        self.write(draft, WriteIntent::ModelTool)
    }

    /// 用户显式表达保存意图（「记住…」）：仍先落 CANDIDATE，由 UI 确认（§26）。
    pub fn propose_explicit(&self, draft: MemoryDraft) -> Result<MemoryItem, ApplicationError> {
        self.write(draft, WriteIntent::ExplicitUserIntent)
    }

    /// UI 确认：`CANDIDATE → ACTIVE`（V6 §15，唯一进入 ACTIVE 的路径之一）。
    pub fn confirm(&self, id: &str) -> Result<MemoryItem, ApplicationError> {
        let mut item = self
            .store
            .get(id)?
            .ok_or_else(|| memory_error(format!("记忆 `{id}` 不存在")))?;
        if item.status != MemoryStatus::Candidate {
            return Err(memory_error(format!(
                "记忆 `{id}` 当前状态为 {}，只有候选状态可以确认",
                item.status.as_str()
            )));
        }
        let now = now_unix();
        item.status = MemoryStatus::Active;
        item.updated_at = now;
        item.metadata = with_metadata(&item.metadata, "confirmed_by", "ui");
        item.metadata = with_metadata(&item.metadata, "confirmed_at", now);
        self.store.upsert(&item)?;
        Ok(item)
    }

    /// UI 直接保存（用户在确认卡上点「记住」并携带草案）：创建即 ACTIVE（§26）。
    pub fn save_confirmed(&self, draft: MemoryDraft) -> Result<MemoryItem, ApplicationError> {
        self.write(draft, WriteIntent::UiConfirmation)
    }

    /// 拒绝候选（用户点「不要」）。
    pub fn reject(&self, id: &str) -> Result<MemoryItem, ApplicationError> {
        self.transition(id, MemoryStatus::Rejected, "拒绝")
    }

    /// 归档（可恢复；归档后不参与默认检索，V6 §20/§95 Case 4）。
    pub fn archive(&self, id: &str) -> Result<MemoryItem, ApplicationError> {
        self.transition(id, MemoryStatus::Archived, "归档")
    }

    /// 编辑内容 / 类别 / 敏感度（管理页；重新走 secret 与长度校验）。
    pub fn update(
        &self,
        id: &str,
        content: &str,
        category: Option<MemoryCategory>,
        sensitivity: Option<MemorySensitivity>,
    ) -> Result<MemoryItem, ApplicationError> {
        let mut item = self
            .store
            .get(id)?
            .ok_or_else(|| memory_error(format!("记忆 `{id}` 不存在")))?;
        let mut draft = MemoryDraft {
            category: category.unwrap_or(item.category),
            content: content.to_string(),
            source_type: item.source_type,
            source_reference: item.source_reference.clone(),
            sensitivity: sensitivity.unwrap_or(item.sensitivity),
            confidence: item.confidence,
            expires_at: item.expires_at,
            metadata: item.metadata.clone(),
        };
        draft.content = draft.content.trim().to_string();
        validate_draft(&draft).map_err(memory_error)?;
        item.category = draft.category;
        item.content = draft.content;
        item.sensitivity = draft.sensitivity;
        item.updated_at = now_unix();
        self.store.upsert(&item)?;
        Ok(item)
    }

    /// 到期清理（幂等；返回本次过期的条数）。
    pub fn expire_due(&self) -> Result<usize, ApplicationError> {
        let now = now_unix();
        let due = self.store.query(&MemoryQuery {
            query: String::new(),
            category: None,
            status: Some(MemoryStatus::Active),
            include_sensitive: true,
            limit: self.config.list_limit.max(200),
        })?;
        let mut expired = 0usize;
        for mut item in due {
            if item.is_expired_at(now) {
                item.status = MemoryStatus::Expired;
                item.updated_at = now;
                self.store.upsert(&item)?;
                expired += 1;
            }
        }
        Ok(expired)
    }

    // -----------------------------------------------------------------------
    // 读取路径
    // -----------------------------------------------------------------------

    /// 单条读取。`include_sensitive = false`（模型/工具路径）时拒绝敏感内容（§69）。
    pub fn get(&self, id: &str, include_sensitive: bool) -> Result<MemoryItem, ApplicationError> {
        let item = self
            .store
            .get(id)?
            .ok_or_else(|| memory_error(format!("记忆 `{id}` 不存在")))?;
        if !include_sensitive && !item.sensitivity.is_model_visible() {
            return Err(memory_error(format!(
                "记忆 `{id}` 标记为敏感，默认不返回（请在知识页查看）"
            )));
        }
        Ok(item)
    }

    /// 列表（管理页：默认展示全部状态与敏感项由调用方显式开启）。
    pub fn list(&self, spec: &MemoryQuery) -> Result<Vec<MemoryItem>, ApplicationError> {
        let mut spec = spec.clone();
        if spec.limit == 0 {
            spec.limit = self.config.list_limit;
        }
        self.expire_due()?;
        let mut items = self.store.query(&spec)?;
        if !spec.include_sensitive {
            items.retain(|item| item.sensitivity.is_model_visible());
        }
        Ok(items)
    }

    /// 检索（模型/工具路径）：只返回 ACTIVE、非敏感，按相关性排序（§21）。
    pub fn search(
        &self,
        keyword: &str,
        category: Option<MemoryCategory>,
        limit: usize,
    ) -> Result<Vec<MemoryItem>, ApplicationError> {
        let limit = if limit == 0 {
            self.config.search_limit
        } else {
            limit
        };
        self.expire_due()?;
        let keyword = keyword.trim();
        // 端口语义：空白分隔关键词、命中任一即候选（CJK 由 bigram 提升召回）。
        let tokens = crate::text::keywords(keyword);
        let mut items = self.store.query(&MemoryQuery {
            query: tokens.join(" "),
            category,
            status: Some(MemoryStatus::Active),
            include_sensitive: false,
            limit: limit.saturating_mul(4).max(limit),
        })?;
        items.retain(|item| item.sensitivity.is_model_visible());
        rank_memories(&mut items, keyword);
        items.truncate(limit);
        if !items.is_empty() {
            let ids: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
            self.store.touch_used(&ids, now_unix())?;
        }
        Ok(items)
    }

    /// 记忆 → 统一检索结果（V6 §54）。
    pub fn to_knowledge_results(
        &self,
        items: &[MemoryItem],
        keyword: &str,
    ) -> Vec<KnowledgeResult> {
        items
            .iter()
            .map(|item| {
                let score = relevance(item, keyword);
                KnowledgeResult::new(
                    KnowledgeSourceKind::Memory,
                    item.id.clone(),
                    format!("{} · {}", item.category.label(), item.category.as_str()),
                    snippet(&item.content, self.config.snippet_chars),
                    score,
                )
                .with_metadata(serde_json::json!({
                    "category": item.category.as_str(),
                    "status": item.status.as_str(),
                    "sensitivity": item.sensitivity.as_str(),
                    "source_type": item.source_type.as_str(),
                    "updated_at": item.updated_at,
                }))
            })
            .collect()
    }

    pub fn stats(&self) -> Result<MemoryStats, ApplicationError> {
        let counts = self.store.count_by_status()?;
        let mut stats = MemoryStats::default();
        for (status, count) in counts {
            match status {
                MemoryStatus::Active => stats.active = count,
                MemoryStatus::Candidate => stats.candidates = count,
                MemoryStatus::Archived => stats.archived = count,
                MemoryStatus::Rejected => stats.rejected = count,
                MemoryStatus::Expired => stats.expired = count,
            }
        }
        Ok(stats)
    }

    pub fn category_counts(&self) -> Result<Vec<(MemoryCategory, usize)>, ApplicationError> {
        Ok(self.store.count_by_category()?)
    }

    // -----------------------------------------------------------------------
    // 内部
    // -----------------------------------------------------------------------

    fn write(
        &self,
        draft: MemoryDraft,
        intent: WriteIntent,
    ) -> Result<MemoryItem, ApplicationError> {
        let mut draft = draft;
        draft.content = draft.content.trim().to_string();
        validate_draft(&draft).map_err(memory_error)?;
        let explicit = intent == WriteIntent::ExplicitUserIntent
            || draft.source_type == MemorySourceType::ExplicitUser;
        let now = now_unix();
        let id = devtoolbox_core::knowledge::stable_id(
            "mem",
            &[
                draft.category.as_str(),
                &draft.content,
                &now.to_string(),
                intent_tag(intent),
            ],
        );
        let decision = resolve_status(intent);
        let mut item = MemoryItem::candidate(id, &draft, now);
        item.status = decision.status;
        item.source_type = source_type_for(&draft, explicit);
        if decision.status == MemoryStatus::Active {
            item.metadata = with_metadata(&item.metadata, "confirmed_by", "ui");
            item.metadata = with_metadata(&item.metadata, "confirmed_at", now);
        }
        self.store.upsert(&item)?;
        Ok(item)
    }

    fn transition(
        &self,
        id: &str,
        status: MemoryStatus,
        verb: &str,
    ) -> Result<MemoryItem, ApplicationError> {
        let mut item = self
            .store
            .get(id)?
            .ok_or_else(|| memory_error(format!("记忆 `{id}` 不存在")))?;
        if item.status == status {
            return Ok(item);
        }
        if item.status == MemoryStatus::Rejected && status == MemoryStatus::Archived {
            return Err(memory_error(format!("记忆 `{id}` 已被拒绝，无法{verb}")));
        }
        item.status = status;
        item.updated_at = now_unix();
        self.store.upsert(&item)?;
        Ok(item)
    }
}

fn intent_tag(intent: WriteIntent) -> &'static str {
    match intent {
        WriteIntent::ModelTool => "model",
        WriteIntent::ExplicitUserIntent => "explicit",
        WriteIntent::UiConfirmation => "ui",
    }
}

fn memory_error(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Memory {
        message: message.into(),
    }
}

impl From<MemoryStoreError> for ApplicationError {
    fn from(error: MemoryStoreError) -> Self {
        memory_error(error.0)
    }
}

fn with_metadata(
    metadata: &serde_json::Value,
    key: &str,
    value: impl serde::Serialize,
) -> serde_json::Value {
    let mut object = match metadata {
        serde_json::Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    object.insert(key.to_string(), serde_json::json!(value));
    serde_json::Value::Object(object)
}

/// 相关性打分（确定性规则，§21 不引入向量库）。
///
/// `score = 0.6 * 正文命中 + 0.2 * 类别标签命中 + 0.2 * 状态权重`
/// 其中「正文命中」= 整串子串命中 → 1.0；否则 = 命中 token 比例。
#[must_use]
pub fn relevance(item: &MemoryItem, keyword: &str) -> f32 {
    let content = item.content.to_lowercase();
    let needle = keyword.trim().to_lowercase();
    let content_score = if needle.is_empty() {
        0.5
    } else if content.contains(&needle) {
        1.0
    } else {
        let tokens: Vec<&str> = needle
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '，' | '、' | '?' | '？'))
            .filter(|token| !token.is_empty())
            .collect();
        if tokens.is_empty() {
            0.5
        } else {
            let hit = tokens
                .iter()
                .filter(|token| content.contains(**token))
                .count();
            hit as f32 / tokens.len() as f32
        }
    };
    let label = item.category.label().to_lowercase();
    let category_score: f32 = if !needle.is_empty() && (label.contains(&needle) || item.category.as_str().contains(&needle)) {
        1.0
    } else {
        0.0
    };
    let status_score: f32 = if item.status == MemoryStatus::Active {
        1.0
    } else {
        0.2
    };
    let score = 0.6 * content_score + 0.2 * category_score + 0.2 * status_score;
    score.clamp(0.0, 1.0)
}

/// 排序：分数降序 → 更新时间降序 → id（保证确定性）。
fn rank_memories(items: &mut [MemoryItem], keyword: &str) {
    items.sort_by(|left, right| {
        relevance(right, keyword)
            .partial_cmp(&relevance(left, keyword))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(right.updated_at.cmp(&left.updated_at))
            .then(left.id.cmp(&right.id))
    });
}
