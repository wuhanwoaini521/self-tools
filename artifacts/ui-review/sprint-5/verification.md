# Sprint 5 verification

**Run date:** 2026-10-26  
**Scope:** FULL STACK — additive History event/story response data plus Geography timeline, navigation, AI context, landform return flow, and Travel save handoff.  
**Worktree:** Existing dirty changes were preserved. No reset, stash, commit, push, merge, or deploy was performed.

## Sprint gates

| Gate | Result | Evidence |
|---|---|---|
| Stable Geography–History identity and provenance | **PASS** | Luoyang uses Geography `luoyang` ↔ reviewed History place `cbdb-place-14693`; the 756 event and all displayed crosswalk evidence retain stable IDs and source `source-curated-backbone-v1`. The 756 year is an event-evidence scope, not a claim about the full historical validity period of the place. Similar names are not used to establish a link. |
| Reviewed story/event timeline | **PASS** | Event `event-anlu-luoyang` returns reviewed, usable story `story-an-lushan-rebellion`. The timeline shows reviewed story nodes with unknown dates preserved as unknown and clearly labels other story nodes as context, not as events at Luoyang. The Luoyang event is the only displayed node with a reviewed stable place-ID proof. |
| Geography ↔ History navigation and retained context | **PASS (browser)** | From the Luoyang timeline, opened the An Lushan story and the Luoyang event in History; both exposed “返回 洛阳地图”. Returning retained the Geography detail and selected 756 event. The AI context chip showed `Geography · 洛阳 · History event · 756`. |
| Landform navigation | **PASS (browser)** | Opened the landform knowledge view from Luoyang detail and returned to the same map/timeline. The view continues to distinguish conceptual models from real DEM coverage. |
| Save through Travel | **PASS (browser + API)** | Added Luoyang with the reviewed event. `GET /api/v1/travel/plans/recent?limit=20` returned the saved plan, including `custom:geography:luoyang`, event/place IDs, review status, and geography/history source IDs in its note. The Collections path was not needed for this Travel save gate. |
| Empty vs. History failure and retry | **PARTIAL / BLOCKED_VERIFY** | The UI has separate empty and error states and a retry control. Browser-forced outage verification did not complete: CDP network routes did not intercept the request; observed History API calls remained HTTP 200. No browser-outage pass is claimed. |
| Responsive layout | **PASS (browser)** | At 390×844 the Geography timeline and landform view rendered without horizontal overflow (`document.documentElement.scrollWidth === 390`). See mobile captures below. |
| AI provider inference | **BLOCKED_EXTERNAL** | The panel reported that an AI provider is not configured. Context presentation was verified; no provider inference was attempted or claimed. |
| Native Tauri acceptance | **BLOCKED_ENV** | Tauri CLI was unavailable. Bundled Chrome 155 could not launch on macOS 12.7.6 due to a missing VideoToolbox symbol, so browser verification used system Chrome 150 over CDP. Native launch/package/E2E remains unverified. |

## Commands and results

- `cd apps/desktop/ui && npm test` — **17 files, 108 tests passed**.
- `cd apps/desktop/ui && npm run build` — **passed** (TypeScript and Vite). Existing warnings remain for mixed static/dynamic `plugin-dialog` imports and the large application chunk.
- `cargo test -p devtoolbox-application --lib` — **599 passed**.
- `cargo test -p devtoolbox-server` — **47 passed**, including the real History database exploration/story association test.
- `cargo test -p devtoolbox-runtime --lib` — **12 passed**.
- `git diff --check` — **passed**.
- Workspace-wide `cargo fmt --all -- --check` has existing formatting differences in already-dirty files; broad formatting was avoided to preserve unrelated user changes.

## Browser evidence

Browser: system Chrome **150.0.7871.125**, 1440×900 desktop and 390×844 responsive viewport. The Sprint 5 isolated server and Vite UI used ports 8081 and 1421 with temporary test data; existing services on 8080/1420 were left untouched.

Before captures are in `before/`. Key after captures:

- `after/geography-detail-landform-1440x900.png` — Luoyang detail, map and return action.
- `after/geography-map-landform-1440x900.png` — map, landform navigation and locality disclaimer.
- `after/geography-history-timeline-1440x900.png` — reviewed story timeline and crosswalk scope.
- `after/geography-history-sources-1440x900.png` — expanded evidence index for the selected Luoyang event.
- `after/history-an-lushan-story-1440x900.png` — History story reader with return-to-map navigation.
- `after/geography-history-mobile-390x844.png` and `after/geography-landforms-mobile-390x844.png` — mobile timeline and landform view.
- `after/travel-saved-context-1440x900.png` — Travel plan with saved Geography/History IDs and provenance in the place note.

## Contract and boundary notes

The History event detail adds `related_stories` and merges those stories’ sources into its existing source list. Geography crosswalks, History detail/story reads, and TravelPlan persistence remain behind their existing API clients. The UI does not infer place identity from names; it labels only events linked through the reviewed stable History place ID and scoped evidence. Story-only nodes are explicitly presented as story context.

The existing worktree contained unrelated modifications before this run. They were preserved. Sprint 5 temporary services/configuration and browser process are stopped/removed after verification; screenshot evidence remains in this directory.
