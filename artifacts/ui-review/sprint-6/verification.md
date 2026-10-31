# Sprint 6 — Collections save and source search verification

**Status: browser-verified** with the regular server + Vite client, using system Chrome 150 over CDP. The bundled agent-browser Chrome 155 could not launch on this macOS image because of a VideoToolbox symbol mismatch; no native Tauri acceptance is claimed.

## Verified journey

1. Opened Geography entity **洛阳** (`luoyang`), created **Sprint 6 Geography–History**, and saved the personal note `sprint6-geography-note-luoyang-anchor-756`.
2. Opened the reviewed History event **洛阳失守** (`event-anlu-luoyang`) from Geography’s stable-ID-linked timeline, then saved it to the same collection with `sprint6-history-note-event-anlu-luoyang-756`.
3. Reopened Collections and confirmed both source items and both personal notes were present.
4. Searched each exact personal-note string in Global Search. Each result was labeled **Collections**, retained its original source, and reopened the corresponding Geography entity or History event detail—not just the collection.
5. At 390×844, verified the Geography and History save forms and controls remain usable. `document.documentElement.scrollWidth` stayed at 390px; at 1440×900 it matched the viewport width.

Collection creation, item-save, and entity/timeline requests returned HTTP 200 in the browser session. The History result reopened the event with its participants and 洛阳 place reference; the Geography result reopened 洛阳 and its reviewed History timeline.

## Evidence

- [Geography save form, desktop](after/geography-save-form-1440x900.png)
- [History save form, desktop](after/history-save-form-1440x900.png)
- [Geography save form, mobile](after/geography-save-form-mobile-390x844.png)
- [History save form, mobile](after/history-save-form-mobile-390x844.png)
- [Collection containing both sources and notes](after/collection-two-sources-1440x900.png)
- [Global Search result — Geography note](after/global-search-geography-note-1440x900.png)
- [Global Search result — History note](after/global-search-history-note-1440x900.png)
- [Geography source reopened from search](after/geography-source-from-search-1440x900.png)
- [History source reopened from search](after/history-source-from-search-1440x900.png)

## Checks

- `pnpm --dir apps/desktop/ui test` — **passed**, 18 files / 112 tests.
- `cargo test -p devtoolbox-application` — **passed**, 601 tests.
- `cargo test -p devtoolbox-server` — **passed**, 48 tests.
- `cargo check --manifest-path apps/desktop/Cargo.toml` — **passed**.
- Targeted Rust formatting checks — **passed**.
- `git diff --check` on the touched UI files — **passed**.
- UI production build/typecheck — **not green in this run**: pnpm reconciled the UI dependencies, reported the ignored `esbuild` install script (`ERR_PNPM_IGNORED_BUILDS`), then TypeScript reported four errors in `ContourOverlay.tsx`, `TerrainSlab.tsx`, and `markdown-decorations.ts` involving duplicate Three.js/CodeMirror type identities. No diagnostic pointed to a Sprint 6 file. Because this followed dependency reconciliation, I cannot claim these errors reproduce against the prior dependency tree. pnpm created an untracked lockfile during reconciliation; that test artifact was removed after verification. No build-script approval was made.

## Scope and contract

Collections persistence was reused; no new persistence model was added. Global Search gained an additive `Collections` result source while retaining each saved item’s original module/type/stable entity ID, so search navigation remains compatible with existing source-detail routes. Collections now refreshes when activated, and the save form selects a newly created collection immediately. No provider inference was needed or tested; native Tauri acceptance remains unverified.
