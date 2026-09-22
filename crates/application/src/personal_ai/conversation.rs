//! 会话历史端口与用例（V11 §96-§101）。
//!
//! **Conversation ≠ Memory**：这里的会话历史是「对话回放」——一条会话的消息序列与
//! 元数据。它**绝不被**记忆服务（`MemoryService` / 记忆检索 / `memory.*` 工具）读取，
//! 也不会自动升级为 `MemoryItem`（V6 §3 边界）。反过来，Memory 也不回流成会话历史。
//! 两个域各有唯一 Source of Truth：`conversations.db` 与 `memory.db` 物理隔离。
//!
//! 端口属于用例层：实现（SQLite `config/conversations.db`）在 infrastructure，
//! 组合根（`apps/desktop`）负责装配（与 `MemoryStorePort` 完全同构）。用例层不感知
//! SQL / 序列化细节，因此本模块的错误类型不携带基础设施类型。

use std::sync::Arc;

use devtoolbox_core::personal_ai::conversation::{
    Conversation, ConversationMessage, ConversationRole, ConversationSummary, truncate_content,
};

use crate::error::ApplicationError;

/// 会话历史默认列表上限（管理页一次拉取的条数；`limit = 0` 视为「用默认值」）。
pub const DEFAULT_CONVERSATION_LIST_LIMIT: usize = 50;

/// 会话历史存储错误（适配层已把基础设施错误转换为可显示文本）。
///
/// 文本面向用户/日志，**不含**消息正文、secret 或 token。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationStoreError(pub String);

impl std::fmt::Display for ConversationStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ConversationStoreError {}

/// 会话历史存储端口（同步；锁内不跨 await，agent 循环内直接调用）。
pub trait ConversationStore: Send + Sync {
    /// 列表（`updated_at` 倒序；`limit = 0` → 不设上限）。
    ///
    /// 已归档会话默认**不在**结果内（可恢复语义：归档只是隐藏，不是删除）。
    fn list(&self, limit: usize) -> Result<Vec<ConversationSummary>, ConversationStoreError>;

    /// 按 id 读取完整会话（含正文）。不存在返回 `None`；归档会话仍可读。
    fn load(&self, conversation_id: &str) -> Result<Option<Conversation>, ConversationStoreError>;

    /// 创建会话，返回完整会话（`conversation_id` 由实现生成，保证唯一）。
    ///
    /// 空标题归一为 [`DEFAULT_CONVERSATION_TITLE`]；`module_origin` 空串归一为 `None`。
    fn create(
        &self,
        title: &str,
        module_origin: Option<&str>,
    ) -> Result<Conversation, ConversationStoreError>;

    /// 追加一条消息（正文超限由实现硬截断）；会话不存在时为受控错误。
    fn append_message(
        &self,
        conversation_id: &str,
        message: &ConversationMessage,
    ) -> Result<(), ConversationStoreError>;

    /// 重命名（空标题被归一为默认标题）。
    fn rename(
        &self,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), ConversationStoreError>;

    /// 归档 / 取消归档（`false` = 恢复）。归档不删消息。
    fn set_archived(
        &self,
        conversation_id: &str,
        archived: bool,
    ) -> Result<(), ConversationStoreError>;

    /// 物理删除：会话与其全部消息一起删除（本端口唯一允许的删除语义）。
    fn delete(&self, conversation_id: &str) -> Result<(), ConversationStoreError>;
}

/// 会话历史用例（薄封装：归一输入、截断正文、收敛错误）。
///
/// 不含业务策略：写入 gate / 检索编排属于其他模块，本用例只保证「历史可回放」。
pub struct ConversationService {
    store: Arc<dyn ConversationStore>,
    list_limit: usize,
}

impl ConversationService {
    #[must_use]
    pub fn new(store: Arc<dyn ConversationStore>) -> Self {
        Self {
            store,
            list_limit: DEFAULT_CONVERSATION_LIST_LIMIT,
        }
    }

    #[must_use]
    pub fn with_list_limit(store: Arc<dyn ConversationStore>, list_limit: usize) -> Self {
        Self { store, list_limit }
    }

