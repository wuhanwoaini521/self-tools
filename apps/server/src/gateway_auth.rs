//! 局域网网关认证标记的验签（Phase 1.4）。
//!
//! 原始 API 仍然只监听 loopback。局域网请求只能由 `scripts/lan-gateway.mjs`
//! 在密码登录通过后转发；网关同时给请求注入一组签名头，让上游知道
//! **是谁**（principal / role），而不是把「loopback 来源」当成桌面身份。
//!
//! 标记（全部小写 HTTP 头）：
//! - `x-selftools-gateway-principal`
//! - `x-selftools-gateway-role`      `admin` | `viewer`
//! - `x-selftools-gateway-expires`   Unix 秒
//! - `x-selftools-gateway-nonce`     每个请求唯一（一次性）
//! - `x-selftools-gateway-signature` `HMAC-SHA256(key, canonical)` 的小写 hex
//!
//! canonical = `METHOD\nPATH\nEXPIRES\nNONCE\nPRINCIPAL\nROLE\nsha256_hex(BODY)`。
//! 密钥来自 `SELF_TOOLS_GATEWAY_SIGNING_KEY`，否则首次启动生成
//! `<data_dir>/gateway-signing.key`（Unix 下 0600）。网关与上游必须读到同一份。
//!
//! 安全规则（fail-closed）：
//! - 只要出现任意一个网关头，就必须五个齐全并通过验签；
//! - 签名失败、过期、nonce 重放、角色未知 → `401 gateway_auth_required`；
//! - 客户端自带的 `x-selftools-*` 由网关在转发前剥离再重注，不会被采信；
//! - 验签通过也只注入 [`AuthenticatedRemote`]，**绝不**当作 `LocalDesktop`。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use serde_json::json;
use sha2::{Digest, Sha256};

use devtoolbox_core::server::SessionTrust;

pub const HEADER_PRINCIPAL: &str = "x-selftools-gateway-principal";
pub const HEADER_ROLE: &str = "x-selftools-gateway-role";
pub const HEADER_EXPIRES: &str = "x-selftools-gateway-expires";
pub const HEADER_NONCE: &str = "x-selftools-gateway-nonce";
pub const HEADER_SIGNATURE: &str = "x-selftools-gateway-signature";

/// 显式共享密钥（推荐生产使用；优先级高于文件）。
pub const KEY_ENV: &str = "SELF_TOOLS_GATEWAY_SIGNING_KEY";
/// 首次启动自动生成的密钥文件名（位于数据目录）。
pub const KEY_FILE_NAME: &str = "gateway-signing.key";

/// 网关签名的请求体上限（与 `DefaultBodyLimit` 一致）。
pub const MAX_SIGNED_BODY: usize = 1024 * 1024;
/// 标记最长有效期（秒）。网关当前签发 120 秒，这里留出余量。
pub const MAX_SIGNATURE_LIFETIME_SECS: i64 = 300;
/// 容忍的时钟偏差（秒）。
pub const CLOCK_SKEW_SECS: i64 = 5;

const MIN_KEY_LEN: usize = 32;
const MAX_KEY_LEN: usize = 256;
const MAX_NONCE_CACHE: usize = 4096;

const HEADERS: [&str; 5] = [
    HEADER_PRINCIPAL,
    HEADER_ROLE,
    HEADER_EXPIRES,
    HEADER_NONCE,
    HEADER_SIGNATURE,
];

/// 网关声明的角色。当前网关只签发 `admin`；`viewer` 留给只读身份。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayRole {
    Admin,
    Viewer,
}

impl GatewayRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            GatewayRole::Admin => "admin",
            GatewayRole::Viewer => "viewer",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "admin" => Some(GatewayRole::Admin),
            "viewer" => Some(GatewayRole::Viewer),
            _ => None,
        }
    }
}

/// 验签通过的远程身份（由中间件注入 request extensions）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedRemote {
    pub principal: String,
    pub role: GatewayRole,
    /// 票据绑定用的会话 id（`gateway:<principal>`），不是桌面会话。
    pub session_id: String,
}

impl AuthenticatedRemote {
    #[must_use]
    pub fn new(principal: impl Into<String>, role: GatewayRole) -> Self {
        let principal = principal.into();
        Self {
            session_id: format!("gateway:{principal}"),
            principal,
            role,
        }
    }

    #[must_use]
    pub fn is_admin(&self) -> bool {
        self.role == GatewayRole::Admin
    }

