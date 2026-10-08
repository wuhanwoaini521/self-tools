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
| Home | PASS | Rebuilt as the reference portal: greeting/stats, AI bubble, continue learning, explore, workspace modules, context and recent items. |
| Review | PARTIAL | Route/shared skin checked; review-specific visual hierarchy and card states remain. |
| History | PARTIAL | Shared skin applied; historical timeline/detail visual redesign remains. |
| Geography | PARTIAL | Shared skin applied; explorer/map-specific redesign remains. |
| Language | PARTIAL | Shared skin applied; language and English subviews are not fully migrated to Sketch components. |
| Study | PARTIAL | Shared skin applied; study-board-specific workspace redesign remains. |
| News | PARTIAL | Shared skin applied; clipping/scrapbook-specific redesign remains. |
| RSS | PARTIAL | Shared skin applied; reading-desk-specific redesign remains. |
| Travel | PARTIAL | Shared skin applied; full Notebook workspace redesign remains. |
| Markdown | PARTIAL | Shared skin applied; editor-specific redesign remains. |
| Collections | PARTIAL | Shared skin applied; collection-management redesign remains. |
| Knowledge | PARTIAL | Shared skin applied; knowledge-universe redesign remains. |
| Graph | PARTIAL | Shared skin applied; graph-canvas and node interactions remain. |
| Search | PARTIAL | Shared skin applied; research-desk result hierarchy remains. |
| Server | PARTIAL | Shared skin applied; tech-lab-specific redesign remains. |
| System / readiness | PARTIAL | Shared skin applied; diagnostic hierarchy redesign remains. |
| Settings dialog | PARTIAL | Paper styling applied; form migration to shared Sketch components remains. |

Applications is a Server sub-area, not a hash route. Settings is a global dialog. Route presence and screenshots do not mean each page has completed its dedicated redesign.

## Functional Regression

- UI unit tests: **PASS** — 13 files, 95 tests (prior run).
- Rust workspace tests: **PASS** — 678 tests (prior run).
- UI typecheck/build: **PASS** (prior run).
- Browser visual QA: **PASS for the scripted route/layout assertions** — 90 states across 5 viewports; this did not cover every page interaction.
- Native WebDriver E2E: **NOT RUN**.
- Lint: **NOT CONFIGURED / NOT RUN**.
- Live data mutations, provider flows and AI tools: **NOT verified end-to-end**.

## UI Review

- Desktop: **PARTIAL** — the paper/graphite skin and Home redesign are visible, but non-Home pages retain much of their legacy markup and information hierarchy.
- Responsive: **PARTIAL** — scripted checks reported no horizontal document overflow; overlays, content visibility and all interactions were not exhaustively checked.
- Screenshots: [`artifacts/ui-review/`](./), with desktop route captures in `desktop/` and additional viewport captures in their named subdirectories.

## Remaining Issues

- This is an incomplete first redesign pass, not full-project UI acceptance.
- Non-Home modules mostly retain legacy feature markup and receive global CSS treatment; per-module information architecture, shared component migration, interaction design, and illustration work remain.
- Prior browser screenshots ran without the Rust API, so they represent empty/degraded states rather than live data workflows.
- Subroutes and in-page states are not fully enumerated in the initial route inventory.
- Accessibility needs targeted validation for contrast, keyboard traversal, modal focus, and reduced-motion behavior.
- Handwritten type uses platform fallbacks to preserve Chinese readability and avoid adding a font payload.
