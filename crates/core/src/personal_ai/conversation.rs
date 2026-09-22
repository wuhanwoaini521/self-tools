//! 会话历史契约（V11 §96-§101）。
//!
//! **Conversation ≠ Memory**：本模块描述的是「会话历史」——一次对话的消息序列与
//! 元数据。它**不是** Personal Memory：内容不会自动成为 `MemoryItem`，记忆服务
//! (`MemoryService` / 记忆检索) **绝不读取**本模块产生的数据（V6 §3 边界在此重申）。
//! 同理，Memory 也不会回流成会话历史：两个域各有唯一 Source of Truth。
//!
//! 三条硬约束：
//! 1. **只存可展示内容**：绝不写入隐藏推理（reasoning / 思维链）、secret、token、
//!    API key、完整 prompt。字段白名单见 [`ConversationMessage`]；
//! 2. **纯 serde 契约，无 IO**：持久化在 infrastructure（SQLite `conversations.db`），
//!    编排在 application，端口契约在 `application::personal_ai::conversation`；
//! 3. **向前/向后兼容**：`#[serde(default)]` 让旧行、缺字段、未来新增字段都能解码，
//!    历史库升级不需要迁移脚本（与 `core::settings::AppSettings` 同策略）。

use serde::{Deserialize, Serialize};

/// 单条消息正文上限（字符数，超出即硬截断）。
///
/// 会话历史是「对话回放」，不是文档存储：20000 字符足够覆盖任何单次模型回答与
/// 工具结果，又能防止把整份文档 / 日志 / 转储塞进历史行把卡片拖死。
pub const CONVERSATION_MAX_CONTENT_CHARS: usize = 20_000;

/// 用户未命名会话时的标题（列表页兜底；不返回空标题）。
pub const DEFAULT_CONVERSATION_TITLE: &str = "新对话";

/// 消息角色（V11 §97：只有三种；system prompt 属于请求，不属于历史）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRole {
    /// 用户输入。
    #[default]
    User,
    /// 模型回答。**只含最终答案**；隐藏推理（reasoning content）不入库。
    Assistant,
    /// 工具执行结果回填。
    Tool,
}

impl ConversationRole {
    pub const ALL: [ConversationRole; 3] = [
        ConversationRole::User,
        ConversationRole::Assistant,
        ConversationRole::Tool,
    ];

    /// 稳定存储值（SQLite `role` 列 / serde 枚举值）。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ConversationRole::User => "user",
            ConversationRole::Assistant => "assistant",
            ConversationRole::Tool => "tool",
        }
    }

    /// 解析存储值；未知值返回 `None`（调用方必须兜底，绝不 panic）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|role| role.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// 一条会话消息（字段白名单：除此五个字段外不允许再持久化任何内容）。
///
/// `#[serde(default)]`：缺字段的旧行解码为默认值，新增字段不影响旧客户端。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationMessage {
    pub role: ConversationRole,
    /// 可展示正文；持久化前会被硬截断到 [`CONVERSATION_MAX_CONTENT_CHARS`] 字符。
    pub content: String,
    /// 产出该消息的模型服务方（如 `openai-compatible`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// 产出该消息的模型名（如 `step-5-preview`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 消息落库时间（Unix epoch 秒）；缺失表示「跟随会话操作时间」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

impl ConversationMessage {
    #[must_use]
    pub fn new(role: ConversationRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            provider: None,
            model: None,
            created_at: None,
        }
    }

    /// 附带 provider / model 元数据（模型回答与工具回填都要标注来源）。
    #[must_use]
    pub fn with_provider(
        mut self,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        self.provider = Some(provider.into());
        self.model = Some(model.into());
        self
    }
}

/// 会话列表项（只含元数据与计数，**不含正文**：列表页不需要正文，
/// 也避免把完整历史一次性推给前端）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationSummary {
    pub conversation_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_origin: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived: bool,
    /// 消息条数（计数，不含正文）。
    pub message_count: usize,
}

impl ConversationSummary {
    #[must_use]
    pub fn from_conversation(conversation: &Conversation) -> Self {
        Self {
            conversation_id: conversation.conversation_id.clone(),
            title: conversation.title.clone(),
            module_origin: conversation.module_origin.clone(),
            created_at: conversation.created_at,
            updated_at: conversation.updated_at,
            archived: conversation.archived,
            message_count: conversation.messages.len(),
        }
    }
}

/// 完整会话（元数据 + 有序消息序列）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Conversation {
    pub conversation_id: String,
    pub title: String,
    /// 创建时间（Unix epoch 秒）。
    pub created_at: i64,
    /// 最近一次变更时间（Unix epoch 秒；单调不回拨）。
    pub updated_at: i64,
    /// 消息序列，按时间正序（最早在前）。
    pub messages: Vec<ConversationMessage>,
    /// 发起源模块（如 `travel` / `history`）；None 表示通用对话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_origin: Option<String>,
    /// 已归档：列表默认隐藏，`load` 仍可读取（可恢复）。
    pub archived: bool,
}

impl Conversation {
    /// 列表项（自动同步归档标志与消息计数）。
    #[must_use]
    pub fn summary(&self) -> ConversationSummary {
        ConversationSummary::from_conversation(self)
    }
}

