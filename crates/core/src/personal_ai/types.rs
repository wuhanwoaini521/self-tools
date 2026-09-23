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

/// 编排追踪视图（V9 §72；V10 §20 扩展决策遥测）：
/// 只含结构与计数，**不含** secret / 正文 / 完整 prompt / 隐藏推理（§73）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationTraceView {
    pub trace_id: String,
    pub decision: String,
    pub plan_rationale: String,
    pub runs: Vec<OrchestrationRunView>,
    /// review 结论（pass / needs_fix / ...）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    pub merged: bool,
    /// 提前停止原因（预算 / 取消）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_early: Option<String>,
    /// 实际生效的决策 provider（rule / jev）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_provider: Option<String>,
    /// 决策策略（direct / research_only / ...）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_strategy: Option<String>,
    /// 决策置信档位（high / uncertain / low）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_confidence: Option<String>,
    /// 决策 reason code（机器可读）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_reason_code: Option<String>,
    /// 决策延迟（ms）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_latency_ms: Option<u64>,
    /// 是否发生过 provider fallback（jev 失败 → rule）。
    #[serde(default)]
    pub decision_fallback: bool,
    /// JEV_SHADOW 下影子 provider 的策略（仅 label；无 CoT）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_decision: Option<String>,
    /// 实际执行的 worker 数。
    #[serde(default)]
    pub worker_count: usize,
}

/// 单次 worker run 的视图（§75：只展示任务/角色/状态/工具数/时长）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationRunView {
    pub task_id: String,
    pub agent_id: String,
    pub state: String,
    pub status: String,
    pub duration_ms: u64,
    pub tool_calls: usize,
    pub tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
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

/// 消息内容片段（V11 §102：PersonalAgent Message 不再只是 String）。
///
/// 隐私铁律（§106）：图片/音频**默认**不进入 Personal Memory；
/// 只有用户显式确认保存才落库。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// 纯文本。
    Text { text: String },
    /// 图片（base64 data URL 或 http(s) 引用；由 provider 决定如何编码）。
    Image {
        /// `base64` / `url` / `file_path`。
        source: String,
        /// data URL 或 base64 数据（不含前缀）。
        data: String,
        /// MIME（如 `image/png`）。
        mime: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// 来源说明（如「学习板快照」）；仅用于 UI 展示。
        caption: Option<String>,
    },
    /// 音频（可选；provider 不支持时受控拒绝，不假装分析）。
    Audio {
        source: String,
        data: String,
        mime: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// 文档引用（不解内容；走 Documents 模块检索）。
    DocumentRef {
        document_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// 学习板快照（V11 §110：board → snapshot → ContentPart）。
    BoardSnapshot {
        board_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// PNG base64。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_base64: Option<String>,
        /// 笔画数（无图时的兜底描述）。
        #[serde(default)]
        stroke_count: usize,
    },
}

impl ContentPart {
    /// 该片段是否需要 provider 的多模态能力。
    #[must_use]
    pub fn requires_vision(&self) -> bool {
        matches!(
            self,
            ContentPart::Image { .. } | ContentPart::BoardSnapshot { .. }
        )
    }

    /// 该片段是否需要音频能力。
    #[must_use]
    pub fn requires_audio(&self) -> bool {
        matches!(self, ContentPart::Audio { .. })
    }

    /// 折叠为可读摘要（日志 / UI；不含 data 本体）。
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            ContentPart::Text { text } => {
                let truncated: String = text.chars().take(40).collect();
                if text.chars().count() > 40 {
                    format!("{truncated}…")
                } else {
                    truncated
                }
            }
            ContentPart::Image { mime, caption, .. } => {
                format!("image[{mime}]{}", caption.as_ref().map(|c| format!(" {c}")).unwrap_or_default())
            }
            ContentPart::Audio { mime, .. } => format!("audio[{mime}]"),
            ContentPart::DocumentRef { document_id, title } => {
                format!("document:{document_id}{}", title.as_ref().map(|t| format!(" ({t})")).unwrap_or_default())
            }
            ContentPart::BoardSnapshot { board_id, stroke_count, .. } => {
                format!("board:{board_id} ({stroke_count} strokes)")
            }
        }
    }
}

