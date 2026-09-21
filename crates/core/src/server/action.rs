//! Safe Action 契约（V7 Track C，§51-§71）。
//!
//! 这是 V7 的安全核心：**模型永远不执行系统操作**，只能请求。
//!
//! ```text
//! 模型 → ActionRequest（typed，无命令字符串）
//!          │
//!          ▼
//!     SafeActionService::plan      → 授权 + 风险 + cooldown 检查
//!          │  Read → 直接执行；System → 签发 Confirmation
//!          ▼
//!     Confirmation（一次性 + 有效期 + fingerprint 绑定）
//!          │
//!     用户确认（UI 调 confirm_action）
//!          ▼
//!     confirm_and_execute          → 重算 fingerprint 比对 → 执行 → 审计
//! ```
//!
//! 设计约束：
//! - [`RegisteredAction`] 是**闭合枚举**——AST 层面不存在「任意命令」表示（§61）；
//! - [`Confirmation`] 绑定 [`ActionRequest::fingerprint`]，确认后参数不可变（§70）；
//! - 一次性 + 短有效期 → 重放与 TOCTOU 都被结构性拒绝（§58/§60）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::health::HealthStatus;
use super::is_valid_id;

/// 会话信任级别（§72-§76：LAN fail-closed 的判定输入）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTrust {
    /// 本地桌面会话（Tauri 窗口 + 用户确认，§76）。
    #[default]
    LocalDesktop,
    /// 已认证远程会话（当前不存在；留给后续身份系统）。
    RemoteAuthenticated,
    /// 无法验证的远程客户端（§73：写操作 disabled）。
    RemoteUntrusted,
}

impl SessionTrust {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SessionTrust::LocalDesktop => "local_desktop",
            SessionTrust::RemoteAuthenticated => "remote_authenticated",
            SessionTrust::RemoteUntrusted => "remote_untrusted",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "local_desktop" => Some(SessionTrust::LocalDesktop),
            "remote_authenticated" => Some(SessionTrust::RemoteAuthenticated),
            "remote_untrusted" => Some(SessionTrust::RemoteUntrusted),
            _ => None,
        }
    }

    /// 是否允许非 Read 操作（§73 fail-closed）。
    #[must_use]
    pub fn allows_writes(self) -> bool {
        matches!(self, SessionTrust::LocalDesktop | SessionTrust::RemoteAuthenticated)
    }
}

/// 已注册的可执行操作（§61：闭合集合；新增操作 = 新增变体，**不是**新字符串）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RegisteredAction {
    /// 重启一个**已注册**服务（§33：V7 唯一写操作）。
    RestartService {
        service_id: String,
        /// 可选：重启后期望达到的状态（默认 Healthy）。
        #[serde(default)]
        expect: HealthStatus,
    },
}

impl RegisteredAction {
    /// 操作类型名（审计 / UI / fingerprint）。
    #[must_use]
    pub fn action_type(&self) -> &'static str {
        match self {
            RegisteredAction::RestartService { .. } => "services.restart",
        }
    }

    /// 目标 id（审计 / cooldown 键）。
    #[must_use]
    pub fn target_id(&self) -> &str {
        match self {
            RegisteredAction::RestartService { service_id, .. } => service_id,
        }
    }

    /// 该操作的固有风险（§53）。
    #[must_use]
    pub fn risk(&self) -> ActionRisk {
        match self {
            RegisteredAction::RestartService { .. } => ActionRisk::System,
        }
    }

    /// 结构化自校验（模型入参 → 已注册操作的唯一入口）。
    ///
    /// 返回 `Err` 的 reason 是**稳定码**（`invalid_service_id` / `unknown_action`），
    /// 不回显模型原始输入（§65 精神：错误不回显内容）。
    pub fn restart_service(service_id: &str) -> Result<Self, String> {
        if !is_valid_id(service_id) {
            return Err("invalid_service_id".to_string());
        }
        Ok(RegisteredAction::RestartService {
            service_id: service_id.to_string(),
            expect: HealthStatus::Healthy,
        })
    }
}

/// 风险等级（§52：沿用 V4/V5 四级，不新增）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRisk {
    #[default]
    Read,
    SafeWrite,
    SensitiveWrite,
    System,
}