    /// 信任级别：admin 允许写（仍需确认 + 审计），其它一律不可信。
    #[must_use]
    pub fn trust(&self) -> SessionTrust {
        if self.is_admin() {
            SessionTrust::RemoteGatewayAdmin
        } else {
            SessionTrust::RemoteUntrusted
        }
    }
}

/// 验签失败原因（不对外暴露细节，只用于内部测试与日志）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayAuthError {
    KeyUnavailable,
    MissingHeader,
    InvalidHeader,
    Expired,
    Replay,
    InvalidSignature,
    UnsupportedRole,
}

/// 共享密钥 + 一次性 nonce 缓存。
pub struct GatewayVerifier {
    key: Option<String>,
    seen: Mutex<HashMap<String, i64>>,
}

impl GatewayVerifier {
    /// 读取显式环境变量或数据目录中的密钥；失败时保持「未配置」并 fail-closed。
    #[must_use]
    pub fn load(config_dir: &Path) -> Self {
        match load_gateway_key(config_dir) {
            Ok(key) => Self::from_key(key),
            Err(error) => {
                eprintln!("[gateway-auth] signing key unavailable: {error}");
                Self::disabled()
            }
        }
    }

    /// 用显式密钥构造（测试 / 嵌入式装配）。
    #[must_use]
    pub fn from_key(key: impl Into<String>) -> Self {
        Self {
            key: Some(key.into()),
            seen: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            key: None,
            seen: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn key_configured(&self) -> bool {
        self.key.is_some()
    }

    /// 验签并返回远程身份。
    ///
    /// `now` 以 Unix 秒传入，便于测试过期路径。nonce 只有在签名通过后才会进入
    /// 缓存，未认证请求无法污染重放表。
    pub fn verify(
        &self,
        method: &str,
        path_and_query: &str,
        headers: &HeaderMap,
        body: &[u8],
        now: i64,
    ) -> Result<AuthenticatedRemote, GatewayAuthError> {
        let key = self
            .key
            .as_deref()
            .ok_or(GatewayAuthError::KeyUnavailable)?;
        let principal = header(headers, HEADER_PRINCIPAL)?;
        let role_raw = header(headers, HEADER_ROLE)?;
        let expires_raw = header(headers, HEADER_EXPIRES)?;
        let nonce = header(headers, HEADER_NONCE)?;
        let signature = header(headers, HEADER_SIGNATURE)?;

        if !valid_principal(principal) || !valid_nonce(nonce) {
            return Err(GatewayAuthError::InvalidHeader);
        }
        let role = GatewayRole::parse(role_raw).ok_or(GatewayAuthError::UnsupportedRole)?;
        let expires: i64 = expires_raw
            .parse()
            .map_err(|_| GatewayAuthError::InvalidHeader)?;
        if expires + CLOCK_SKEW_SECS < now {
            return Err(GatewayAuthError::Expired);
        }
        if expires > now + MAX_SIGNATURE_LIFETIME_SECS + CLOCK_SKEW_SECS {
            return Err(GatewayAuthError::InvalidHeader);
        }

        let expected = sign_gateway_request(
            key,
            method,
            path_and_query,
            expires,
            nonce,
            principal,
            role.as_str(),
            body,
        );
        let expected_bytes = decode_hex(&expected).ok_or(GatewayAuthError::InvalidHeader)?;
        let actual_bytes = decode_hex(signature).ok_or(GatewayAuthError::InvalidHeader)?;
        if !constant_time_eq(&expected_bytes, &actual_bytes) {
            return Err(GatewayAuthError::InvalidSignature);
        }

        let nonce_key = format!("{principal}\n{nonce}");
        let mut seen = self.seen.lock();
        if seen.contains_key(&nonce_key) {
            return Err(GatewayAuthError::Replay);
        }
        if seen.len() >= MAX_NONCE_CACHE {
            seen.retain(|_, expires_at| *expires_at + CLOCK_SKEW_SECS >= now);
        }
        seen.insert(nonce_key, expires);
        drop(seen);

        Ok(AuthenticatedRemote::new(principal, role))
    }
}

/// 请求是否携带任意网关标记。只有「一个都没有」才当作普通 loopback 请求。
#[must_use]
pub fn carries_gateway_headers(headers: &HeaderMap) -> bool {
    HEADERS.iter().any(|name| headers.contains_key(*name))
}

/// 计算签名（小写 hex）。网关的 Node 实现必须逐字节一致。
///
/// 参数就是 canonical 的字段本身；拆 struct 只会让「哪个字段参与签名」变模糊。
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn sign_gateway_request(
    key: &str,
    method: &str,
    path_and_query: &str,
    expires: i64,
    nonce: &str,
    principal: &str,
    role: &str,
    body: &[u8],
) -> String {
    let body_hash = hex_lower(&Sha256::digest(body));
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        path_and_query,
        expires,
        nonce,
        principal,
        role,
        body_hash
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("hmac accepts any key");
    mac.update(canonical.as_bytes());
    hex_lower(&mac.finalize().into_bytes())
}

/// 读取 / 首次生成共享密钥。
pub fn load_gateway_key(config_dir: &Path) -> Result<String, String> {
    if let Ok(raw) = std::env::var(KEY_ENV) {
        let key = raw.trim().to_string();
        validate_key(&key)?;
        return Ok(key);
    }
    load_or_create_key_file(config_dir)
}

fn load_or_create_key_file(config_dir: &Path) -> Result<String, String> {
    let path = config_dir.join(KEY_FILE_NAME);
    if path.exists() {
        let key = std::fs::read_to_string(&path)
            .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?
            .trim()
            .to_string();
        validate_key(&key)?;
        return Ok(key);
    }

    // 两个 UUIDv4 拼接 = 256 bit 随机 hex；不用时间/PID 之类的弱随机源。
    let generated = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    std::fs::create_dir_all(config_dir)
        .map_err(|error| format!("创建数据目录 {} 失败：{error}", config_dir.display()))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            use std::io::Write;
            writeln!(file, "{generated}").map_err(|error| error.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
            Ok(generated)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // 网关与上游同时首次启动：输的那一方重新读取赢家写入的密钥。
            let key = std::fs::read_to_string(&path)
                .map_err(|error| error.to_string())?
                .trim()
                .to_string();
            validate_key(&key)?;
            Ok(key)
        }
        Err(error) => Err(format!("写入 {} 失败：{error}", path.display())),
    }
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.len() < MIN_KEY_LEN || key.len() > MAX_KEY_LEN {
        return Err(format!(
            "{KEY_ENV} / {KEY_FILE_NAME} 长度必须在 {MIN_KEY_LEN}..={MAX_KEY_LEN} 之间"
        ));
    }
    if key.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("网关签名密钥不能包含空白或控制字符".to_string());
    }
    Ok(())
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, GatewayAuthError> {
    headers
        .get(name)
        .ok_or(GatewayAuthError::MissingHeader)?
        .to_str()
        .map_err(|_| GatewayAuthError::InvalidHeader)
}

fn valid_principal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '@'))
}

