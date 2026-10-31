# Sprint 7 — Knowledge cards in the Learning OS

**Status: browser-verified through the local HTTP client and server.** Native Tauri runtime acceptance was not performed; its Rust target was compile-checked. Browser runs used isolated temporary data on loopback and system Chrome 150 over CDP.

## Verified journey

1. Opened a saved History event (**洛阳失守**, `event-anlu-luoyang`) in Collections and created then edited a user-authored question/answer card. The existing collection note was copied into the editable personal note. Reloading the browser and reopening the editor retained the edited content.
2. Added a saved Geography city (**洛阳**, `luoyang`) to the temporary collection and created a Geography card through the same editor. The Geography fixture item was added through the local Collections HTTP endpoint; the card itself was created in the browser.
3. Opened both modules in Review Center, entered answers, and submitted **Good** ratings. The queue/stat counts refreshed; receipts showed the next due date and persisted learning progress. The current rating copy now describes relative intervals instead of promising fixed day counts that do not match the shared scheduler.
4. Used the receipt source links to reopen the History event and Geography city. The pages rendered the expected source entities.
5. Restarted the local server against the same temporary SQLite database. Both cards, their schedules, and both progress rows remained present. After the final verification pass, each test card had `repetition_count=2`, a three-day interval, and two correct review events.

For repeated UI review passes, only the isolated QA fixture’s `due_at` was moved to the present so the already-reviewed card could re-enter the queue; its SRS repetition history was not reset. A separate infrastructure test verifies same-request retry idempotency, rollback on failed writes, schema migration, and persistence after reopening the database.

## Responsive and accessibility checks

- At **390×844**, Review Center and the Knowledge card editor were usable with no horizontal document overflow (`documentWidth === 390`). Collections now stacks its sidebar and content on narrow screens; the page pane scrolls so editor actions remain reachable.
- Axe on `.review-center-page`: **0 violations**, 1 incomplete color-contrast assessment where the tool could not determine a background image’s color.
- Axe on `.knowledge-review-panel`: **0 violations**. The audit caught and prompted fixes for the Review Center stat colors and the editor’s source metadata contrast.
- No page errors were reported after a clean browser reload. Vite emitted normal development/HMR notices during iterative code changes.

## Evidence

- [Edited History card](after/knowledge-card-edited-1920x844.png)
- [Geography card editor](after/geography-card-editor-1440x900.png)
- [Knowledge card editor, mobile; save controls visible](after/knowledge-card-editor-mobile-390x844.png)
- [Review rating options](after/review-rating-options-1440x900.png)
- [Review receipt with SRS and progress](after/review-progress-receipt-1440x900.png)
- [Review receipt, mobile](after/review-progress-receipt-mobile-390x844.png)
- [History source reopened from Review Center](after/history-source-return-1920x844.png)
- [Geography source reopened from Review Center](after/geography-source-return-1440x900.png)

## Checks

- `cd apps/desktop/ui && npm test` — **passed**, 19 files / 117 tests.
- `cd apps/desktop/ui && npm run build` — **passed**, TypeScript and Vite production build. Existing warnings remain for the large main chunk and mixed static/dynamic `@tauri-apps/plugin-dialog` imports.
- `cargo test -p devtoolbox-application -p devtoolbox-infrastructure -p devtoolbox-server` — **passed**, 929 tests across 8 suites.
- `cargo check -p devtoolbox-desktop -p devtoolbox-server -p devtoolbox-runtime` — **passed**, 21 crates compiled.
- Targeted `rustfmt --edition 2024 --check` on the changed Learning model/service/store/API files — **passed**.
- `git diff --check` — **passed**.
- `cargo fmt --all -- --check` — **not clean** because other already-modified, unrelated History, Geography, routes, and Personal AI files in the existing worktree need formatting. Those files were not reformatted to avoid expanding this task into unrelated changes.

## Scope, contract, and remaining verification

Knowledge cards reuse the existing `UniversalReviewCard`, SQLite learning store, `LearningService`, SRS scheduler, Review Center, and Collections source references. The new card operations and optional review idempotency key are additive to the HTTP/Tauri boundary; card content is explicitly user-authored and is not AI-generated or marked verified. No separate scheduler or review system was introduced.

The local HTTP guard was exercised: browser writes require a loopback Origin, and the route test rejects a missing Origin. Native Tauri launch, AI-provider behavior, broader security review, and manual UI deletion were not tested. `npm ci` reported five audit findings; dependencies and lockfiles were left unchanged and those findings were not triaged here.
