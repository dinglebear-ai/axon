# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0](https://github.com/dinglebear-ai/axon/compare/android-v1.6.4...android-v2.0.0) (2026-08-08)


### ⚠ BREAKING CHANGES

* artifact-bearing responses return opaque artifact IDs instead of server filesystem paths, and web source options use the canonical `headers` object shape.

### Added

* **#298:** post-smoke followups — scope=page, watch create, mutates_if, presentation tokens ([d05eab3](https://github.com/dinglebear-ai/axon/commit/d05eab3302a0427d931fe7a96170f07bf9d7ea02))
* **android:** memories client methods + route-contract fixture fix ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-I) ([d05addf](https://github.com/dinglebear-ai/axon/commit/d05addf08d0d28f9ed19bfc63e907dcf71a2bf9f))
* **apps:** web token hardening, palette unified job polling, android memory/session client ([238ead7](https://github.com/dinglebear-ai/axon/commit/238ead7d924e79d49cf2f46e946b2fe1fa1934a8))
* complete [#298](https://github.com/dinglebear-ai/axon/issues/298) closeout wave with structured source progress ([5bb4395](https://github.com/dinglebear-ai/axon/commit/5bb4395ede88b0f9ae6ae186d835bb3fd6787daa))
* complete pipeline unification closeout ([b987c1b](https://github.com/dinglebear-ai/axon/commit/b987c1b41319a8f0ed06e778ab5f1f27c0053d40))
* **mobile:** MobileSession status/source_refs/draft/sync_version + Android entity ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-I, bead .13) ([c08068a](https://github.com/dinglebear-ai/axon/commit/c08068aeb8241c56a121afe0343b638d48b2b04c))


### Fixed

* address phase 1 review feedback ([1b65f4a](https://github.com/dinglebear-ai/axon/commit/1b65f4a8453188bca553e1a8999f20c43ac10f81))
* **android:** migrate legacy verb routes onto POST /v1/sources ([a204e3e](https://github.com/dinglebear-ai/axon/commit/a204e3e34347d6c8ff017fcacca868ecf953fe36))
* **ci:** prevent Android packaging heap exhaustion ([#509](https://github.com/dinglebear-ai/axon/issues/509)) ([ae34209](https://github.com/dinglebear-ai/axon/commit/ae34209ace3e93c1686c7b1ba71fd5cc2e4d6868))
* harden live CLI and source pipeline workflows ([#491](https://github.com/dinglebear-ai/axon/issues/491)) ([d1bca00](https://github.com/dinglebear-ai/axon/commit/d1bca00dd6f6b20ecae670c2a0222d77ae2c670f))
* **release:** sync component versions after release PRs ([f7d0cfc](https://github.com/dinglebear-ai/axon/commit/f7d0cfc79572e4bd8ec1ce5d3a3e9501005c2133))
* **review:** close source watch review gaps ([dec600e](https://github.com/dinglebear-ai/axon/commit/dec600e3a94d00c0fa6a5e341f654300ca30c26c))
* **web:** distinct operation_id for POST /v1/graph/query ([#298](https://github.com/dinglebear-ai/axon/issues/298)) ([ef56dc9](https://github.com/dinglebear-ai/axon/commit/ef56dc9e3b493fda31115f4694c008d495cde788))


### Changed

* **android:** core/feature module package split + monolith fixes ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-I, U3-06) ([fa5f4e7](https://github.com/dinglebear-ai/axon/commit/fa5f4e7f3459dcebe678640408f58131c95493fb))
* split closeout monoliths ([1f50180](https://github.com/dinglebear-ai/axon/commit/1f501801cb93cc6a15e3415e1d7b83678ade8499))

## [1.6.4] - 2026-08-03

### Changed

- Raise the Gradle heap ceiling for reliable debug APK packaging while retaining bounded worker concurrency.

## [1.6.3] - 2026-08-01

### Changed

- Bound Gradle heap and worker concurrency for reliable CI and release builds.

## [1.6.2](https://github.com/jmagar/axon/compare/android-v1.6.1...android-v1.6.2) (2026-07-17)

### Changed

* adopt canonical web `headers` options and source-indexing terminology for the unified pipeline (#298 closeout) ([#442](https://github.com/jmagar/axon/pull/442))

## [1.6.1](https://github.com/jmagar/axon/compare/android-v1.6.0...android-v1.6.1) (2026-07-14)


### Fixed

* **release:** sync component versions after release PRs ([4d023e7](https://github.com/jmagar/axon/commit/4d023e72b5951c7468c843a906ca9ceb10336a09))

## [1.6.0](https://github.com/jmagar/axon/compare/android-v1.5.1...android-v1.6.0) (2026-07-11)


### Added

* **#298:** post-smoke followups — scope=page, watch create, mutates_if, presentation tokens ([e01592f](https://github.com/jmagar/axon/commit/e01592ff278bcd5543924a9e87c2072d346d7878))
* **android:** memories client methods + route-contract fixture fix ([#298](https://github.com/jmagar/axon/issues/298) WS-I) ([47b7ca8](https://github.com/jmagar/axon/commit/47b7ca8d7e54d4d6a8aca3c63a5fe704683d97af))
* **apps:** web token hardening, palette unified job polling, android memory/session client ([a17dc86](https://github.com/jmagar/axon/commit/a17dc864dafb67064819ea12c2ccdc004d01eec4))
* **mobile:** MobileSession status/source_refs/draft/sync_version + Android entity ([#298](https://github.com/jmagar/axon/issues/298) WS-I, bead .13) ([f3cbf9e](https://github.com/jmagar/axon/commit/f3cbf9eac6298df523da13142ad92102e47e920f))


### Fixed

* **android:** migrate legacy verb routes onto POST /v1/sources ([1d423c9](https://github.com/jmagar/axon/commit/1d423c95f4e0dd86983e2da7b743b89e3c96eb8f))
* **web:** distinct operation_id for POST /v1/graph/query ([#298](https://github.com/jmagar/axon/issues/298)) ([a379a59](https://github.com/jmagar/axon/commit/a379a598ec75dc2f93de90db64ac586fc570fcdf))


### Changed

* **android:** core/feature module package split + monolith fixes ([#298](https://github.com/jmagar/axon/issues/298) WS-I, U3-06) ([c700ebc](https://github.com/jmagar/axon/commit/c700ebc4816ac81434e8fbf175b148cbd099409f))

## [1.5.1] - 2026-07-05

### Changed

- Regenerate Android API contract fixtures for source pipeline contract alignment.

## [1.5.0] - 2026-06-29

### Changed

- Polish mobile Ask, FAB, Jobs, Sessions, Settings, and status recovery surfaces with clearer hierarchy, actions, and elevation.
- Share compact Android action button styling across settings and status recovery controls.
- Make OAuth the recommended Android setup flow so first-run users can enter a server URL, complete browser sign-in, and stay logged in.
- Add a Recent Activity surface that merges locally submitted jobs with live job status and opens job details.

### Fixed

- Report clipboard copy failures instead of showing false success for status recovery diagnostics.
- Clarify setup health-check copy so it does not overstate auth or collection validation.
- Keep accepted async injection jobs pending until they report a final successful status.
- Surface OAuth launch failures and partial Jobs refresh failures to users.

## [1.4.4] - 2026-06-28

## [1.4.3] - 2026-06-26

### Fixed

- Type JobProgress wire contract, secure /v1/purge, address PR review findings

## [1.4.2] - 2026-06-25

## [1.4.1] - 2026-06-24

### Changed

- Align mobile request defaults and generated client contract handling with server transport policy.

## [1.4.0] - 2026-06-21

### Added

- Add oauth sign-in and mobile sessions
- Add per-component changelogs and register them in release manifest

### Changed

- Route android collections through generated adapter

### Fixed

- Harden oauth and session sync
- Align oauth scopes and settings auth states
- Polish mobile screen states
- Complete oauth token exchange
- Clear android generated client verification
- Address openapi client review issues

## [1.3.1] - 2026-06-16

### Added

- Motion polish for shell navigation
- Staggered list reveals + press-feedback sweep
- Polish the Ask screen
- Polish FAB op flow; move ListReveal to common
- Tier-1 motion — crossfade state swaps + result-list reveals
- Mobile-native Ask/Knowledge polish + Aurora alignment
- Ask-screen motion suite, Noto Sans body font, header cleanup
- Give message bubbles depth (gradient + glow + lift)
- Bubble grouping, stateful borders, selection affordance
- Composer-style Ask input — multiline, visible mode, clear, multi-attach
- Move Ask/Chat back to send button with a mode badge
- Draggable FAB clear of the prompt + present send button
- Labeled ops grid + higher-contrast prompt input
- Restore radial op ring, now with a label under each icon
- More fitting op icons

### Changed

- Address PR #221 review — feedback, invariants, tests, split
- Derive AxonPalette dark colors from aurora lib; fold AxonColors.kt
- Replace inline Color(0x) literals with AxonTheme.colors.*

### Fixed

- Send-button mode badge — caret in the bottom-right
- Close two residual PR #221 review findings
- Clean-sweep the out-of-scope review notes

## [1.3] - 2026-06-14

### Fixed

- Keep typed nav route serializers

## [1.2] - 2026-06-14

### Added

- Axon Android app with Ask/Search/Sources/Settings + Aurora design system
- Add Scrape, Map, Crawl, Research tool screens with 5-tab nav
- Stream ask responses via SSE /v1/ask/stream
- Comprehensive UI polish pass — Aurora components, icons, empty states, status indicators
- Merge axon Android app into main
- Pager shell + FAB mode selector + in-app document view
- UrlValidator — client-side fail-fast for non-http(s) URLs
- Wire models for /v1/{summarize,search,ingest,jobs,doctor,suggest,domains}
- AxonClient — summarize, searchWeb, ingest{Start,Get,List,Cancel}, status, doctor, suggest, domains (+ R7 shared ConnectionPool/Dispatcher)
- AxonRepository — summarize/searchWeb/ingestStart/listJobs/cancelJob/status/doctor/suggest/domains UI mappings
- Ask mode auto follow-up — inline prior 6 turns, reset on mode switch
- Summarize mode UI + ViewModel; swap StubModeForm
- RecentJobsRepository — persist submitted jobIds with dedup-by-jobId + LRU cap
- Jobs page — single-flow flatMapLatest polling (visible tab only), virtualized list, status header
- Knowledge page — 4 tabs (Suggest/Sources/Domains/Stats) + R11 30s memoization + R4 chunked Stats
- System page — Doctor only with R4 chunked rendering (Stack/Config deferred)
- Search mode UI + ViewModel (Tavily web search); R16 queue-full callout; swap StubModeForm
- Ingest mode UI — submit/status/cancel; R13 URL.host endsWith validation; persists jobId to RecentJobsRepository
- EncryptedTokenStore + dataExtractionRules
- Idempotent boot-time token migration to EncryptedTokenStore
- ModeOptionsApplicator + Repository + 9 per-mode forms
- Mode-options nav route + FLAG_SECURE + wire cog to nav
- Pager shell + FAB mode selector + in-app document view — v4.12.2
- AuroraProgressBar + AuroraStatusDot composables
- DrawerSection + AxonRail composable
- OverlayDrawer + stub DrawerSectionContent
- RailScaffold composable + AskScreen onOpenDocument param
- Replace HorizontalPager nav with RailScaffold
- Remove legacy operations screen files
- FabOp enum — 10 operations for ring launcher
- FabRing — 360° spring-animated operation ring
- FabOpInputCard + FabLauncher wired into AskScreen
- ChatBubble + InjectionCard composables
- AskScreen chat bubbles + FAB op injection
- Session Room entity + DAO + DB v2 migration — task 12
- SessionsViewModel + SessionsDrawerContent — task 13
- JobsOverviewViewModel + JobsOverviewItem for drawer — task 14
- JobsDrawerContent for active-jobs overview — task 15
- SuggestScreen + SuggestRoute nav — task 16
- KnowledgeDrawerContent + Management + Setup stubs — task 17
- ManagementViewModel — stats + doctor async calls
- ManagementDrawerContent — Monitor/Stack/Dedupe/Sync/Config sub-items
- SetupViewModel — smoke + doctor async calls
- SetupDrawerContent — Preflight/Setup/Smoke/Doctor/Debug sub-items
- Wire Management and Setup drawers with onOpenSettings nav
- Phase 3 — FAB fixes + MGMT/SETUP drawers wired; bump versionCode=2 versionName=1.1
- Phase 3 — FAB fixes + Management/Setup drawers wired (#144)

### Changed

- Simplify — shared state helpers, AxonClient execute helper, withToken wrapper, tool URL form
- Kotlin quality pass — StringBuilder streaming, named timeouts, reactive settings, KDocs
- Code-review fixes — dedupe shells, typed nav callback, doc chunking
- PR-review fixes — DocumentVM retry, chunking fallback, tests, comment trim
- Extract ModeContentHost + Resource sealed interface + shared StringChunking
- /code-review fixes — Resource smart-cast, ResourceContent helper, lifecycle-aware ConnectionStatus, UrlValidator.hostOrNull
- Address review — shared DrawerSubItem, AxonColors tokens, fix alpha/touch-targets/dead-state
- Simplifier pass — default chevron in DrawerSubItem, hoist FabRing constants, smart-cast FabState.Input

### Fixed

- Address review findings — thread safety, null safety, architecture, security
- Toolkit review — silent failures, type safety, new value classes, expanded tests
- Resolve Codex PR comments — portable Aurora path, remove local.properties, wire collection setting
- Reactive token reload, exhaustive-when branches, retry button
- Drop unsupported collection from scrape/crawl requests; fix test type errors
- Correct CrawlStatusResponse to unwrap {"job":{}} envelope
- Show input fields above keyboard on all screens
- Draggable floating FAB + per-mode cog left of Send
- Rounded-square FAB w/ Aurora tokens + connection-status indicator
- Truncate server error body to 200 chars in AxonClient.execute()
- PR-#142 review remediation + Ask hang + Jobs/Knowledge/System crash
- Address all 22 review findings from PR #142 code review
- Address all 20 coderabbitai review findings — v4.12.1
- Code-review cleanup + build fixes — v4.12.2
- Shimmer clipped to fill, linear pulse easing
- Gate infinite transitions, clip shimmer edges
- RailScaffold - AnimatedVisibility scoped to content column not full screen
- Use Box not Column for Ask+drawer overlay container
- AllSessions sort — remove spurious pinned_at DESC, keep updatedAt DESC
- @Upsert, sessions index, DEBUG-gate destructive migration — task 12 quality
- Wrap SessionRow DropdownMenu in Box for proper anchoring
- Sessions onSelect callback + error color from MaterialTheme — task 13 quality
- ErrorMessage only on all-kinds-fail, remove dead pollJob — task 14 quality
- Edge-to-edge status bar insets + jobs list path
- BackHandler dismisses FAB ring on back press
- Center FAB ring on screen so all 10 ops are visible; add dim backdrop
- RunCatching prevents stuck Loading; Resource<String> replaces bespoke state; Preflight fail-first priority
- Dismiss drawer before navigating to Settings/Suggest
- RAG review remediation — services↔vector cycle break, ingest/embed hardening, CI gates
- Repair release workflow
- Repair release workflow
- Update security crypto for launch crash