impl ActionRisk {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ActionRisk::Read => "read",
            ActionRisk::SafeWrite => "safe_write",
            ActionRisk::SensitiveWrite => "sensitive_write",
            ActionRisk::System => "system",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "read" => Some(ActionRisk::Read),
            "safe_write" => Some(ActionRisk::SafeWrite),
            "sensitive_write" => Some(ActionRisk::SensitiveWrite),
            "system" => Some(ActionRisk::System),
            _ => None,
        }
    }

    /// 是否需要用户确认（§54）。
    #[must_use]
    pub fn requires_confirmation(self) -> bool {
        !matches!(self, ActionRisk::Read)
    }
}

/// 授权决策（§73/§74）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum ActionAuthorizationDecision {
    /// 允许（可继续走确认 / 执行）。
    Allowed,
    /// 拒绝：稳定 reason 码 + 人类可读说明（不含内容）。
    Denied { reason: String, detail: String },
}

impl ActionAuthorizationDecision {
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, ActionAuthorizationDecision::Allowed)
    }

    #[must_use]
    pub fn denied(reason: impl Into<String>, detail: impl Into<String>) -> Self {
        ActionAuthorizationDecision::Denied {
            reason: reason.into(),
            detail: detail.into(),
        }
    }
}

/// 授权策略（§75：V7 只建接口；当前实现 fail-closed）。
pub trait ActionRiskPolicy: Send + Sync {
    fn authorize(&self, action: &RegisteredAction, trust: SessionTrust) -> ActionAuthorizationDecision;
}

/// 默认策略（§73/§74/§76）：
/// - `RemoteUntrusted` → 一切非 Read 拒绝；
/// - `LocalDesktop` / `RemoteAuthenticated` → 允许（仍需确认 + 审计）。
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultActionRiskPolicy;

impl ActionRiskPolicy for DefaultActionRiskPolicy {
    fn authorize(&self, action: &RegisteredAction, trust: SessionTrust) -> ActionAuthorizationDecision {
        if action.risk().requires_confirmation() && !trust.allows_writes() {
            return ActionAuthorizationDecision::denied(
                "untrusted_session",
                "当前会话无法验证身份：系统修改操作已禁用",
            );
        }
        ActionAuthorizationDecision::Allowed
    }
}

/// 动作请求（§55：确认票据的输入）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action: RegisteredAction,
    /// 发起会话（审计用）。
    pub session_id: String,
    /// 人类可读摘要（UI 展示；由服务生成，不来自模型原文）。
    pub summary: String,
    /// 风险（冗余自 `action.risk()`；保留以便审计与指纹）。
    pub risk: ActionRisk,
    /// 请求时间（Unix 秒）。
    pub created_at: i64,
}

impl ActionRequest {
    /// 结构化指纹（§58）：`action_type | target_id | canonical(parameters) | risk`。
    ///
    /// 确认与执行都用**同一个函数**计算 → 任何字段变化都会导致不匹配。
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let canonical = canonical_json(&self.action);
        format!(
            "{}|{}|{}|{}",
            self.action.action_type(),
            self.action.target_id(),
            canonical,
            self.risk.as_str()
        )
    }
}

/// 确认票据（§55-§60）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Confirmation {
    pub id: String,
    /// 绑定的请求指纹（§58：防 TOCTOU / 确认 A 执行 B）。
    pub request_fingerprint: String,
    /// UI 展示字段。
    pub action_type: String,
    pub target_id: String,
    pub human_readable_summary: String,
    pub risk: ActionRisk,
    pub created_at: i64,
    pub expires_at: i64,
    /// 状态（§60：一次性）。
    pub state: ConfirmationState,
    /// 发起会话。
    pub session_id: String,
}

impl Confirmation {
    #[must_use]
    pub fn is_usable(&self, now: i64) -> bool {
        matches!(self.state, ConfirmationState::Pending) && now < self.expires_at
    }
}

/// 确认状态。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationState {
    #[default]
    Pending,
    Confirmed,
    Consumed,
    Expired,
    Cancelled,
}

/// 执行结果（§62）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutcome {
    Success,
    Failed,
    Denied,
    Expired,
    Cancelled,
}

