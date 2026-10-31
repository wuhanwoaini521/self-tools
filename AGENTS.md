# Repository Agent Guidance

These rules apply to all work in this repository, including `apps/`, `crates/`, scripts, tests, and documentation. More-specific nested `AGENTS.md` files may add domain guidance; they do not override the cross-cutting frontend/backend boundary below. `history-data-pipeline/AGENTS.md` remains the specialized guide for that submodule.

Before editing, inspect the current worktree and preserve unrelated user changes. Read the relevant module guidance and tests. Keep changes scoped, run the narrowest relevant tests, and report incomplete work honestly.

# Frontend / Backend Architecture Boundary

This project follows a strict frontend/backend separation.

The backend is the product core.

The frontend is a replaceable client.

The architecture must allow the entire frontend UI to be redesigned or replaced without changing backend business logic.

Likewise, backend business behavior should be changeable without requiring frontend modifications as long as the API contract remains compatible.

Canonical API-boundary audit and current gaps: [`docs/architecture/frontend-backend-coupling-audit.md`](docs/architecture/frontend-backend-coupling-audit.md). Do not treat the audit's existing coupling as permission to add more.

## Core Rules

### 1. Frontend-only tasks

When a task is related to UI, UX, layout, styles, theme, design systems, components, visual redesign, animations, page composition, frontend interaction, or responsive behavior, treat it as:

**SCOPE LOCK: FRONTEND ONLY**

Allowed changes:

- frontend pages and components
- frontend styles, themes and design tokens
- frontend state, view models / adapters, and accessibility
- frontend tests

Do not modify backend domain logic, services, database logic, repositories, providers, LLM provider logic, AI business rules, or API behavior.

If a frontend task appears to require a backend change, do not silently modify the backend. First determine whether the UI can use the existing API contract. Only change backend code when the task explicitly requires backend behavior changes or an API change is explicitly approved; identify the scope change before implementation.

### 2. Backend-only tasks

When a task is related to business rules, domain logic, AI orchestration, provider behavior, persistence, route optimization, validation, knowledge linking, data processing, or backend services, treat it as:

**SCOPE LOCK: BACKEND ONLY**

Allowed changes:

- domain, application services and use cases
- repositories, providers and persistence
- backend configuration and backend tests

Do not modify frontend pages, components, CSS, layout, design systems, or visual behavior unless the API contract itself must change. If it must change, classify the task as **FULL STACK** and preserve compatibility where possible.

### 3. API Contract is the boundary

Frontend/backend communication must use stable, versioned API contracts. The API contract—not a React component, Tauri command, database model, or internal service—is the product boundary.

Frontend must not directly depend on database models, ORM entities, backend internal classes, provider SDKs, or backend implementation details. Define response/request DTOs; prefer generated client types from the backend OpenAPI contract when available. Keep frontend clients and transport/auth/retry/error handling out of page components.

Backend must not return UI-specific concepts such as modal state, drawer state, CSS values, component names, colors, or page layout. Return semantic product state and let each client choose its presentation.

BAD:

```json
{ "showModal": true, "buttonColor": "blue" }
```

GOOD:

```json
{ "requiresConfirmation": true, "risk": "low" }
```

### 4. Frontend must not own business logic

Frontend may own rendering, user interaction, navigation, form input, local UI state, accessibility, visual state, and client-side cache.

Frontend must not own core business rules. Business decisions belong in backend domain/application services and must be exposed through API operations.

BAD: a React component decides how to optimize a travel route.

GOOD: the client calls `POST /api/v1/travel/trips/{id}/optimize` and renders the backend result.

### 5. Backend must not depend on UI

Backend code must not depend on React components, frontend routes, sidebar state, UI themes, visual design, or frontend-specific page concepts. Backend capabilities should remain usable by the current frontend, a redesigned frontend, desktop, mobile, CLI, and independent API clients.

### 6. Frontend must be replaceable