fn valid_nonce(value: &str) -> bool {
    (8..=128).contains(&value.len())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn hex_lower(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(raw: &str) -> Option<Vec<u8>> {
    let raw = raw.trim();
    if !raw.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for pair in bytes.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push(((high << 4) | low) as u8);
    }
    Some(out)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

/// 中间件：验签成功时注入 [`AuthenticatedRemote`]；失败一律 401。
pub async fn gateway_auth(
    State(verifier): State<Arc<GatewayVerifier>>,
    request: Request,
    next: Next,
) -> Response {
    if !carries_gateway_headers(request.headers()) {
        return next.run(request).await;
    }

    let (mut parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_SIGNED_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => return unauthorized(),
    };
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    match verifier.verify(
        parts.method.as_str(),
        path_and_query,
        &parts.headers,
        &bytes,
        unix_now(),
    ) {
        Ok(remote) => {
            parts.extensions.insert(remote);
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        Err(_) => unauthorized(),
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({
            "code": "gateway_auth_required",
            "message": "网关认证标记无效或已过期；请重新登录局域网网关",
        })),
    )
        .into_response()
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

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn headers_with(method: &str, path: &str, body: &[u8], now: i64) -> HeaderMap {
        let expires = now + 120;
        let nonce = "nonce-0000000000000001";
        let signature = sign_gateway_request(
            KEY,
            method,
            path,
            expires,
            nonce,
            "lan-owner",
            "admin",
            body,
        );
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_PRINCIPAL, "lan-owner".parse().unwrap());
        headers.insert(HEADER_ROLE, "admin".parse().unwrap());
        headers.insert(HEADER_EXPIRES, expires.to_string().parse().unwrap());
        headers.insert(HEADER_NONCE, nonce.parse().unwrap());
        headers.insert(HEADER_SIGNATURE, signature.parse().unwrap());
        headers
    }

    #[test]
    fn valid_signature_injects_gateway_admin_not_desktop() {
        let verifier = GatewayVerifier::from_key(KEY);
        let now = 1_800_000_000;
        let headers = headers_with("POST", "/api/v1/server/actions/restart", b"{}", now);
        let remote = verifier
            .verify(
                "POST",
                "/api/v1/server/actions/restart",
                &headers,
                b"{}",
                now,
            )
            .expect("verified");
        assert_eq!(remote.principal, "lan-owner");
        assert_eq!(remote.role, GatewayRole::Admin);
        assert_eq!(remote.session_id, "gateway:lan-owner");
        assert_eq!(remote.trust(), SessionTrust::RemoteGatewayAdmin);
        assert_ne!(remote.trust(), SessionTrust::LocalDesktop);
    }

    /// 与 `scripts/lan-gateway.test.mjs` 里同名向量逐字节一致。
    ///
    /// 两端各算一次同样的输入，任何一侧改了 canonical 拼接顺序、大小写或
    /// body 哈希方式，都会在这里先炸，而不是等到真机联调时 401。
    #[test]
    fn signature_matches_the_node_gateway_test_vector() {
        let signature = sign_gateway_request(
            "test-gateway-signing-key-0123456789abcdef",
            "POST",
            "/api/v1/server/actions/restart",
            1_800_000_120,
            "0123456789abcdef0123456789abcdef",
            "lan-owner",
            "admin",
            br#"{"serviceId":"self-tools"}"#,
        );
        assert_eq!(
            signature,
            "6e53cd9d24e1206762b74810c6d4be60fa1439985fbcef97f1104e065283dd5d"
        );
    }

    #[test]
    fn tampered_body_and_signature_are_rejected() {
        let verifier = GatewayVerifier::from_key(KEY);
        let now = 1_800_000_000;
        let headers = headers_with("POST", "/api/v1/server/actions/restart", b"{\"a\":1}", now);
        assert_eq!(
            verifier.verify(
                "POST",
                "/api/v1/server/actions/restart",
                &headers,
                b"{\"a\":2}",
                now
            ),
            Err(GatewayAuthError::InvalidSignature)
        );

        let mut tampered = headers.clone();
        let signature = tampered
            .get(HEADER_SIGNATURE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let mut chars: Vec<char> = signature.chars().collect();
        chars[0] = if chars[0] == '0' { '1' } else { '0' };
        let flipped: String = chars.into_iter().collect();
        tampered.insert(HEADER_SIGNATURE, flipped.parse().unwrap());
        assert_eq!(
            verifier.verify(
                "POST",
                "/api/v1/server/actions/restart",
                &tampered,
                b"{\"a\":1}",
                now
            ),
            Err(GatewayAuthError::InvalidSignature)
        );
    }

    #[test]
    fn path_method_and_body_are_bound_into_the_signature() {
        let verifier = GatewayVerifier::from_key(KEY);
        let now = 1_800_000_000;
        let headers = headers_with("POST", "/api/v1/server/actions/restart", b"{}", now);
        assert_eq!(
            verifier.verify(
                "GET",
                "/api/v1/server/actions/restart",
                &headers,
                b"{}",
                now
            ),
            Err(GatewayAuthError::InvalidSignature)
        );
        assert_eq!(
            verifier.verify("POST", "/api/v1/server/status", &headers, b"{}", now),
            Err(GatewayAuthError::InvalidSignature)
        );
    }

    #[test]
    fn expired_replayed_and_future_markers_are_rejected() {
        let verifier = GatewayVerifier::from_key(KEY);
        let now = 1_800_000_000;
        let headers = headers_with("GET", "/api/v1/server/status", b"", now);
        verifier
            .verify("GET", "/api/v1/server/status", &headers, b"", now)
            .expect("first use");

        // 同一 nonce 第二次 = 重放。
        assert_eq!(
            verifier.verify("GET", "/api/v1/server/status", &headers, b"", now),
            Err(GatewayAuthError::Replay)
        );

        let late = headers_with("GET", "/api/v1/server/status", b"", now);
        assert_eq!(
            verifier.verify(
                "GET",
                "/api/v1/server/status",
                &late,
                b"",
                now + 121 + CLOCK_SKEW_SECS + 1
            ),
            Err(GatewayAuthError::Expired)
        );

        let far_future = headers_with("GET", "/api/v1/server/status", b"", now + 10_000);
        assert_eq!(
            verifier.verify("GET", "/api/v1/server/status", &far_future, b"", now),
            Err(GatewayAuthError::InvalidHeader)
        );
    }

    #[test]
    fn partial_or_unknown_headers_fail_closed() {
        let verifier = GatewayVerifier::from_key(KEY);
        let now = 1_800_000_000;
        let mut headers = headers_with("GET", "/api/v1/server/status", b"", now);
        headers.remove(HEADER_SIGNATURE);
        assert_eq!(
            verifier.verify("GET", "/api/v1/server/status", &headers, b"", now),
            Err(GatewayAuthError::MissingHeader)
        );

        let mut unknown_role = headers_with("GET", "/api/v1/server/status", b"", now);
        unknown_role.insert(HEADER_ROLE, "root".parse().unwrap());
        assert_eq!(
            verifier.verify("GET", "/api/v1/server/status", &unknown_role, b"", now),
            Err(GatewayAuthError::UnsupportedRole)
        );

        assert!(carries_gateway_headers(&headers));
        assert!(!carries_gateway_headers(&HeaderMap::new()));
    }

    async fn echo_remote(
        axum::extract::Extension(remote): axum::extract::Extension<AuthenticatedRemote>,
        body: String,
    ) -> axum::Json<serde_json::Value> {
        axum::Json(json!({
            "principal": remote.principal,
            "session_id": remote.session_id,
            "body": body,
        }))
    }

    fn signed_headers(method: &str, path: &str, body: &[u8], nonce: &str) -> HeaderMap {
        let expires = unix_now() + 120;
        let signature = sign_gateway_request(
            KEY,
            method,
            path,
            expires,
            nonce,
            "lan-owner",
            "admin",
            body,
        );
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_PRINCIPAL, "lan-owner".parse().unwrap());
        headers.insert(HEADER_ROLE, "admin".parse().unwrap());
        headers.insert(HEADER_EXPIRES, expires.to_string().parse().unwrap());
        headers.insert(HEADER_NONCE, nonce.parse().unwrap());
        headers.insert(HEADER_SIGNATURE, signature.parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn middleware_injects_remote_identity_and_rejects_replayed_markers() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/echo", axum::routing::post(echo_remote))
            .layer(axum::middleware::from_fn_with_state(
                Arc::new(GatewayVerifier::from_key(KEY)),
                gateway_auth,
            ));

        let body = br#"{"serviceId":"self-tools"}"#;
        let headers = signed_headers("POST", "/echo", body, "nonce-middleware-0001");
        let request = || {
            let mut builder = Request::builder()
                .method("POST")
                .uri("/echo")
                .header("content-type", "application/json");
            for (name, value) in &headers {
                builder = builder.header(name.as_str(), value);
            }
            builder.body(Body::from(body.to_vec())).unwrap()
        };

        let response = app.clone().oneshot(request()).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("body");
        let seen: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(seen["principal"], "lan-owner");
        assert_eq!(seen["session_id"], "gateway:lan-owner");
        assert_eq!(seen["body"], String::from_utf8_lossy(body).as_ref());

        // 同一 nonce 再放一次 = 重放。
        let replay = app.clone().oneshot(request()).await.expect("response");
        assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);

        // body 与签名不一致 → 401。
        let tampered = Request::builder()
            .method("POST")
            .uri("/echo")
            .header("content-type", "application/json")
            .body(Body::from(br#"{"serviceId":"other"}"#.to_vec()))
            .unwrap();
        let mut tampered = tampered;
        let headers = signed_headers("POST", "/echo", body, "nonce-middleware-0002");
        for (name, value) in &headers {
            tampered.headers_mut().insert(name.clone(), value.clone());
        }
        let response = app.clone().oneshot(tampered).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_passes_unsigned_loopback_requests_through() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::new(GatewayVerifier::disabled()),
                gateway_auth,
            ));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn key_file_is_generated_once_and_reused() {
        let directory = tempfile::tempdir().unwrap();
        let first = load_or_create_key_file(directory.path()).expect("generate");
        let second = load_or_create_key_file(directory.path()).expect("reuse");
        assert_eq!(first, second);
        let path = directory.path().join(KEY_FILE_NAME);
        let text = std::fs::read_to_string(path).expect("key file");
        assert_eq!(text.trim(), first);
        assert!(first.len() >= MIN_KEY_LEN);
    }
}
