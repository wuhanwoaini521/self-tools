//! Personal AI Hub 数据契约（V4 §11/§12/§15/§19/§22）。
//!
//! 全部为纯 serde 类型：前端（TS）、application（Rust）、未来 HTTP 面共用同一形状。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// App Context（V4 §22-§24，P0）
// ---------------------------------------------------------------------------

/// 当前实体引用。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntityRef {
    /// 实体类型（如 `person` / `event`）。
    pub kind: String,
    /// 实体规范 id。
    pub id: String,
    /// 展示名（如「毛泽东」），可为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// 选中内容引用（V4 §4 的「选中了什么」；本轮保留字段位）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionRef {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// 统一 App Context：AI 必须知道用户当前在哪里、在看什么（V4 §4/§22）。
///
/// 所有权分工：**Frontend 负责「我在哪」**（module/page/entity），
/// 业务模块的 ContextProvider 负责「这个 entity 的业务上下文」。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppContext {
    /// 模块 id（如 `history`；缺省为通用会话）。
    pub module: Option<String>,
    /// 页面名（如 `person-detail` / `event-detail`）。
    pub page: Option<String>,
    /// 当前实体（如 History Person 页的 毛泽东）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntityRef>,
    /// 当前选中内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<SelectionRef>,
    /// 附加视图状态（前端自定义，宽松透传）。
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub view_state: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Tool（V4 §18-§21）
// ---------------------------------------------------------------------------

/// 工具风险分级。V4 只装配 Read（安全边界在组合根强制）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    Read,
    SafeWrite,
    SensitiveWrite,
    System,
}

/// 工具契约：name（`module.action`）+ 描述 + JSON Schema + 风险 + 所属模块。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// 形如 `history.search`。
    pub name: String,
    pub description: String,
    /// 入参 JSON Schema（对象）。
    pub input_schema: serde_json::Value,
    pub risk: ToolRisk,
    pub module: String,
}

/// 统一工具执行结果（V4 §32）：不要让每个工具返回不同随意字符串。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolResult {
    pub ok: bool,
    /// 成功时的结构化数据。
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
    /// 失败原因（用户可读）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 附加元数据（如命中实体列表，供 UI Block / 行为决策）。
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl ToolResult {
    #[must_use]
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            data,
            error: None,
            metadata: serde_json::Value::Null,
        }
    }
    #[must_use]
    pub fn ok_with_metadata(data: serde_json::Value, metadata: serde_json::Value) -> Self {
        Self {
            ok: true,
            data,
            error: None,
            metadata,
        }
    }
    #[must_use]
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: serde_json::Value::Null,
            error: Some(message.into()),
            metadata: serde_json::Value::Null,
        }
    }
}

/// 模型对工具的一次调用（执行端输入）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Module（V4 §16-§17）
// ---------------------------------------------------------------------------

/// 模块描述：注册表据此发现并对外宣布 self-tools 现有模块。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModuleDescriptor {
    /// 模块 id（如 `history`）。
    pub id: String,
    pub display_name: String,
    pub description: String,
    /// 模块能力标签（如 ["search","entity","enrichment_state"]）。
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// 该模块暴露的工具名（`history.search` …）。
    #[serde(default)]
    pub tools: Vec<String>,
}

// ---------------------------------------------------------------------------
// Action（V4 §13/§51）
// ---------------------------------------------------------------------------

/// Action 类型。V4 定义 4 种；V6 增加 3 种知识层动作（§80：尽量少扩展）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Navigate,
    OpenEntity,
    RefreshView,
    ShowPanel,
    /// 打开知识库中的文档（V6 §128）。
    OpenDocument,
    /// 打开允许根内的文件（V6 §46/§129：backend 不执行 shell，只产出请求）。
    OpenFile,
    /// 请求用户确认保存一条 Memory（V6 §81）。
    ConfirmMemory,
    /// 请求用户确认一个 SYSTEM 操作（V7 §56：携带确认票据，不可变）。
    ConfirmAction,
    /// 打开一个已注册应用（V7 §46：URL 来自注册表，不是模型输入）。
    OpenApp,
}

/// Action 请求。`kind` 序列化为 `type` 以贴合协议示例：
/// `{"type":"navigate","module":"history","target":{...}}`。
///
/// **执行边界（V4 §51）**：模型只产出请求；Frontend/Application 决定是否执行。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub kind: ActionKind,
    pub module: String,
    /// 目标（按 kind 解释；如 Navigate + EntityRef）。
    pub target: serde_json::Value,
}

