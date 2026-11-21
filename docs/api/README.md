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

Regenerate after adding, removing, or renaming a route:

```bash
UPDATE_OPENAPI=1 cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

On PowerShell:

```powershell
$env:UPDATE_OPENAPI=1; cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

Do not edit the snapshot by hand unless you are also changing the router. A failing contract test means the HTTP surface changed and needs an explicit snapshot update, not a silent drift.
