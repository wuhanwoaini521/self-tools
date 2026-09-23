//! Personal Memory — 纯数据模型（V6 Track A）。
//!
//! 边界（V6 §3）：
//! - `Conversation != Personal Memory`：会话消息永不自动成为 MemoryItem；
//! - `Business Knowledge != Personal Memory`：History/Travel/Geography 数据不属于此；
//! - `Document != Memory`：整份文档不进 Memory；
//! - 每条 Memory 必须带**来源**与**生命周期**（created/updated/last_used/expires）。
//!
//! 本模块无任何 IO：持久化在 infrastructure（`config/memory.db`），
//! 写入 gate 在 `core::memory::gate`。

use serde::{Deserialize, Serialize};

/// Memory 类别（第一版只少量明确类别，V6 §9）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// 偏好（如「喜欢历史/美食/摄影类型的旅行」）。
    Preference,
    /// 个人事实（如「家在杭州」）。
    PersonalFact,
    /// 项目事实（如「self-tools 是个人中心项目」）。
    ProjectFact,
    /// 环境（如「家中 Personal Server 是 macOS」）。
    Environment,
    /// 习惯/例行（如「周末整理资料」）。
    Routine,
    /// 用户给 AI 的长期指令（如「搜索默认只搜指定目录」）。
    Instruction,
}

impl MemoryCategory {
    pub const ALL: [MemoryCategory; 6] = [
        MemoryCategory::Preference,
        MemoryCategory::PersonalFact,
        MemoryCategory::ProjectFact,
        MemoryCategory::Environment,
        MemoryCategory::Routine,
        MemoryCategory::Instruction,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryCategory::Preference => "preference",
            MemoryCategory::PersonalFact => "personal_fact",
            MemoryCategory::ProjectFact => "project_fact",
            MemoryCategory::Environment => "environment",
            MemoryCategory::Routine => "routine",
            MemoryCategory::Instruction => "instruction",
        }
    }

    /// 中文标签（UI / prompt 展示）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            MemoryCategory::Preference => "偏好",
            MemoryCategory::PersonalFact => "个人事实",
            MemoryCategory::ProjectFact => "项目事实",
            MemoryCategory::Environment => "环境",
            MemoryCategory::Routine => "习惯",
            MemoryCategory::Instruction => "指令",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// Memory 生命周期状态（V6 §12）。`Expired` 由 `expires_at` 派生，也会落库。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// 待确认（模型只能产生此状态）。
    Candidate,
    /// 已生效（仅用户确认可达成）。
    Active,
    /// 用户拒绝。
    Rejected,
    /// 已归档（可恢复；排除默认检索）。
    Archived,
    /// 已过期。
    Expired,
}

impl MemoryStatus {
    pub const ALL: [MemoryStatus; 5] = [
        MemoryStatus::Candidate,
        MemoryStatus::Active,
        MemoryStatus::Rejected,
        MemoryStatus::Archived,
        MemoryStatus::Expired,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryStatus::Candidate => "candidate",
            MemoryStatus::Active => "active",
            MemoryStatus::Rejected => "rejected",
            MemoryStatus::Archived => "archived",
            MemoryStatus::Expired => "expired",
        }
    }

    /// 默认检索只考虑 ACTIVE（V6 §95 Case 4）。
    #[must_use]
    pub fn is_retrievable(self) -> bool {
        matches!(self, MemoryStatus::Active)
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// Memory 来源类型（V6 §13）：每条 Memory 必须知道「从哪里来的」。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySourceType {
    /// 用户明确表达保存意图。
    ExplicitUser,
    /// 对话中产生的候选（默认不 Active）。
    ConversationCandidate,
    /// 导入（文件/批量）。
    Import,
    /// 系统派生。
    System,
}

impl MemorySourceType {
    pub const ALL: [MemorySourceType; 4] = [
        MemorySourceType::ExplicitUser,
        MemorySourceType::ConversationCandidate,
        MemorySourceType::Import,
        MemorySourceType::System,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MemorySourceType::ExplicitUser => "explicit_user",
            MemorySourceType::ConversationCandidate => "conversation_candidate",
            MemorySourceType::Import => "import",
            MemorySourceType::System => "system",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// 敏感度（V6 §69）。`Sensitive` 永不进入模型请求（工具与自动注入都过滤）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySensitivity {
    Normal,
    Private,
    Sensitive,
}

impl MemorySensitivity {
    pub const ALL: [MemorySensitivity; 3] = [
        MemorySensitivity::Normal,
        MemorySensitivity::Private,
        MemorySensitivity::Sensitive,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MemorySensitivity::Normal => "normal",
            MemorySensitivity::Private => "private",
            MemorySensitivity::Sensitive => "sensitive",
        }
    }