// ---------------------------------------------------------------------------
// UI Block（V4 §15）
// ---------------------------------------------------------------------------

/// UI Block 类型。V4 定义 5 种；V6 增加 5 种知识层结构化 UI（§38/§75）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiBlockKind {
    EntityList,
    EntityCard,
    SourceList,
    KeyValue,
    TimelinePreview,
    /// Memory 候选/结果列表（含确认入口）。
    MemoryList,
    /// 文档命中列表。
    DocumentList,
    /// 单个文档卡片。
    DocumentCard,
    /// 文档引用（回答内联引用：文档 + 位置）。
    DocumentReference,
    /// 文件命中列表（含 Open 入口）。
    FileList,
}

/// 结构化 UI Block：AI 返回结构化 UI，而不只是 Markdown。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiBlock {
    pub kind: UiBlockKind,
    pub title: String,
    /// 数据形状由 kind 决定（前端渲染器解析）。
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Agent Request / Response（V4 §11/§12）
// ---------------------------------------------------------------------------

/// Agent 请求。会话历史由后端按 `session_id` 维护（V4 §34）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentRequest {
    /// 用户本次消息。
    pub message: String,
    /// 会话 id（缺省 = 一次性会话）。
    pub session_id: Option<String>,
    /// 用户当前 UI 状态。
    pub app_context: AppContext,
    /// 本次允许使用的模块列表（由前端 status 提供；空 = 全部已注册模块）。
    pub capabilities: Vec<String>,
    /// 语言（如 `zh-CN`），缺省后端默认。
    pub locale: Option<String>,
}

/// UI 可见的会话消息快照（角色 + 文本），用于前端重绘。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    /// `user` / `assistant` / `tool`。
    pub role: String,
    pub content: String,
}

/// 工具调用轨迹（V4 §12 的 tool_trace metadata）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolTraceEntry {
    pub tool: String,
    pub ok: bool,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 聚合用量（V4 §66）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub duration_ms: u64,
    pub tool_rounds: u8,
}