    /// 列表（`limit = 0` → 默认上限；已归档默认隐藏）。
    pub fn list(&self, limit: usize) -> Result<Vec<ConversationSummary>, ApplicationError> {
        let limit = if limit == 0 {
            self.list_limit
        } else {
            limit
        };
        Ok(self.store.list(limit)?)
    }

    /// 读取完整会话（含正文）。
    pub fn load(&self, conversation_id: &str) -> Result<Option<Conversation>, ApplicationError> {
        Ok(self.store.load(conversation_id)?)
    }

    /// 创建会话；`module_origin` 空串归一为 `None`。
    pub fn create(
        &self,
        title: &str,
        module_origin: Option<&str>,
    ) -> Result<Conversation, ApplicationError> {
        let origin = module_origin
            .map(str::trim)
            .filter(|value| !value.is_empty());
        Ok(self.store.create(title, origin)?)
    }

    /// 追加一条消息（自动截断正文到 [`CONVERSATION_MAX_CONTENT_CHARS`] 字符）。
    pub fn append(
        &self,
        conversation_id: &str,
        message: ConversationMessage,
    ) -> Result<(), ApplicationError> {
        let mut stored = message;
        stored.content = truncate_content(&stored.content);
        self.store.append_message(conversation_id, &stored)?;
        Ok(())
    }

    /// 构造一条用户消息并追加。
    pub fn append_user(
        &self,
        conversation_id: &str,
        content: &str,
    ) -> Result<(), ApplicationError> {
        self.append(
            conversation_id,
            ConversationMessage::new(ConversationRole::User, content),
        )
    }

    /// 构造一条模型回答并追加（携带 provider / model 来源元数据）。
    ///
    /// 只写**最终答案**：隐藏推理（reasoning content）不属于本端口的字段白名单。
    pub fn append_assistant(
        &self,
        conversation_id: &str,
        content: &str,
        provider: &str,
        model: &str,
    ) -> Result<(), ApplicationError> {
        self.append(
            conversation_id,
            ConversationMessage::new(ConversationRole::Assistant, content)
                .with_provider(provider, model),
        )
    }

    /// 构造一条工具结果回填并追加（元数据同样标注来源）。
    pub fn append_tool_result(
        &self,
        conversation_id: &str,
        content: &str,
        provider: &str,
        model: &str,
    ) -> Result<(), ApplicationError> {
        self.append(
            conversation_id,
            ConversationMessage::new(ConversationRole::Tool, content)
                .with_provider(provider, model),
        )
    }

    /// 重命名。
    pub fn rename(&self, conversation_id: &str, title: &str) -> Result<(), ApplicationError> {
        Ok(self.store.rename(conversation_id, title)?)
    }

    /// 归档 / 恢复。
    pub fn set_archived(
        &self,
        conversation_id: &str,
        archived: bool,
    ) -> Result<(), ApplicationError> {
        Ok(self.store.set_archived(conversation_id, archived)?)
    }

    /// 物理删除（连消息一起删）。
    pub fn delete(&self, conversation_id: &str) -> Result<(), ApplicationError> {
        Ok(self.store.delete(conversation_id)?)
    }
}

fn conversation_error(message: impl Into<String>) -> ApplicationError {
    // 复用最接近的既有变体：会话历史属于 Personal AI 域（`AgentErrorKind::Session`）。
    // 不新增 ApplicationError 变体：`PersonalAi(AgentError::session)` 的 code 已经是
    // `personal_ai_session_error`，前端分类语义与「会话错误」完全一致。
    ApplicationError::PersonalAi(devtoolbox_core::AgentError::session(message))
}

