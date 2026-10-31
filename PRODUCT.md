# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Likely a single person using a personal knowledge workspace across desktop and mobile browser surfaces. The README describes the product as a personal workspace; the exact primary user profile remains unconfirmed.

## Product Purpose

Bring AI conversations, personal files and documents, subscribed reading, history and geography exploration, language learning, and study tools into one interface. The intended outcome is that personal information can accumulate and be found again.

## Positioning

A broad personal workspace that combines locally based data and multiple learning and knowledge tools, with AI able to use registered knowledge tools and page context. This synthesis is inferred from the README and current app structure.

## Operating Context

The product runs as a Tauri desktop application and also provides PWA resources for mobile and tablet layouts. Some capabilities depend on user-configured AI, maps, search, weather, speech, and remote identity services; local features remain usable without those services.

## Capabilities and Constraints

- The current interface has 16 route-backed feature pages; this redesign stage covers only the desktop app frame, Home, and global AI floating entry.
- Preserve current routes, feature behavior, data flow, local persistence, and settings.
- UI work is FRONTEND ONLY. The frontend communicates through existing clients and stable contracts; provider secrets stay on the backend.
- Existing application data and feature pages outside the named scope must remain intact.

## Brand Commitments

The user's 2026-10-31 feedback supersedes the earlier bright, frosted-glass direction: the application frame, Home, and global AI entry should read as a mature, text-first personal archive. Use opaque surfaces from the active theme, clear hierarchy, restrained accent use, and concise navigation. Remove hand-drawn stickers, per-module hue coding, decorative gradients and blur, and glowing capsule styling. Preserve the selected light/dark theme, all routes and feature behavior, existing data flows, and API boundaries.

## Evidence on Hand

- Product description and capability summary: `README.md`.
- Current route and workspace structure: `apps/desktop/ui/src/App.tsx`.
- Current Home content and data states: `apps/desktop/ui/src/features/home/HomePage.tsx`.
- Available screenshot `artifacts/ui-review/desktop/home.png` may be stale; freshness is unverified.
- No confirmed product logo or external imagery was established during this task.

## Product Principles

- Keep personal progress and saved information recoverable from the existing product data sources.
- Make it easy to resume work across the app's modules.
- Keep AI access contextual while preserving user control over actions and memory writes.
- Do not expose provider secrets in client UI or client configuration.

## Accessibility & Inclusion

No product-specific requirements were supplied. Preserve semantic controls, keyboard access, visible focus, readable contrast, responsive layouts, and reduced-motion support.

## Open Decisions

- The exact target user profile and which Home modules should have highest priority were not confirmed. For this implementation, infer priority from current Home behavior and preserve its existing content and actions.
- The preferred theme default is not confirmed. Keep the existing theme selection behavior.
