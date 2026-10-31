# Sprint 4 verification

**Run date:** 2026-10-26  
**Scope:** FULL STACK — additive Exploration API metadata and reviewed seed data, Geography/History/Travel client journey, plus TravelPlan consistency QA. Travel CAS implementation and the broad dirty worktree predate this run; the deterministic two-session HTTP regression test was added during this run. No reset, stash, commit, or push was performed.

## Gates

| Gate | Result | Evidence |
|---|---|---|
| TravelPlan multi-session consistency | **PASS** | `independent_sessions_conflict_when_a_stale_edit_changes_trip_dates`: two HTTP clients read revision 1; A saves stop changes at revision 2; B's stale date edit receives `409 travel_plan_conflict`; a reload retains A's stop and has no saved B date context. The test uses an explicit oneshot barrier, not a timing sleep. Existing stale-revision, SQLite-writer, migration, and client persistence tests also passed earlier in this run. |
| Geography–History stable IDs and provenance | **PASS** | Reviewed Chengdu and Luoyang links; source IDs, stable event-place IDs, relation kind, event evidence time scope, and provenance are retained. Luoyang's Geography record is modern Wikidata Q187136, under Henan Q43684; its reviewed History anchor is CBDB `cbdb-place-14693`, event-place record `event-place-ef7ba1621307cfb31edd377c`, for `event-anlu-luoyang` (756). The 756 scope is event evidence, not the full validity period of the place. |
| Negative / ambiguity / outage cases | **PASS (API/backend)** | Tests cover absent Geography and History IDs, no name-based match for Shanghai/Yangtze, the rejected `cbdb-place-100409` (董卓进京, 189; date conflict), ambiguous reverse links, and an unavailable History port returning a server error instead of fabricated empty data. Reverse lookup's no-match contract is JSON `null` with HTTP 200. Browser-level forced network failure was not verified: CDP request routing did not intercept the request (the observed request remained HTTP 200). |
| Geography → History → Travel journey | **PASS (browser)** | Chrome 150: searched Luoyang; opened its reviewed History event; navigated to the History event detail and back to the retained History view; added Luoyang to a TravelPlan; reloaded it from Recent Trips; navigated Travel → Geography → back to the same trip. The saved itinerary retained the event ID and source IDs in its note. |
| Offline Geography fallback | **PASS (browser)** | With remote MapLibre tiles unavailable, both the main canvas and Luoyang detail render local coordinate plots and entity lists. Switching to Administrative displayed 10 local relations; detail showed 2 entities and its 1 located-in relation. Plot explicitly disclaims navigation/boundary accuracy. |
| Responsive sanity | **PASS (browser)** | At 390×844, Geography and Travel had `documentElement.scrollWidth === clientWidth === 390`; the detail offline map also fit its container. |
| AI context | **PASS** | AI panel displayed `Travel · 洛阳`. An `AgentRequest` with Travel/Luoyang `app_context` was accepted by the HTTP contract and returned the expected configuration gate, not a 422/schema error. |
| AI provider call | **BLOCKED_EXTERNAL** | `/api/v1/ai/status` on both the existing local HTTP service and the isolated Sprint 4 service reported `configured:false`, `provider:null`, `model:null`; redacted secret flags were false. The controlled chat request returned `400 ai_not_configured`. No external provider call was made. |
| Native Tauri acceptance | **BLOCKED_ENV** | `cargo test -p devtoolbox-desktop` passed (9 tests), but `cargo tauri --version` reports no Tauri CLI and the npm Tauri CLI package is unavailable. Native app launch/package/E2E therefore remains unverified. |
| Formatting | **WARN** | New Rust modules pass targeted `rustfmt --check`; scoped `git diff --check` passed. Workspace `cargo fmt --all -- --check` reports formatting differences across already-dirty unrelated files. No workspace-wide formatting was applied to avoid rewriting user changes. |

## Commands and results

- `cd apps/desktop/ui && npm test` — **16 files, 104 tests passed**.
- `cd apps/desktop/ui && npm run build` — **passed**. Existing Vite warnings remain for the plugin-dialog mixed static/dynamic import and the >500 kB application chunk.
- `cargo test -p devtoolbox-core exploration::tests --lib` — **3 passed**.
- `cargo test -p devtoolbox-application exploration --lib` — **1 passed**.
- `cargo test -p devtoolbox-infrastructure geography::store::tests --lib` — **3 passed**, including additive seeding into a legacy DB while preserving a user row.
- `cargo test -p devtoolbox-server --bin devtoolbox-server independent_sessions_conflict_when_a_stale_edit_changes_trip_dates` — **1 passed**.
- `cargo test -p devtoolbox-server --bin devtoolbox-server exploration_location_uses_reviewed_crosswalk_and_real_history_data` — **1 passed**.
- `cargo test -p devtoolbox-server --bin devtoolbox-server exploration_unmapped_geography_has_a_real_empty_state` — **1 passed**.
- `cargo test -p devtoolbox-server --bin devtoolbox-server geography_detail_unknown_id_is_404` — **1 passed**.
- `cargo test -p devtoolbox-desktop` — **9 passed**.

## Screenshot baseline

The before set was captured **2026-10-26 12:46 UTC**, before Sprint 4 edits, at the commit in `before/commit.txt`. Travel before/after images use the same existing 8080 API/data and matching viewport sizes: **1280×800, 1440×900, 1920×1080**. Geography/History after captures use an isolated current-code server (`127.0.0.1:8081`, temporary data directory) against the repository History DuckDB; the before Geography seed had 14 entities, while the after seed has 16 (Henan and Luoyang added). Browser was system Chrome 150 over CDP because bundled Chrome 155 cannot launch on macOS 12.7.6.

Key captures:

- `before/geography-1440x900.png` → `after/geography-landing-1440x900.png` (blank remote map → labeled offline coordinates and local points)
- `after/geography-offline-administrative-1440x900.png`, `after/geography-luoyang-detail-1440x900.png`, `after/geography-luoyang-history-1440x900.png`, `after/geography-luoyang-evidence-1440x900.png`
- `before/history-1440x900.png` → `after/history-overview-1440x900.png`; event detail: `after/history-luoyang-event-1440x900.png`
- `before/travel-{1280x800,1440x900,1920x1080}.png` → matching `after/travel-*` captures
- Journey/provider context: `after/travel-luoyang-exploration-1440x900.png`, `after/travel-return-to-trip-1440x900.png`, `after/travel-ai-context-1440x900.png`
- Mobile checks: `after/geography-luoyang-mobile-390x844.png`, `after/travel-mobile-390x844.png`
- `after/geography-query-unsubmitted-1440x900.png` is retained only as troubleshooting output from the unsuccessful CDP network-abort attempt; it is not acceptance evidence.

## Architecture and worktree notes

The API change is additive under `/api/v1`: `crosswalks` supports multiple reviewed links; the old singular `crosswalk` is populated only for exactly one link. Relation type, confidence (kept unknown where unsupported), evidence time scope, and source provenance cross the API boundary. Unknown IDs and ambiguous reverse links do not fall back to names. `docs/exploration/EXPLORATION_API_V1.md` records the compatibility behavior and rejected candidate.

Frontend-only presentation work added offline fallbacks, explicit no-result/error states, visible provenance, and return navigation. The TravelPlan business implementation was not changed for this UI work. The initial worktree already contained many unrelated modifications; review and implementation were kept scoped, and no unrelated files were reset or cleaned. The temporary Sprint 4 Vite config and isolated services on ports 1421/8081 were removed/stopped; existing services on 1420/8080 were left untouched.
