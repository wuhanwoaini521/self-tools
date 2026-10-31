# Sprint 11 — Release-Candidate Validation

**Date:** 2026-10-29  
**Decision: NO-GO for distribution.** A macOS release-mode `.app` was built and launched with isolated data, and the product/test suites are broadly green. It is not a distributable RC: the app is unsigned and Gatekeeper rejects it, there is no signing identity, Tauri’s DMG packaging failed during Finder customization, the exact release UI was not interactively verified, and the dependency audit found unresolved advisories.

No files were staged; no commit, tag, push, publish, merge, or deployment occurred. No production or existing ignored app data was read or changed. All launch, migration, backup, and restore fixtures used temporary directories.

## 1. Traceable source baseline

- Git baseline: `main` at `f58efa28734208ea195d586d6b52b49a97f510a7`.
- Candidate ID: `5f184a1125597edf`.
- Standalone source archive: [`rc-source-snapshot.tar.gz`](rc-source-snapshot.tar.gz), SHA-256 `5f184a1125597edf21ec409a3ee0afd4abfe6a2d94457b9fd7ea63350004e8b3`.
- File/path list, per-file hashes, lockfile hashes, History submodule commit, excluded roots, and build-tool versions: [`source-manifest.json`](source-manifest.json).
- The archive overlays the dirty tracked source and non-artifact untracked source. It deliberately excludes all `artifacts/`, ignored config/data, `target/`, `node_modules/`, generated UI/browser QA output, and private/local mise config. It includes the immutable History DuckDB resource. The exact worktree diff hash in the manifest also covers excluded artifact changes; the raw diff and ignored data are not included.
- At capture the worktree remained broadly dirty (124 tracked modifications, one deletion, 53 untracked status entries, zero staged). The report and source archive add untracked Sprint 11 evidence after that count.
- `apps/desktop/ui/src/features/geography/AmapMap.tsx` remains deleted. No restoration or approval is implied. The current Geography UI uses MapLibre-based components and no live references to `AmapMap` were found; the deletion’s approval/replacement history remains unresolved.

## 2. Readiness matrix