    /// 是否允许进入模型上下文（V6 §69：`Sensitive` 默认不入模型）。
    #[must_use]
    pub fn is_model_visible(self) -> bool {
        !matches!(self, MemorySensitivity::Sensitive)
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|sensitivity| sensitivity.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// 待写入的 Memory 草案（还未获得 id / 时间戳 / 状态）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryDraft {
    pub category: MemoryCategory,
    pub content: String,
    pub source_type: MemorySourceType,
    /// 来源引用（会话 id / 文件路径 / 工具名……「为什么保存」）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<String>,
    pub sensitivity: MemorySensitivity,
    /// 模型/规则给出的置信度（0.0–1.0）。
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// 附加元数据（自由形状；不得承载 secret）。
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl Default for MemoryDraft {
    fn default() -> Self {
        Self {
            category: MemoryCategory::PersonalFact,
            content: String::new(),
            source_type: MemorySourceType::ConversationCandidate,
            source_reference: None,
            sensitivity: MemorySensitivity::Normal,
            confidence: 0.5,
            expires_at: None,
            metadata: serde_json::Value::Null,
        }
    }
}

impl MemoryDraft {
    #[must_use]
    pub fn new(category: MemoryCategory, content: impl Into<String>) -> Self {
        Self {
            category,
            content: content.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_source(mut self, source_type: MemorySourceType, reference: Option<String>) -> Self {
        self.source_type = source_type;
        self.source_reference = reference;
        self
    }

    #[must_use]
    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    #[must_use]
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// 一条长期用户信息（V6 §11）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub category: MemoryCategory,
    pub content: String,
    pub status: MemoryStatus,
    pub source_type: MemorySourceType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub confidence: f32,
    pub sensitivity: MemorySensitivity,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl MemoryItem {
    /// 由草案构造候选（模型路径的唯一出口，V6 §113）。
    #[must_use]
    pub fn candidate(id: impl Into<String>, draft: &MemoryDraft, now: i64) -> Self {
        Self {
            id: id.into(),
            category: draft.category,
            content: draft.content.trim().to_string(),
            status: MemoryStatus::Candidate,
            source_type: draft.source_type,
            source_reference: draft.source_reference.clone(),
            created_at: now,
            updated_at: now,
            last_used_at: None,
            expires_at: draft.expires_at,
            confidence: draft.confidence,
            sensitivity: draft.sensitivity,
            metadata: draft.metadata.clone(),
        }
    }

    /// 派生过期状态（`expires_at` 已到且当前仍是 Active，V6 §12）。
    #[must_use]
    pub fn is_expired_at(&self, now: i64) -> bool {
        matches!(self.status, MemoryStatus::Active) && self.expires_at.is_some_and(|at| at <= now)
    }

    /// 是否可进入检索结果（模型可见路径，V6 §22/§23/§69）。
    #[must_use]
    pub fn is_retrievable_at(&self, now: i64) -> bool {
        self.status.is_retrievable()
            && !self.is_expired_at(now)
            && self.sensitivity.is_model_visible()
    }
}

/// 关键词检索参数（V6 §21：keyword + category + limit，不引入 Vector DB）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryQuery {
    /// 关键词（空 = 不过滤，按时间倒序）。
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<MemoryCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MemoryStatus>,
    /// 是否包含 Sensitive（自动注入与模型工具恒为 false）。
    pub include_sensitive: bool,
    pub limit: usize,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: None,
            status: None,
            include_sensitive: false,
            limit: 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip_snake_case() {
        for category in MemoryCategory::ALL {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(json, format!("\"{}\"", category.as_str()));
            let back: MemoryCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(back, category);
            assert_eq!(MemoryCategory::parse(category.as_str()), Some(category));
            assert!(!category.label().is_empty());
        }
        assert_eq!(MemoryCategory::parse("nope"), None);
    }

    #[test]
    fn status_and_source_round_trip() {
        for status in MemoryStatus::ALL {
            let json = serde_json::to_string(&status).unwrap();
            let back: MemoryStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
            assert_eq!(MemoryStatus::parse(status.as_str()), Some(status));
        }
        for source in MemorySourceType::ALL {
            let json = serde_json::to_string(&source).unwrap();
            let back: MemorySourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, source);
        }
        for sensitivity in MemorySensitivity::ALL {
            let json = serde_json::to_string(&sensitivity).unwrap();
            let back: MemorySensitivity = serde_json::from_str(&json).unwrap();
            assert_eq!(back, sensitivity);
            assert_eq!(
                sensitivity.is_model_visible(),
                sensitivity != MemorySensitivity::Sensitive
            );
        }
    }

    #[test]
    fn candidate_from_draft_is_never_active() {
        let draft = MemoryDraft::new(
            MemoryCategory::Environment,
            "  Docker 数据在 /Volumes/Data/docker ",
        )
        .with_source(MemorySourceType::ExplicitUser, Some("chat:1".into()));
        let item = MemoryItem::candidate("m1", &draft, 1_000);
        assert_eq!(item.status, MemoryStatus::Candidate);
        assert_eq!(item.content, "Docker 数据在 /Volumes/Data/docker");
        assert_eq!(item.created_at, 1_000);
        assert_eq!(item.updated_at, 1_000);
        assert_eq!(item.last_used_at, None);
    }

    #[test]
    fn retrievable_requires_active_visible_unexpired() {
        let draft = MemoryDraft::new(MemoryCategory::Preference, "喜欢历史旅行");
        let mut item = MemoryItem::candidate("m1", &draft, 10);
        assert!(!item.is_retrievable_at(10)); // candidate 不可检索

        item.status = MemoryStatus::Active;
        assert!(item.is_retrievable_at(10));

        item.sensitivity = MemorySensitivity::Sensitive;
        assert!(!item.is_retrievable_at(10)); // §69
        item.sensitivity = MemorySensitivity::Normal;

        item.expires_at = Some(20);
        assert!(item.is_retrievable_at(19));
        assert!(item.is_expired_at(20));
        assert!(!item.is_retrievable_at(20));

        item.status = MemoryStatus::Archived;
        item.expires_at = None;
        assert!(!item.is_retrievable_at(10));
    }

    #[test]
    fn memory_query_defaults_exclude_sensitive() {
        let query = MemoryQuery::default();
        assert!(!query.include_sensitive);
        assert_eq!(query.limit, 20);
    }
}
