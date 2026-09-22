//! 远程身份（V8 §26-§42）。
//!
//! 只定义**抽象**：V8 不实现 Authorization Server（§29：没有现成 provider 时
//! 不要临时造不安全的用户名密码系统）。infrastructure 提供 Fake 供 CI（§106）。
//!
//! 铁律：
//! - bearer 只从 `Authorization` header 来（§32，解析在 core 的 `McpCredential`）；
//! - 认证后端出错 → `ProviderUnavailable`，调用方 **DENY**（§42）；
//! - token 永不进入错误信息 / 日志 / 审计（§79）。

use devtoolbox_core::mcp::{AuthFailure, McpCredential, McpPrincipal};

/// 远程身份提供者（§26）。
pub trait RemoteIdentityProvider: Send + Sync {
    /// 用凭证换取 principal。任何失败都必须返回**具体** `AuthFailure`（§41）。
    fn authenticate(&self, credential: &McpCredential) -> Result<McpPrincipal, AuthFailure>;
}

/// 认证后端不可用（供上层转换为 `ProviderUnavailable`；这里显式建模，
/// 避免把 infra 错误含糊成 invalid token）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthProviderUnavailable(pub String);

impl std::fmt::Display for AuthProviderUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 不回显凭证；只说明后端不可用。
        formatter.write_str("identity provider unavailable")
    }
}

impl std::error::Error for AuthProviderUnavailable {}

/// 未配置任何 provider 时的占位实现：**一律拒绝**（§6：没有身份系统时
/// Remote MCP Writes = DISABLED；§40：不可信远程 = 0 tools）。
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyAllIdentityProvider;

impl RemoteIdentityProvider for DenyAllIdentityProvider {
    fn authenticate(&self, _credential: &McpCredential) -> Result<McpPrincipal, AuthFailure> {
        Err(AuthFailure::Unauthenticated)
    }
}

/// 静态 token → principal 的 Fake provider（§99/§106：CI 与本地开发用）。
///
/// token 表在构造时注入（不读配置、不落盘）；**永不记录 token**。
#[derive(Debug, Default)]
pub struct StaticTokenIdentityProvider {
    /// token → (principal_id, client_id, scope 列表, issuer, expires_at)。
    tokens: parking_lot::Mutex<
        Vec<(
            String,
            String,
            String,
            Vec<&'static str>,
            Option<String>,
            Option<i64>,
        )>,
    >,
}

impl StaticTokenIdentityProvider {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 token → principal 映射（测试辅助；生产由 OAuth provider 替代）。
    pub fn insert(
        &self,
        token: impl Into<String>,
        principal_id: impl Into<String>,
        client_id: impl Into<String>,
        scopes: Vec<&'static str>,
        issuer: Option<String>,
        expires_at: Option<i64>,
    ) {
        self.tokens.lock().push((
            token.into(),
            principal_id.into(),
            client_id.into(),
            scopes,
            issuer,
            expires_at,
        ));
    }

    /// 按 token 认证（大小写敏感比较；不做常量时间优化——本地/Fake 场景）。
    fn lookup(&self, token: &str) -> Option<McpPrincipal> {
        self.tokens
            .lock()
            .iter()
            .find(|(known, ..)| known == token)
            .map(|(_, principal_id, client_id, scopes, issuer, expires_at)| {
                McpPrincipal {
                    principal_id: principal_id.clone(),
                    client_id: client_id.clone(),
                    transport: devtoolbox_core::mcp::McpTransport::Http,
                    trust: devtoolbox_core::mcp::McpTrustLevel::RemoteAuthenticated,
                    authenticated: true,
                    scopes: scopes
                        .iter()
                        .filter_map(|scope| devtoolbox_core::mcp::McpScope::parse(scope))
                        .collect(),
                    issuer: issuer.clone(),
                    expires_at: *expires_at,
                }
            })
    }
}

impl RemoteIdentityProvider for StaticTokenIdentityProvider {
    fn authenticate(&self, credential: &McpCredential) -> Result<McpPrincipal, AuthFailure> {
        let McpCredential::Bearer(token) = credential else {
            return Err(AuthFailure::Unauthenticated);
        };
        match self.lookup(token) {
            Some(principal) => {
                let now = unix_now();
                if principal.is_expired(now) {
                    // §41：过期是独立失败态（client 可据此刷新）。
                    return Err(AuthFailure::ExpiredToken);
                }
                Ok(principal)
            }
            // §32/§79：错误信息绝不回显 token 本身。
            None => Err(AuthFailure::InvalidToken),
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_provider_rejects_everything() {
        let provider = DenyAllIdentityProvider;
        assert_eq!(
            provider.authenticate(&McpCredential::Bearer("x".into())),
            Err(AuthFailure::Unauthenticated)
        );
        assert_eq!(
            provider.authenticate(&McpCredential::None),
            Err(AuthFailure::Unauthenticated)
        );
    }

    #[test]
    fn static_provider_authenticates_known_token() {
        let provider = StaticTokenIdentityProvider::new();
        provider.insert(
            "tok-1",
            "p-1",
            "pi",
            vec!["selftools.read", "server.read"],
            Some("https://issuer.example".into()),
            None,
        );
        let principal = provider
            .authenticate(&McpCredential::Bearer("tok-1".into()))
            .expect("auth");
        assert_eq!(principal.principal_id, "p-1");
        assert_eq!(principal.client_id, "pi");
        assert!(principal.authenticated);
        assert!(principal.has_scope(&devtoolbox_core::mcp::McpScope::parse("server.read").unwrap()));
    }

    #[test]
    fn static_provider_distinguishes_failure_modes() {
        let provider = StaticTokenIdentityProvider::new();
        provider.insert("valid", "p", "c", vec!["selftools.read"], None, Some(1));
        // 未知 token → InvalidToken（不回显 token）。
        assert_eq!(
            provider.authenticate(&McpCredential::Bearer("unknown".into())),
            Err(AuthFailure::InvalidToken)
        );
        // 过期 → ExpiredToken。
        assert_eq!(
            provider.authenticate(&McpCredential::Bearer("valid".into())),
            Err(AuthFailure::ExpiredToken)
        );
        // 无凭证 → Unauthenticated。
        assert_eq!(
            provider.authenticate(&McpCredential::None),
            Err(AuthFailure::Unauthenticated)
        );
    }

    #[test]
    fn error_text_never_echoes_token() {
        let error = AuthProviderUnavailable("backend down".into());
        let text = error.to_string();
        assert_eq!(text, "identity provider unavailable");
        assert!(!text.contains("backend"), "{text}");
    }
}