/// 模型能力声明（V11 §103）：provider 能表达自己支持什么。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelCapabilities {
    pub text: bool,
    /// 图片理解（vision）。
    pub vision: bool,
    /// 音频理解。
    pub audio: bool,
    /// 工具调用。
    pub tool_calling: bool,
}

impl ModelCapabilities {
    /// 仅文本（未配置 / 未知模型的保守默认）。
    #[must_use]
    pub fn text_only() -> Self {
        Self {
            text: true,
            vision: false,
            audio: false,
            tool_calling: false,
        }
    }

    /// 是否支持给定片段集合。
    #[must_use]
    pub fn supports(&self, parts: &[ContentPart]) -> bool {
        parts.iter().all(|part| match part {
            ContentPart::Text { .. } | ContentPart::DocumentRef { .. } => true,
            ContentPart::Image { .. } | ContentPart::BoardSnapshot { .. } => self.vision,
            ContentPart::Audio { .. } => self.audio,
        })
    }

    /// 不支持时的可读原因（§104：明确说明，不假装分析）。
    #[must_use]
    pub fn unsupported_reason(&self, parts: &[ContentPart]) -> Option<String> {
        if self.supports(parts) {
            return None;
        }
        if parts.iter().any(ContentPart::requires_vision) && !self.vision {
            return Some("当前模型不支持图片（vision）；请切换到支持视觉的模型，或用文字描述内容。".into());
        }
        if parts.iter().any(ContentPart::requires_audio) && !self.audio {
            return Some("当前模型不支持音频；请用文字转写后发送。".into());
        }
        None
    }
}

/// Agent 请求。会话历史由后端按 `session_id` 维护（V4 §34）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentRequest {
    /// 用户本次消息（文本；与 `parts` 等价冗余，兼容旧前端）。
    pub message: String,
    /// 多模态片段（V11 §102；空 = 仅文本）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ContentPart>,
    /// 会话 id（缺省 = 一次性会话）。
    pub session_id: Option<String>,
    /// 用户当前 UI 状态。
    pub app_context: AppContext,
    /// 本次允许使用的模块列表（由前端 status 提供；空 = 全部已注册模块）。
    pub capabilities: Vec<String>,
    /// 语言（如 `zh-CN`），缺省后端默认。
    pub locale: Option<String>,
}

impl AgentRequest {
    /// 纯文本快捷构造（兼容 V4-V10 调用方）。
    #[must_use]
    pub fn text(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::default()
        }
    }

    /// 有效片段：`parts` 非空则用之，否则用 `message` 构造单个 Text。
    #[must_use]
    pub fn effective_parts(&self) -> Vec<ContentPart> {
        if self.parts.is_empty() {
            if self.message.is_empty() {
                Vec::new()
            } else {
                vec![ContentPart::Text {
                    text: self.message.clone(),
                }]
            }
        } else {
            self.parts.clone()
        }
    }

    /// 折叠后的纯文本（供 prompt / 日志）：文本原样 + 多模态摘要。
    #[must_use]
    pub fn flattened_text(&self) -> String {
        let parts = self.effective_parts();
        let mut out = String::new();
        for part in &parts {
            match part {
                ContentPart::Text { text } => out.push_str(text),
                other => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&format!("[{}]", other.summary()));
                }
            }
        }
        out
    }
}

/// UI 可见的会话消息快照（角色 + 文本），用于前端重绘。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    /// `user` / `assistant` / `tool`。
    pub role: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Agent 进度事件（V11 验收反馈：过程可见 + 失败可诊断）
// ---------------------------------------------------------------------------

