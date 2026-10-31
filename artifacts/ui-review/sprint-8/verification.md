# Sprint 8 — Personal Hub and quality gates

**Status: browser-verified on the local HTTP client/server; native Tauri acceptance remains unverified.** The Hub reads existing Learning OS, Collections, Travel, News, and local recent-note sources. No second data store or review scheduler was introduced.

## Delivered

- Reworked Home into a Personal Hub centered on due reviews, continuing learning, saved History/Geography references and personal notes, recent Travel plans, News headlines, and recent notes.
- Each backend resource loads independently and has loading, empty, retryable failure, and populated states. Data comes from the existing clients: `learningClient`, `travelClient`, and `newsClient`.
- Home links resume the existing History and Geography source routes, open Review Center, manage saved knowledge in Collections, and reopen the exact saved Travel plan by its existing context. Direct `#travel?...` startup was also exercised.
- Added persisted note editing to saved Collection items through `PATCH /api/v1/learning/collection-items/{id}/note` and the equivalent Tauri command. It preserves collection/source identity and updates only the personal note plus collection recency. Empty notes clear; input is limited to 4,000 characters.
- Aligned frontend Collection DTOs with the existing Rust response fields (`item_count`, `added_at`). No existing serialized response was changed.
- Updated locked frontend dependencies to patched versions: DOMPurify 3.4.17, Vite 7.3.7, Vitest and `@vitest/mocker` 5.0.5, and `source-map-js` 1.2.2. This resolved the production HTML-sanitizer finding and development-toolchain findings; no package manifest change was required.

## Browser acceptance

System Chrome 150 over CDP; isolated QA data directory; Vite at `127.0.0.1:1420` and the loopback server at `127.0.0.1:8080`.

- Home showed real Learning progress and due count, continue-learning History/Geography records, two saved source references with user notes, and a saved Luoyang Travel plan with dates and custom-place data. News returned no headlines and recent notes were empty, so their real empty states appeared instead of fabricated samples.
- Stopped the isolated backend and reloaded Home. Learning, Collections, Travel, and News independently rendered their own failure/retry states; the Hub did not misrepresent the outage as empty data. The backend was restored and all real data loaded again.
- Edited the Geography Collection note through the UI, restarted the server against the same temporary data directory, and confirmed the new note persisted with `entity_id=luoyang` and the collection item intact.
- Reopened the exact saved Travel plan from Home and from a direct `#travel?city=...&days=...&date_start=...&date_end=...` URL. Travel loaded its saved title, dates, custom place, and note.
- Rechecked personal-card deletion after server restart: the deleted card returns 404 and is absent from the due queue; the original History entity, Collection references/notes, and learning progress remain. The progress endpoint still reports the existing study/review counts.
- Tested 1280×800, 1440×900, 1920×1080, 768×1024, and 390×844. No horizontal overflow was measured at these sizes. On mobile, Collection note textarea and save/cancel controls were in view and reachable.
- Final clean-reload browser error check: no page errors.

### Accessibility

- Axe 4.12.1 on `.personal-hub-page`: **0 violations**, 1 incomplete color-contrast assessment covering four nodes that Axe could not fully resolve against the themed background.
- Axe 4.12.1 on `.collection-note-editor`: **0 violations, 0 incomplete checks**.

### Evidence

- [Home — desktop 1440×900](after/home-desktop-1440x900.png)
- [Home — 1280×800](after/home-1280x800.png), [1920×1080](after/home-1920x1080.png), [tablet 768×1024](after/home-tablet-768x1024.png)
- [Home — mobile 390×844](after/home-mobile-390x844.png), [mobile bottom/empty states](after/home-mobile-bottom-390x844.png)
- [Backend-outage failure states](after/home-api-failure-1440x900.png)
- [Collection note editor — desktop](after/collection-note-editor-1440x900.png), [mobile](after/collection-note-editor-mobile-390x844.png)
- [Saved Travel plan resumed](after/travel-plan-resumed-1440x900.png)
- [Deleted card / Collection still present](gate0/deleted-card-collection-persists-1440x900.png)

## Verification

- `cd apps/desktop/ui && npm test` — **passed**, 20 files / 119 tests.
- `cd apps/desktop/ui && npm run build` — **passed**, TypeScript and Vite production build. Existing warnings remain for mixed static/dynamic `@tauri-apps/plugin-dialog` imports and chunks larger than 500 kB.
- `cd apps/desktop/ui && npm audit --audit-level=low` — **0 vulnerabilities**.
- `cd apps/desktop/ui && npm audit --omit=dev --audit-level=low` — **0 vulnerabilities**.
- `cargo test -p devtoolbox-application -p devtoolbox-infrastructure -p devtoolbox-server` — **passed**, 931 tests across 8 suites.
- `cargo check -p devtoolbox-desktop -p devtoolbox-server -p devtoolbox-runtime` — **passed**. This is compile-check coverage, not a native app launch.
- Targeted `rustfmt --edition 2024 --check` on the edited Learning application/store/API files — **passed**.
- `git diff --check` — **passed**.
- `cargo fmt --all -- --check` — **not clean** because already-modified History, Geography, route, Personal AI, and shared desktop/server files contain formatting drift. Those unrelated regions were not reformatted.

## Security, AI, native, and worktree status

- The additive note mutation uses the existing loopback Host/Origin guard. The API test rejects a missing Origin; CORS remains disabled. No provider credentials or secrets were added.
- No real AI provider was configured or called. The Ask AI control remains an entry to the existing panel; provider behavior is not part of this acceptance.
- Native Tauri runtime/WebKit acceptance was not performed. The desktop target was compile-checked only.
- Existing dirty worktree changes were preserved. No reset, clean, stash, commit, push, merge, or deployment was performed. The isolated QA server, Vite server, Chrome process, and temporary QA data directory were stopped/removed after verification.
