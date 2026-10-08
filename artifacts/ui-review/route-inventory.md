# Route / UI Audit Inventory

Source of truth: `apps/desktop/ui/src/App.tsx` (`PAGE_IDS`, `NAV_GROUPS`, rendered `.page-pane` sections).

## Routes (16)

| Hash route | Label | Component |
|---|---|---|
| `#home` | Home | `features/home/HomePage.tsx` |
| `#review` | Review | `features/learning/ReviewCenterPage.tsx` |
| `#graph` | Graph | `features/learning/KnowledgeGraphPage.tsx` |
| `#collections` | Collections | `features/learning/CollectionsPage.tsx` |
| `#markdown` | Markdown | `features/markdown/MarkdownPage.tsx` |
| `#rss` | RSS | `features/rss/RssPage.tsx` |
| `#news` | News | `features/news/NewsPage.tsx` |
| `#travel` | Travel | `features/travel/TravelPage.tsx` |
| `#geography` | Geography | `features/geography/GeographyPage.tsx` |
| `#history` | History | `features/history/HistoryPage.tsx` |
| `#language` | Language | `features/language/LanguagePage.tsx` |
| `#knowledge` | Knowledge | `features/knowledge/KnowledgePage.tsx` |
| `#study-board` | Study | `features/study/StudyBoardPage.tsx` |
| `#server` | Server | `features/server/ServerPage.tsx` |
| `#system` | System / readiness | `features/system/SystemReadinessPage.tsx` |
| `#search` | Search | `features/system/GlobalSearchPage.tsx` |

Settings is a global dialog, not a hash route. Applications is a Server sub-area (not a separately registered route). Deep links also support `#study`, entity query parameters, and module-specific queries through `navigateToHash`.

## Component inventory

- Shell: `App.tsx` (top bar, grouped sidebar, responsive bottom nav, global search, settings, AI panel)
- Shared page scaffold: `components/PageShell.tsx`
- AI: `features/ai/AIPanel.tsx`, `features/ai/AIBubble.tsx`
- Sketch library: `components/sketch/SketchKit.tsx`, `SketchIllustrations.tsx`
- Page modules: History, Geography, Language/English, Study, News, RSS, Travel, Markdown, Collections, Knowledge/Graph, Search, Server/System.

## Style inventory

- Legacy/module styles: `src/styles.css` (existing feature CSS)
- Pencil tokens + global skin + Sketch components: `src/theme/pencil.css`
- Theme registry: `src/theme/themes.ts`, `src/theme/ThemeManager.ts`

## Migration notes

- All 16 registered route panes inherit the global paper/graphite/capability styling and the shared App Shell.
- Home is the reference implementation, including the AI bubble, Continue Learning, Explore, Fusion Workspace, reading context, notes, and recent exploration.
- Existing feature components and data workflows remain mounted and were not removed. Their visual migration is primarily through shared design tokens and global component skin; feature-specific markup/business logic was intentionally preserved.
- Browser UI review runs without the Rust API, so API connection warnings are expected in screenshots; route/layout assertions remain active.