/// Agent 单轮执行的阶段（顺序即流水线顺序）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStage {
    /// 组装上下文（模块 / 工具 / 检索增强）。
    Preparing,
    /// 决策：选直接回答还是编排（V10 DecisionEngine）。
    Deciding,
    /// 多 Agent 编排执行（worker 并行 / review / merge）。
    Orchestrating,
    /// 模型调用（chat completion）。
    Thinking,
    /// 工具执行。
    CallingTool,
    /// 合成最终回答。
    Composing,
    /// 完成。
    Done,
    /// 失败（携带可读原因）。
    Failed,
}

impl AgentStage {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStage::Preparing => "preparing",
            AgentStage::Deciding => "deciding",
            AgentStage::Orchestrating => "orchestrating",
            AgentStage::Thinking => "thinking",
            AgentStage::CallingTool => "calling_tool",
            AgentStage::Composing => "composing",
            AgentStage::Done => "done",
            AgentStage::Failed => "failed",
        }
    }

    /// 中文标签（UI 直接用；后端不返回英文让前端翻译）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AgentStage::Preparing => "准备上下文",
            AgentStage::Deciding => "选择策略",
            AgentStage::Orchestrating => "多 Agent 执行",
            AgentStage::Thinking => "模型思考",
            AgentStage::CallingTool => "调用工具",
            AgentStage::Composing => "生成回答",
            AgentStage::Done => "完成",
            AgentStage::Failed => "失败",
        }
    }
}

/// 进度事件（推给 UI；只含结构与计数，无正文 / secret / 隐藏推理）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentProgress {
    pub stage: AgentStage,
    /// 阶段内计数（如第几个 worker / 第几次工具调用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 稳定错误码（仅 Failed；与 `AgentErrorKind::code` 对齐）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl AgentProgress {
    #[must_use]
    pub fn new(stage: AgentStage) -> Self {
        Self {
            stage,
            detail: None,
            error_code: None,
        }
    }

    #[must_use]
    pub fn with_detail(stage: AgentStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            detail: Some(detail.into()),
            error_code: None,
        }
    }

    #[must_use]
    pub fn failed(error_code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            stage: AgentStage::Failed,
            detail: Some(detail.into()),
            error_code: Some(error_code.to_string()),
        }
    }
}

/// 进度接收端口（实现方推送给 UI；同时最多一个订阅者）。
pub trait AgentProgressSink: Send + Sync {
    fn emit(&self, progress: &AgentProgress);
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
    /// 多 Agent 编排追踪（V9 §72：无 secret / 无正文）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<OrchestrationTraceView>,
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
            orchestration: None,
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
    fn agent_stage_labels_and_serialization() {
        assert_eq!(AgentStage::Preparing.as_str(), "preparing");
        assert_eq!(AgentStage::CallingTool.label(), "调用工具");
        assert_eq!(AgentStage::Done.as_str(), "done");
        assert_eq!(AgentStage::Failed.label(), "失败");
    }

    #[test]
    fn progress_event_carries_structure_only() {
        let thinking = AgentProgress::with_detail(AgentStage::Thinking, "round 1");
        assert_eq!(thinking.stage, AgentStage::Thinking);
        assert_eq!(thinking.detail.as_deref(), Some("round 1"));
        assert!(thinking.error_code.is_none());

        let progress = AgentProgress::failed("personal_ai_provider_error", "模型返回 400");
        assert_eq!(progress.stage, AgentStage::Failed);
        assert_eq!(progress.error_code.as_deref(), Some("personal_ai_provider_error"));
        // 序列化形状：阶段 + detail + error_code；无其它字段。
        let json = serde_json::to_value(&progress).unwrap();
        assert_eq!(json["stage"], "failed");
        assert_eq!(json["error_code"], "personal_ai_provider_error");
        assert!(json.get("session_id").is_none());
    }

    #[test]
    fn default_app_context_is_general() {
        assert!(AppContext::default().is_general());
    }

