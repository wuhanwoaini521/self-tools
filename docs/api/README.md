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

Regenerate after adding, removing, or renaming a route:

```bash
UPDATE_OPENAPI=1 cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

On PowerShell:

```powershell
$env:UPDATE_OPENAPI=1; cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

Do not edit the snapshot by hand unless you are also changing the router. A failing contract test means the HTTP surface changed and needs an explicit snapshot update, not a silent drift.
