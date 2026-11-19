# HTTP API contract (v1)

Canonical snapshot: [`openapi.v1.json`](openapi.v1.json).

This is the P0 compatibility freeze for `apps/server`:

- Every Axum route in `routes.rs` and `backup_api.rs` (production `router` only) must appear in the snapshot.
- `GET /api/v1/openapi.json` serves the same file the tests compile in.
- Web `transport.ts` HTTP mappings must resolve to a snapshot path + method (path parameters compared by shape, not name).
- Response bodies are intentionally `object` until a later DTO/OpenAPI schema pass. The frozen error envelope is `ErrorBody`: `{ "code": string, "message": string }`.

Regenerate after adding, removing, or renaming a route:

```bash
UPDATE_OPENAPI=1 cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

On PowerShell:

```powershell
$env:UPDATE_OPENAPI=1; cargo test -p devtoolbox-server openapi_snapshot_matches_router_surface
```

Do not edit the snapshot by hand unless you are also changing the router. A failing contract test means the HTTP surface changed and needs an explicit snapshot update, not a silent drift.