impl ActionOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ActionOutcome::Success => "success",
            ActionOutcome::Failed => "failed",
            ActionOutcome::Denied => "denied",
            ActionOutcome::Expired => "expired",
            ActionOutcome::Cancelled => "cancelled",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "success" => Some(ActionOutcome::Success),
            "failed" => Some(ActionOutcome::Failed),
            "denied" => Some(ActionOutcome::Denied),
            "expired" => Some(ActionOutcome::Expired),
            "cancelled" => Some(ActionOutcome::Cancelled),
            _ => None,
        }
    }
}

/// 审计条目（§64）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: i64,
    pub session_id: String,
    pub action_type: String,
    pub target_id: String,
    pub risk: ActionRisk,
    /// 是否经过用户确认。
    pub confirmed: bool,
    pub result: ActionOutcome,
    pub duration_ms: u64,
    /// 稳定错误码（不含内容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// 确定性 JSON 规范化（键排序）——指纹必须与序列化顺序无关。
#[must_use]
pub fn canonical_json<T: Serialize>(value: &T) -> String {
    let plain = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    let sorted = sort_keys(plain);
    serde_json::to_string(&sorted).unwrap_or_default()
}

fn sort_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (key, item) in map {
                sorted.insert(key, sort_keys(item));
            }
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_keys).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(service_id: &str) -> ActionRequest {
        ActionRequest {
            action: RegisteredAction::restart_service(service_id).expect("valid id"),
            session_id: "session-1".into(),
            summary: "重启服务 self-tools".into(),
            risk: ActionRisk::System,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn restart_rejects_injection_shapes() {
        // §155：服务 id 无法拼成命令。
        for bad in ["foo; rm -rf /", "--label", "foo bar", "Foo", ""] {
            let error = RegisteredAction::restart_service(bad).expect_err("must reject");
            assert_eq!(error, "invalid_service_id");
        }
    }

    #[test]
    fn fingerprint_is_stable_and_binding() {
        let first = request("self-tools");
        let second = request("self-tools");
        assert_eq!(first.fingerprint(), second.fingerprint(), "同请求 → 同指纹");

        // 目标变化 → 指纹变化（§58：确认 A 不得执行 B）。
        let other = request("geo-explorer");
        assert_ne!(first.fingerprint(), other.fingerprint());

        // 风险变化 → 指纹变化。
        let mut escalated = request("self-tools");
        escalated.risk = ActionRisk::SensitiveWrite;
        assert_ne!(first.fingerprint(), escalated.fingerprint());
    }

    #[test]
    fn canonical_json_ignores_key_order() {
        let left = serde_json::json!({"b": 1, "a": {"d": 2, "c": 3}});
        let right = serde_json::json!({"a": {"c": 3, "d": 2}, "b": 1});
        assert_eq!(canonical_json(&left), canonical_json(&right));
    }

    #[test]
    fn untrusted_session_denies_system_actions() {
        let policy = DefaultActionRiskPolicy;
        let action = RegisteredAction::restart_service("self-tools").expect("valid");
        assert!(policy.authorize(&action, SessionTrust::LocalDesktop).is_allowed());
        let denied = policy.authorize(&action, SessionTrust::RemoteUntrusted);
        assert!(!denied.is_allowed());
        match denied {
            ActionAuthorizationDecision::Denied { reason, .. } => {
                assert_eq!(reason, "untrusted_session")
            }
            _ => panic!("expected denied"),
        }
    }

    #[test]
    fn confirmation_expiry_and_single_use() {
        let confirmation = Confirmation {
            id: "cfm-1".into(),
            request_fingerprint: "fp".into(),
            action_type: "services.restart".into(),
            target_id: "self-tools".into(),
            human_readable_summary: "重启 self-tools".into(),
            risk: ActionRisk::System,
            created_at: 1_000,
            expires_at: 1_060,
            state: ConfirmationState::Pending,
            session_id: "s".into(),
        };
        assert!(confirmation.is_usable(1_030), "有效期内可用");
        assert!(!confirmation.is_usable(1_061), "过期不可用");
        let consumed = Confirmation {
            state: ConfirmationState::Consumed,
            ..confirmation.clone()
        };
        assert!(!consumed.is_usable(1_030), "一次性：已消费不可用");
    }

    #[test]
    fn risk_requires_confirmation_only_for_writes() {
        assert!(!ActionRisk::Read.requires_confirmation());
        assert!(ActionRisk::SafeWrite.requires_confirmation());
        assert!(ActionRisk::System.requires_confirmation());
    }
}