The project must support replacing Pencil UI with Modern, Pixel, Minimal, Terminal, Mobile, or another complete client without modifying backend business logic. For visual-only changes, the expected backend diff is zero. A backend change is justified only when the product capability or API contract is changing.

### 7. Backend changes should preserve frontend compatibility

If backend implementation changes while the API contract remains stable, frontend code should not need changes. Prefer additive fields and backward-compatible behavior, stable DTOs, and explicit API versions. Avoid unnecessary breaking changes; document and test any unavoidable contract migration.

### 8. Provider access belongs to backend

Frontend code must not directly call LLM providers (including OpenAI, OpenCode Go or Jev), or secret-bearing map, weather, search, places, news, or other external APIs. The required flow is:

`Frontend → self-tools backend → Provider`

Provider API keys and secrets remain backend-side. Do not return secret values in read responses, logs, or client configuration. Use redacted status/configured metadata and write-only secret updates.

### 9. Business state source of truth

Persistent business state must live in backend/domain persistence. Frontend state may cache it, but must not be the sole source of truth. A refresh or a different client must be able to recover trips, activities, notebook state, knowledge state, backend settings, and asynchronous business-job state from the backend contract. UI-only preferences may remain client-side.

### 10. Testing boundaries

Relevant features should support backend-only tests, frontend-only tests (mocking the API contract), API contract tests, and full E2E tests. A frontend redesign should not require backend tests to be rewritten unless product behavior or the contract changed. A backend refactor should not require frontend tests to be rewritten unless the contract changed.

## Task Scope Lock

Before changing code, classify the task as one of:

- **FRONTEND ONLY** — UI/UX/design/presentation; default for UI redesign work.
- **BACKEND ONLY** — business logic, persistence, providers or services; default for backend business work.
- **FULL STACK** — an explicitly required API/contract change and its consumers.
- **ARCHITECTURE** — dependency-boundary or cross-cutting migration.

Before implementation, identify the expected touched areas and the API contract impact. If the task clearly belongs to one scope, do not casually modify files outside it. After implementation, review `git diff` and confirm unrelated layers and pre-existing user changes were not modified.

## Architecture Self-Check

Before finishing any task, verify:

- Did I modify backend code during a frontend-only task?
- Did I modify frontend code during a backend-only task?
- Did I introduce business rules into frontend components?
- Did I introduce UI concepts into backend responses?
- Did I bypass the API layer?
- Did I expose provider secrets to frontend code?
- Did I introduce a breaking API change unnecessarily?
- Could another frontend still use this backend?
- Could the backend still work if the current frontend were deleted?

If an answer reveals accidental coupling, fix it before completing the task or explicitly report the blocker and scope change. Do not claim a boundary gate passed when it was not tested.

## Travel Module Boundary

Travel UI may change freely. Travel business capabilities remain backend-owned, including:

- trip creation and persistence
- itinerary generation and place verification
- routing, route optimization and schedule validation
- AI travel operations and proposal/application semantics
- weather-aware suggestions and budget calculations
- notebook persistence, trip versions and business undo/redo

Frontend owns itinerary presentation, map/timeline layout, cards, buttons, drag visuals, AI bubble rendering, animations, theme, and responsive behavior. Do not make React state or browser storage the only source of truth for trips or itinerary edits. A complete Travel UI redesign must reuse the same backend capabilities without rewriting them.

## Design System Boundary

Pencil is one frontend design system only. Pencil, Sketch, Paper, Notebook and Hand-drawn concepts must not enter backend domain models, business services, provider contracts, or persistence. Future Pixel, Modern, Minimal, Terminal, or Mobile design systems must not require backend awareness of the active theme.

## Documentation and migration

Existing ADRs may describe historical decisions and current legacy contracts. Do not erase historical records. When they conflict with this boundary for new work, add a clearly dated superseding note and follow this file. The current audit is a debt inventory, not an exception to these rules.
