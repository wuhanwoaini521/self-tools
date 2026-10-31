# Sprint 2 Travel — Browser Acceptance

**Date:** 2026-10-25  
**Environment:** Vite UI at `127.0.0.1:1420` → loopback HTTP server at `127.0.0.1:8080`, isolated temporary Travel database, Chrome via `agent-browser` CDP. This is a real browser/API integration check, not a native Tauri/WebKit run.

## Verified workflows

- **No-provider manual creation:** created a blank three-day Ningbo plan; the backend returned it from the recent-plans endpoint at revision 1. No AI/provider request was needed.
- **Persistence and reload:** reloaded the browser, reopened the recent plan, and confirmed its data came from the backend.
- **Editing:** saved title, trip notes, two custom stops, per-stop notes, and stop ordering; confirmed the ordered keys and notes through the HTTP read endpoint.
- **Failure and retry:** took the browser offline, edited a note, observed `Failed · 保存失败`, and verified the per-tab local journal held the draft. Restored connectivity and retried; the server advanced the revision and the journal cleared.
- **Failure recovery after reload:** repeated an offline edit, reloaded the browser, resumed the pending draft, and verified it was written to the backend.
- **Two-client conflict:** two browser tabs loaded the same revision. Client A saved revision 3; stale client B received the Conflict UI and its draft remained journaled while the backend still contained A's content. After explicitly selecting overwrite, B saved revision 4 and its journal cleared.
- **Navigation/unmount during save:** delayed a real browser save request before it reached the local API, then changed destination so the keyed `TravelGuide` unmounted. Releasing the request persisted the edit, cleared its local journal, and refreshed the still-mounted recent-plan/draft list without a stale recovery badge.
- **Cross-module data:** Travel opened matching Hangzhou History events and the seeded Chengdu Geography entity. When WebGL2 was unavailable, Geography displayed a map-unavailable fallback while retaining the entity facts.
- **Accessibility:** axe on `.travel-page` reported **0 violations** (26 passes); it marked 15 text nodes incomplete because of the layered paper background. Manual contrast calculation across 115 text/placeholder nodes found a minimum of **5.22:1**, with none below WCAG AA. The itinerary guide scope separately reported 0 violations and 0 incomplete checks (25 passes).

A false `Saved` state for a brand-new blank manual plan was found during the first pass. Manual creation now persists the initial blank plan; an untouched guide without a backend record reports `Draft` rather than claiming it is saved. The in-flight unmount check also exposed a stale parent draft-list state; the save-completion callback now refreshes the still-mounted parent even after its guide child unmounts. The contrast review fixed the save/day/navigation colors and a low-contrast Travel hero badge; all manually measured text remains at or above 5.22:1.

## Visual evidence

- `travel-plan-1280x800.png`
- `travel-plan-1440x900.png`
- `travel-plan-1920x1080.png`
- `travel-save-failed-1920x1080.png`
- `travel-two-client-conflict-1440x900.png`
- `geography-map-fallback-1440x900.png`

## Limits

Native Tauri/WebKit E2E, external AI/map/weather provider calls, and LAN/WAN access were not tested. Geography/History checks used the local seeded matches only. Browser QA used a temporary database; no production or user Travel records were modified.
