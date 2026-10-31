//! Loopback-only HTTP guard for the unauthenticated local API.
//!
//! This is defense in depth, not authentication: the server refuses non-loopback
//! binds, rejects non-local Host values (DNS-rebinding protection), and requires
//! browser-originated unsafe requests to have a loopback Origin. CORS is not an
//! authorization mechanism and is intentionally not enabled here.

use std::net::IpAddr;

use axum::Json;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(crate) fn is_loopback_bind(address: std::net::SocketAddr) -> bool {
    address.ip().is_loopback()
}

/// Reject unexpected Host and cross-site browser writes before any handler runs.
pub(crate) async fn local_http_guard(request: Request<Body>, next: Next) -> Response {
    let Some(host) = request.headers().get(axum::http::header::HOST) else {
        return forbidden("invalid_host", "Host header is required");
    };
    if !host.to_str().ok().is_some_and(is_loopback_authority) {
        return forbidden("invalid_host", "Only loopback Host values are accepted");
    }

    let method = request.method();
    let is_safe = method == Method::GET || method == Method::HEAD || method == Method::OPTIONS;
    if !is_safe {
        let Some(origin) = request.headers().get(axum::http::header::ORIGIN) else {
            return forbidden("origin_required", "Unsafe requests require a local Origin");
        };
        if !origin.to_str().ok().is_some_and(is_loopback_origin) {
            return forbidden(
                "invalid_origin",
                "Only loopback browser origins are accepted",
            );
        }
    }

    next.run(request).await
}

fn is_loopback_origin(value: &str) -> bool {
    let Some((scheme, authority)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        || authority.is_empty()
        || authority.contains(['/', '?', '#', '@'])
    {
        return false;
    }
    is_loopback_authority(authority)
}

fn is_loopback_authority(value: &str) -> bool {
    let Ok(authority) = value.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    if authority.as_str().contains('@') {
        return false;
    }
    let host = authority
        .host()
        .trim_start_matches('[')
        .trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn forbidden(code: &'static str, message: &'static str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "code": code, "message": message })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::{is_loopback_authority, is_loopback_bind, is_loopback_origin, local_http_guard};

    #[test]
    fn bind_must_be_loopback() {
        assert!(is_loopback_bind(
            "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
        ));
        assert!(is_loopback_bind(
            "[::1]:8080".parse::<SocketAddr>().unwrap()
        ));
        assert!(!is_loopback_bind(
            "0.0.0.0:8080".parse::<SocketAddr>().unwrap()
        ));
        assert!(!is_loopback_bind(
            "192.168.1.2:8080".parse::<SocketAddr>().unwrap()
        ));
    }

    #[test]
    fn host_and_origin_allow_only_loopback_authorities() {
        assert!(is_loopback_authority("localhost:8080"));
        assert!(is_loopback_authority("127.0.0.1:8080"));
        assert!(is_loopback_authority("[::1]:8080"));
        assert!(!is_loopback_authority("attacker.example"));
        assert!(!is_loopback_origin("https://attacker.example"));
        assert!(!is_loopback_origin("null"));
        assert!(is_loopback_origin("http://127.0.0.1:1420"));
    }

    #[tokio::test]
    async fn rejects_missing_or_external_origin_on_writes() {
        let app = axum::Router::new()
            .route(
                "/write",
                axum::routing::post(|| async { StatusCode::NO_CONTENT }),
            )
            .layer(axum::middleware::from_fn(local_http_guard));

        let missing_origin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/write")
                    .header("host", "127.0.0.1:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_origin.status(), StatusCode::FORBIDDEN);

        let external_origin = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/write")
                    .header("host", "127.0.0.1:8080")
                    .header("origin", "https://attacker.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(external_origin.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rejects_rebinding_host_and_accepts_local_browser_write() {
        let app = axum::Router::new()
            .route(
                "/write",
                axum::routing::post(|| async { StatusCode::NO_CONTENT }),
            )
            .layer(axum::middleware::from_fn(local_http_guard));

        let rebinding = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/write")
                    .header("host", "attacker.example")
                    .header("origin", "https://attacker.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rebinding.status(), StatusCode::FORBIDDEN);

        let local = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/write")
                    .header("host", "127.0.0.1:8080")
                    .header("origin", "http://localhost:1420")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(local.status(), StatusCode::NO_CONTENT);
    }
}