    #[test]
    fn content_part_round_trips_all_kinds() {
        let parts = vec![
            ContentPart::Text { text: "看这道题".into() },
            ContentPart::Image {
                source: "base64".into(),
                data: "iVBORw0KGgo=".into(),
                mime: "image/png".into(),
                caption: Some("学习板快照".into()),
            },
            ContentPart::Audio {
                source: "base64".into(),
                data: "AAAA".into(),
                mime: "audio/webm".into(),
                duration_ms: Some(1200),
            },
            ContentPart::DocumentRef {
                document_id: "doc-1".into(),
                title: Some("合同".into()),
            },
            ContentPart::BoardSnapshot {
                board_id: "board-1".into(),
                title: None,
                image_base64: Some("iVBOR".into()),
                stroke_count: 12,
            },
        ];
        for part in &parts {
            let json = serde_json::to_value(part).expect("serialize");
            let back: ContentPart = serde_json::from_value(json).expect("deserialize");
            assert_eq!(&back, part);
        }
        // tag 是 snake_case 且明确。
        let json = serde_json::to_value(&parts[1]).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["mime"], "image/png");
    }

    #[test]
    fn vision_requirement_and_capability_gate() {
        let text_only = ModelCapabilities::text_only();
        assert!(text_only.text);
        assert!(!text_only.vision);
        // 图片 → 需要 vision。
        let image = ContentPart::Image {
            source: "base64".into(),
            data: "x".into(),
            mime: "image/png".into(),
            caption: None,
        };
        assert!(image.requires_vision());
        assert!(!text_only.supports(std::slice::from_ref(&image)));
        let reason = text_only
            .unsupported_reason(std::slice::from_ref(&image))
            .expect("reason");
        assert!(reason.contains("不支持图片"), "{reason}");
        // 显式开启 vision → 支持。
        let vision = ModelCapabilities {
            text: true,
            vision: true,
            audio: false,
            tool_calling: true,
        };
        assert!(vision.supports(std::slice::from_ref(&image)));
        assert!(vision.unsupported_reason(std::slice::from_ref(&image)).is_none());
        // 音频同理。
        let audio = ContentPart::Audio {
            source: "base64".into(),
            data: "x".into(),
            mime: "audio/webm".into(),
            duration_ms: None,
        };
        assert!(!vision.supports(std::slice::from_ref(&audio)));
        assert!(
            vision
                .unsupported_reason(std::slice::from_ref(&audio))
                .is_some_and(|reason| reason.contains("不支持音频"))
        );
    }

    #[test]
    fn agent_request_legacy_message_still_works() {
        // 旧前端只发 message → effective_parts 派生单个 Text。
        let request = AgentRequest::text("你好");
        assert!(request.parts.is_empty());
        assert_eq!(request.effective_parts().len(), 1);
        assert_eq!(request.flattened_text(), "你好");
        // 序列化不含 parts（向后兼容契约）。
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("parts").is_none(), "空 parts 不序列化");
    }

    #[test]
    fn agent_request_parts_flatten_with_summaries() {
        let request = AgentRequest {
            message: "检查这块板".into(),
            parts: vec![
                ContentPart::Text { text: "检查这块板".into() },
                ContentPart::BoardSnapshot {
                    board_id: "b1".into(),
                    title: None,
                    image_base64: Some("SECRETBASE64DATA".into()),
                    stroke_count: 3,
                },
            ],
            ..AgentRequest::default()
        };
        let flat = request.flattened_text();
        assert!(flat.contains("检查这块板"));
        assert!(flat.contains("board:b1 (3 strokes)"), "{flat}");
        // 扁平文本不含 base64 数据本体（日志/prompt 安全）。
        assert!(!flat.contains("SECRETBASE64DATA"));
    }

    #[test]
    fn content_part_summary_never_leaks_data() {
        let image = ContentPart::Image {
            source: "base64".into(),
            data: "SECRETDATA".into(),
            mime: "image/png".into(),
            caption: Some("题板".into()),
        };
        assert!(!image.summary().contains("SECRETDATA"));
        assert!(image.summary().contains("题板"));
    }
}
