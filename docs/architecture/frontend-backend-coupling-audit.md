# FRONTEND / BACKEND COUPLING AUDIT

**Scope:** repository-wide static audit of `apps/desktop/ui`, `apps/desktop`, `apps/server`, `crates/{core,application,infrastructure}`, API routing, frontend persistence/provider references, and existing API tests. This is a baseline audit, not a claim that decoupling is complete. Existing unrelated worktree edits were preserved.

## Executive Summary

The Rust domain/application/infrastructure split is substantially real, and the server now exposes a broad `/api/v1` surface. However, the product is **not yet a backend-first, replaceable-client architecture**. The main blockers are Travel business state/algorithms in React, UI-shaped AI contracts in core, settings responses that expose provider secrets, browser-side provider access, and incomplete/informal HTTP coverage. A second UI cannot currently reproduce all important operations through stable APIs.

## CRITICAL COUPLING

1. **Travel business state, algorithms, and persistence live in the frontend.**
   - `apps/desktop/ui/src/features/travel/TravelGuide.tsx`: computes distance (`haversineDistance`), transit estimates, stop times, per-day route summaries, weather advice, attraction matching/deduplication/filter decisions; stores selected attractions, assigned days, and must-visit state in `localStorage` (`travel-plan:*`). These are not merely rendering decisions. A different client cannot restore or share this state.
   - `TravelPage.tsx`: validates date ranges/day limits and assembles a natural-language research request from city, preferences, and must-visit places. `research state`, event polling and result transitions are coordinated in the React page.
   - Current Travel HTTP API has research start/progress/recent-guide/load-guide and provider-test endpoints, but **no** trip CRUD, persisted itinerary edits, optimize/validate, ideas, budget, notebook, undo/version API, or AI propose/apply resources. No trip create/get/update/optimize replacement test is possible against the present API.

2. **Backend-owned provider secrets cross into renderer/UI state and a provider is called directly by the browser.**
   - `apps/server/src/ai_api.rs` GET/PUT serializes the full `AppSettings`; the settings contract includes AI, Travel, Geography and other provider-key fields (`crates/core/src/settings.rs`). `SettingsDialog.tsx`, `App.tsx`, and feature props receive those values.
   - `features/geography/AmapMap.tsx` places the AMap JS key and security code in browser globals/script URL and loads `webapi.amap.com` directly. Travel also passes Geography credentials to a frontend map component.
   - This violates the provider boundary and means renderer-visible settings are not a backend-only secret store. Secret values should be write-only at the client boundary; responses should expose configured/status metadata only.

3. **AI response contracts are explicitly UI-shaped in the backend.**
   - `crates/core/src/personal_ai/types.rs`: `UiBlock` / `UiBlockKind` is part of `AgentResponse`; `ActionKind` includes `navigate`, `show_panel`, `refresh_view`, etc.
   - `crates/application/src/personal_ai/prompt.rs` instructs the model to emit `ui_blocks` with frontend renderer kinds. Tools also return `metadata.ui_hint.ui_blocks`.
   - `AppContext.page` and tests pass page identifiers such as `home`. This couples backend protocol and prompt behavior to particular presentation concepts. Backend responses should express domain capabilities, outcomes, confirmation requirements and semantic data, not view/widget instructions.

4. **The HTTP API is not yet a complete interchangeable client boundary.**
   - Frontend feature clients call command names (`transport.invoke("travel_research_start", ...)`); `transport.ts` maps those names to HTTP paths in one table or to Tauri IPC. This is a useful adapter boundary but the consumer-facing abstraction is still a Tauri-command vocabulary, not a documented stable API contract.
   - `httpTransport` deliberately rejects unimplemented commands. Travel history has an `isTauriRuntime()` gate, and Tauri-only filesystem/dialog operations remain in Markdown and import flows. Therefore many routes/features do not behave equivalently in a plain HTTP client.

## HIGH COUPLING

1. **No formal/generated OpenAPI contract or contract snapshot was found.** No `utoipa`, `specta`, `ts-rs`, OpenAPI file, or contract-generation command exists. TypeScript API shapes (`src/types.ts`, `features/*/*Types.ts`) are hand-maintained. Server handlers serialize application/core types directly in multiple places. The AI request test documents one payload shape, but does not establish a complete generated contract or breaking-change gate.
2. **Errors are not a shared semantic contract.** Server `ErrorBody` contains only `code` and `message`; the frontend parses `message | error | detail` heuristically and treats every HTTP 404 as `null`, including errors that may not mean an optional resource is absent. There are no common retryable/details fields or typed client error mapping.
3. **Async operation protocol is feature-specific and transport-dependent.** Travel uses a session ID plus React `setInterval` polling; the generic frontend `subscribe` implementation is a no-op for HTTP, while AI progress is delivered through Tauri events. There is no common `QUEUED/RUNNING/SUCCEEDED/FAILED` job DTO/API usable by any client.
4. **Backend and UI settings are mixed.** `AppSettings` contains provider/server configuration and `theme_mode`; the same broad settings object is loaded/saved through the UI. Theme preferences should be frontend-owned; provider and server secrets should be backend-owned and redacted on reads.
5. **A small amount of assessment/business policy is frontend-owned outside Travel.** `features/language/reviewMatch.ts` decides answer correctness/“close” thresholds and feedback in `ReviewSession.tsx`. This affects learning outcomes and should become an application use case/response. Some geography seed facts are embedded in `GeographyPage.tsx` as UI fallback data.

