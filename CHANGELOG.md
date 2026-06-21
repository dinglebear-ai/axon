# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.16.5] - 2026-06-20

### Added

- Add Lumen-style local code search (#245)

## [5.16.3] - 2026-06-20

### Added

- Add oauth sign-in and mobile sessions

### Fixed

- Harden oauth and session sync
- Align oauth scopes and settings auth states
- Add qdrant url purge and refresh ci artifacts
- Address openapi client review issues
- Split active job progress backfill migration

## [5.16.0] - 2026-06-16

### Added

- Gate codex MCP/skills/hooks loading behind AXON_CODEX_LOAD_USER_CONFIG
- Install codex CLI in the container image; drop host-only guard
- JS/TS declaration parity — arrow-fn, function-expression, exported consts, enums
- Python/Go/Rust/Bash declaration parity — decorators, lambdas, interfaces, macros
- Declaration-driven assembly + residual sweep + zero-decl prose fallback
- JSON/YAML/TOML structured-data chunking
- Make crawl cache opt-in
- Add spider adaptive concurrency

### Changed

- Data-driven tree-sitter query registry for declaration extraction

### Fixed

- Kill app-server process group via syscall; mount auth.json read-only
- Address PR review — ESRCH cleanup, keep good answers, tests, docs
- Register AXON_CODEX_LOAD_USER_CONFIG in contracts; de-flake grandchild test
- Cap leading-comment prefix so declaration body is never starved
- Route .tsx to JSX grammar; scope method rules; review fixes
- Bound oversized residual prose chunks to the cap
- Recover container bodies + per-spec Go declaration names
- Drop unnecessary std::path qualification in codex home_tests
- Route json/yaml/toml through the code path in embed
- Address adaptive review feedback
- Address adaptive policy review

## [5.12.0] - 2026-06-14

### Added

- Add --exclude-path to skip repo paths during git ingest

### Fixed

- Lower default crawl cache TTL from 24h to 1h

## [5.11.1] - 2026-06-14

### Fixed

- Comprehensive RAG-pipeline review remediation (rebased on main)

## [5.11.0] - 2026-06-14

### Added

- Ship Tauri palette, remove GPUI apps/desktop

### Fixed

- Preserve source docs without chunk caps

## [5.10.1] - 2026-06-14

### Fixed

- Unify secret redaction behind core::redact

## [5.10.0] - 2026-06-14

### Added

- Add codex app-server backend
- Add release binary updater (#211)
- WARC output, Chrome web-automation, and RSS/Atom ingest (#212)

### Changed

- Simplify codex handshake loop
- Reuse shared timeout helpers
- Simplify codex model lookup

### Fixed

- Harden codex app-server cleanup
- Address codex app-server review gaps
- Address codex backend review findings
- Address pr review toolkit findings

## [5.9.2] - 2026-06-13

### Added

- Normalize file chunk metadata

### Fixed

- Quiet sparse indexing fallback warnings
- Honor broadcast buffer config (#205)

## [5.9.1] - 2026-06-11

### Added

- Add persistent agent memory

### Fixed

- Polish operation result rendering

## [5.9.0] - 2026-06-10

### Added

- Symbol-aware GitHub code chunking + code-aware ranking (supersedes #187) (#192)
- Add Android APK release workflow (#195)
- Render command artifact handles in panel + /api/panel/artifact route
- Windows install support — install.ps1, cross-platform install_self()
- Cross-platform self-install via `axon palette install`
- Unify file-ingest engine across all git providers and embed (#202)

### Changed

- Split page.tsx into five modules to satisfy monolith policy (#200)

### Fixed

- Include error cause in query-family MCP responses
- Bound error source-chain walk against cyclic sources
- Add userConfig and MCP auth headers to axon plugin
- RAG review remediation — services↔vector cycle break, ingest/embed hardening, CI gates
- Rename expand_tilde → expand_home in test sidecar

## [5.4.2] - 2026-06-09

### Added

- Improve crawl progress display and Chrome timeout floor
- Replace sync file logger with tracing-appender rolling writer
- Simplify axon — remove full-stack/web, keep CLI+MCP+lite-mode
- Add tracing progress bundle
- Complete axon plugin scaffold with skills, agents, and monolith splits
- Add unified web panel server
- Add ssh remote deployment setup
- Scaffold xtask and tighten service contracts
- Heartbeat + cancellation tokens, size-based logging, Chrome security, CVE-2026-42327
- Doc-chunk cache with moka single-flight + generation invalidation (pmc)
- Adaptive ask_full_docs via AskQueryForms.use_dual (721)
- Add ask headless backend
- Default ask to gemini flash lite headless
- Load ~/.axon/.env in dotenv resolution chain
- Default AXON_DATA_DIR to ~/.axon, flatten redundant axon/ subpath
- Wire [tei] TOML keys through Config
- Wire [workers] + [search] TOML keys through Config
- Add OAuth 2.0 + Google login to MCP HTTP server
- Canonicalize axon appdata home
- Canonicalize Axon appdata home
- Add generic server client config
- Add first-party action API
- Return portable artifact handles
- Route stateful commands through server mode
- Implement true client/server mode
- Make document reads content-first
- Wire synthesis_prompt into ask_completion_request; embed skill via include_str!; add runtime override from AXON_DATA_DIR
- Add gemini to docker image; auto-sync dev binary to PATH and container
- Native Gemini skill invocation for axon-rag-synthesize
- Add axon setup hook subcommand for Claude Code SessionStart
- Enqueue crawl jobs from search results
- GPUI palette + Windows cross-compile + SSH removal (#89)
- Improve ask follow-up context
- Surface crawl status errors
- Improve job observability
- Expand TOML schema for move-toml tuning knobs
- Add compat shim deprecation warnings to env migration path
- Verify and complete env boundary reduction
- Complete action API hardening followups
- Harden retrieval quality diagnostics
- Payload schema versioning + extractor_name keyword
- Add config fields for vertical/antibot/structured/ladder
- Establish test sidecar pattern + migrate 2 reference files
- Migrate inline tests to _tests.rs in src/ingest/ and src/jobs/
- Migrate inline tests to _tests.rs in src/services/ (26 files)
- Migrate inline tests to _tests.rs in src/vector/ (12 files)
- Migrate inline tests to _tests.rs in src/core/ (29 files)
- Add structured-data parallel pass (JSON-LD/__NEXT_DATA__/SvelteKit)
- DOM retry ladder before Chrome fallback
- Antibot challenge-page detection (8 WAF signatures)
- JSON data-island walker for thin/SPA page recovery
- Next.js App Router __next_f string-leaf scanner
- Add Cargo feature placeholders + runtime env-gate docs
- Wire detect_challenge into crawl path before thin-page filter
- Normalize structured tracing events for ladder/antibot/structured
- Wire structured-data pass into crawl collector pipeline
- Vertical-extractor framework (src/extract/) + github_repo reference
- Add 12 vertical extractors
- MCP vertical_scrape action + CLI AXON_VERTICAL shortcut
- Establish test sidecar pattern + migrate 2 reference files
- Verify env/config migration end-to-end
- Markdown rendering, Tab completion, status dot, Aurora polish (v2.2.2)
- Add `axon config` command for managing .env and config.toml
- Auto-continue ask conversations
- Session management flags (--new-session, --list-sessions, --resume, --continue)
- Remove OPENAI_* env vars and OpenAI client path; unify on Gemini headless
- Security hardening — required_scope catch-all, LoopbackDev bypass, scope promotions (v4.0.0)
- Family 1 — GET /v1/{sources,domains,stats,doctor,status} (epic 2qva)
- Family 2 — sync POST /v1/{query,retrieve,suggest,map,search,research,scrape}
- Family 3 — async job POST/GET/cancel for crawl,embed,extract,ingest
- Family 4 — admin /v1/{migrate,dedupe,watch*} with unconditional auth
- Family 5 — deprecate /v1/actions + bump 4.0.0→4.1.0
- Add dedicated REST API routes
- Wire --research-depth, type ResearchPayload, address 19 review findings
- Add OpenAPI endpoint metadata
- Secrets scanner, sparse checkout CI, clippy fixes, docs (v4.2.0)
- UA overhaul, crates.io rustdoc JSON, docs.rs vertical, all vertical upgrades + new verticals (v4.2.0)
- Add endpoint discovery
- Cut over clients to REST
- Add GitLab and forge ingest
- Add to_llm_text() LLM-optimised markdown transform
- Add crawl --format llm parse-time guard
- Sync McpScrapeFormat::Llm to MCP wire protocol
- Add --format llm post-crawl stdout stream for axon crawl
- Fix 7 gaps vs bead w2wf acceptance criteria
- Implement extra payload for all 17 vertical extractors + split payload_indexes
- Index reddit_subreddit and yt_channel as keyword payload fields
- Add Qdrant indexes for promoted gh_* fields
- Use tree-sitter code chunking for source files
- Port webclaw diff and brand tools as axon commands v4.3.0
- Add exact domain indexed sources
- Batch full-doc fetch via qdrant_batch_retrieve_by_urls
- Add Tauri palette and harden search crawl (#136)
- Centralize REST contracts and generated client types
- Trim axon status default response (axon_rust-9pbb) (#133)
- Align CLI palette with Aurora design tokens
- Aurora design-system polish — hyperlinks, panels, tables, sparklines, color flag, live status
- --wait spinner for crawl, embed, extract, ingest
- Live per-page crawl progress + Aurora CLI polish + sessions streaming
- Show elapsed time and error count in crawl status display
- Add openai-compatible backend and palette polish
- Stream ask responses
- Pager shell + FAB mode selector + in-app document view
- Pager + FAB shell, operation mode expansion, form-keys package — v4.12.0
- Pager shell + FAB mode selector + in-app document view — v4.12.2
- JSON-RPC 2.0 / MCP / ACP protocol probing --probe-rpc (v4.13.0)
- Rename `stack` command to `compose` + surface real ingest error causes (v4.14.0) (#146)
- Add --probe-rpc-subdomains flag + MCP candidate output types
- Synthesize MCP candidate URLs with PSL apex derivation
- Add strict positive-signal probe entry for synthesized candidates
- Probe synthesized MCP candidates during --probe-rpc
- Make initial fetch non-fatal under --probe-rpc for bare endpoints
- Expose probe_rpc + probe_rpc_subdomains on endpoints action
- Expose probe_rpc + probe_rpc_subdomains on /v1/endpoints
- Auto-fire scheduler + create-time task_type validation (v4.15.0)
- Llms.txt probe — parse + merge into sitemap backfill & map — v4.17.0 (#152)
- URL change-detection watch (diff, summarize, artifact, clustered crawl) — v4.18.0 (#151)
- Adopt spider 2.51 crawl-efficiency features (etag/AIMD/52x) (#153)
- Add 'axon setup install' + self-install in plugin-hook
- Raise chunk cap, demote mirror sources, collapse near-duplicates
- Scale context budget to the configured model's window
- Scale retrieval depth (chunks, candidate pool, prefetch) by model tier
- SearXNG backend + full-content research synthesis; model-scaled summarize budget
- SearXNG for `search` too + research snippet-only toggle + searxng tests
- Show streamed= and ttft= in timing line
- Stream ask research summarize in server mode
- Auto-index evidence sources
- Run all CLI/MCP actions in-process; remove artifacts action (#161)
- Add codex-app-server LLM backend and saved provider profiles
- Live crawl job view backed by a real crawl event stream
- Pulse the live-crawl status dots while a crawl runs
- Seed_url origin tracking + `axon refresh` command (#186)
- Aurora side-panel launcher from design handoff (#191)
- Recursive + AST-aware local directory embed (axon_rust-mzj9)
- Classify gpt-* as Medium tier + extend model-comparison runner

### Changed

- 2 more beads — score-before-clone + dispatch score telemetry (d71.22/31)
- 2 more beads — full-doc rerank + flatten context by score (0fz, az9)
- Rename crates/ to src/, adopt standard single-crate layout
- Split build_config.rs into smaller modules
- Simplify with shared helpers
- Share query and ask pipeline
- Simplify shared pipeline
- Polish async CLI output
- Simplify review cleanup
- Remove ACP and use Gemini headless
- Tighten logging color helpers and env-var checks
- Simplify query, setup, config, services, and vector ops
- Simplify search crawl followups
- Move search auto-crawl logic to services layer
- Split oversized search service files
- Extract inline tests to sidecars (services crawl, query, ingest)
- Extract inline tests to sidecars (gemini, retrieval, status)
- Split core/content.rs into submodules
- Split vector/ops/qdrant/client.rs into submodules
- Split ask/context/build.rs and action_api/commands.rs
- Split logic files into submodules; fix web/server mod.rs violations
- Finish ask context helper split
- Split system.rs into submodules under monolith cap
- Version bump 2.1.1 + CHANGELOG for test sidecar epic
- Avoid full-HTML to_lowercase + strict src/type attr boundaries
- Move vertical dispatch to services layer (the right place)
- Remove AXON_LITE compat shim + env/config hardening (v2.2.1)
- Consolidate container detection into running_in_container() helper (Docker+Podman+env-var)
- Migrate inline #[cfg(test)] mods to sidecar files
- Split gitea.rs into gitea/{client,embed}.rs
- Remove redundant ext_clone in chunk_code block
- Centralize MCP target parsing in services
- Simplify Aurora helpers from code-simplifier review
- Use typed is_lock_busy + eliminate double error parse
- Address code-review findings from DB contention hardening
- Simplify streaming follow-up
- Split axon plugin into axon + axon-mcp
- Route CLI create through shared validate_every_seconds
- Call axon binary directly from hooks; port env mapping into the binary
- Make SessionStart hook probe-only — never deploy

### Fixed

- Address lavra-review findings (P1+P2+P3)
- 26-bead grind — observability + lite-mode UX + retrieval guards
- 3 more beads — dedupe primary, sparse warn, vector docs (d71.9/10/16)
- 8 more beads — vector docs/perf/tests/instrumentation (d71.x)
- 2 more beads — header validation + always-on error diagnostics (d71.33/35)
- 4 more beads — date cache, typed bodies, MCP hybrid override, drain tests (d71.23/25/32 + cr5.14)
- Persist lite job config snapshots
- Remove ask authoritative allowlist
- Finish lite snapshot review fixes
- Gate ask rerank on RRF mode
- Revalidate stale vector mode cache
- Complete retrieval follow-up beads
- Resolve check/clippy/security regressions introduced by P1+P2
- Reject parent dirs in HOME
- Harden MCP auth and ACP quick wins
- Harden remote deploy setup
- Carry PR 65 review fixes onto ssh deployment
- Repair compose env path drift
- Address compose deploy review comments
- Address ask perf PR review
- Security + symlink + trim review remediations
- Address Copilot PR review comments
- Pr-review-toolkit P1 + critical test gaps
- Make config defaults pure
- Avoid duplicate ask output
- Renumber ask source citations
- Align headless llm paths
- Make headless the canonical ask backend
- Address MCP retrieval contract issues
- Expose RAG evaluation tool actions
- Address RAG tool review feedback
- Tolerate unavailable log file appender
- Complete ug6 remediation
- Complete crawl chunking review epic
- Address crawl chunking PR feedback
- Tolerate unavailable baseline answer
- Address ACP removal review findings
- Address PR review comments
- Address MCP smoke CI failures
- Remove ACP from generated MCP schema docs
- Improve crawl status recovery reporting
- Set rmcp allowed_hosts from AXON_MCP_ALLOWED_ORIGINS
- Address canonical home review feedback
- Resolve completed PR review threads
- Secure first-party action auth
- Remove stale ask server url alias
- Persist server-mode scrape artifacts
- Ignore blank optional path envs
- Unify server-mode status output
- Isolate internal service http routing
- Address PR review feedback
- Make claude review best effort
- Finish server-mode renderer split
- Remove URL-disjoint constraint from select_context_indices; enable full-doc skip gate; reduce ask_doc_chunk_limit 192→48
- Address PR #83 review comments
- Strengthen injection-defense assertion to guard full sentence
- Force inline mode for status action so dashboard widget renders
- Inject live status into widget HTML at read_resource time
- Remove await from app.connect() per MCP Apps spec
- Serve web and mcp on one port
- Align doctor and config docs with gemini runtime
- Use internal client for doctor service probes
- Probe internal doctor services without ssrf resolver
- Colored logging in Docker + local time zone in container
- Remove unused ansi_bold; convert aurora doc to inner comments
- Adapt Gemini headless parser to CLI 0.41.2 stream-json changes
- Harden search crawl job validation
- Address final review findings
- Address PR review comments
- Update Next.js security release
- Address crawl status review comments
- Address Wave 1 review P1/P2 findings
- Address retrieval quality review comments
- Align topical overlap token policy
- Add git to Dockerfile + fix GitHub clone auth + sort all job types by active first
- Harden SQLite against corruption under heavy concurrent load
- Address all review findings from test sidecar epic
- Address lavra-review P2 findings — DoS caps, UTF-8 preservation, dead round-trip
- Scope __NEXT_DATA__ extraction to id-attributed script (cubic #4)
- Wire orphan services/error/taxonomy.rs into the error module
- Add git to Dockerfile + fix GitHub clone auth + sort all job types by active first
- Fix test failures from review changes
- Override AXON_DATA_DIR in compose — env_file leaks host path otherwise
- Service URL overrides, collection sentinel, env restore in tests
- Scope flag scan + add get overrides + accept _-prefixed env keys
- Review nits + repair contract-test drift inherited from 022d189
- Mark module-layout fence as \`\`\`text so doctest stops compiling it
- Restore OPENAI_* to active env vars + tighten is_valid_env_key
- Drop repeating animation on launch-time health-check dot
- Unblock test + mcp-schema-doc-sync (inherited from main)
- Address cubic findings in output stripping and .env.example
- Sync version files + drop stale GOOGLE_*/AXON_LOG_* from env contract allowlist
- Sync Cargo.lock + version-bearing files + .env.example contract
- Address review findings (ask-reset guard, hint slot width, .env.example)
- Sync version files + drop stale GOOGLE_*/AXON_LOG_* from env contract allowlist
- Address final review threads (ui stale-sweep, .env.example AXON_LITE, matrix surfaces)
- Address PR #103 review threads
- Address review feedback on OPENAI_* removal
- Register AXON_LOG_PATH in env boundary migration matrix
- Debug→write, ElicitDemo explicit, invariant comments, tighter tests
- Add 403-scope-boundary test + sync package.json version
- Address lavra-review findings on epic 2qva PR
- Address cubic + silent-failure-hunter + code-reviewer findings
- Expand bad_request markers in classify_service_error
- Scope discrimination + watch tests + classify_service_error tests
- Address cubic P1/P2 + CodeRabbit review threads
- Address REST API review feedback
- Address remaining REST API review feedback
- Mask upstream error response messages
- Sync web and README versions
- Address follow-up PR feedback
- Build portable Windows palette
- Address 3 new cubic P2 threads
- Address 11 new CodeRabbit threads from merge push
- Use spawn_blocking for embed path fs ops (PRRT_kwDORS2O8s6C_kxm)
- Add trace_tests.rs sidecar (was gitignored by build/ pattern)
- Sync MCP action help contract
- Address endpoint discovery review feedback
- Resolve endpoint discovery review followups
- Address endpoint discovery review nits
- Address endpoint discovery review threads
- Clarify vertical scrape MCP schema docs
- Tighten endpoint discovery telemetry
- Align MCP summarize discovery
- Address server mode REST review threads
- Harden REST cutover live API edges
- Require JSON content type for dedupe body
- Address PR #116 review threads (8 threads)
- Always use https for api_base and clone_url
- Address 12 of 18 PR #122 review threads
- Address 2 new PR #116 review threads
- Import Embed as _ to resolve WebAssets::get in CI (empty apps/web/out/)
- Use anyhow!("{}", e) instead of anyhow!("{e}") for error conversion
- Address 7 of 8 PR #121 review threads
- Remove duplicate "(Tailscale)" in vector/CLAUDE.md
- Register AXON_ENDPOINT_*_CONCURRENCY in boundary matrix
- Acquire bundle semaphore per-fetch, not per-session
- Address PR #118 review threads
- Use version-matched entry for license/MSRV/edition/title
- Update reddit_tests to use shared build_reddit_post_extra_payload
- Address PR #122 review threads
- Address 12 of 18 PR #122 review threads
- Add brand+diff to MCP tool description and fix route names
- Promote brand.url and diff.url_a/url_b to required String
- Strip U+2028/U+2029 from LLM link labels, type-safe ChallengeVendor, centralize crawl validate_url (zzre.1.2, jej7.1.2, wbm7)
- Treat extension-less path segments as directory endpoints in auto-scope (b4y)
- Support admin OAuth ingest in server mode
- Preserve domain source export limits
- Keep endpoints write scoped
- Correct batch retrieve limit (retrieve_max_points not scroll limit)
- Accept positional ask text in server mode
- Harden job monitor state handling
- TTY-detect in --color=auto + force ANSI in --color=always for tracing
- Panel alignment, watch reliability, color install order + tests
- Honor --quiet in axon status --watch + lint clean session doc
- Harden aurora status watch polish
- Retry qdrant payload index PUTs + show embed progress while initializing
- Crawl_progress_summary shows nothing while starting/discovering
- Log handler errors before masking them in the response body
- Extract/ingest progress summary blind spots + suggest table polish
- Remove unnecessary to_string() on &'static str mode field
- Make startup reclaim + status list non-fatal under DB contention
- Sync web version + isolate test env from ambient AXON vars
- Downgrade SQLITE_BUSY watchdog sweep to WARN
- Revert busy_timeout to 30s — 5s caused lock starvation in production
- Batch server-mode session uploads to respect prepared-docs cap
- Unwrap server-mode payload before human render
- Path_contains → path_includes in retrieve_tests (httpmock API)
- Cap qdrant_batch_retrieve_by_urls + add sidecar tests
- Polish palette commands and qdrant quantization
- Snapshot openai compat config safely
- Return retrieval degradation warnings
- Preserve malformed progress warnings
- Preserve ingest task phase
- Harden config fallback and ingest
- Harden ask streaming lifecycle
- Compact output UI + map normalize_url
- PR-#142 review remediation + Ask hang + Jobs/Knowledge/System crash
- Address all 20 coderabbitai review findings — v4.12.1
- CDP Page.navigate deadlock + endpoint noise/quality fixes (v4.12.4)
- Collapse nested if-let in probe_rpc_endpoints (clippy)
- Harden --probe-rpc concurrency, timeout, MCP fidelity, bounded reads (v4.13.1) (#145)
- Address review — split fetch module, first-party for apex subdomains, surface blocked candidates, SSRF guard in probe_candidate
- Wire probe_rpc + probe_rpc_subdomains through dispatch_endpoints (rest-api-parity)
- Centralize every_seconds bounds + keep lease single-flight
- Unblock CI (-D warnings) + surface wedged FAILED-status write
- Serve Swagger UI in debug builds + resync OpenAPI spec version
- Pin Qdrant quantization + HNSW in RAM on create (v4.18.3)
- Skip compose deploy when stack already healthy on session start
- Probe configured MCP HTTP bind in hook fast-path, not hardcoded :8001
- Give full documents context-budget priority over chunks
- Extract fonts from linked stylesheets
- Address PR #158 review (SSRF guard, url-key, budget cap, preflights)
- Pageno pagination + RAII loopback guard (PR #158 round 2)
- Version-bump + env-contract fixups exposed by 4.20.0 bump
- State-restoring loopback guard, page on empty (not dup), doc surfaces
- Publish axon action schema in tool input (#160)
- Keep default single-url extract on exact page (#159)
- Wire ask and summarize to stream LLM tokens by default
- Collapse nested if to satisfy clippy collapsible_if
- Stream tokens to stdout with flush; fix ask double-print
- Emit streamed= and ttft= in timing line without --ask-diagnostics
- Harden mcp task results and embed validation
- Codex context budget + single-source provider-list backend resolution
- Blend the collapsed crawl tray into the command bar
- Clear pre-existing clippy/fmt/monolith debt blocking pre-push
- Address PR #188 review — log on code-chunk JoinError + canonicalize failure
- Repair release pipeline, align install.sh, fix plugin binary sync (#193)

## [0.35.1] - 2026-04-05

### Added

- Path exclusion, live crawl progress, and infra hardening
- Harden worker recovery and expand reliability test coverage
- Implement ingest_github, ingest_reddit, ingest_youtube + s6 worker
- Research command, Tavily search, monolith splits, engine/worker refactors
- Delete axon batch + fix extract --urls CSV
- Add shadcn button, input, tabs, scroll-area, badge
- WS protocol types, bioluminescent theme, WS proxy config
- WS connection hook with exponential backoff + providers
- Add all dashboard components
- Assemble dashboard with all components wired
- Add serve command with axum web UI + remove search auto-crawl
- Add stdout streaming protocol types, hook state, and raw renderer
- Typed renderers + result normalizer pipeline (Phase B)
- Job lifecycle renderer + all modes grouped by category
- Phase D polish — helper selectors, recent run targets, command options panel
- Crawl progress components + UI polish + progress granularity
- Crawl download routes — pack, zip, and per-file downloads
- Doctor report renderer, options reorder, result panel polish
- Ship Pulse workspace foundation with RAG and copilot API
- Add omnibox file mentions and root env fallback for pulse APIs
- Refresh pulse UI styling and architecture docs
- Add path-first artifact contract, schema resource, and smoke coverage
- Align status action parity and refresh docs
- Improve CLI diagnostics and refresh web accent mapping
- Harden crawl/mcp flows and resolve PR review threads
- Pulse workspace overhaul + refresh schedules + crawl download pack
- Axon-web service + chrome Dockerfile move + web-server s6 worker
- Conversation memory fallback + claude binary mount
- Thinking blocks, empty bubble fix, hot-reload s6, sccache
- Project-owned claude config dir + headless CLI flags
- Settings page, session cards, workspace persistence, PWA scaffold
- Settings redesign, MCP config/agents pages, PlateJS theming, status indicators
- SSRF hardening, AMQP reconnect backoff, multi-lane workers, expanded tests
- Add /api/workspace route for AXON_WORKSPACE file browsing
- Add workspace (FolderOpen) nav icon to omnibox toolbar
- Add CodeViewer component with line numbers and copy button
- Establish design token foundation — fonts, palette, motion, atmosphere, shadows, a11y
- Add /workspace file explorer page with tree + viewer
- Add CodeViewer component with line numbers and copy button
- Button/input hover micro-interactions, branded focus rings, scrollbar contrast fix
- Status bar persistence, @mention discovery tip, staggered suggestions
- Motion, empty state, message alignment, tool badge discoverability, mobile pane labels, divider improvements
- Modal delete dialogs, MCP single save, settings typography, empty states, layout improvements
- PlateJS editor integration, pnpm-watcher s6 service, chrome health fix
- Workspace virtual dirs, Claude folder, landing editor, header normalization
- Remove hard borders, glow separators, word wrap
- Tasks page - task scheduler dashboard with CRUD and manual run
- Logs page - Docker compose log viewer with SSE streaming
- Hoist PulseSidebar to AppShell — visible on all pages
- Xterm.js terminal emulator at /terminal
- UseShellSession hook — dedicated /ws/shell WebSocket
- Terminal page — real PTY shell via useShellSession
- /docs knowledge base page — filesystem-backed manifest reader
- /jobs/[id] detail page — status, stats, timing, config, live polling
- Replace DesktopViewMode/DesktopPaneOrder with showChat/showEditor booleans
- Rewrite use-split-pane for 3-panel chevron layout
- Remove view-mode toggle buttons from PulseToolbar
- Update use-pulse-persistence for showChat/showEditor
- 3-panel collapsible layout — chat left, editor right, chevron strips
- Jobs dashboard — color badges, stats bar, sort, relative time, smart truncation, hover actions
- Cortex virtual folder in sidebar — diagnostic pages for status/doctor/sources/domains/stats
- Plate.js editor enhancements — slash, DnD, callouts, toggles, TOC, block selection, AI menu, comments, export
- Xterm.js terminal enhancements + Cortex layout refactor
- Null-safety hardening, CmdKPalette refactor + AI command validation
- Add HTTP transport with Google OAuth + cleanup
- Add evaluate page, cortex suggest API, image SHA verification, CLI help contract; consolidate modules and expand command docs (v0.3.0)
- V0.5.0 — services-layer refactor complete + editor tabs + CmdK + scripts
- V0.6.0 — web workspace/sidebar updates + TEI retry fixes
- V0.7.0 — ACP pulse agent routing, frontend wiring, and scrape/embed hardening
- Address all ACP review findings (v0.7.4)
- Address all ACP review findings (v0.7.4)
- Add --root-selector/--exclude-selector + clean_markdown_whitespace (v0.7.5)
- Zed alignment patterns + ACP permission plumbing (v0.8.0)
- Reboot UI shell + logs SSE fix + CORS config + biome cleanup (v0.9.0)
- Add git-metadata helper for repo/branch enrichment
- Enrich session list with git repo/branch metadata
- Handle session_fallback event in stream pipeline
- Add useAxonSession for JSONL session history
- Add useAxonAcp for real ACP WebSocket prompt submission
- Wire AxonShell to real session data and ACP WebSocket
- Wire AxonSidebar to real SessionSummary list
- Disable AxonPromptComposer submit during streaming, add spinner
- Add loading/error states to AxonMessageList
- Wire AxonShell to real ACP/session data, add hooks + UI polish (v0.11.0)
- Add editor_update WS message type to protocol
- Wire <axon:editor> XML blocks to PlateJS editor
- Unified axon ingest + structured metadata + MCP artifacts (v0.12.0)
- Multi-agent sessions sidebar — Claude + Codex + Gemini (v0.13.0)
- Add text-splitter + tree-sitter grammar crates
- Auth hardening + Pulse workspace panes + CLI cleanup (v0.14.0)
- Finalize mcp transport and review hardening (v0.15.0)
- Promote reboot UI to root route, move legacy dashboard to /legacy
- Update reboot sidebar page links for root route
- Wire Docker stats and NeuralCanvas intensity into reboot shell
- Wire message edit and retry in reboot chat
- Add settings dialog with canvas profile to reboot shell
- Add graph worker, services layer, artifact context isolation, and toolchain bump (v0.16.0)
- Complete GraphRAG rollout and prune reboot remnants
- Add assistant sessions API route and scanner
- Add assistant rail mode to config
- Render assistant session list in sidebar
- Wire assistant mode sessions through shell and ACP
- Ship assistant mode and stabilize verification gates (v0.18.0)
- Persist MCP config and harden session scanning
- ACP session persistence — survive WebSocket disconnects
- Performance/accessibility audit fixes + density feature + state split
- Harden session lifecycle and developer tooling
- Refresh shell mission control and provider branding
- Merge feat/github-code-aware-chunking into main (v0.21.1)
- Merge fix/pr-review-fixes-crawl-refactor into main (v0.21.2)
- Delete axon batch + fix extract --urls CSV
- Add shadcn button, input, tabs, scroll-area, badge
- WS protocol types, bioluminescent theme, WS proxy config
- WS connection hook with exponential backoff + providers
- Add all dashboard components
- Assemble dashboard with all components wired
- Add serve command with axum web UI + remove search auto-crawl
- Add stdout streaming protocol types, hook state, and raw renderer
- Typed renderers + result normalizer pipeline (Phase B)
- Job lifecycle renderer + all modes grouped by category
- Phase D polish — helper selectors, recent run targets, command options panel
- Crawl progress components + UI polish + progress granularity
- Crawl download routes — pack, zip, and per-file downloads
- Doctor report renderer, options reorder, result panel polish
- Ship Pulse workspace foundation with RAG and copilot API
- Add omnibox file mentions and root env fallback for pulse APIs
- Refresh pulse UI styling and architecture docs
- Add path-first artifact contract, schema resource, and smoke coverage
- Align status action parity and refresh docs
- Improve CLI diagnostics and refresh web accent mapping
- Harden crawl/mcp flows and resolve PR review threads
- Pulse workspace overhaul + refresh schedules + crawl download pack
- Axon-web service + chrome Dockerfile move + web-server s6 worker
- Conversation memory fallback + claude binary mount
- Thinking blocks, empty bubble fix, hot-reload s6, sccache
- Project-owned claude config dir + headless CLI flags
- Settings page, session cards, workspace persistence, PWA scaffold
- Settings redesign, MCP config/agents pages, PlateJS theming, status indicators
- SSRF hardening, AMQP reconnect backoff, multi-lane workers, expanded tests
- Add /api/workspace route for AXON_WORKSPACE file browsing
- Add workspace (FolderOpen) nav icon to omnibox toolbar
- Add CodeViewer component with line numbers and copy button
- Establish design token foundation — fonts, palette, motion, atmosphere, shadows, a11y
- Add /workspace file explorer page with tree + viewer
- Add CodeViewer component with line numbers and copy button
- Button/input hover micro-interactions, branded focus rings, scrollbar contrast fix
- Status bar persistence, @mention discovery tip, staggered suggestions
- Motion, empty state, message alignment, tool badge discoverability, mobile pane labels, divider improvements
- Modal delete dialogs, MCP single save, settings typography, empty states, layout improvements
- PlateJS editor integration, pnpm-watcher s6 service, chrome health fix
- Workspace virtual dirs, Claude folder, landing editor, header normalization
- Remove hard borders, glow separators, word wrap
- Tasks page - task scheduler dashboard with CRUD and manual run
- Logs page - Docker compose log viewer with SSE streaming
- Hoist PulseSidebar to AppShell — visible on all pages
- Xterm.js terminal emulator at /terminal
- UseShellSession hook — dedicated /ws/shell WebSocket
- Terminal page — real PTY shell via useShellSession
- /docs knowledge base page — filesystem-backed manifest reader
- /jobs/[id] detail page — status, stats, timing, config, live polling
- Replace DesktopViewMode/DesktopPaneOrder with showChat/showEditor booleans
- Rewrite use-split-pane for 3-panel chevron layout
- Remove view-mode toggle buttons from PulseToolbar
- Update use-pulse-persistence for showChat/showEditor
- 3-panel collapsible layout — chat left, editor right, chevron strips
- Jobs dashboard — color badges, stats bar, sort, relative time, smart truncation, hover actions
- Cortex virtual folder in sidebar — diagnostic pages for status/doctor/sources/domains/stats
- Plate.js editor enhancements — slash, DnD, callouts, toggles, TOC, block selection, AI menu, comments, export
- Xterm.js terminal enhancements + Cortex layout refactor
- Null-safety hardening, CmdKPalette refactor + AI command validation
- Add HTTP transport with Google OAuth + cleanup
- Add evaluate page, cortex suggest API, image SHA verification, CLI help contract; consolidate modules and expand command docs (v0.3.0)
- V0.5.0 — services-layer refactor complete + editor tabs + CmdK + scripts
- V0.6.0 — web workspace/sidebar updates + TEI retry fixes
- V0.7.0 — ACP pulse agent routing, frontend wiring, and scrape/embed hardening
- Address all ACP review findings (v0.7.4)
- Address all ACP review findings (v0.7.4)
- Add --root-selector/--exclude-selector + clean_markdown_whitespace (v0.7.5)
- Zed alignment patterns + ACP permission plumbing (v0.8.0)
- Reboot UI shell + logs SSE fix + CORS config + biome cleanup (v0.9.0)
- Add git-metadata helper for repo/branch enrichment
- Enrich session list with git repo/branch metadata
- Handle session_fallback event in stream pipeline
- Add useAxonSession for JSONL session history
- Add useAxonAcp for real ACP WebSocket prompt submission
- Wire AxonShell to real session data and ACP WebSocket
- Wire AxonSidebar to real SessionSummary list
- Disable AxonPromptComposer submit during streaming, add spinner
- Add loading/error states to AxonMessageList
- Wire AxonShell to real ACP/session data, add hooks + UI polish (v0.11.0)
- Add editor_update WS message type to protocol
- Wire <axon:editor> XML blocks to PlateJS editor
- Unified axon ingest + structured metadata + MCP artifacts (v0.12.0)
- Multi-agent sessions sidebar — Claude + Codex + Gemini (v0.13.0)
- Add text-splitter + tree-sitter grammar crates
- Auth hardening + Pulse workspace panes + CLI cleanup (v0.14.0)
- Finalize mcp transport and review hardening (v0.15.0)
- Promote reboot UI to root route, move legacy dashboard to /legacy
- Update reboot sidebar page links for root route
- Wire Docker stats and NeuralCanvas intensity into reboot shell
- Wire message edit and retry in reboot chat
- Add settings dialog with canvas profile to reboot shell
- Add graph worker, services layer, artifact context isolation, and toolchain bump (v0.16.0)
- Complete GraphRAG rollout and prune reboot remnants
- Add assistant sessions API route and scanner
- Add assistant rail mode to config
- Render assistant session list in sidebar
- Wire assistant mode sessions through shell and ACP
- Ship assistant mode and stabilize verification gates (v0.18.0)
- Persist MCP config and harden session scanning
- ACP session persistence — survive WebSocket disconnects
- Performance/accessibility audit fixes + density feature + state split
- Harden session lifecycle and developer tooling
- Refresh shell mission control and provider branding
- Merge feat/github-code-aware-chunking into main (v0.21.1)
- Merge fix/pr-review-fixes-crawl-refactor into main (v0.21.2)
- Merge feat/web-integration-review-fixes into main (v0.23.3)
- Scrape format params, search pagination, TEI chunking metadata, Qdrant retry
- Pulse shell redesign, AI elements, hybrid search, new API routes (v0.25.0)
- Handle session_info_update WS event and force-refresh cache bypass
- V0.26.0 — Chrome stealth extract, temporal search, spawn_blocking ingest
- Shared embed_with_retry, session refactors, MCP query filters
- Add thiserror dep, migrate HttpError to derive, add JobError enum
- V0.27.0 — ACP prewarm, services routing, error context, docker split
- Pulse shell UI, web server utilities, MCP/core refactoring, logging fixes
- Split export.rs monolith + fix acp_llm test regression
- Tier 1 embedding quality — asymmetric encoding + semantic chunking
- Streaming synthesis, ACP eager warm-up, hybrid search fix, quality tests
- Add HNSW config (m=32, ef_construct=256) and INT8 quantization to ensure_collection()
- Persistent ACP session cache, acp_llm module split, pulse_chat events
- Add lite_mode flag — AXON_LITE=1 makes PG/Redis/AMQP optional
- Lite mode backend + BM42/query retrieval improvements
- Bump agent-client-protocol to 0.10.2
- WAF diagnostics, enqueue-only LiteBackend, serve preflight auto-terminate

### Changed

- Address query/ask/retrieve/extract command hotspots
- Apply command perf stack updates and pin rust 1.93.1 in CI
- Module splits for ranking/ask/queue_injection, expose engine tuning flags, improve monolith tooling
- Flatten mod.rs → file-per-module + github ingest module
- Split jobs/common into modules + status UI polish + crawl hardening
- Pulse module splits + ask gates + omnibox/toolbar polish
- Split all monolith extractions into focused modules
- Split wave 4 — 8 remaining allowlisted monolith files
- Split wave 5 — final 6 allowlisted monolith files
- CLI command handlers, MCP wiring, and web fixes
- Split monolith-violating files (route.ts, use-pulse-chat.ts)
- Hoist git enrichment to outer project loop
- Rename Reboot* components to Axon*
- Rename remaining REBOOT_ constants to AXON_
- Performance/scalability fixes + modern Rust idioms (v0.11.2)
- Sync dispatch helpers + session guard scaffold (v0.13.2)
- Extract shared log stream hook from AxonLogsDialog
- Remove all reboot naming — rename to shell
- Web performance & accessibility improvements
- Route mcp embed ingest handlers through services layer
- Complete mcp lifecycle and screenshot rewires to services
- Route cli lifecycle and system commands through services
- Route web async ingest modes through direct services
- Address query/ask/retrieve/extract command hotspots
- Flatten mod.rs → file-per-module + github ingest module
- Split jobs/common into modules + status UI polish + crawl hardening
- Pulse module splits + ask gates + omnibox/toolbar polish
- Split all monolith extractions into focused modules
- Split wave 4 — 8 remaining allowlisted monolith files
- Split wave 5 — final 6 allowlisted monolith files
- CLI command handlers, MCP wiring, and web fixes
- Split monolith-violating files (route.ts, use-pulse-chat.ts)
- Hoist git enrichment to outer project loop
- Rename Reboot* components to Axon*
- Rename remaining REBOOT_ constants to AXON_
- Performance/scalability fixes + modern Rust idioms (v0.11.2)
- Sync dispatch helpers + session guard scaffold (v0.13.2)
- Extract shared log stream hook from AxonLogsDialog
- Remove all reboot naming — rename to shell
- Web performance & accessibility improvements
- Route mcp embed ingest handlers through services layer
- Complete mcp lifecycle and screenshot rewires to services
- Route cli lifecycle and system commands through services
- Route web async ingest modes through direct services
- GraphArgs subcommand, job_output/url_inputs utils, qdrant scroll hardening, ws-messages tests (v0.25.2)
- Multi-crate security hardening, full-review remediation, shared utilities
- Fix clippy dead-code and style warnings across all changed files
- Finish shared runtime cutover
- Thread build_config() into MCP server; delete load_mcp_config()
- Sessions refactor + ACP warm session path + artifact path fix
- Extract print_list_footer; filter_jobs_for_status_view takes &[T]
- PR #60 simplification — dedup lift_err, status warn, worker drop, deadline fix

### Fixed

- Address all unresolved PR review threads
- Address all PR review comments — security, correctness, and doc fixes
- Harden SSRF defences, async correctness, and job reliability
- SSRF blacklist covers localhost?query and localhost#fragment variants; add regression tests
- Address PR critical/high review threads
- Locale URL filtering + status UI cleanup
- Address all 82 PR #4 review issues + upgrade deps to latest
- Extract shared CopyButton, cap stdout arrays, explicit switch cases
- Phase B review fixes — renderIntent routing + deduplicate utils
- Phase C review fixes — cancel routing, PHASE_META, ingest removal
- Phase D review fixes — hooks order, depth flag, exhaustive deps
- Resolve pulse UI lint warnings and align renderer changes
- Address PR API review threads batch 1
- Address remaining PR review threads comprehensively
- Expose axon-web on 0.0.0.0, normalize test pg_url, update snapshots
- Default save collection to AXON_COLLECTION / cortex instead of 'pulse'
- Ensure Qdrant collection exists before upsert
- Land review fixes, test env alignment, and changelog/session plumbing
- Address 6 PR review comments
- Use Number.isNaN instead of global isNaN
- Replace !important with :root specificity for slate placeholder CSS
- Remove dangling useRouter() call from omnibox
- Add Settings2 icon import to omnibox + changelog update
- Address all 12 PR review comments from cubic-dev-ai
- Fix duplicate tool badges and raw-JSON response in Pulse chat
- Prefix unused liveToolUses prop + update changelog sha
- Address all P0/P1/P2 code review issues — 8-agent team landing
- Spawn_heartbeat_task helper, Redis cancel timeouts, async I/O, unit tests
- Resolve symlink traversal and path canonicalize bypasses
- Jobs-dashboard Biome lint compliance - hook deps and unused imports
- Remove CrawlFileExplorer from results-panel, delete stub
- Remove unused selectedFile/selectFile from results-panel destructure
- Restore selectedFile/selectFile in results-panel with inline file list
- Use ExtractedSection in results-panel instead of inline file list
- Resolve inotify watch limit, EADDRINUSE port race, and node_modules ownership
- Install uvx for neo4j-memory MCP, add pnpm-dev finish script
- Pulse autosave skip file-read, pre-delete stale vectors, editor doc-reload, z-index
- Pulse autosave update-in-place + editor hardening
- Remove unused showChatRef from use-split-pane
- Remove unused verticalDragStartRef from pulse-workspace destructure
- Pulse workspace quality fixes — collapse guard, editor flex, aria
- Pulse dual-hydration race + both-collapsed restore guard
- Address Cortex dashboard review findings
- Address code review findings from Plate.js editor enhancements
- Wire AIKit into CopilotKit + address open items
- Mobile omnibox sizing — sidebar auto-collapse + ResizeObserver + height:1px fix
- Omnibox activation guard + remove prompt debounce
- Address 5 PR review comments (threads 1, 2, 5, 6, 7)
- Restore sccache config; patch minimatch ReDoS (CVE high x2)
- Api-fetch header merge, token scope, permissionLevel default, CSP, loopback, eviction order (threads #2,3,18,23,24,25,29,33)
- Auto-scroll MAX_LINES, Enter double-fire, clipboard fallback, empty text guard, unreachable boundary, allowlist expiry (threads #19,27,28,32,35,47)
- Exec_id guard, suggestion staleness, useCallback deps, isProcessing sync, empty content (threads #1,4,26,30,31,34)
- Add missing log crate dependency for web execute module
- Address 3 code review findings (C-02, M-04, L-06)
- Bump aws-lc-sys 0.37.1 → 0.38.0 via aws-lc-rs 1.16.1
- Add missing tables/indexes to migration, fix scaling.md network (CR-B, CR-C, CR-N)
- H-03 SQL params in process.rs, spawn_blocking safety, ? operator cleanup, CLAUDE_* env passthrough
- H-03 SQL parameterization (extract/ingest/crawl), spawn_blocking, ANTHROPIC_API_KEY allowlist, sitemap tests
- Set TZ=UTC in vitest config and update snapshot timestamps
- Select requested page and scope embed to current run
- Address review comments — security, correctness, and flag propagation
- Address PR comments — MCP error sanitization, event field names, cancel safety, flag validation
- Mode ref routing, log visibility, facet limit clamps
- Arrow fns, session id, proxy headers, pulse chat, chunk fix, dispatch split
- Address PR review feedback (batch 2)
- Address PR review feedback (batch 4 - frontend)
- Address frontend PR review comments (batch 8)
- Address remaining CodeRabbit review comments (batch 9)
- Address PR review batch 10 — thread-safety, stale ref, and cleanup
- Remove pulse_chat direct-dispatch flags from ALLOWED_FLAGS
- Forward session_fallback through route handler + fix types
- Use randomUUID for message IDs + add ACP types to WsServerMsg
- Wrap onTurnComplete callback in useCallback
- Guard history sync during streaming, fix timestamp display
- Add repo/branch to sidebar filter and card display
- Add repo/branch to SessionSummary type
- Suppress biome dep warning, format shell-server.mjs
- Use apiFetch to inject x-api-key on session load
- Address API route PR review comments (threads 2,6,12,14,20,39)
- Address Pulse component PR review comments (threads 9,13,15,24,25,54,55)
- Address AI elements component PR review comments (threads 23,28,29,32,33,35,45)
- Address reboot and terminal component PR review comments (threads 42,44,46,49,50,52,53)
- Address misc/infra PR review comments (threads 1,3,4,5,7,11,34,36,37,40,41,43,51)
- Address Codex + Copilot PR review comments
- Address all PR review comments + implement SEC-7 session-scoped permission routing
- Progress display + embed list polish + crawl batch resilience (v0.13.1)
- Address 14 CodeRabbit/cubic-dev-ai PR comments
- Address 114 CodeRabbit threads + remove dead run_*_native functions (v0.13.3)
- GitHub clone auth + progress display fixes (v0.14.2)
- Remove unused LogEntry import from axon-logs-dialog
- Resolve TypeScript build errors from Plate.js untyped APIs
- Align config path to mcp.json across web/api/docs
- Harden session reliability and generic embed pipeline
- Address all 48 PR review thread comments
- Address 11 PR review comments from #42
- Clear streaming flag on message when result arrives
- Make session list loading reliable
- Fix next.config.ts typos, URL validation, and page.tsx re-export
- PR review fixes, crawl engine helpers, v0.21.2
- Locale URL filtering + status UI cleanup
- Address all 82 PR #4 review issues + upgrade deps to latest
- Extract shared CopyButton, cap stdout arrays, explicit switch cases
- Phase B review fixes — renderIntent routing + deduplicate utils
- Phase C review fixes — cancel routing, PHASE_META, ingest removal
- Phase D review fixes — hooks order, depth flag, exhaustive deps
- Resolve pulse UI lint warnings and align renderer changes
- Address PR API review threads batch 1
- Address remaining PR review threads comprehensively
- Expose axon-web on 0.0.0.0, normalize test pg_url, update snapshots
- Default save collection to AXON_COLLECTION / cortex instead of 'pulse'
- Ensure Qdrant collection exists before upsert
- Land review fixes, test env alignment, and changelog/session plumbing
- Address 6 PR review comments
- Use Number.isNaN instead of global isNaN
- Replace !important with :root specificity for slate placeholder CSS
- Remove dangling useRouter() call from omnibox
- Add Settings2 icon import to omnibox + changelog update
- Address all 12 PR review comments from cubic-dev-ai
- Fix duplicate tool badges and raw-JSON response in Pulse chat
- Prefix unused liveToolUses prop + update changelog sha
- Address all P0/P1/P2 code review issues — 8-agent team landing
- Spawn_heartbeat_task helper, Redis cancel timeouts, async I/O, unit tests
- Resolve symlink traversal and path canonicalize bypasses
- Jobs-dashboard Biome lint compliance - hook deps and unused imports
- Remove CrawlFileExplorer from results-panel, delete stub
- Remove unused selectedFile/selectFile from results-panel destructure
- Restore selectedFile/selectFile in results-panel with inline file list
- Use ExtractedSection in results-panel instead of inline file list
- Resolve inotify watch limit, EADDRINUSE port race, and node_modules ownership
- Install uvx for neo4j-memory MCP, add pnpm-dev finish script
- Pulse autosave skip file-read, pre-delete stale vectors, editor doc-reload, z-index
- Pulse autosave update-in-place + editor hardening
- Remove unused showChatRef from use-split-pane
- Remove unused verticalDragStartRef from pulse-workspace destructure
- Pulse workspace quality fixes — collapse guard, editor flex, aria
- Pulse dual-hydration race + both-collapsed restore guard
- Address Cortex dashboard review findings
- Address code review findings from Plate.js editor enhancements
- Wire AIKit into CopilotKit + address open items
- Mobile omnibox sizing — sidebar auto-collapse + ResizeObserver + height:1px fix
- Omnibox activation guard + remove prompt debounce
- Address 5 PR review comments (threads 1, 2, 5, 6, 7)
- Restore sccache config; patch minimatch ReDoS (CVE high x2)
- Api-fetch header merge, token scope, permissionLevel default, CSP, loopback, eviction order (threads #2,3,18,23,24,25,29,33)
- Auto-scroll MAX_LINES, Enter double-fire, clipboard fallback, empty text guard, unreachable boundary, allowlist expiry (threads #19,27,28,32,35,47)
- Exec_id guard, suggestion staleness, useCallback deps, isProcessing sync, empty content (threads #1,4,26,30,31,34)
- Add missing log crate dependency for web execute module
- Address 3 code review findings (C-02, M-04, L-06)
- Bump aws-lc-sys 0.37.1 → 0.38.0 via aws-lc-rs 1.16.1
- Add missing tables/indexes to migration, fix scaling.md network (CR-B, CR-C, CR-N)
- H-03 SQL params in process.rs, spawn_blocking safety, ? operator cleanup, CLAUDE_* env passthrough
- H-03 SQL parameterization (extract/ingest/crawl), spawn_blocking, ANTHROPIC_API_KEY allowlist, sitemap tests
- Set TZ=UTC in vitest config and update snapshot timestamps
- Select requested page and scope embed to current run
- Address review comments — security, correctness, and flag propagation
- Address PR comments — MCP error sanitization, event field names, cancel safety, flag validation
- Mode ref routing, log visibility, facet limit clamps
- Arrow fns, session id, proxy headers, pulse chat, chunk fix, dispatch split
- Address PR review feedback (batch 2)
- Address PR review feedback (batch 4 - frontend)
- Address frontend PR review comments (batch 8)
- Address remaining CodeRabbit review comments (batch 9)
- Address PR review batch 10 — thread-safety, stale ref, and cleanup
- Remove pulse_chat direct-dispatch flags from ALLOWED_FLAGS
- Forward session_fallback through route handler + fix types
- Use randomUUID for message IDs + add ACP types to WsServerMsg
- Wrap onTurnComplete callback in useCallback
- Guard history sync during streaming, fix timestamp display
- Add repo/branch to sidebar filter and card display
- Add repo/branch to SessionSummary type
- Suppress biome dep warning, format shell-server.mjs
- Use apiFetch to inject x-api-key on session load
- Address API route PR review comments (threads 2,6,12,14,20,39)
- Address Pulse component PR review comments (threads 9,13,15,24,25,54,55)
- Address AI elements component PR review comments (threads 23,28,29,32,33,35,45)
- Address reboot and terminal component PR review comments (threads 42,44,46,49,50,52,53)
- Address misc/infra PR review comments (threads 1,3,4,5,7,11,34,36,37,40,41,43,51)
- Address Codex + Copilot PR review comments
- Address all PR review comments + implement SEC-7 session-scoped permission routing
- Progress display + embed list polish + crawl batch resilience (v0.13.1)
- Address 14 CodeRabbit/cubic-dev-ai PR comments
- Address 114 CodeRabbit threads + remove dead run_*_native functions (v0.13.3)
- GitHub clone auth + progress display fixes (v0.14.2)
- Remove unused LogEntry import from axon-logs-dialog
- Resolve TypeScript build errors from Plate.js untyped APIs
- Align config path to mcp.json across web/api/docs
- Harden session reliability and generic embed pipeline
- Address all 48 PR review thread comments
- Address 11 PR review comments from #42
- Clear streaming flag on message when result arrives
- Make session list loading reliable
- Fix next.config.ts typos, URL validation, and page.tsx re-export
- PR review fixes, crawl engine helpers, v0.21.2
- Web integration security, protocol, and performance fixes (v0.23.0)
- Address TypeScript PR review issues (threads 2,9,12,15)
- Complete crates/web review remediation + embed worker crash fix (v0.23.1)
- Complete crates/web review remediation + embed worker crash fix (v0.23.1)
- Address all PR review findings — security, monolith, test hardening (v0.23.2)
- V0.23.3 — amortized rate-limit eviction, sentinel hardening, elicit error mask, docs
- Pipeline resilience — skip failed docs, upsert-first, tune retry budget
- Pass full error to logError in API catch blocks (PR #5,6,8,9,12,15,19,21)
- Clamp timeout values before persisting in settings pane
- Clear stale error state on successful MCP server save
- Use childrenArray as useEffect dependency instead of length
- Remove duplicate env-var table from README
- Stabilize createSetStateActionBridge refs with useMemo — prevent subscription churn (P1)
- Wire forceRefresh option through useAxonSession to fetchSessionWithRetry
- Extract RUNTIME_EVENT_TYPES as single source of truth for WS event allowlist
- Add generation-based cancellation to onSessionInfoUpdate
- Switch to chat pane after mobile assistant selection in sidebar
- Use shared PANE_WIDTH_MIN constant instead of hardcoded 320px
- Use non-blocking emit in stderr reader and finalization — prevent channel backpressure hang
- Restore MCP card navigation after href removal
- Advance search_start by chunk_len minus overlap for correct line ranges
- Reset session generation counter on manual session switch to prevent stale session_info_update overwrite
- Address all 13 PR review comments from cubic-dev-ai
- Bump web deps and enforce CI audit
- Address PR comments #4,#5,#6,#7,#8,#9,#10,#11
- Address PR review threads 2,3,7,8,9,11,15,16 — TS/web security and correctness
- Remove unused type import, use direct re-export for CopilotStreamEvent
- Move hnsw_ef to dense prefetch arm, use inspect_err, dispatch in evaluate
- Address frontend, docker, and changelog review issues
- Named-vector support and error resilience in compute_similarity
- Validate smoke coverage across full and lite modes
- Normalize embed json contract and clean test warnings
- Split shell state and harden follow-up tooling
- Address PR #60 review comments — hooks, canvas, preflight
- Wire MCP start handlers to service context; fix crawl/embed early-return
- Fix mcporter test suite — help routes, export guard, graph message, schedule conditioning
- Close DNS rebinding TOCTOU window via SsrfBlockingResolver (H3)
- Address PR #59 review comments — all 39 threads
- Resolve Box<dyn Error> Send+Sync bounds in graph context and worker

