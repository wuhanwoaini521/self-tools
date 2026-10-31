# Sprint 3 · Travel → Geography → History → Travel QA

| Field | Value |
|---|---|
| Date | 2026-10-25 |
| Client | Local Vite HTTP client (`127.0.0.1:1421`) |
| API | Isolated local server (`127.0.0.1:8081`) |
| Browser session | `sprint3-travel-qa-e2e9f616e4cc` |
| Scope | Chengdu linked-data journey, plan handoff/recovery, desktop/mobile visual and axe checks |
| Data safety | Temporary config with a copy of Geography SQLite and an empty Travel DB; source History DuckDB read-only. Existing server on `8080`, Vite on `1420`, and original Travel DB were not used or modified. |

## Summary

| Severity | Count |
|---|---:|
| Critical | 0 |
| High | 0 |
| Medium | 0 |
| Low | 2 |
| **Total** | **2** |

Finding status: **2 discovered, 2 fixed and rechecked, 0 open**.

## Journey evidence

- Browser-driven flow: Travel selected Chengdu → Geography opened the Chengdu entity → Geography displayed the reviewed crosswalk `chengdu` ↔ `cbdb-place-406333` and the reviewed 756 event 唐玄宗入蜀 → History event detail opened → Geography returned to the active Travel plan → “加入当前行程” inserted a sourced Chengdu stop. Final captures: [Travel plan](screenshots/travel-final-desktop.png), [Geography crosswalk](screenshots/geography-crosswalk-final.png), [History event](screenshots/history-event-fixed.png), [recovered plan](screenshots/travel-restored.png).
- The Travel API returned `POST /api/v1/travel/plans` **200**. After browser reload, the plan appeared under “最近行程”; reopening it restored the Chengdu stop and its History/Geography provenance. `GET /api/v1/travel/plans` returned **200**.
- `GET /api/v1/exploration/geography/chengdu/history` returned **200** in the browser; the real event and reviewed source links were visible in the UI.
- Browser console/page errors: none observed (React DevTools informational message only).
- Application AI-context tests cover the stable Geography→History tool hint, History→Geography crosswalk, and Travel dates/preferences/departure/current stops/notes. Live model response was not tested because the provider is unconfigured.
- Automation: application Rust tests **599 passed**; server Rust tests **46 passed**; crosswalk core test **1 passed**; frontend **104 tests passed**; production TypeScript/Vite build passed; desktop Rust test target compiled with `--no-run`.
- Initial desktop axe-core on Travel: **0 violations**, 43 passes; color-contrast review incomplete for 50 nodes. After the visual fixes, mobile Travel axe-core: **0 violations**, 42 passes; contrast review remains incomplete for 9 nodes and is manual/inconclusive.
- At 390×844, `documentElement.scrollWidth` equaled `innerWidth` (390px); no horizontal overflow. The screenshot did reveal the overlap below.

## Findings

### ISSUE-001: Floating Ask AI button overlaps a Travel quick-stop suggestion

| Field | Value |
|---|---|
| Severity | low |
| Category | visual / responsive |
| URL | `http://127.0.0.1:1421/#travel` |
| Repro video | N/A — static layout issue |
| Status | Fixed and rechecked |

At initial capture (390×844), the fixed Ask AI button overlapped the “宽窄巷子” quick-stop chip. The overlap reproduced on a second capture; initial rectangles intersected (`Ask AI`: x=262.9–376.0, y=720–762; chip: x=258.1–328.6, y=715.6–737.6). On mobile the launcher is now a 44×44 icon-only control. Recheck: launcher x=334–378 and chip x=258.1–328.6, with no intersection. See [rechecked mobile Travel](screenshots/travel-mobile-fixed.png).

**Repro steps**

1. Open the Travel view at a 390×844 viewport with the Chengdu planner visible. See [reproduced overlap](screenshots/issue-001-mobile-overlap.png).

### ISSUE-002: History event drawer heading is partly covered by the app bar

| Field | Value |
|---|---|
| Severity | low |
| Category | visual |
| URL | `http://127.0.0.1:1421/#history` |
| Repro video | N/A — static layout issue |
| Status | Fixed and rechecked |

At initial capture the drawer header began behind the fixed 56px global app bar, clipping its title. The drawer now starts below the app bar. Recheck at 1440×1000: drawer top=56px, app bar bottom=56px, title bounds y=97–130px; the title is fully visible. The 390×844 drawer also starts at y=56 with no horizontal overflow. See [desktop recheck](screenshots/history-event-fixed.png) and [mobile recheck](screenshots/history-event-mobile-fixed.png).

**Repro steps**

1. Open History, search for 唐玄宗入蜀, then open its result at 1440×1000. See [drawer header clipping](screenshots/issue-002-history-header-clipped.png).

## Recheck after fixes

- Mobile Ask AI overlap no longer intersects the quick-stop chip at 390×844; `documentElement.scrollWidth === innerWidth` (390px).
- History event drawer title is visible below the app bar at desktop and mobile sizes; mobile width remains 390px with no overflow.
- The original issue screenshots above preserve what was observed before the fixes; the recheck captures are separate files.
- Impeccable detector: the new crosswalk block's side accent was removed. Four remaining side-tab warnings and one decorative-grid advisory point to unchanged, pre-existing regions of `styles.css` outside this delta; left untouched to avoid unrelated styling changes.

## Limitations

- This is HTTP-browser validation only; native Tauri/WebKit execution was not performed.
- The AI provider is not configured in the isolated runtime, so no live model response was tested. Backend context assembly and stable cross-module tool routing are covered by the passing application tests.
- `cargo test -p devtoolbox-desktop --no-run` proves compilation only; no native Tauri/WebKit interaction was performed.
- The existing pre-task dirty worktree had no trustworthy Sprint 3 “before” capture; screenshots here are genuine post-change captures, not a before/after comparison.
