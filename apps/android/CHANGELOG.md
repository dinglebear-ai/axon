# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