## MEDIUM COUPLING

1. **Feature clients are thin RPC wrappers, not yet domain API clients.** The components mostly call feature clients rather than `fetch`, which is good, but clients expose command names and duplicate request/response types. URL construction, error behavior and runtime split remain centralized in `transport.ts`.
2. **Frontend still owns some local durable state.** `StudyBoardPage.tsx` contains a legacy localStorage import/migration path; `TravelGuide.tsx` actively persists itinerary edits. `ThemeManager.ts` and English immersive reader preferences use localStorage for presentation preferences, which is acceptable. These categories need explicit separation.
3. **Platform-specific UI capabilities need adapters for another client.** Tauri file dialogs, file opening, filesystem access and OS integration are appropriate presentation/platform responsibilities, but must be hidden behind client ports if the UI is replaced or a web/mobile client is added.
4. **Server route composition currently knows concrete infrastructure in places.** `apps/server/src/routes.rs` accepts `KnowledgeRuntime`, `LanguageStore`, `TravelDeps`, settings and import-path implementations alongside application services. This is composition-root wiring rather than UI coupling, but it increases server/platform coupling and should be normalized behind runtime/application ports as modules are migrated.

## ACCEPTABLE / ALREADY HEALTHY

- Rust `core` and `application` have no React/TypeScript/CSS dependencies. `apps/server` is an Axum composition root and calls application services; domain logic is not implemented in React for most modules.
- Personal AI prompt construction and LLM orchestration are in `crates/application/src/personal_ai`; provider implementations are in infrastructure. News/RSS, Language and most learning persistence are backend-backed.
- Direct `fetch` usage in the normal frontend data path is concentrated in `src/transport.ts`; components generally use feature clients. Tauri `invoke` is likewise centralized there.
- Theme, navigation, local interaction state, keyboard behavior and visual mapping are legitimate client responsibilities. Platform-specific dialog/open-file behavior is also client-side, provided it is adapted rather than treated as a business API.
- The repository already has versioned `/api/v1` routes and backend router/service tests; those are a foundation, not evidence of a complete stable contract.

## Backend UI-dependency check

**Result: not zero.** No Rust dependency imports React, CSS, or component code, and no backend returns colors/layout/modal booleans. But the AI application protocol contains `UiBlock`, UI block renderer kinds, navigation/panel actions, and page-oriented context. Those are semantic UI dependencies and must be replaced with client-neutral capability/result contracts before claiming `Backend UI Dependencies: 0`.

## Replaceability / test readiness

| Gate | Current audit result | Evidence / blocker |
|---|---|---|
| Backend-only tests | PARTIAL | Rust workspace/domain/application tests exist; several infrastructure and server composition paths remain concrete. |
| Frontend-only tests | PARTIAL | UI tests exist, but no full API-contract mock boundary or generated contract test is enforced. |
| Contract tests | FAIL | No OpenAPI generation/snapshot/compatibility check; only endpoint-specific Rust tests. |
| Thin-client replacement | FAIL | Travel has no persisted trip-edit/optimize/ideas APIs, and the current API cannot reproduce frontend localStorage route state. |
| Backend change with unchanged UI | UNPROVEN | No explicit contract-invariant test proving internal business changes leave generated client types/UI untouched. |
| Full E2E | PARTIAL | Existing visual/navigation coverage is not a live Travel CRUD/AI/optimize workflow. |

## Recommended implementation order

1. Freeze this audit as the baseline; preserve behavior and make migrations one bounded context at a time.
2. Establish API DTO/OpenAPI generation and contract compatibility tests for existing `/api/v1` routes before changing payloads.
3. Add write-only backend-secret DTOs and separate UI preferences from backend configuration; stop returning API keys and remove direct AMap/provider credentials from renderer calls.
4. Complete one vertical slice (Travel) in order: domain/application model and persistence → stable CRUD/plan/job/AI-operation API → API client + generated types → remove React business computation and localStorage source of truth → thin non-React client test.
5. Migrate other business rules (for example language review matching), then replace AI `UiBlock`/view actions with semantic response/action contracts.
6. Expand API clients and replacement tests by bounded context; keep React tests mocked at the API contract boundary. Run backend-only, frontend-only, contract, and live E2E gates independently.

**Verdict: FAIL for the requested end-state today; the existing layered Rust architecture and `/api/v1` server provide a useful foundation.** No changes were made during the audit itself.