impl From<ConversationStoreError> for ApplicationError {
    fn from(error: ConversationStoreError) -> Self {
        conversation_error(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::personal_ai::conversation::{
        CONVERSATION_MAX_CONTENT_CHARS, DEFAULT_CONVERSATION_TITLE,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存端口实现（测试替身：验证用例的归一化与错误收敛）。
    struct InMemoryConversationStore {
        conversations: Mutex<HashMap<String, Conversation>>,
        sequence: Mutex<u64>,
        now: Mutex<i64>,
    }

    impl InMemoryConversationStore {
        fn new() -> Self {
            Self {
                conversations: Mutex::new(HashMap::new()),
                sequence: Mutex::new(1),
                now: Mutex::new(1_000),
            }
        }

        fn tick(&self) -> i64 {
            let mut now = self.now.lock().expect("now poisoned");
            *now += 1;
            *now
        }

        fn next_id(&self) -> String {
            let mut sequence = self.sequence.lock().expect("sequence poisoned");
            *sequence += 1;
            format!("conv-{}", *sequence)
        }
    }

    impl ConversationStore for InMemoryConversationStore {
        fn list(&self, limit: usize) -> Result<Vec<ConversationSummary>, ConversationStoreError> {
            let mut out: Vec<ConversationSummary> = self
                .conversations
                .lock()
                .expect("conversations poisoned")
                .values()
                .filter(|conversation| !conversation.archived)
                .map(Conversation::summary)
                .collect();
            out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            out.truncate(limit);
            Ok(out)
        }

        fn load(
            &self,
            conversation_id: &str,
        ) -> Result<Option<Conversation>, ConversationStoreError> {
            Ok(self
                .conversations
                .lock()
                .expect("conversations poisoned")
                .get(conversation_id)
                .cloned())
        }

        fn create(
            &self,
            title: &str,
            module_origin: Option<&str>,
        ) -> Result<Conversation, ConversationStoreError> {
            let title = if title.trim().is_empty() {
                DEFAULT_CONVERSATION_TITLE
            } else {
                title
            };
            let now = self.tick();
            let conversation = Conversation {
                conversation_id: self.next_id(),
                title: title.to_string(),
                created_at: now,
                updated_at: now,
                messages: Vec::new(),
                module_origin: module_origin.map(str::to_string),
                archived: false,
            };
            self.conversations
                .lock()
                .expect("conversations poisoned")
                .insert(conversation.conversation_id.clone(), conversation.clone());
            Ok(conversation)
        }

        fn append_message(
            &self,
            conversation_id: &str,
            message: &ConversationMessage,
        ) -> Result<(), ConversationStoreError> {
            let mut conversations = self.conversations.lock().expect("conversations poisoned");
            let conversation = conversations.get_mut(conversation_id).ok_or_else(|| {
                ConversationStoreError(format!("conversation {conversation_id} not found"))
            })?;
            conversation.messages.push(message.clone());
            drop(conversations);
            self.tick();
            Ok(())
        }

        fn rename(
            &self,
            conversation_id: &str,
            title: &str,
        ) -> Result<(), ConversationStoreError> {
            let title = if title.trim().is_empty() {
                DEFAULT_CONVERSATION_TITLE
            } else {
                title
            };
            let mut conversations = self.conversations.lock().expect("conversations poisoned");
            let conversation = conversations.get_mut(conversation_id).ok_or_else(|| {
                ConversationStoreError(format!("conversation {conversation_id} not found"))
            })?;
            conversation.title = title.to_string();
            Ok(())
        }

        fn set_archived(
            &self,
            conversation_id: &str,
            archived: bool,
        ) -> Result<(), ConversationStoreError> {
            let mut conversations = self.conversations.lock().expect("conversations poisoned");
            let conversation = conversations.get_mut(conversation_id).ok_or_else(|| {
                ConversationStoreError(format!("conversation {conversation_id} not found"))
            })?;
            conversation.archived = archived;
            Ok(())
        }

        fn delete(&self, conversation_id: &str) -> Result<(), ConversationStoreError> {
            self.conversations
                .lock()
                .expect("conversations poisoned")
                .remove(conversation_id);
            Ok(())
        }
    }

    fn store() -> Arc<dyn ConversationStore> {
        Arc::new(InMemoryConversationStore::new())
    }

    #[test]
    fn create_normalizes_blank_title_and_empty_origin() {
        let service = ConversationService::new(store());
        let created = service.create("   ", Some("  ")).expect("创建成功");
        assert_eq!(created.title, DEFAULT_CONVERSATION_TITLE);
        assert!(created.module_origin.is_none(), "空白来源归一为 None");

        let named = service.create("杭州两日游", Some("travel")).expect("创建成功");
        assert_eq!(named.title, "杭州两日游");
        assert_eq!(named.module_origin.as_deref(), Some("travel"));
    }

    #[test]
    fn append_truncates_overlong_content_before_reaching_store() {
        let service = ConversationService::new(store());
        let created = service.create("长文本", None).expect("创建成功");
        service
            .append_user(&created.conversation_id, &"长".repeat(3_000))
            .expect("追加成功");
        let huge = "长".repeat(CONVERSATION_MAX_CONTENT_CHARS + 100);
        service
            .append_user(&created.conversation_id, &huge)
            .expect("追加成功");

        let loaded = service
            .load(&created.conversation_id)
            .expect("读取成功")
            .expect("会话存在");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].content.chars().count(), CONVERSATION_MAX_CONTENT_CHARS);
    }

    #[test]
    fn assistant_and_tool_messages_carry_provider_metadata() {
        let service = ConversationService::new(store());
        let created = service.create("多轮", None).expect("创建成功");
        service
            .append_user(&created.conversation_id, "帮我规划杭州两日游")
            .expect("追加成功");
        service
            .append_assistant(
                &created.conversation_id,
                "第一天：西湖。",
                "openai-compatible",
                "step-5-preview",
            )
            .expect("追加成功");
        service
            .append_tool_result(
                &created.conversation_id,
                "查询到 12 个景点",
                "openai-compatible",
                "step-5-preview",
            )
            .expect("追加成功");

        let loaded = service
            .load(&created.conversation_id)
            .expect("读取成功")
            .expect("会话存在");
        assert_eq!(loaded.messages.len(), 3);
        assert_eq!(loaded.messages[0].role, ConversationRole::User);
        assert_eq!(loaded.messages[1].role, ConversationRole::Assistant);
        assert_eq!(loaded.messages[1].provider.as_deref(), Some("openai-compatible"));
        assert_eq!(loaded.messages[1].model.as_deref(), Some("step-5-preview"));
        assert_eq!(loaded.messages[2].role, ConversationRole::Tool);
    }

    #[test]
    fn append_to_missing_conversation_surfaces_as_application_error() {
        let service = ConversationService::new(store());
        let error = service
            .append_user("missing", "你好")
            .expect_err("不存在的会话必须报错");
        // 复用 PersonalAi(Session) 变体：前端 code 与「会话错误」语义一致。
        assert!(matches!(error, ApplicationError::PersonalAi(ref agent) if agent.code() == "personal_ai_session_error"));
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn store_error_maps_to_personal_ai_session_code() {
        let error: ApplicationError = ConversationStoreError("boom".into()).into();
        let message = error.to_string();
        assert!(
            message.contains("personal_ai_session_error: boom"),
            "复用最接近的既有变体，不新增 ApplicationError 变体: {message}"
        );
        assert!(
            matches!(&error, ApplicationError::PersonalAi(agent) if agent.kind == devtoolbox_core::personal_ai::error::AgentErrorKind::Session),
            "错误归类到 Session"
        );
    }

    #[test]
    fn list_hides_archived_and_applies_limit() {
        let service = ConversationService::new(store());
        let first = service.create("第一个", None).expect("创建成功");
        let second = service.create("第二个", None).expect("创建成功");
        service.set_archived(&first.conversation_id, true).expect("归档成功");

        let visible = service.list(0).expect("列表成功");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].conversation_id, second.conversation_id);

        // 默认上限：limit = 0 用 DEFAULT_CONVERSATION_LIST_LIMIT，不做隐式通配。
        assert!(service.list(0).expect("列表成功").len() <= DEFAULT_CONVERSATION_LIST_LIMIT);
    }

    #[test]
    fn rename_and_delete_round_trip_through_service() {
        let service = ConversationService::new(store());
        let created = service.create("旧标题", None).expect("创建成功");
        service
            .rename(&created.conversation_id, "新标题")
            .expect("重命名成功");
        let loaded = service
            .load(&created.conversation_id)
            .expect("读取成功")
            .expect("会话存在");
        assert_eq!(loaded.title, "新标题");

        service.delete(&created.conversation_id).expect("删除成功");
        assert!(
            service
                .load(&created.conversation_id)
                .expect("读取成功")
                .is_none(),
            "删除后不再可读"
        );
    }
}
