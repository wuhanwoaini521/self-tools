# HTTP API contract (v1)

Canonical snapshot: [`openapi.v1.json`](openapi.v1.json).

This is the P0 compatibility freeze for `apps/server`:

- Every Axum route in `routes.rs` and `backup_api.rs` (production `router` only) must appear in the snapshot.
- `GET /api/v1/openapi.json` serves the same file the tests compile in.
- Web `transport.ts` HTTP mappings must resolve to a snapshot path + method (path parameters compared by shape, not name).
- Response bodies are intentionally `object` until a later DTO/OpenAPI schema pass. The frozen error envelope is `ErrorBody`: `{ "code": string, "message": string }`.

## AI progress stream (SSE)

`GET /api/v1/ai/runs/{trace_id}/events` is the only `text/event-stream` endpoint:

- each frame is `id: {seq}` + `event: progress` + `data: {trace_id, seq, stage, detail, error_code, state, timestamp}`;
- `seq` is monotonic per run and `state` is one of `running` / `done` / `failed`; the stream ends after a terminal frame;
- reconnect with `Last-Event-ID` (or `?after=seq`) to replay from the per-run ring buffer;
- a cursor older than the buffer returns `409 {"code":"job_cursor_expired"}`; call `GET /api/v1/ai/runs/{trace_id}` for the latest state instead (unknown runs return `404 job_not_found`, e.g. after a service restart);
- keep-alive comments are sent every 15 seconds; `X-Accel-Buffering: no` is set so proxies do not buffer frames.

The desktop app emits the same `JobProgressEvent` shape over Tauri, so both clients see one sequence.

## Readiness (read-only, no auth)

- `GET /api/v1/readiness` → `{overall, checks[13], generated_at}`. `overall` follows the aggregate rule (blocking `failed` > `degraded` > `ready`; `not_configured` never affects it). Check ids are the stable `ReadinessCheckId` set (`backend`, `database`, `ai_provider`, `vision`, `decision`, `jev`, `search`, `mcp_local`, `mcp_remote`, `home_server`, `backup`, `pwa_secure_context`, `device_session`); labels are Chinese display text.
- `GET /api/v1/readiness/diagnostics` → the same 13 items as a flat array (`id`, `label`, `status`, `detail`, `blocking`), same status and wording as the report.
- `detail` is controlled display text only (`已配置` / `未配置` / capability booleans): no key, base URL, model name or path ever appears, so the response is safe to serve over the LAN gateway.
- `pwa_secure_context` and `device_session` are owned by the frontend: the backend only knows `false` and marks them `由前端判定（安全上下文 / 设备会话）` instead of fabricating a verdict. Clients must override those two with browser facts.
- Contract tests: `cargo test -p devtoolbox-server readiness_`.

## Home Server endpoints and gateway auth

Reads (loopback or authenticated LAN gateway):

- `GET /api/v1/server/status`, `/server/services`, `/server/apps`
- `GET /api/v1/server/actions/recent?limit=20`
- `GET /api/v1/server/logs?serviceId=…&logSourceId=…&maxLines=…&maxBytes=…&maxAgeSecs=…` — limits are clamped, output is redacted, and `untrusted: true` is always set.
- `GET /api/v1/server/mcp`

Writes require a gateway-authenticated **admin** marker:

- `POST /api/v1/server/actions/restart` `{"serviceId": "…"}` → returns a confirmation ticket only.
- `POST /api/v1/server/actions/{id}/confirm` `{"serviceId": "…"}` → executes (one-time, same principal, before expiry).
- `POST /api/v1/server/actions/{id}/cancel` → cancels a pending ticket (same principal).

Without the marker a write returns `401 {"code":"gateway_auth_required"}`; a non-admin marker returns `403 gateway_admin_required`; cross-principal or replayed tickets surface as `outcome: "denied"` and are audited. The gateway adds `X-SelfTools-Gateway-{Principal,Role,Expires,Nonce,Signature}` after stripping any client-supplied `X-SelfTools-*`/`X-Forwarded-*` headers; the signature is HMAC-SHA256 over `METHOD\nPATH\nEXPIRES\nNONCE\nPRINCIPAL\nROLE\nsha256_hex(BODY)`.

## Settings writes (gateway admin only)

- `PUT /api/v1/settings/public` — partial update of `PublicSettingsUpdate`: `theme_mode`, `ui_theme`, `rss_refresh_minutes`, `editor_font_size`, `auto_save`, `markdown_default_view`, and the whitelisted `ai` / `travel` / `decision` groups. Unknown fields (including `workspace_path`, `knowledge`, `server`, `secrets`) are rejected with 400. Empty strings clear optional values; numbers/bools cannot be unset.
- `POST /api/v1/settings/secrets` — body `{"updates":[{"field":"ai_api_key","value":"…"}]}`; `field` is the `SecretSettingsField` enum, `value: null` clears. The response is `SecretSettingsStatus` (booleans only); the value is never returned, logged, or audited.
- `GET /api/v1/settings` keeps its redacted shape; the legacy `POST /api/v1/settings` stays `403 settings_write_requires_desktop`.

Both write endpoints require the gateway admin marker; a direct loopback write returns `401 gateway_auth_required`. A settings file that fails to parse aborts the write (`settings_write_failed`) instead of being overwritten with defaults. Each successful change is audited as `settings_update` / `settings_secret_update` with the changed field names as `target_id` and no values.

Provider connectivity tests need no desktop-only path: `POST /api/v1/travel/test/llm|amap|qweather` use the same providers and wording as the desktop commands, read credentials from stored settings, and ignore request-body keys.

## Language data and News gaps

- `POST /api/v1/language/nce/import` / `POST /api/v1/language/dict/import` return `{"started":true,"jobId":"…"}`; progress is polled at `GET /api/v1/language/import-status` (`running`, `jobId`, `kind`, counts, `cancelled`, `lastError`).
- `POST /api/v1/language/nce/import/{jobId}/cancel` and `POST /api/v1/language/dict/import/{jobId}/cancel` cancel the active job only; an old or unknown id returns `404 job_not_found`.
- `POST /api/v1/language/starter/install` `{"only": null}` installs the built-in starter datasets (idempotent).
- `POST /api/v1/language/speaking/feedback` takes `{sentenceId, transcript, durationMs, targetMs, longPausesMs}`; the target sentence is looked up server-side, so a client cannot choose its own scoring target.
- `POST /api/v1/news/sources/{id}/category` `{"category":"tech"}` reclassifies a news source (same categories as the desktop command); invalid values return `400 news_invalid_category`.

Regenerate after adding, removing, or renaming a route:

```bash
UPDATE_OPENAPI=1 cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

On PowerShell:

```powershell
$env:UPDATE_OPENAPI=1; cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

Do not edit the snapshot by hand unless you are also changing the router. A failing contract test means the HTTP surface changed and needs an explicit snapshot update, not a silent drift.
