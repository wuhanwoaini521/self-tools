# Pencil UI Redesign Report

## Route Coverage

- Registered hash routes: **16**; all 16 were opened and captured in the prior visual pass.
- Sidebar navigation smoke check: **16 / 16**.
- Viewports: desktop 1440×900, tablet landscape, tablet portrait, mobile, small mobile.
- Captured states: **90**. Inventory: [`route-inventory.md`](./route-inventory.md).
- This is route/render coverage, not complete redesign coverage.

## Design System

- **Tokens:** paper, graphite, pencil palette, sketch strokes, shadows, rotation, typography, motion, responsive tokens in `apps/desktop/ui/src/theme/pencil.css`.
- **Components:** SketchCard/Panel/Button/IconButton/Input/Textarea/Select/Tabs/Badge/Progress/Tooltip/Popover/Dialog/Drawer/Table/EmptyState/SectionHeader/Breadcrumb/PageHeader/StatCard/Paper/StickyNote/Divider/Illustration/ModuleCard/NavigationItem in `apps/desktop/ui/src/components/sketch/`.
- **Typography:** platform handwritten display fallbacks, readable body font stack, monospace numeric/code styles.
- **Motion:** paper lift/straighten and unfold/fade transitions; reduced-motion and mobile rotation rules.
- **Illustrations:** inline SVG line art for major modules.

## Pages

| Page | Status | Notes |
|---|---|---|
| Home | PASS | Rebuilt as the reference portal; AI suggestion actions now remain fully legible and readable at rest. |
| Review | PARTIAL | Route/shared skin checked; review-specific visual hierarchy and card states remain. |
| History | PARTIAL | Era navigation now uses balanced cards, the first era no longer renders an empty placeholder, period context aligns cleanly, and event summaries are readable; broader History IA and remaining sections still need migration. |
| Geography | PARTIAL | Shared skin applied; explorer/map-specific redesign remains. |
| Language | PARTIAL | Recent-learning actions wrap instead of shrinking, and lesson rows have more readable spacing; other language subviews remain on legacy layouts. |
| Study | PARTIAL | Shared skin applied; study-board-specific workspace redesign remains. |
| News | PARTIAL | Shared skin applied; clipping/scrapbook-specific redesign remains. |
| RSS | PARTIAL | Shared skin applied; reading-desk-specific redesign remains. |
| Travel | PARTIAL | Search form now has consistent inset spacing and responsive preference/date rows; the full Notebook workspace and trip workflows remain incomplete. |
| Markdown | PARTIAL | Toolbar rows grow instead of clipping, and Split stays visible at desktop/tablet widths; the editor/workspace still needs a dedicated redesign. |
| Collections | PARTIAL | Shared skin applied; collection-management redesign remains. |
| Knowledge | PARTIAL | Shared skin applied; knowledge-universe redesign remains. |
| Graph | PARTIAL | Shared skin applied; graph-canvas and node interactions remain. |
| Search | PARTIAL | Shared skin applied; research-desk result hierarchy remains. |
| Server | PARTIAL | Shared skin applied; tech-lab-specific redesign remains. |
| System / readiness | PARTIAL | Shared skin applied; diagnostic hierarchy redesign remains. |
| Settings dialog | PARTIAL | Paper styling applied; form migration to shared Sketch components remains. |

Applications is a Server sub-area, not a hash route. Settings is a global dialog. Route presence and screenshots do not mean each page has completed its dedicated redesign.

## Functional Regression

- UI unit tests: **PASS** — 13 files, 95 tests (current run).
- Rust workspace tests: **PASS** — 678 tests (prior run; not rerun in this pass).
- UI typecheck/build: **PASS** (current run).
- Browser visual QA: **PARTIAL** — all 90 states across 5 viewports were rendered; route, navigation, overflow, and accessible-button assertions passed. The harness exited non-zero on a third-party map tile response (`tiles.mapterhorn.com/4/15/6.webp`, HTTP 404) during small-mobile Server capture.
- Native WebDriver E2E: **NOT RUN**.
- Lint: **NOT CONFIGURED / NOT RUN**.
- Live data mutations, provider flows and AI tools: **NOT verified end-to-end**.

## UI Review

- Desktop: **PARTIAL** — the Home assistant suggestions, History period/timeline controls, Language recent-learning row, Markdown toolbar, and Travel search form received targeted fixes. Most other modules still retain legacy markup and information hierarchy.
- Responsive: **PARTIAL** — route checks reported no document-level horizontal overflow across five viewports; content visibility, overlays and all interactions were not exhaustively checked.
- Reviewed post-fix screenshots: [`post-fix/`](./post-fix/) (Home, History, Language, Markdown at desktop/tablet/mobile, Travel at desktop/tablet/mobile).

## Remaining Issues

- This remains an incomplete redesign pass, not full-project UI acceptance.
- Non-Home modules mostly retain legacy feature markup; per-module information architecture, shared component migration, interaction design, and illustration work remain. The screenshot fixes do not imply full module completion.
- Prior browser screenshots ran without the Rust API, so they represent empty/degraded states rather than live data workflows.
- Subroutes and in-page states are not fully enumerated in the initial route inventory.
- Accessibility needs targeted validation for contrast, keyboard traversal, modal focus, and reduced-motion behavior.
- Handwritten type uses platform fallbacks to preserve Chinese readability and avoid adding a font payload.
