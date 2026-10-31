# Frontend / Backend Decoupling Report

**Status:** First architecture pass only. This report does not claim that the requested end-state has been reached.

## Before

The audit is recorded in [`frontend-backend-coupling-audit.md`](./frontend-backend-coupling-audit.md). Key blockers:

- Travel itinerary rules, route/distance estimates, weather recommendations, edits and durable plan state are in React/`localStorage`.
- Browser code receives provider secrets; AMap JS is loaded directly by the Geography UI.
- Backend Personal AI contracts contain `UiBlock`, UI renderer kinds and navigation/panel actions.
- Existing `/api/v1` endpoints are broad but incomplete for a replaceable client; Travel CRUD/optimization/ideas/budget/versioning endpoints are absent.
- No complete OpenAPI document, generated frontend types, compatibility snapshot or contract CI gate was found.

## Architecture After

Production architecture is **unchanged in this pass**. Existing layered Rust core/application/infrastructure and Axum `/api/v1` remain in place. Added:

- `scripts/api-smoke/history.mjs`: a Node built-in-fetch client that exercises History HTTP APIs without React or Tauri.
- `docs/architecture/frontend-backend-coupling-audit.md`: repository audit and ordered migration plan.

This proves a small read-only History API slice can be consumed independently; it does not establish full frontend replaceability.

## Logic Moved

**None.** No business logic or provider configuration was moved in this pass. This avoids claiming behavior-preserving migration without the domain/API and persistence work it requires.

## API Contract

Existing versioned History slice exercised by the thin client:

- `GET /api/v1/history/home`
- `GET /api/v1/history/periods/{id}`

The server also has History search/events/people/works/stories and APIs for Language, Learning, Geography, News, Knowledge, Settings and Travel. The current Travel API is research-session oriented (`research/start`, `research/progress`, recent guides, guide lookup, provider checks), not the requested Trip CRUD/planning/optimization contract.

The checked-in API is **not yet a generated/formal OpenAPI contract**; endpoint assertions in the smoke script are a narrow runtime shape check, not a breaking-change CI gate.

## Frontend Dependencies

**Not yet API-only.** Most UI data access is behind feature clients and `transport.ts`, but clients use command-name vocabulary and the transport selects Tauri IPC versus a partial HTTP mapping. Travel contains business rules and local persistence; Settings/Geography expose provider credentials; Tauri filesystem/dialog and AI progress paths are not equivalent HTTP APIs.

## Backend UI Dependencies

**Not zero.** No backend crate imports React or CSS, and no colors/layout/modal booleans were found. However, core/application AI protocol types and prompts depend on UI blocks and presentation-oriented actions/page context.

## Provider Security

**FAIL.** LLM/search/weather provider implementations are backend-side, but the complete settings object can return configured API keys to the renderer, and the Geography frontend sends its AMap JS key/security code to a third-party browser SDK. A write-only secret API and server-side/provider proxy design are still required.

## Tests

- **Backend:** `cargo test -p devtoolbox-server` PASS — 37 tests. Full workspace tests were not run.
- **Contract:** **PARTIAL** — `node scripts/api-smoke/history.mjs` passed against `127.0.0.1:8080` (31 periods and the first period's detail); no OpenAPI generation/snapshot test exists.
- **Frontend:** No frontend changes were made for this architecture pass; the last UI run was 95 unit tests and build, not an API-mock boundary suite.
- **E2E:** Not run for live Travel CRUD/AI/optimization.

## Replacement Test

- **History read-only thin client:** PASS — Node script called the server directly, without React, Tauri or frontend package imports.
- **Requested Travel replacement test:** FAIL / unavailable — backend currently lacks trip create/get/update/optimize/save APIs and frontend itinerary state remains localStorage-backed.

## Backend Change Test

**UNPROVEN.** No internal business implementation was changed while holding a generated/stable API contract fixed.

## Verdict

**PARTIAL** for the audit plus one independently consumed API slice; **FAIL** for the requested full decoupling acceptance criteria. The remaining blockers are explicitly listed in the audit and should be addressed in small, tested bounded-context migrations before further UI work.
