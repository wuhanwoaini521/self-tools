//! P0 HTTP contract: versioned `/api/v1` OpenAPI snapshot and compatibility gate.
//!
//! This module does **not** invent DTOs. It freezes the live router surface
//! (method + path template + error envelope) so adding, removing, or renaming
//! an operation fails `cargo test` until `docs/api/openapi.v1.json` is updated.

use axum::http::header;
use axum::response::IntoResponse;

/// Checked-in OpenAPI 3.1 snapshot. Served as `GET /api/v1/openapi.json`.
pub const OPENAPI_JSON: &str = include_str!("../../../docs/api/openapi.v1.json");

pub async fn spec() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        OPENAPI_JSON,
    )
}

#[cfg(test)]
mod tests {
    use super::OPENAPI_JSON;
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn production_router_sources() -> [&'static str; 2] {
        ["src/routes.rs", "src/backup_api.rs"]
    }

    fn strip_tests_module(source: &str) -> &str {
        source
            .find("\nmod tests {")
            .map(|index| &source[..index])
            .unwrap_or(source)
    }

    fn matching_paren(source: &str, open_at: usize) -> usize {
        let bytes = source.as_bytes();
        assert_eq!(bytes[open_at], b'(');
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;
        for (offset, byte) in bytes[open_at..].iter().enumerate() {
            let index = open_at + offset;
            if in_string {
                if escape {
                    escape = false;
                    continue;
                }
                match *byte {
                    b'\\' => escape = true,
                    b'"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match *byte {
                b'"' => in_string = true,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return index;
                    }
                }
                _ => {}
            }
        }
        panic!("unbalanced .route( starting at {open_at}");
    }

    fn methods_in(expr: &str) -> Vec<&'static str> {
        const NAMES: [(&str, &str); 5] = [
            ("get", "get"),
            ("post", "post"),
            ("put", "put"),
            ("patch", "patch"),
            ("delete", "delete"),
        ];
        let mut found = Vec::new();
        for (needle, method) in NAMES {
            let mut search = expr;
            while let Some(at) = search.find(needle) {
                let before = &search[..at];
                let after = &search[at + needle.len()..];
                let token_start = before
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
                if token_start && after.trim_start().starts_with('(') {
                    found.push(method);
                    break;
                }
                search = &search[at + needle.len()..];
            }
        }
        found
    }

    fn extract_router_ops(source: &str) -> Vec<(String, String)> {
        let source = strip_tests_module(source);
        let mut ops = Vec::new();
        let mut cursor = 0;
        while let Some(rel) = source[cursor..].find(".route(") {
            let start = cursor + rel + ".route".len();
            let close = matching_paren(source, start);
            let inner = &source[start + 1..close];
            let path = inner
                .split('"')
                .nth(1)
                .expect("route path string")
                .to_string();
            let methods = methods_in(inner);
            assert!(
                !methods.is_empty(),
                "no HTTP method parsed for route {path}: {inner}"
            );
            for method in methods {
                ops.push((method.to_string(), path.clone()));
            }
            cursor = close + 1;
        }
        ops
    }

    fn tag_for(path: &str) -> &'static str {
        if path == "/health" {
            return "meta";
        }
        let rest = path.strip_prefix("/api/v1/").unwrap_or(path);
        match rest.split('/').next().unwrap_or("misc") {
            "readiness" => "readiness",
            "memory" => "knowledge",
            "documents" => "knowledge",
            "files" => "knowledge",
            "search" => "knowledge",
            "knowledge" => "knowledge",
            "history" => "history",
            "language" => "language",
            "english" => "language",
            "settings" => "settings",
            "ai" => "ai",
            "conversations" => "ai",
            "travel" => "travel",
            "learning" => "learning",
            "geography" => "geography",
            "exploration" => "exploration",
            "news" => "news",
            "rss" | "articles" => "rss",
            "study-boards" => "study-boards",
            "backups" => "backups",
            "openapi.json" => "meta",
            _ => "misc",
        }
    }

    fn path_parameters(path: &str) -> Vec<Value> {
        path.split('/')
            .filter_map(|segment| {
                let name = segment.strip_prefix('{')?.strip_suffix('}')?;
                Some(json!({
                    "name": name,
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string" }
                }))
            })
            .collect()
    }

    fn success_content(path: &str) -> Value {
        if path.ends_with("/events") {
            json!({
                "text/event-stream": { "schema": { "type": "string" } }
            })
        } else if path.ends_with("/audio") {
            json!({
                "audio/mpeg": { "schema": { "type": "string", "format": "binary" } }
            })
        } else {
            json!({
                "application/json": { "schema": { "type": "object", "additionalProperties": true } }
            })
        }
    }

    fn operation_id(method: &str, path: &str) -> String {
        let mut id = format!("{method}_");
        for ch in path.chars() {
            match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' => id.push(ch),
                _ => {
                    if !id.ends_with('_') {
                        id.push('_');
                    }
                }
            }
        }
        id.trim_matches('_').to_string()
    }

    fn build_openapi(ops: &[(String, String)]) -> Value {
        let mut paths: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        for (method, path) in ops {
            let mut op = json!({
                "operationId": operation_id(method, path),
                "tags": [tag_for(path)],
                "responses": {
                    "200": {
                        "description": "Success",
                        "content": success_content(path)
                    },
                    "default": {
                        "description": "Error",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ErrorBody" }
                            }
                        }
                    }
                }
            });
            let params = path_parameters(path);
            if !params.is_empty() {
                op["parameters"] = Value::Array(params);
            }
            if method == "post" || method == "patch" || method == "put" {
                op["requestBody"] = json!({
                    "required": false,
                    "content": {
                        "application/json": {
                            "schema": { "type": "object", "additionalProperties": true }
                        }
                    }
                });
            }
            paths
                .entry(path.clone())
                .or_default()
                .insert(method.clone(), op);
        }

        let mut path_objects = serde_json::Map::new();
        for (path, methods) in paths {
            let mut method_objects = serde_json::Map::new();
            for (method, op) in methods {
                method_objects.insert(method, op);
            }
            path_objects.insert(path, Value::Object(method_objects));
        }

        json!({
            "openapi": "3.1.0",
            "info": {
                "title": "DevToolbox HTTP API",
                "version": "v1",
                "description": "Loopback product API. P0 freezes method + path templates and the ErrorBody envelope; response payloads remain opaque JSON objects until DTO generation."
            },
            "servers": [{ "url": "http://127.0.0.1:8080" }],
            "paths": path_objects,
            "components": {
                "schemas": {
                    "ErrorBody": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["code", "message"],
                        "properties": {
                            "code": { "type": "string" },
                            "message": { "type": "string" }
                        }
                    }
                }
            }
        })
    }

    fn snapshot_text(document: &Value) -> String {
        let mut text = serde_json::to_string_pretty(document).expect("serialize openapi");
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }

    fn collect_router_ops(manifest: &Path) -> Vec<(String, String)> {
        let mut ops = Vec::new();
        for rel in production_router_sources() {
            let source = std::fs::read_to_string(manifest.join(rel))
                .unwrap_or_else(|error| panic!("read {}: {error}", rel));
            ops.extend(extract_router_ops(&source));
        }
        ops.sort();
        ops.dedup();
        ops
    }

    fn normalize_path_shape(path: &str) -> String {
        let without_query = path.split('?').next().unwrap_or(path);
        let mut out = String::new();
        let mut in_param = false;
        for ch in without_query.chars() {
            match ch {
                '{' => {
                    in_param = true;
                    out.push('{');
                    out.push('}');
                }
                '}' => in_param = false,
                _ if in_param => {}
                _ => out.push(ch),
            }
        }
        out
    }

    fn extract_transport_ops(source: &str) -> Vec<(String, String)> {
        let mut method = "get";
        let mut ops = Vec::new();
        for raw_line in source.lines() {
            let line = raw_line.trim();
            if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") {
                continue;
            }
            if line.contains("HTTP_POST_ENDPOINTS") {
                method = "post";
            } else if line.contains("HTTP_DELETE_ENDPOINTS") {
                method = "delete";
            } else if line.contains("HTTP_PATCH_ENDPOINTS") {
                method = "patch";
            } else if line.contains("HTTP_ENDPOINTS") {
                method = "get";
            }
            let Some(start) = line.find("/api/v1/") else {
                continue;
            };
            let rest = &line[start..];
            let mut path = String::new();
            let chars: Vec<char> = rest.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                    let inner_start = i + 2;
                    let mut depth = 1;
                    let mut j = inner_start;
                    while j < chars.len() && depth > 0 {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    let inner: String = chars[inner_start..j.saturating_sub(1)].iter().collect();
                    if inner.contains("encodeURIComponent") {
                        path.push('{');
                        path.push('}');
                        i = j;
                        continue;
                    }
                    break;
                }
                if chars[i] == '`' || chars[i] == '"' || chars[i] == '?' {
                    break;
                }
                path.push(chars[i]);
                i += 1;
            }
            if path.starts_with("/api/v1/") {
                ops.push((method.to_string(), path));
            }
        }
        ops.sort();
        ops.dedup();
        ops
    }

    #[test]
    fn openapi_snapshot_matches_router_surface() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut ops = collect_router_ops(&manifest);
        let has_spec_route = ops
            .iter()
            .any(|(method, path)| method == "get" && path == "/api/v1/openapi.json");
        assert!(
            has_spec_route,
            "router must expose GET /api/v1/openapi.json"
        );

        let document = build_openapi(&ops);
        let expected = snapshot_text(&document);
        let snapshot_path = workspace_root().join("docs/api/openapi.v1.json");

        if std::env::var_os("UPDATE_OPENAPI").is_some() {
            std::fs::write(&snapshot_path, &expected).expect("write openapi snapshot");
            return;
        }

        let actual = std::fs::read_to_string(&snapshot_path).expect("read openapi snapshot");
        assert_eq!(
            actual.replace("\r\n", "\n"),
            expected.replace("\r\n", "\n"),
            "OpenAPI snapshot drifted from the Axum router. Re-run with UPDATE_OPENAPI=1 (see docs/api/README.md)."
        );
        assert_eq!(
            OPENAPI_JSON.replace("\r\n", "\n"),
            expected.replace("\r\n", "\n"),
            "compiled-in OPENAPI_JSON must match docs/api/openapi.v1.json"
        );

        ops.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
        let snapshot: Value = serde_json::from_str(&actual).expect("snapshot json");
        assert_eq!(snapshot["openapi"], "3.1.0");
        assert_eq!(
            snapshot["components"]["schemas"]["ErrorBody"]["required"],
            json!(["code", "message"])
        );
        let mut snapshot_ops = BTreeSet::new();
        for (path, methods) in snapshot["paths"].as_object().expect("paths") {
            for method in methods.as_object().expect("methods").keys() {
                snapshot_ops.insert((method.clone(), path.clone()));
            }
        }
        let router_ops: BTreeSet<_> = ops.into_iter().collect();
        assert_eq!(router_ops, snapshot_ops);
    }

    #[test]
    fn web_transport_paths_are_in_openapi_snapshot() {
        let transport =
            std::fs::read_to_string(workspace_root().join("apps/desktop/ui/src/transport.ts"))
                .expect("read transport.ts");
        let snapshot: Value = serde_json::from_str(OPENAPI_JSON).expect("snapshot json");
        let mut allowed = BTreeSet::new();
        for (path, methods) in snapshot["paths"].as_object().expect("paths") {
            for method in methods.as_object().expect("methods").keys() {
                allowed.insert((method.clone(), normalize_path_shape(path)));
            }
        }
        let client_ops = extract_transport_ops(&transport);
        assert!(
            !client_ops.is_empty(),
            "expected HTTP_ENDPOINTS paths in transport.ts"
        );
        let mut missing = Vec::new();
        for (method, path) in client_ops {
            let shape = normalize_path_shape(&path);
            if !allowed.contains(&(method.clone(), shape.clone())) {
                missing.push(format!("{method} {path} (shape {shape})"));
            }
        }
        assert!(
            missing.is_empty(),
            "web transport maps commands to HTTP paths missing from OpenAPI:\n{}",
            missing.join("\n")
        );
    }
}
