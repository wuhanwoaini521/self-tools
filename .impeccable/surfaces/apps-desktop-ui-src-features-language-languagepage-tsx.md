---
version: 1
slug: "apps-desktop-ui-src-features-language-languagepage-tsx"
primary_target: "apps/desktop/ui/src/features/language/LanguagePage.tsx"
related_targets: ["apps/desktop/ui/src/features/language/english/EnglishSection.tsx", "apps/desktop/ui/src/features/language/english/EnglishHome.tsx", "apps/desktop/ui/src/features/home/HomePage.tsx", "apps/desktop/ui/src/theme/liquid-glass.css", "apps/desktop/ui/src/styles.css", "apps/desktop/ui/src/theme/pencil.css", "DESIGN.md"]
---

# Surface scope

Mode: Operate. Primary surface: the Language module's English learning workspace. Related surfaces: shared application frame and Home.

The user studies English privately at home on a desktop and Android phone. The job is to choose a short practice, work through travel situations and course material, then recover durable progress on another visit. Preserve existing routes, controls, course flows, theme selection, accessibility, API clients, and data boundaries. Scope is FRONTEND ONLY; do not change backend behavior or API contracts. Preserve all unrelated worktree changes.

## Direction contract

THESIS: Make the existing personal workspace feel lighter and more inviting while keeping each learning action easy to find and resume.

OWN-WORLD: Follow the user's pinned reference: a luminous field moving from sky blue through violet into apricot and soft yellow; translucent glass surfaces with crisp bright edges, quiet depth, and clear graphite text. Let the reference's color flow live in the shell and a small number of focus surfaces; keep long reading areas calm and high contrast. Preserve the active theme's semantics and dark appearance. Keep the existing navigation, familiar controls, Manrope typography, and status colors for their meanings. Do not add illustration or decorative motion.

STORY: The user enters through the existing Language route, chooses the next English practice, opens a travel scene, word, grammar lesson, or reading, and returns to backend-backed learning progress. Existing New Concept English, review, and plan flows remain available.

FIRST VIEWPORT: On desktop, retain the top bar and side navigation, with the Language page aligned to the shared content grid and its English entry clear. On mobile, retain the established bottom navigation, stack lesson content, and keep every primary action within the viewport without horizontal scrolling. Use the same route and controls in both layouts.

FORM: User-pinned sunrise glass workspace. Signature move: soft blue-violet and apricot light visible through a few translucent surfaces against the app canvas. Code-led. Direction seed reference: 121a4598fe4dccd5296f225277f30f68.

FINISH: Preserve real data states, routes, callbacks, keyboard access, visible focus, 44px-class mobile actions, and reduced motion. Review the final CSS cascade and rendered desktop/mobile surfaces. Update DESIGN.md. Do not create raster assets for this scope.

IMPLEMENTED (2026-10-31): aurora field and glass chrome in `apps/desktop/ui/src/theme/sunrise-glass.css` (imported last by `main.tsx`); default palette in `theme/pencil.css`; Travel English content view in `features/language/english/TravelEnglishSection.tsx` with its entry in `EnglishHome.tsx` (no-course, imported-course, and error states) and `EnglishSection.tsx`; static material in `travelContent.ts`. Verified with `tsc`, `vitest`, `vite build`, and browser screenshots at 1440×900 / 390×844 (light and Nord dark).
