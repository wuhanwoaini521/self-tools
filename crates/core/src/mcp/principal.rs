//! Principal / Scope / Trust（V8 §20-§42）。
//!
//! `McpPrincipal` 描述**调用方**（MCP client + 身份），不是 self-tools 的用户
//! 身份（§27：Remote identity 只表示「谁在调用 MCP」）。

use serde::{Deserialize, Serialize};

/// 传输层（§5：Local != Remote 的判定输入）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// 本地 STDIO（同机进程管道）。
    #[default]
    Stdio,
    /// Streamable HTTP（本机 loopback 或局域网）。
    Http,
}

impl McpTransport {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            McpTransport::Stdio => "stdio",
            McpTransport::Http => "http",
        }
    }
}

/// 信任级别（§21）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTrustLevel {
    /// 本地受信进程（STDIO / loopback 同机调用）。**仍受 Tool Risk 约束**（§22）。
    #[default]
    LocalTrusted,
    /// 已认证远程调用方。
    RemoteAuthenticated,
    /// 无法验证的远程调用方（§40：0 tools）。
    RemoteUntrusted,
}

impl McpTrustLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            McpTrustLevel::LocalTrusted => "local_trusted",
            McpTrustLevel::RemoteAuthenticated => "remote_authenticated",
            McpTrustLevel::RemoteUntrusted => "remote_untrusted",
        }
    }

    /// 是否允许任何工具暴露（§40：不可信远程 = 0 tools）。
    #[must_use]
    pub fn can_expose_tools(self) -> bool {
        !matches!(self, McpTrustLevel::RemoteUntrusted)
    }
}

/// Scope（§36）：细字符串新类型，避免到处传裸字符串。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct McpScope(String);

impl McpScope {
    /// 已知 scope（§36：先做 8 个，不一开始几十个）。
    pub const READ: &'static str = "selftools.read";
    pub const MEMORY_READ: &'static str = "memory.read";
    pub const DOCUMENTS_READ: &'static str = "documents.read";
    pub const FILES_READ: &'static str = "files.read";
    pub const SERVER_READ: &'static str = "server.read";
    pub const HISTORY_ENRICH: &'static str = "history.enrich";
    pub const MEMORY_WRITE: &'static str = "memory.write";
    pub const SERVER_ACTION: &'static str = "server.action";

    /// 解析单个 scope；未知值 → `None`（不静默接受）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim().to_ascii_lowercase();
        let known = [
            Self::READ,
            Self::MEMORY_READ,
            Self::DOCUMENTS_READ,
            Self::FILES_READ,
            Self::SERVER_READ,
            Self::HISTORY_ENRICH,
            Self::MEMORY_WRITE,
            Self::SERVER_ACTION,
        ];
        known.contains(&trimmed.as_str()).then_some(Self(trimmed))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否覆盖所需 scope（`selftools.read` 覆盖所有 module read）。
    #[must_use]
    pub fn covers(&self, required: &McpScope) -> bool {
        if self == required {
            return true;
        }
        // 宽 scope：`selftools.read` 覆盖 `{memory,documents,files,server}.read`。
        self.0 == Self::READ
            && matches!(
                required.0.as_str(),
                Self::MEMORY_READ | Self::DOCUMENTS_READ | Self::FILES_READ | Self::SERVER_READ
            )
    }
}

/// 调用方身份（§20）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpPrincipal {
    /// 主体 id（认证后由 identity provider 给出；本地 = `local`）。
    pub principal_id: String,
    /// client id（用于跨 client 隔离与审计，§58/§103）。
    pub client_id: String,
    pub transport: McpTransport,
    pub trust: McpTrustLevel,
    #[serde(default)]
    pub authenticated: bool,
    #[serde(default)]
    pub scopes: Vec<McpScope>,
    /// issuer（OAuth/OIDC 校验用；本地为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// token 过期时间（Unix 秒；本地 = `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl McpPrincipal {
    /// 本地 STDIO / loopback 的受信调用方（§22：risk 仍生效）。
    #[must_use]
    pub fn local(client_id: impl Into<String>) -> Self {
        Self {
            principal_id: "local-principal".to_string(),
            client_id: client_id.into(),
            transport: McpTransport::Stdio,
            trust: McpTrustLevel::LocalTrusted,
            authenticated: true,
            scopes: Vec::new(),
            issuer: None,
            expires_at: None,
        }
    }

    /// 未认证远程调用方（§40：0 tools）。
    #[must_use]
    pub fn anonymous_remote(client_id: impl Into<String>) -> Self {
        Self {
            principal_id: "anonymous".to_string(),
            client_id: client_id.into(),
            transport: McpTransport::Http,
            trust: McpTrustLevel::RemoteUntrusted,
            authenticated: false,
            scopes: Vec::new(),
            issuer: None,
            expires_at: None,
        }
    }

    #[must_use]
    pub fn has_scope(&self, required: &McpScope) -> bool {
        self.scopes.iter().any(|scope| scope.covers(required))
    }

    /// 是否已过期（本地受信无过期概念；远程按 `expires_at`）。
    #[must_use]
    pub fn is_expired(&self, now: i64) -> bool {
        self.expires_at.is_some_and(|expires| now >= expires)
    }

    /// 审计友好的摘要（**不含 token**，§79）。
    #[must_use]
    pub fn audit_label(&self) -> String {
        format!(
            "{}@{} via {} [{}]",
            self.principal_id,
            if self.client_id.is_empty() {
                "unknown-client"
            } else {
                self.client_id.as_str()
            },
            self.transport.as_str(),
            self.trust.as_str()
        )
    }
}

