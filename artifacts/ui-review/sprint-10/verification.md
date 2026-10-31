# Sprint 10 — Product Maturity Verification

**Date:** 2026-10-28  
**Status:** Complete for the locally verifiable scope; live-provider acceptance remains blocked by provider configuration.  
**Scope:** Sprint 10 product maturity across frontend reliability/performance, AI tool permissions, isolated recovery, and native Tauri coverage. Sprint 9’s shared PersonalAgent integration was retained and re-verified; this report does not reattribute the pre-existing Sprint 9 work to Sprint 10.  
**Contract impact:** No API shape or version change. Home uses existing API clients. AI risk classifications use the existing `ToolRisk` contract.

## 1. Executive result

The Home → Review → Home journey now refreshes persisted progress without blanking valid prior data. A stopped-local-server test confirmed that Home shows an actionable error while retaining stale content, then updates after retry once the isolated server is restored. The initial production bundle and Home request set are smaller, and Language is loaded on first visit rather than Home startup. The 66-tool registry was audited; four sensitive writes remain discoverable for catalog/audit but are not exposed to the automatic model path and are rejected by direct registry execution. Native macOS Tauri/WebKit E2E passed 17/17 tests.

## 2. Scope and boundaries

Classification: **PRODUCT MATURITY / FULL STACK, with no API contract change.** Frontend reliability/code-splitting changes are in the UI; AI authorization remains backend-owned. No Home business rules moved into React, no provider calls or secrets were added to the frontend, and no database schema/API version changed.

## 3. Worktree and data safety

The repository was already broadly dirty. All work was done in place without reset, clean, stash, checkout, stage, commit, push, merge, or deploy. The pre-existing deletion of `apps/desktop/ui/src/features/geography/AmapMap.tsx` remains untouched. Review the aggregate `git diff` as a mixed-worktree diff, not as a Sprint 10-only patch.

Review fixtures and writable server data were isolated under `/tmp/self-tools-sprint10-server.*`; the existing bundled History DuckDB was supplied as a read source only. No writes were made to production data. Temporary review cards and progress remained confined to the isolated test store.

## 4. Sprint 9 integration baseline retained

The worktree retains the shared PersonalAgent across Geography, History, Learning, and Travel; trusted citations; reviewed stable-ID exploration links; read-only `learning.due_reviews`; confirmation/revision-gated Travel proposals; cancellation/trace IDs; and shared HTTP/Tauri runtime. This Sprint 10 run revalidated native registry assembly/cancellation and the Travel command boundary. Live-provider behavior remains untested here.

## 5. API boundary and client context

Home refreshes through existing learning, collection, travel, and news API clients. The UI does not reach into repositories or provider SDKs. `AppContext` narrows tool exposure only; it is client-provided and is **not authentication or authorization**. Backend risk checks remain authoritative for automatic tool execution.

## 6. Journey A — Home → Review → Home

On the isolated local server, a temporary review card was created, answered, rated, and completed. Returning Home caused a fresh read of persisted Learning progress without replacing the page with a loading blank. A native WebKit test also verified Home progress refresh after returning from Review.

Evidence: `after/home-refresh-review-card.png`, `after/home-refresh-review-revealed.png`, and `after/home-refresh-recovered-all-final.png`.

## 7. Journey B — failure, stale data, retry, recovery

A fourth temporary review was completed, then the isolated HTTP server on loopback port 8080 was stopped before returning Home. Home showed a per-resource retry/error state, retained the last successful three-item progress/list, and displayed the local-service failure. After restarting the same server against the same `/tmp` data directory, the backend still reported four completed reviews; retrying the Learning resource updated Home to four. The remaining resource retries also cleared after service recovery.

Evidence: `after/home-server-error-stale-data.png`, `after/home-server-recovered.png`, and `after/home-refresh-recovered-all-final.png`. The final backend response after restart reported `studied_topics_today=4`, `average_mastery=61.5`, and four continue items.

## 8. Travel and persistent journey coverage

Native E2E round-tripped a backend-owned Travel itinerary through Tauri commands and verified assembled PersonalAgent registry/cancellation. HTTP/Tauri runtime sharing remains intact. No real AI-generated Travel proposal was tested because no provider was configured.

## 9. Home refresh UX and visual evidence

Home refreshes when it becomes active, preserves the previous valid resource value during refresh, and renders an independent retry/error state when a resource refresh fails. This was observed against a real local server outage, not a mocked model response. Individual collection, Travel, and News retries also cleared after service recovery. Before/after Home captures cover 1280×800, 1440×900, and 1920×1080; the lazy Language view is captured at 1440×900.

## 10. Deferred Language loading

Language is lazy-loaded on first navigation. The production build emitted `LanguagePage` at 221,405 raw bytes (about 60.5 KB gzip in the final local compression check). The first Home view no longer requests Language endpoints; opening Language loaded its chunk and then made its feature-specific requests.

## 11. Performance method and results

