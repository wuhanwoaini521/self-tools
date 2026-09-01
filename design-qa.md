# Home Story to Study Design QA

## Evidence

- Source visual truth: `output/playwright/reference-story-to-study.png` (selected ImageGen result, Story to Study direction)
- Implementation screenshot: `output/playwright/home-story-to-study-final.png`
- Side-by-side comparison: `output/playwright/story-to-study-comparison.png`
- Viewport: 1440 x 1024 CSS px
- Source pixels: 1487 x 1058; implementation pixels: 1440 x 1024; the comparison page scales both images to a common column width while preserving aspect ratio.
- State: Home page, dark/default theme, browser preview without Tauri data stores; implementation intentionally shows the graceful empty RSS/history state.
- Focused comparison: the main article panel, place panel, language panel, history context, and activity sections were checked together in the side-by-side comparison.

## Findings

No actionable P0/P1/P2 differences remain.

- The existing top app bar and navigation remain in place as an intentional product-shell constraint; the selected visual target's alternate sidebar is not copied into the application shell.
- The implementation uses the existing Manrope font, Phosphor icons, design tokens, and the repository's Natural Earth asset for the place panel.
- The browser preview's empty RSS/history content is an expected non-Tauri state; Tauri runtime data populates these sections through the existing commands.

## Interaction checks

- Geography place link navigates to the existing Geography page.
- History context link navigates to the existing History page.
- Language practice link navigates to the existing Language page.
- New note and article actions retain the existing Markdown/RSS intents.
- Desktop layout was captured at 1440 x 1024; responsive rules were added for 1050px and 720px breakpoints.
- Implementation console checked: no application errors; the comparison wrapper had only an expected missing favicon request.

## Comparison history

1. Initial implementation used the global blue accent. The Home scope was updated with teal/amber local tokens to align with the selected visual target.
2. Final implementation screenshot was recaptured after the token adjustment; no P0/P1/P2 findings remained.

## Implementation Checklist

- [x] Story-led Home hierarchy
- [x] RSS article entry point
- [x] Geography place context and map asset
- [x] History context entry point
- [x] Language practice panel
- [x] Recent notes and recent exploration sections
- [x] Responsive layout and empty states
- [x] Build and type-check
- [x] Browser interaction smoke test

## Follow-up Polish

- Add a backend `HomeSnapshot` aggregate when cross-domain entity matching is ready, so RSS-to-place/history/language associations are source-backed instead of independently recommended.
- Add a dedicated Language home intent for opening a specific word directly from Home when a review card is available.

final result: passed