/// 凭证（§31/§32：只从 `Authorization` header 取 bearer；URL 查询参数非法）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum McpCredential {
    /// 无凭证。
    #[default]
    None,
    /// `Authorization: Bearer <token>`（值只在内存中传递，禁止日志/审计）。
    Bearer(String),
}

impl McpCredential {
    /// 从 `Authorization` header 值解析（§32）。
    ///
    /// 大小写不敏感的 `bearer` scheme；其它 scheme（basic / token）→ `None`。
    #[must_use]
    pub fn from_authorization_header(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        let (scheme, rest) = trimmed.split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return None;
        }
        let token = rest.trim();
        (!token.is_empty()).then(|| McpCredential::Bearer(token.to_string()))
    }

    /// 是否为空。
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, McpCredential::None)
    }
}

/// 认证失败（§41：四态，不模糊成 internal error）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthFailure {
    /// 未提供凭证。
    Unauthenticated,
    /// 凭证无效（签名 / 格式 / 未知 client）。
    InvalidToken,
    /// 凭证已过期。
    ExpiredToken,
    /// scope 不足（§41：认证成功但授权失败时也用它表达）。
    InsufficientScope,
    /// 认证后端不可用（§42：fail-closed）。
    ProviderUnavailable,
}

impl AuthFailure {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuthFailure::Unauthenticated => "unauthenticated",
            AuthFailure::InvalidToken => "invalid_token",
            AuthFailure::ExpiredToken => "expired_token",
            AuthFailure::InsufficientScope => "insufficient_scope",
            AuthFailure::ProviderUnavailable => "provider_unavailable",
        }
    }

    /// 人类可读说明（不含任何凭证内容）。
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            AuthFailure::Unauthenticated => "缺少访问凭证",
            AuthFailure::InvalidToken => "访问凭证无效",
            AuthFailure::ExpiredToken => "访问凭证已过期",
            AuthFailure::InsufficientScope => "凭证权限不足",
            AuthFailure::ProviderUnavailable => "认证服务暂不可用",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_principal_is_trusted_but_not_root() {
        let principal = McpPrincipal::local("pi");
        assert!(principal.authenticated);
        assert_eq!(principal.trust, McpTrustLevel::LocalTrusted);
        // §22：local trusted 不等于跳过 risk —— 它没有隐含 scope。
        assert!(principal.scopes.is_empty());
        assert!(principal.trust.can_expose_tools());
    }

    #[test]
    fn anonymous_remote_exposes_nothing() {
        let principal = McpPrincipal::anonymous_remote("curl");
        assert!(!principal.authenticated);
        assert!(
            !principal.trust.can_expose_tools(),
            "§40：不可信远程 = 0 tools"
        );
        assert!(!principal.has_scope(&McpScope::parse("selftools.read").expect("scope")));
    }

    #[test]
    fn scope_covers_module_reads() {
        let wide = McpScope::parse("selftools.read").expect("scope");
        let memory = McpScope::parse("memory.read").expect("scope");
        let action = McpScope::parse("server.action").expect("scope");
        assert!(wide.covers(&memory), "宽 scope 覆盖 module read");
        assert!(!wide.covers(&action), "宽 scope 不覆盖写操作");
        assert!(memory.covers(&memory));
        assert!(!memory.covers(&wide));
    }

    #[test]
    fn unknown_scope_is_rejected() {
        assert!(McpScope::parse("everything").is_none());
        assert!(McpScope::parse("").is_none());
        assert!(McpScope::parse("  SERVER.READ  ").is_some(), "trim + 小写");
    }

    #[test]
    fn bearer_is_only_read_from_authorization_header() {
        assert_eq!(
            McpCredential::from_authorization_header("Bearer abc123"),
            Some(McpCredential::Bearer("abc123".into()))
        );
        assert_eq!(
            McpCredential::from_authorization_header("bearer abc123"),
            Some(McpCredential::Bearer("abc123".into()))
        );
        // §31：查询参数 / 其它 scheme / 空 token 一律拒绝。
        assert_eq!(
            McpCredential::from_authorization_header("Basic abc123"),
            None
        );
        assert_eq!(McpCredential::from_authorization_header("Bearer"), None);
        assert_eq!(McpCredential::from_authorization_header("Bearer   "), None);
        assert!(McpCredential::None.is_none());
    }

    #[test]
    fn principal_expiry_uses_timestamp() {
        let mut principal = McpPrincipal::local("pi");
        assert!(!principal.is_expired(1_000), "本地无过期");
        principal.expires_at = Some(100);
        assert!(principal.is_expired(100), "到期即过期");
        assert!(!principal.is_expired(99));
    }

    #[test]
    fn audit_label_never_contains_token() {
        let principal = McpPrincipal {
            principal_id: "p-1".into(),
            client_id: "c-1".into(),
            transport: McpTransport::Http,
            trust: McpTrustLevel::RemoteAuthenticated,
            authenticated: true,
            scopes: vec![McpScope::parse("server.read").expect("scope")],
            issuer: Some("https://issuer.example".into()),
            expires_at: Some(1),
        };
        let label = principal.audit_label();
        assert!(label.contains("p-1@c-1"), "{label}");
        assert!(label.contains("http"), "{label}");
        assert!(label.contains("remote_authenticated"), "{label}");
    }
}
