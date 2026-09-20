//! Session Memory（V4 §34）：会话级消息存储，**不是**长期记忆。
//!
//! - Key：`session_id`；Value：消息序列（上限裁剪）。
//! - 并发安全（RwLock），避免同一 session 并发写产生乱序（V4 §69）。
//! - 持久化 = P1；本轮内存实现。
//! - Conversation ≠ Memory：这里的数据绝不进入未来 PersonalMemoryStore。

use std::collections::HashMap;
use std::sync::RwLock;

use devtoolbox_core::ChatMessage;

/// 会话上限（裁剪到最近 N 条）。
const DEFAULT_MAX_MESSAGES: usize = 60;

/// 会话存储端口（同步；agent 循环单线程内调用）。
pub trait SessionStore: Send + Sync {
    /// 读会话消息（无则空）。
    fn load(&self, session_id: &str) -> Vec<ChatMessage>;
    /// 追加消息并裁剪。
    fn append(&self, session_id: &str, messages: &[ChatMessage]);
    /// 清空会话。
    fn clear(&self, session_id: &str);
}

/// 内存会话存储。
pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<String, Vec<ChatMessage>>>,
    max_messages: usize,
}

impl InMemorySessionStore {
    #[must_use]
    pub fn new() -> Self {
        Self::with_max(DEFAULT_MAX_MESSAGES)
    }

    #[must_use]
    pub fn with_max(max_messages: usize) -> Self {
        Self { sessions: RwLock::new(HashMap::new()), max_messages }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for InMemorySessionStore {
    fn load(&self, session_id: &str) -> Vec<ChatMessage> {
        self.sessions
            .read()
            .expect("session store poisoned")
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    fn append(&self, session_id: &str, messages: &[ChatMessage]) {
        let mut store = self.sessions.write().expect("session store poisoned");
        let bucket = store.entry(session_id.to_string()).or_default();
        bucket.extend(messages.iter().cloned());
        let over = bucket.len().saturating_sub(self.max_messages);
        if over > 0 {
            bucket.drain(..over);
        }
    }

    fn clear(&self, session_id: &str) {
        self.sessions.write().expect("session store poisoned").remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(msg: &str) -> ChatMessage {
        ChatMessage::user(msg)
    }

    #[test]
    fn store_round_trip_and_trim() {
        let store = InMemorySessionStore::with_max(3);
        store.append("s1", &[user("a"), user("b"), user("c"), user("d")]);
        let loaded = store.load("s1");
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].content.as_deref(), Some("b"));
    }

    #[test]
    fn sessions_are_isolated() {
        let store = InMemorySessionStore::with_max(3);
        store.append("alpha", &[user("x")]);
        store.append("beta", &[user("y")]);
        assert_eq!(store.load("alpha").len(), 1);
        store.clear("alpha");
        assert!(store.load("alpha").is_empty());
        assert_eq!(store.load("beta").len(), 1);
    }
}