| Gate | Status | Evidence / remaining limit |
|---|---|---|
| Source identity | **PASS** | Standalone, hashed archive and manifest above. |
| Production UI build | **PASS** | `npm run build`; typecheck and Vite production bundle succeeded. Bundle still warns about chunks above 500 KB. |
| macOS release `.app` build | **PASS (local, unsigned)** | Tauri release bundle built for macOS 12.7.6 x86_64. App bundle is ~86 MB. Executable SHA-256: `4b8b624aa74ffe3f23a6f39e3e3eaf331eb29002b3527b2ddc9aa495d12ad11d`. |
| Bundled History data | **PASS** | `Contents/Resources/history-data-pipeline/dist/history.duckdb` is present and its SHA-256 matches source asset `0b355639c3d0adc5cbc393464f58944a0e88009eed0c7636a1db2204f6353eb0`. Startup did not report a missing/unreadable History resource. Repository opens History read-only. |
| Release app startup/data isolation | **PASS (startup only)** | The exact release executable stayed running and initialized 13 stores under a temporary `SELF_TOOLS_HOME/config`; no real app config was used. The host’s visible session was locked, so app-window rendering and manual interaction were not observed. |
| Legacy RSS→News migration | **PASS (isolated release launch)** | Seeded a temporary legacy `dashboard.db` with 3 feeds/4 articles (one `kind='news'` source). Release startup retained all RSS rows, moved one source/two articles (one starred) into `news.db`, removed `feeds.kind`, and wrote the migration marker. Fixture and output stayed under `/tmp`. |
| Learning/Travel schema migration | **PASS (tests)** | Workspace tests include `old_review_card_schema_migrates_without_losing_existing_cards` and `older_plan_schema_migrates_without_losing_saved_itinerary`. |
| Backup/restore | **PARTIAL** | New isolated integration drill snapshots real `LearningStore` and `TravelStore` SQLite databases, clears only fixture sources, restores to a separate directory, checks SQLite integrity, and reopens Knowledge collection/note, review card/SRS progress, and Travel itinerary/custom-place data. It observed Learning `journal_mode=WAL` and Travel `DELETE`. The manifest intentionally contained only `learning.db.bak` and `travel.db.bak`; settings/credentials, language media, other stores, and an app-level restore flow were not covered. BackupService is not wired into the desktop app/API, so this is not end-user backup acceptance. |
| Rust product/workspace tests | **PASS** | `cargo test --workspace --locked`: **1,337 passed, 0 failed**, including the release recovery drill. |
| UI unit tests | **PASS** | **21 files, 125 tests passed.** |
| Browser/viewport QA | **PASS (presentation only)** | **90 rendered states across 5 viewports.** The browser harness ran without the Rust API; Vite logged expected `127.0.0.1:8080` connection refusals. This is not API integration evidence. |
| Native macOS Tauri E2E | **PASS (instrumented test build)** | **5 spec files, 17 tests passed** on macOS/WebKit 605.1.15. This build includes the test-only WebDriver plugin and is not the release binary. A first attempt hit a missing output directory in the disposable snapshot; after creating that fixture directory, the full rerun passed. WDIO emitted non-failing IPC/cleanup warnings. |
| Exact release UI automation | **NOT VERIFIED** | The production release binary intentionally lacks the test-only embedded WebDriver plugin; attaching WDIO to it timed out. The locked host session also prevented visible manual acceptance. Do not equate the instrumented E2E pass with release-binary UI acceptance. |
| Formatting | **PASS** | `cargo fmt --all -- --check` and focused test-file formatting passed. |
| Clippy | **FAIL** | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` reports 3 lints in existing dirty source: needless lifetimes in `crates/core/src/exploration.rs` (two sites) and collapsible `if` in `crates/core/src/travel/plan.rs`. These files were not changed to avoid altering unrelated user work. |
| JavaScript dependency audit | **MIXED** | Full and production `apps/desktop/ui` audits report 0 advisories. `apps/desktop` test tooling reports **21 high / 2 moderate** advisories in WDIO/WebDriver and transitive development dependencies; they are not in the UI production dependency set, but remain a CI/build-tool supply-chain concern. Fixes reported by npm require a WDIO major upgrade. |
| Rust dependency audit | **FAIL / review required** | `cargo audit` over `Cargo.lock` reports **17 advisory matches**, plus 9 unmaintained and 2 unsound notices. Findings include `quick-xml 0.37.5`, `rustls 0.23.43`/`rustls-webpki 0.103.15`/`webpki-roots 1.0.9`, and `zip 2.4.2`/`6.0.0`; 3 `quinn-proto` advisories are not in the current x86_64 macOS dependency tree. Code-path review narrows, but does not clear, findings: rustls is reached through HTTP clients; the app’s quick-xml code uses plain `Reader` (not `NsReader`); the direct ZIP importer reads members rather than calling filesystem extraction. The lockfile still fails audit and needs updates or formal risk acceptance. |
| Signing/notarization/installer | **FAIL** | No valid code-signing identities were available. The built `.app` is unsigned; `codesign --verify` reports no signature and Gatekeeper assessment rejects it. Tauri’s `.dmg` bundler created/mounted a temporary image but failed when its Finder AppleScript timed out (`AppleEvent -1712`); no DMG was produced. Notarization was not possible. |
| Toolchain reproducibility | **PARTIAL** | Build used macOS 12.7.6 x86_64, Rust/Cargo 1.99.1, Node 24.21.0, npm 11.19.0, Tauri CLI 2.11.4. The checked-in `mise.lock` is version 3 but installed mise 2026.9.12 only supports version 2; it pins Node 24.22.0, unlike the local 24.21.0. The release build succeeds directly, but the project toolchain lock is not reproducible in this environment. |
| Live-provider acceptance | **NOT TESTED** | Provider was unconfigured in the prior baseline (`configured: false`). Inference, cancellation against a live provider, model-generated citations, and AI-generated Travel proposal acceptance remain unverified. |
| Other platforms/architectures | **NOT TESTED** | This validation covers only a local x86_64 macOS/WebKit environment; no Apple Silicon, Windows, or Linux RC run was performed. |

## 3. Build and runtime evidence

The final source archive was extracted to the path recorded in the manifest. Dependencies were installed with `npm ci` from each package directory. The release app was built with Tauri CLI 2.11.4 using the production Tauri configuration and `--no-sign`; `CARGO_TARGET_DIR` pointed to an isolated temporary build directory. The resulting Mach-O is x86_64, version `0.1.0`, and the bundle reports a minimum macOS version of 10.13. This is a local engineering build, not a signed release artifact.

The app was launched directly with `SELF_TOOLS_HOME` pointing at a fresh temporary root. It remained alive, initialized its app stores there, and emitted no History resource error. A separate launch against a temporary legacy RSS fixture verified the migration above. Both temporary roots were isolated; neither used the user’s normal Tauri Application Support directory.

## 4. Security and data boundary notes

- Tauri’s default capability grants `core:default`, `dialog:default`, and `opener:allow-open-url`; no shell permission was found. The configured CSP still permits inline script/style and `http:` images alongside AMap origins; tighten/justify these before distribution.
- The provider status remains unconfigured; no live credentials or provider requests were used.
- A possible migration from any pre-existing project-relative `config/` data to the new release `app_config_dir` was not exercised. Existing ignored user data was deliberately not inspected or copied. Before adoption, decide and test a safe, explicit migration/backup path rather than assuming that data is absent.
- The restore drill does not establish a complete user-facing backup product. In particular, no desktop route/composition registers `BackupService`, and credentials/settings/media are outside this drill.

## 5. RC decision and release blockers

**NO-GO.** Keep this candidate internal. Re-open RC review only after:

1. Resolve the Rust advisory set (or document explicit, reviewed exceptions), address the vulnerable Rustls path, and update/audit locked dependencies. Upgrade or formally risk-accept vulnerable WDIO test tooling.
2. Pin a supported, matching mise/Node/Rust toolchain and rerun from a clean lockfile install.
3. Provide a Developer ID signing identity, sign/notarize/staple the app, produce and verify the intended installer, and test Gatekeeper acceptance. Diagnose the Finder-dependent DMG failure in a supported GUI build environment.
4. Decide the old project-relative data migration and verify it with a disposable legacy fixture without touching real user data.
5. Define/implement the backup scope and an app-level restore acceptance path, or explicitly remove backup completeness from this RC claim.
6. Run exact release-binary UI acceptance in an unlocked macOS GUI session; retain native test evidence but do not substitute the WebDriver-enabled test build.
7. Clear the three Clippy failures without overwriting unrelated work, then rerun the quality gate.
8. If AI is part of the release claim, configure a provider in a separate approved test environment and run live inference/cancellation/citation/Travel confirmation acceptance.

## 6. Worktree protection / exclusions

`AmapMap.tsx` was not restored or staged. No reset, clean, stash, checkout, stage, commit, tag, push, merge, or deploy occurred. Build/test dependencies, outputs, databases, backup fixtures, and app config were kept in the extracted source snapshot or temporary paths. The source archive and report are evidence artifacts only; no existing ignored config/database contents are included.