/// 硬截断正文到 [`CONVERSATION_MAX_CONTENT_CHARS`] 字符。
///
/// 按字符而非字节裁剪：中文 / emoji 不会被切成半个 UTF-8 序列。
/// 幂等：已截断的文本再次调用结果不变。
#[must_use]
pub fn truncate_content(content: &str) -> String {
    if content.chars().count() <= CONVERSATION_MAX_CONTENT_CHARS {
        return content.to_string();
    }
    content
        .chars()
        .take(CONVERSATION_MAX_CONTENT_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_json_decodes_with_defaults() {
        // 旧行 / 最小持久化形状：缺字段必须全部落到默认值，不得解码失败。
        let raw = r#"{"conversation_id":"c-1"}"#;
        let conversation: Conversation = serde_json::from_str(raw).expect("最小形状可解码");
        assert_eq!(conversation.conversation_id, "c-1");
        assert!(conversation.title.is_empty());
        assert_eq!(conversation.created_at, 0);
        assert_eq!(conversation.updated_at, 0);
        assert!(conversation.messages.is_empty());
        assert!(conversation.module_origin.is_none());
        assert!(!conversation.archived);
    }

    #[test]
    fn message_optional_metadata_defaults_to_none() {
        let raw = r#"{"role":"user","content":"你好"}"#;
        let message: ConversationMessage = serde_json::from_str(raw).expect("可解码");
        assert_eq!(message.role, ConversationRole::User);
        assert_eq!(message.content, "你好");
        assert!(message.provider.is_none());
        assert!(message.model.is_none());
        assert!(message.created_at.is_none());

        // 缺 role 也必须有确定值（旧行可能是无角色占位）。
        let legacy: ConversationMessage =
            serde_json::from_str(r#"{"content":"旧行"}"#).expect("可解码");
        assert_eq!(legacy.role, ConversationRole::User);
    }

    #[test]
    fn unknown_role_is_a_serde_error() {
        // 记录边界：未知 role 由 serde 拒绝，因此持久化适配层必须自行兜底
        // （`ConversationRole::parse().unwrap_or_default()`），不能把错误上抛。
        let parsed: Result<ConversationMessage, _> =
            serde_json::from_str(r#"{"role":"system","content":"x"}"#);
        assert!(parsed.is_err(), "system 不属于会话历史角色白名单");
    }

    #[test]
    fn message_json_carries_no_hidden_reasoning_field() {
        let message = ConversationMessage::new(ConversationRole::Assistant, "答案")
            .with_provider("openai-compatible", "step-5-preview");
        let value = serde_json::to_value(&message).expect("可序列化");
        let object = value.as_object().expect("消息是 JSON 对象");

        // 字段白名单（唯一允许出现在消息 JSON 里的键）。新增字段必须先在这里登记，
        // 否则本条失败——防止隐藏推理 / secret / token 悄悄入库。
        let allowed = ["role", "content", "provider", "model", "created_at"];
        let unexpected: Vec<&str> = object
            .keys()
            .map(String::as_str)
            .filter(|key| !allowed.contains(key))
            .collect();
        assert!(
            unexpected.is_empty(),
            "字段白名单不得扩容（发现未知键: {unexpected:?}）；隐藏推理 / secret / token 一律不入库"
        );
        // 显式设置的字段必须真的落库。
        assert_eq!(object["content"], "答案");
        assert_eq!(object["provider"], "openai-compatible");
        assert_eq!(object["model"], "step-5-preview");
        assert_eq!(object["role"], "assistant");
    }

    #[test]
    fn truncate_content_caps_and_keeps_utf8_boundaries() {
        assert_eq!(truncate_content("短文本"), "短文本");
        let long = "会".repeat(CONVERSATION_MAX_CONTENT_CHARS + 5);
        let capped = truncate_content(&long);
        assert_eq!(capped.chars().count(), CONVERSATION_MAX_CONTENT_CHARS);
        // 幂等：再次截断不再变化。
        assert_eq!(truncate_content(&capped), capped);

        // 多字节字符不被切成乱码。
        let mixed = "🙂".repeat(CONVERSATION_MAX_CONTENT_CHARS + 1);
        assert_eq!(
            truncate_content(&mixed).chars().count(),
            CONVERSATION_MAX_CONTENT_CHARS
        );
    }

    #[test]
    fn summary_omits_message_bodies() {
        let conversation = Conversation {
            conversation_id: "c-1".into(),
            title: "旅行计划".into(),
            created_at: 10,
            updated_at: 20,
            messages: vec![
                ConversationMessage::new(ConversationRole::User, "帮我规划杭州两日游"),
                ConversationMessage::new(ConversationRole::Assistant, "第一天……"),
            ],
            module_origin: Some("travel".into()),
            archived: false,
        };
        let summary = conversation.summary();
        assert_eq!(summary.conversation_id, "c-1");
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.module_origin.as_deref(), Some("travel"));
        assert!(!summary.archived);

        let json = serde_json::to_string(&summary).expect("可序列化");
        assert!(!json.contains("帮我规划"), "摘要不得携带任何正文");
    }
}