Compared before/after production Vite builds in Chromium via CDP at 1440×900. Five fresh navigations per version ran with browser cache disabled. These are local measurements, not a cross-device guarantee.

| Measure | Before | After | Result |
|---|---:|---:|---:|
| Median FCP | 80 ms | 80 ms | No measurable median change |
| Median DOMContentLoaded | 235.3 ms | 228.8 ms | 6.5 ms faster (~2.8%) |
| Median load | 236.1 ms | 229.6 ms | 6.5 ms faster (~2.8%) |
| Home-start API requests | 16 | 5 | 11 fewer (~68.8%); Language calls deferred |
| Main JS gzip size | ~846.8 KB | ~788.7 KB | ~58.0 KB smaller (~6.9%) |

Initial main JS raw size changed from 2,892,464 to 2,669,261 bytes. Median FCP did not improve; the load-time difference is small and directional. Large chunks remain, notably MapLibre.

## 12. AI catalog audit

The isolated local API status/catalog reported **14 modules and 66 tools**. Risk distribution: **58 Read, 4 SafeWrite, 4 SensitiveWrite, 0 System**. This snapshot does not establish provider inference behavior.

## 13. Tool permission changes

These tools remain registered for discoverability/audit but are `SensitiveWrite`, excluded from model-available tools/module descriptors, and blocked by the automatic execution gate until an explicit approval flow exists:

- `memory.archive`
- `rss.mark_read`
- `study-board.save`
- `study-board.snapshot`

Read and SafeWrite tools remain eligible subject to module/context scoping. Catalog visibility is not execution authority.

## 14. Authorization and failure-path evidence

Application tests verify sensitive tools are excluded from model exposure, omitted from narrowed module descriptors, and rejected when attempted by a model. Registry tests exercise both unscoped and context-scoped execution gates. Existing write-path tests verify blocked attempts do not mutate data. These are deterministic contract/unit tests, not evidence of live-provider behavior.

## 15. Native Tauri coverage

`npm --prefix apps/desktop run test:e2e` passed on macOS WebKit 605.1.15: **5 spec files, 17 tests, 0 failures**. Coverage included navigation, Home/Review progress refresh, settings persistence and secret redaction, safe-action confirmation, AI unconfigured empty state, PersonalAgent registry/cancellation, Travel persistence, and Markdown shortcuts. This is native Tauri/WebKit evidence for this machine, not Windows/Linux/mobile acceptance.

The WDIO run emitted non-failing Tauri-service cleanup/IPC warnings (`core.invoke` fallback and mock-store cleanup without a session ID); all specs still passed.

## 16. Isolated recovery and restart behavior

Failure injection used only the local test server and its `/tmp` store. Stopping and restarting that server preserved the four review results, and the UI recovered through retry controls. No production service was stopped and no production store was written. The bundled History DB was not modified.

## 17. Verification ledger

- `cargo test --workspace` — **1,333 passed, 0 failed** across the Rust workspace.
- `cargo test -p devtoolbox-application personal_ai:: --lib` — **176 passed**.
- `cargo test -p devtoolbox-runtime --lib` — **13 passed**.
- `cargo check -p devtoolbox-server` — passed.
- `cargo check -p devtoolbox-desktop` — passed.
- `npm --prefix apps/desktop/ui test` — **21 files, 125 tests passed**.
- `npm --prefix apps/desktop/ui run build -- --outDir /tmp/self-tools-sprint10-build-final` — TypeScript and production Vite build passed.
- Native E2E — **5 specs, 17 tests passed**.
- `cargo fmt --all -- --check`, `git diff --check`, and Node syntax checks for the edited E2E/config/build scripts — passed.

## 18. Limitations and follow-up

The configured-provider flag remained false. No genuine inference, live cancellation, model-generated citation, or Travel-proposal acceptance was tested. Performance was measured on one local browser/device; FCP was unchanged at the median and large chunks remain. Native acceptance is macOS-only. The worktree remains dirty by design; no cleanup, commit, or deploy was performed.

## Five closing answers

1. **Did Sprint 10 meet its locally verifiable goals?** Yes: Home journey/recovery, performance/API reduction, tool risk audit, isolated restart persistence, and native Tauri coverage were verified.
2. **What changed measurably?** Home startup API calls fell 16→5, initial JS gzip fell about 6.9%, and median DOMContentLoaded/load improved about 6.5 ms; median FCP was unchanged.
3. **Are sensitive AI writes protected?** Yes for the automatic tool path: four tools are hidden from model schemas/descriptors and rejected at registry execution; explicit user approval remains future work.
4. **Was live AI/provider behavior proven?** No. The provider is unconfigured; unit, contract, browser-empty-state, and native tests do not claim live inference.
5. **Was production data or the user's worktree disturbed?** No production writes, reset/clean/stash, commit, push, merge, or deploy occurred. Temporary writes stayed in the isolated `/tmp` test store; existing dirty files and the pre-existing Geography deletion were preserved.
