---
version: 1
slug: "apps-desktop-ui-src-features-home-homepage-tsx"
primary_target: "apps/desktop/ui/src/features/home/HomePage.tsx"
related_targets: ["apps/desktop/ui/src/App.tsx","apps/desktop/ui/src/features/ai/AIBubble.tsx","apps/desktop/ui/src/styles.css","apps/desktop/ui/src/main.tsx","apps/desktop/ui/src/theme/liquid-glass.css"]
---

# Surface scope

Mode: Operate. Primary surface: Home. Related surfaces: desktop application frame and the global AI launcher.

The likely user is one person returning to a personal knowledge workspace between short sessions. The job is to see what can be resumed, recover saved material, and open another tool without losing place. Use only existing content, actions, loading, empty, and error states. Preserve routes, feature behavior, data flow, theme selection, and API boundaries. This is FRONTEND ONLY; do not change other feature page content or backend behavior. Existing dirty files are user work and must be preserved.

## Direction contract

THESIS: Self Tools collects work across learning, reading, notes, travel, and AI; Home should make that growing personal archive easy to resume and retrieve.

OWN-WORLD: Use the logic of a personal reference index—stable groups, clear titles, concise context, and consistent rows—not literal library decoration. Keep the selected theme's opaque surfaces and semantic colors. One existing accent marks the current location and primary action; status colors carry meaning only. Use the existing Manrope family with a compact, readable scale. Remove hand-drawn motifs, module-by-module hues, decorative gradients, blur glass, and glowing capsules.

STORY: The person arrives, sees the current date and next useful action, resumes or reviews work, then scans saved knowledge, trips, reading, and notes. The assistant stays easy to reach without competing with the work.

FIRST VIEWPORT: On desktop, keep the current top bar and side navigation. Show the Home heading and primary resume action first; present streak and mastery as quiet supporting facts, then group existing modules into typographic sections and flat rows separated by subtle rules. Use one active-nav treatment across groups. The AI control remains in the top bar and opens the existing opaque assistant panel. On mobile, preserve bottom navigation, stack the same content, and keep the 48px AI control above the safe-area inset without horizontal scrolling.

FORM: Reference Index. Signature move: a shared title–context–action row rhythm across Home modules, with a single restrained accent for focus and selection; module identity comes from its actual text and icon rather than a separate color. Code-led. Direction seed reference: 0b359399ec67e7df7f6cf302e03a025a.

FINISH: Preserve real data states, routes, callbacks, keyboard access, visible focus, 44px-class mobile actions, and reduced motion. Review the final CSS cascade and rendered desktop/mobile surfaces. Update DESIGN.md. Do not create raster assets for this scope.
