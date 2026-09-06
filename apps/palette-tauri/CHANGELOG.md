# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [6.2.0](https://github.com/dinglebear-ai/axon/compare/palette-v6.1.0...palette-v6.2.0) (2026-08-31)


### Added

* add full Codex app-server control to Palette ([#594](https://github.com/dinglebear-ai/axon/issues/594)) ([b3d8b62](https://github.com/dinglebear-ai/axon/commit/b3d8b622eac62634ba525799f982d67978aaecdd))

## [6.1.0](https://github.com/dinglebear-ai/axon/compare/palette-v6.0.0...palette-v6.1.0) (2026-08-21)


### Added

* add artifact candidate crawl and enrichment pipeline ([#569](https://github.com/dinglebear-ai/axon/issues/569)) ([4d3d9a4](https://github.com/dinglebear-ai/axon/commit/4d3d9a4d895695e4280cb81ad0177545e5f19ea2))
* **palette:** ship Tauri Android support ([#571](https://github.com/dinglebear-ai/axon/issues/571)) ([ba2ba27](https://github.com/dinglebear-ai/axon/commit/ba2ba27d4354c1d484eb2f6a437ae95a35210ade))


### Changed

* harden and accelerate unified source pipeline ([#570](https://github.com/dinglebear-ai/axon/issues/570)) ([ec8ef7f](https://github.com/dinglebear-ai/axon/commit/ec8ef7fa463de019f285e50fce8d22c9df19b376))

## [6.0.0](https://github.com/dinglebear-ai/axon/compare/palette-v5.14.5...palette-v6.0.0) (2026-08-09)


### ⚠ BREAKING CHANGES

* artifact-bearing responses return opaque artifact IDs instead of server filesystem paths, and web source options use the canonical `headers` object shape.

### Added

* **#298:** post-smoke followups — scope=page, watch create, mutates_if, presentation tokens ([d05eab3](https://github.com/dinglebear-ai/axon/commit/d05eab3302a0427d931fe7a96170f07bf9d7ea02))
* **apps:** web token hardening, palette unified job polling, android memory/session client ([238ead7](https://github.com/dinglebear-ai/axon/commit/238ead7d924e79d49cf2f46e946b2fe1fa1934a8))
* complete [#298](https://github.com/dinglebear-ai/axon/issues/298) closeout wave with structured source progress ([5bb4395](https://github.com/dinglebear-ai/axon/commit/5bb4395ede88b0f9ae6ae186d835bb3fd6787daa))
* complete pipeline unification closeout ([b987c1b](https://github.com/dinglebear-ai/axon/commit/b987c1b41319a8f0ed06e778ab5f1f27c0053d40))
* **mobile:** MobileSession status/source_refs/draft/sync_version + Android entity ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-I, bead .13) ([c08068a](https://github.com/dinglebear-ai/axon/commit/c08068aeb8241c56a121afe0343b638d48b2b04c))
* **palette:** Files view split-pane, bulk ingest, AI-edit diff, SFTP browsing ([#393](https://github.com/dinglebear-ai/axon/issues/393)) ([f0edee2](https://github.com/dinglebear-ai/axon/commit/f0edee275d48bd2683db57427dd1f3a33693272d))
* **palette:** GitHub view Feed tab + two-pane split ([#394](https://github.com/dinglebear-ai/axon/issues/394)) ([9e18545](https://github.com/dinglebear-ai/axon/commit/9e185454e7c84f6d47504ea7a4fb96fef4a9b070))


### Fixed

* address phase 1 review feedback ([1b65f4a](https://github.com/dinglebear-ai/axon/commit/1b65f4a8453188bca553e1a8999f20c43ac10f81))
* **deps:** resolve Dependabot advisories ([#540](https://github.com/dinglebear-ai/axon/issues/540)) ([f3872c3](https://github.com/dinglebear-ai/axon/commit/f3872c3cfa121072212ba2a68cf22580f765bb6c))
* harden live CLI and source pipeline workflows ([#491](https://github.com/dinglebear-ai/axon/issues/491)) ([d1bca00](https://github.com/dinglebear-ai/axon/commit/d1bca00dd6f6b20ecae670c2a0222d77ae2c670f))
* **jobs:** remove legacy job families ([ae2fa82](https://github.com/dinglebear-ai/axon/commit/ae2fa828e86099945e2f352faba30ac992a120c3))
* **palette:** migrate scrape/crawl/embed/ingest onto POST /v1/sources ([040e84f](https://github.com/dinglebear-ai/axon/commit/040e84f4404646783adf974aff12b9e51af6f67a))
* **release:** repair artifact build contracts ([ec5d6ac](https://github.com/dinglebear-ai/axon/commit/ec5d6ac883c294c4f5bc0b78fbfbf852ba27c656))
* **release:** sync component versions after release PRs ([f7d0cfc](https://github.com/dinglebear-ai/axon/commit/f7d0cfc79572e4bd8ec1ce5d3a3e9501005c2133))
* **review:** close source watch review gaps ([dec600e](https://github.com/dinglebear-ai/axon/commit/dec600e3a94d00c0fa6a5e341f654300ca30c26c))
* **web:** distinct operation_id for POST /v1/graph/query ([#298](https://github.com/dinglebear-ai/axon/issues/298)) ([ef56dc9](https://github.com/dinglebear-ai/axon/commit/ef56dc9e3b493fda31115f4694c008d495cde788))


### Changed

* split closeout monoliths ([1f50180](https://github.com/dinglebear-ai/axon/commit/1f501801cb93cc6a15e3415e1d7b83678ade8499))

## [5.14.5] - 2026-08-01

### Changed

- Align generated API typings with the finalized CLI live-remediation contracts.

## [5.14.4] - 2026-07-30

### Changed

- Align Palette source files with the fleet module and workspace conventions.

## [5.14.3](https://github.com/jmagar/axon/compare/palette-v5.14.2...palette-v5.14.3) (2026-07-17)

### Changed

* consume opaque artifact identifiers and canonical action/resource output from the unified pipeline (#298 closeout) ([#442](https://github.com/jmagar/axon/pull/442))

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
