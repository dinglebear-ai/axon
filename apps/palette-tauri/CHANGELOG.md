# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.14.2](https://github.com/jmagar/axon/compare/palette-v5.14.1...palette-v5.14.2) (2026-07-15)


### Fixed

* **jobs:** remove legacy job families ([ba0b29b](https://github.com/jmagar/axon/commit/ba0b29b27119dc93de97edacbfdd6b6348d33771))

## [5.14.1](https://github.com/jmagar/axon/compare/palette-v5.14.0...palette-v5.14.1) (2026-07-14)


### Fixed

* **release:** sync component versions after release PRs ([4d023e7](https://github.com/jmagar/axon/commit/4d023e72b5951c7468c843a906ca9ceb10336a09))

## [5.14.0](https://github.com/jmagar/axon/compare/palette-v5.13.0...palette-v5.14.0) (2026-07-14)


### Added

* **#298:** post-smoke followups — scope=page, watch create, mutates_if, presentation tokens ([e01592f](https://github.com/jmagar/axon/commit/e01592ff278bcd5543924a9e87c2072d346d7878))
* **apps:** web token hardening, palette unified job polling, android memory/session client ([a17dc86](https://github.com/jmagar/axon/commit/a17dc864dafb67064819ea12c2ccdc004d01eec4))
* **mobile:** MobileSession status/source_refs/draft/sync_version + Android entity ([#298](https://github.com/jmagar/axon/issues/298) WS-I, bead .13) ([f3cbf9e](https://github.com/jmagar/axon/commit/f3cbf9eac6298df523da13142ad92102e47e920f))


### Fixed

* **palette:** migrate scrape/crawl/embed/ingest onto POST /v1/sources ([d74ed35](https://github.com/jmagar/axon/commit/d74ed35089a488ba9ccee72e934ebd89c0e5ce7a))
* **web:** distinct operation_id for POST /v1/graph/query ([#298](https://github.com/jmagar/axon/issues/298)) ([a379a59](https://github.com/jmagar/axon/commit/a379a598ec75dc2f93de90db64ac586fc570fcdf))

## [5.13.0](https://github.com/jmagar/axon/compare/palette-v5.12.4...palette-v5.13.0) (2026-07-09)


### Added

* **palette:** Files view split-pane, bulk ingest, AI-edit diff, SFTP browsing ([#393](https://github.com/jmagar/axon/issues/393)) ([f88914f](https://github.com/jmagar/axon/commit/f88914f2f349f1a33cb50a27005d874f96681a8d))
* **palette:** GitHub view Feed tab + two-pane split ([#394](https://github.com/jmagar/axon/issues/394)) ([b4ae3d9](https://github.com/jmagar/axon/commit/b4ae3d9004a41cdb74ced7998f77bf8daea888b1))

## [5.12.4] - 2026-07-05

### Changed

- Regenerate Palette API bindings for source pipeline contract alignment.

## [5.12.3] - 2026-06-29

### Fixed

- Prevent settings tabs, fields, and auth controls from clipping in narrow palette layouts.

## [5.12.2] - 2026-06-28

### Fixed

- Strip origin in dev proxy

## [5.12.1] - 2026-06-28

## [5.12.0] - 2026-06-26

### Added

- Sync job views, live-refresh, structured views, typed builders

## [5.11.4] - 2026-06-25

### Fixed

- Guard destructive actions
- Satisfy sqlite hardening release gates

## [5.11.3] - 2026-06-25

### Fixed

- Satisfy sqlite hardening release gates

## [5.11.2] - 2026-06-25

## [5.11.1] - 2026-06-24

### Changed

- Align REST transport request defaults and generated client contract updates.

## [5.11.0] - 2026-06-21

### Added

- OAuth 2.0 "Sign in with Google" — Authorization Code + PKCE with a loopback
  redirect and dynamic client registration, run entirely in the Rust shell and
  coexisting with the existing static bearer token. Includes reactive 401
  refresh, secure token storage (`oauth.json`, mode 0o600), a signed-out notice,
  and shell diagnostics logged to `~/.axon/logs/palette.log`.

## [5.10.5] - 2026-06-21

### Added

- Add per-component changelogs and register them in release manifest

## [5.10.4] - 2026-06-20

### Fixed

- Add qdrant url purge and refresh ci artifacts
- Address openapi client review issues
- Keep selected action-row glow from clipping at panel edge

## [5.10.2] - 2026-06-16

### Fixed

- Resync Aurora Input warnings

## [5.10.1] - 2026-06-16

### Changed

- Model view as a reducer; dissolve setter drilling (A-M1/A-M2)

## [5.10.0] - 2026-06-16

### Added

- Add Tauri palette and harden search crawl (#136)
- Add openai-compatible backend and palette polish
- Stream ask responses
- Pager shell + FAB mode selector + in-app document view
- Dinglebear-style footer, slim rows, hide titlebar
- Pager + FAB shell, operation mode expansion, form-keys package — v4.12.0
- Pager shell + FAB mode selector + in-app document view — v4.12.2
- Integrate mock alignment shell
- Live crawl job view backed by a real crawl event stream
- Pulse the live-crawl status dots while a crawl runs
- Show selected action's icon in the input instead of a mode badge (click icon or Esc to clear)
- Parsed stats/status views, evaluate side-by-side (baseline vs RAG), instant-launch no-input actions
- Self-host Aurora fonts
- Add action switcher

### Changed

- Simplify streaming follow-up
- Split App.tsx under the 500-line monolith cap
- Re-sync Aurora primitives from corrected registry; thin token override
- Route raw <button>s through the Aurora Button primitive
- Drop dead Badge/Separator primitives; defer input/kbd migration
- Migrate inputs/kbd onto Input/Kbd unstyled primitive

### Fixed

- Address PR feedback for palette blur setting
- Polish palette commands and qdrant quantization
- Omit collection from summarize requests
- Constrain native axon bridge
- Send target for github ingest
- Honor collection env default
- Show async jobs as queued
- Surface settings read failures
- Restore command field layout
- Log tray window operation failures
- Harden config fallback and ingest
- Harden ask streaming lifecycle
- Increase reqwest client timeout 120s → 300s to survive Gemini synthesis
- Compact output UI + map normalize_url
- Blur-to-hide, accurate action matching, dynamic window height
- Window height fits content using screen.availHeight, restore scroll
- Accessibility + accent-swap review fixes
- Simplify + token-ify + a11y from review waves
- Blend the collapsed crawl tray into the command bar
- Point axonClient test mock at the ./invoke wrapper
- Size the browse window to its content, not a per-item formula
- Rank suggestions by match quality + stop redundant resizes
- UI polish — fill window (no white corners), focus no longer expands, mode hides suggestions, flush footer, restyled mode pill, drop dup result row + brand tooltip
- Bigger resizable result window, double-click maximize, no hide-while-reviewing; fix results stuck at 56px on reopen (strip)
- Generate Streamdown's Tailwind utilities (scan node_modules/streamdown)
- Scroll the action list to keep the keyboard selection in view
- Address all review findings from issue #177 (#201)
- Polish operation result rendering
- Refine operation reader highlighting
- Tighten operation result polish
- Show connection test feedback
- Tighten result panel height
- Render normalized ask answers
- Use convertFileSrc for screenshot preview to satisfy Tauri CSP
- Remediate UI/UX review findings (a11y, perf, consolidation)
- Address PR review findings (keydown rebind, lazy error boundary, flaky guard)
- Resolve palette audit alerts