/// 统一 Agent 响应（V4 §12）：message + actions + ui_blocks + tool_trace + usage。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub session_id: String,
    /// 最终回复文本。
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_blocks: Vec<UiBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_trace: Vec<ToolTraceEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentUsage>,
    /// 会话快照（上限裁剪，供 UI 重建历史）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<AgentMessage>,
    /// 提供方与模型名（不含 key）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl AgentUsage {
    #[must_use]
    pub fn combine(self, other: AgentUsage) -> AgentUsage {
        AgentUsage {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            total_tokens: self.total_tokens + other.total_tokens,
            duration_ms: self.duration_ms + other.duration_ms,
            tool_rounds: self.tool_rounds,
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助构造函数
// ---------------------------------------------------------------------------

impl Action {
    /// 构造 Navigate 动作（V4 §13 示例形状）。
    #[must_use]
    pub fn navigate(module: impl Into<String>, target: serde_json::Value) -> Self {
        Self {
            kind: ActionKind::Navigate,
            module: module.into(),
            target,
        }
    }

    /// 打开知识库文档（V6 §128）：target 形如
    /// `{"document_id":…,"title":…,"location":…}`。
    #[must_use]
    pub fn open_document(target: serde_json::Value) -> Self {
        Self {
            kind: ActionKind::OpenDocument,
            module: "documents".into(),
            target,
        }
    }

    /// 打开允许根内的文件（V6 §46/§129）：**由前端执行**，backend 不运行 shell。
    /// target 形如 `{"file_id":…,"path":…,"file_name":…}`。
    #[must_use]
    pub fn open_file(target: serde_json::Value) -> Self {
        Self {
            kind: ActionKind::OpenFile,
            module: "files".into(),
            target,
        }
    }

    /// 请求用户确认保存一条 Memory（V6 §81）：target 形如
    /// `{"memory_id"|"draft":{…},"content":…,"category":…,"source_type":…}`。
    #[must_use]
    pub fn confirm_memory(target: serde_json::Value) -> Self {
        Self {
            kind: ActionKind::ConfirmMemory,
            module: "memory".into(),
            target,
        }
    }
}

impl AppContext {
    /// 是否为空上下文（通用会话）。
    #[must_use]
    pub fn is_general(&self) -> bool {
        self.module.is_none() && self.entity.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_context_round_trip() {
        let ctx = AppContext {
            module: Some("history".into()),
            page: Some("person-detail".into()),
            entity: Some(EntityRef {
                kind: "person".into(),
                id: "mao_zedong".into(),
                label: Some("毛泽东".into()),
            }),
            selection: None,
            view_state: serde_json::Value::Null,
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["module"], "history");
        assert_eq!(json["entity"]["id"], "mao_zedong");
        let back: AppContext = serde_json::from_value(json).unwrap();
        assert_eq!(ctx, back);
    }

    #[test]
    fn navigate_action_serializes_as_type() {
        let action = Action::navigate(
            "history",
            serde_json::json!({"type": "event", "id": "zunyi_meeting"}),
        );
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["type"], "navigate");
        assert_eq!(json["module"], "history");
        assert_eq!(json["target"]["id"], "zunyi_meeting");
        let back: Action = serde_json::from_value(json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn ui_block_round_trip() {
        let block = UiBlock {
            kind: UiBlockKind::EntityList,
            title: "相关事件".into(),
            data: serde_json::json!([{"kind":"event","id":"e1","title":"遵义会议"}]),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["kind"], "entity_list");
        let back: UiBlock = serde_json::from_value(json).unwrap();
        assert_eq!(block, back);
    }

    #[test]
    fn v6_action_and_block_kinds_round_trip() {
        for (action, expected) in [
            (
                Action::open_document(serde_json::json!({"document_id": "doc-1"})),
                "open_document",
            ),
            (
                Action::open_file(serde_json::json!({"file_id": "file-1"})),
                "open_file",
            ),
            (
                Action::confirm_memory(serde_json::json!({"memory_id": "m1"})),
                "confirm_memory",
            ),
        ] {
            let json = serde_json::to_value(&action).unwrap();
            assert_eq!(json["type"], expected);
            let back: Action = serde_json::from_value(json).unwrap();
            assert_eq!(back, action);
        }
        assert_eq!(Action::open_file(serde_json::Value::Null).module, "files");
        assert_eq!(
            Action::confirm_memory(serde_json::Value::Null).module,
            "memory"
        );

        for (kind, expected) in [
            (UiBlockKind::MemoryList, "memory_list"),
            (UiBlockKind::DocumentList, "document_list"),
            (UiBlockKind::DocumentCard, "document_card"),
            (UiBlockKind::DocumentReference, "document_reference"),
            (UiBlockKind::FileList, "file_list"),
        ] {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, expected);
            let back: UiBlockKind = serde_json::from_value(json).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn tool_spec_risk_serialization() {
        let spec = ToolSpec {
            name: "history.search".into(),
            description: "search".into(),
            input_schema: serde_json::json!({"type":"object"}),
            risk: ToolRisk::Read,
            module: "history".into(),
        };
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["risk"], "read");
        let back: ToolSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn agent_response_round_trip_with_blocks() {
        let response = AgentResponse {
            session_id: "s1".into(),
            message: "找到了".into(),
            actions: vec![Action::navigate("history", serde_json::json!({"id":"x"}))],
            ui_blocks: vec![UiBlock {
                kind: UiBlockKind::EntityList,
                title: "t".into(),
                data: serde_json::json!([]),
            }],
            tool_trace: vec![ToolTraceEntry {
                tool: "history.search".into(),
                ok: true,
                duration_ms: 3,
                note: None,
            }],
            usage: Some(AgentUsage {
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
                duration_ms: 4,
                tool_rounds: 1,
            }),
            messages: vec![AgentMessage {
                role: "user".into(),
                content: "hi".into(),
            }],
            provider: Some("fake".into()),
            model: Some("fake-model".into()),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["actions"][0]["type"], "navigate");
        assert_eq!(json["ui_blocks"][0]["kind"], "entity_list");
        let back: AgentResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back.usage.unwrap().tool_rounds, 1);
        assert_eq!(back.actions[0].kind, ActionKind::Navigate);
        assert_eq!(back.tool_trace.len(), 1);
        assert_eq!(back.messages[0].content, "hi");
    }

    #[test]
    fn default_app_context_is_general() {
        assert!(AppContext::default().is_general());
    }
}
