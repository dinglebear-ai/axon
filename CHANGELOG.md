# Changelog
Last Modified: 2026-03-15 (session: v0.25.0 — Pulse shell redesign, AI elements, hybrid search, new API routes)

## [0.25.0] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` relative to `main` (`96773a08`).

### Highlights

- **Pulse shell redesign** — comprehensive overhaul of all shell components: `axon-shell.tsx`, `axon-shell-state.ts`, `axon-sidebar.tsx`, `axon-prompt-composer.tsx`, `axon-message-list.tsx`, `axon-mcp-pane.tsx`, `axon-settings-pane.tsx`, `axon-terminal-pane.tsx`, `axon-logs-dialog.tsx`, `axon-pane-handle.tsx`, `axon-shell-resize-divider.tsx`, mobile pane switcher, density selector, and canvas profile selector.
- **AI elements components** — new structured components for AI conversation rendering: `conversation.tsx`, `confirmation.tsx`, `message.tsx`, `queue.tsx`, `tool.tsx` for displaying ACP turn results in the shell.
- **Hybrid vector+sparse search** — new `crates/vector/ops/qdrant/hybrid.rs` combining dense embedding and sparse BM25 retrieval for improved recall; sparse query support added to `ops/sparse.rs` and wired into `query.rs`.
- **New API routes** — `/api/ai/chat` (SSE LLM streaming), `/api/ai/command` (Plate.js editor AI), `/api/logs` (Docker container log SSE stream), `/api/workspace` (filesystem browser).
- **New UI primitives** — `alert-dialog.tsx`, `card.tsx`, `progress.tsx`, `sheet.tsx`, `skeleton.tsx` from shadcn/ui added to component library.
- **TEI pipeline refactor** — `qdrant_store.rs` split into `qdrant_store/` module, `code_embed.rs` deleted (logic merged into pipeline), `text_embed.rs` consolidated; `pipeline.rs` restructured for clearer chunking + embed + upsert flow.
- **DB migration** — `migrations/002_job_status_indexes.sql` adds composite indexes on job status columns for query performance.
- **Server-side jobs lib** — `apps/web/lib/server/jobs.ts` extracted as shared server-side job querying layer; `/api/jobs` and `/api/jobs/[id]` routes updated to use it.
- **Shell-store** — `lib/shell-store.ts` and `axon-shell-state.ts` refactored; `use-is-mobile.ts` hook added.
- **CLI/crawl improvements** — crawl audit manifest, job contracts, `crates/cli/commands/common.rs` hardening.
- **Config** — `build_config.rs`, `config.rs`, `config_impls.rs` updated for new feature flags.
- **Docs** — `ARCHITECTURE.md`, `JOB-LIFECYCLE.md`, all ingest docs updated; `CLAUDE.md` files refreshed for `crates/ingest`, `crates/vector`, `crates/cli`, and `apps/web`.

### Commits since `main` (`96773a08`)

| SHA | Message |
|-----|---------|
| (pending) | feat(web,vector): Pulse shell redesign, AI elements, hybrid search, new API routes (v0.25.0) |

## [0.24.1] — fix/embed-pipeline-resilience

This section documents commits on `fix/embed-pipeline-resilience` relative to `main` (`e9353d67`).

### Highlights

- **Embed pipeline resilience (v0.24.1)** — three structural fixes for the embed pipeline identified via systematic debugging of production ingest failures: (1) pipeline skip-and-continue — `run_embed_pipeline` now catches per-doc errors and continues instead of aborting the entire batch, tracking failures in `EmbedSummary.docs_failed`; (2) upsert-first pattern — removed pre-delete step that caused permanent data loss when TEI timed out mid-embed, replaced with deterministic UUID v5 point IDs (overwrite on upsert) followed by `qdrant_delete_stale_tail` only after successful upsert; (3) TEI retry budget tuning — `TEI_MAX_RETRIES_DEFAULT` reduced from 10 to 5, worst-case budget 181s fits inside 300s doc timeout. Two new tests: `embed_summary_exposes_docs_failed` and `tei_max_retries_default_fits_doc_timeout`.

### Commits since `main` (`e9353d67`)

| SHA | Message |
|-----|---------|
| 96773a08 | fix(embed): pipeline resilience — skip failed docs, upsert-first, tune retry budget (v0.24.1) |

## [0.24.0] — main

This section documents commits on `main` relative to `fe11a78d`.

### Highlights

- **Scrape format params, search pagination, TEI chunking metadata, Qdrant retry (`e9353d67`)** — MCP scrape format parameters, search result pagination, TEI chunking metadata fields, and Qdrant upsert retry logic.
- **GitHub ingest code review fixes (`954f480c`)** — all code review findings addressed for ingest/github module.
- **Centralized heartbeat (`a8fae674`)** — heartbeat logic moved into `worker_lane` via `wrap_with_heartbeat`, eliminating per-worker duplication.
- **Batch pipeline deletion (`89c4011d`)** — removed `EmbedDocument`, `embed_documents_batch`, and `embed_pipeline.rs` in favor of unified `PreparedDoc` pipeline.
- **Unified PreparedDoc pipeline (`8d22e7f5`–`99dfb55d`)** — migrated sessions, reddit, youtube, github issues/PRs/wiki/metadata, and github files to the unified `PreparedDoc` embed pipeline.
- **PreparedDoc metadata fields (`95add431`)** — `source_type`, `content_type`, `title`, `extra` fields added to `PreparedDoc`, exposed `embed_prepared_docs` entry point.
- **Pre-chunking optimization (`1a78dc82`)** — github files pre-chunked before TEI batching, eliminating 413 fallback path.
- **Worker CPU tuning (`5802ff62`)** — CPU-based lane defaults and async stale-tail deletes for better throughput.

## [Unreleased] — feat/web-integration-review-fixes

This section documents commits on `feat/web-integration-review-fixes` relative to `main` (`fe11a78d`).

### Highlights

- **Secondary violation fixes (v0.23.3)** — six targeted fixes from follow-up review: (1) `rate_limiter.rs` O(N) `retain()` on every request replaced with amortized sweep (AtomicU64 gate, at most once per 60s window); (2) `handlers_elicit.rs` raw error detail `{e}` no longer forwarded to MCP client — logged server-side, generic `"elicitation failed"` returned; (3) `ws_send.rs` sentinel type hardcoded to `"log"` (was preserving original event type, producing malformed `command.*` messages without required `ctx` fields); (4) `config/mcporter.json` hardcoded `/home/jmagar/.local/bin/axon` replaced with portable `axon` (assumes PATH); (5) `docs/MCP-TOOL-SCHEMA.md` corrected: `auto-inline` added to `ResponseMode` enum, `path` field marked required only for `head|grep|wc|read|delete` (not `list|search|clean`), `pattern` noted as required for both `grep` and `search`; (6) `elicit_demo` added to `action:help` discoverable action map in `handlers_system.rs`.

- **All PR review findings addressed (v0.23.2, `ebd54fd6`)** — complete batch of security, monolith, and test-hardening fixes: 14 Dependabot vulnerabilities resolved (13 npm via `pnpm.overrides`, 1 Rust `quinn-proto` via `cargo update`); `WEB-INTEGRATION-REVIEW.md` removed from branch git history via `git-filter-repo`; 15 PR review threads marked resolved; MCP elicitation wired (`rmcp` feature enabled, `ElicitDemoRequest`/`AxonRequest::ElicitDemo` defined, `handlers_elicit.rs` handler integrated); `ws_handler.rs` rate-limit extracted to `ws_handler/rate_limiter.rs`; `docker_stats.rs` tests split to `docker_stats/tests.rs`; `execute/cancel.rs` helpers extracted; `fingerprint_mcp_servers` uses SHA-256 instead of raw JSON; `session_cache::read_replay_buffer` delegates to `drain_replay_buffer`; worker lane env-var tests save/restore state; `axon-ws-exec.ts` cancel guarded on `backendJobId`; `axon-shell-state.ts` explicit rejection on non-auto-approve path; `ws-protocol.ts` named interfaces extracted.

- **`crates/web` comprehensive review — all 31 findings resolved (v0.23.1)** — complete remediation of all P0/P1/P2/P3 and security findings from the 2026-03-13 review. Security: loopback PTY auth bypass removed (SEC-1); ACP `enable_fs`/`enable_terminal` capability flags now threaded end-to-end (SEC-2); `DefaultHasher` replaced with JSON string key for ACP session cache (SEC-3); empty `session_id` filtered to `None` to prevent system-prompt bypass (SEC-4); `subtle::ConstantTimeEq` replaces hand-rolled token comparison to eliminate length side-channel (SEC-6). Critical/high: `MutexGuard` held across `.await` fixed; `dispatch_search_and_info_modes` split below 120-line hard limit; `ws_handler.rs` reduced 510→432 lines via test module extraction; all `.expect()`/`.unwrap()` in production paths replaced; `JoinSet` cleanup on WS disconnect; `session_ownership` DashMap cleanup on disconnect; rate-limit state moved to process-wide `AppState` `DashMap` keyed by client IP (bypassed reconnect); `docker_stats` task wrapped in restart loop. Medium: `handle_command` refactored to accept `ExecCommandContext` directly (removes 8-param lint suppress); `handle_ws`/`handle_ws_message`/`handle_pulse_chat` all reduced below 80-line warn threshold via helper extraction; `spawn_blocking` for `resolve_exe()` filesystem probes; `LazyLock` for env var caching; page-cache subtracted from Docker memory metrics. Low: `biased;` added to forward task `select!` (output prioritized over stats); `crawl_files` detection changed from fragile substring scan to typed JSON struct; rate-limit errors now use `WsEventV2::CommandError` envelope; `read_file` messages rate-limited; dead `WsEventV2::JobStatus`/`JobProgress` variants removed; `ASYNC_SUBPROCESS_MODES` empty constant removed; `POLL_INTERVAL_MS` corrected 1000→500ms. Also: `ws_handler.rs` new module `crates/web/ws_handler/` with extracted tests; 1266 lib tests passing.

- **Embed worker crash fix** — `poll_next_delivery` in `crates/jobs/worker_lane/amqp.rs` returned `Ok(None)` when an in-flight future completed via `FuturesUnordered::next()`, which `parse_delivery_result` correctly mapped to `DeliveryOutcome::Break` (consumer stream ended), terminating the lane. Fixed by returning `timeout(Duration::ZERO, pending())` → `Err(Elapsed)` → `Continue` instead. Regression test added in `worker_lane.rs`.

- **Web integration full-review fixes (v0.23.0)** — 5 critical and 12 high findings from a comprehensive `apps/web ↔ crates/web` integration review addressed across 20 files. Security: `check_auth()` now reads `Authorization`/`x-api-key` headers (tokens no longer forced into query strings / access logs); CORS preflight uses an explicit header allowlist instead of reflecting arbitrary client headers; ACP sessions are bound to originating WS connection (cross-session interference prevented); shell PTY input capped at 64 KB; debug-build auth bypass now emits a prominent `log::warn!`. Protocol: `acp_resume_result` field renamed `success` → `ok` to match TypeScript Zod schema (session resume was silently broken); `permission_request` ACP events fully wired through TypeScript WS handler; all `format!()`-based JSON replaced with `serde_json::json!()` (injection-safe); four ACP permission flags (`enable_fs`, `enable_terminal`, `permission_timeout_secs`, `adapter_timeout_secs`) wired through `ALLOWED_FLAGS` → `params.rs` (UI controls now functional). Performance: WS channel-full drops replaced with visible `[output truncated]` sentinel; sync-mode concurrency semaphore added (`AXON_MAX_SYNC_CONCURRENT`, default 16); per-connection WS execute rate limiting (120 req/60s); dead ACP adapter evicted from `SESSION_CACHE` on `run_turn()` error; `axon-ws-exec.ts` singleton sends abort-triggered cancel to server and caps pending map at 100. Code quality: `NO_JSON_MODES` updated to reflect service-layer routing; `pulse_chat_probe`/`mcp_refresh` documented as internal-only; editor system prompt extracted to named constant; 22 pre-existing TypeScript `noUncheckedIndexedAccess` test errors fixed; 862 tests passing.

## [Unreleased] — fix/pr-review-fixes-crawl-refactor

This section documents commits on `fix/pr-review-fixes-crawl-refactor` relative to `main` (`82ecd6e1`).

### Highlights

- **PR review fixes + crawl engine refactoring (v0.21.2)** — eight code-review findings addressed: `.expect()` in `handlers_broker.rs` replaced with `unreachable!()`; `i64 as i32` clamp added in `orphaned_pending_threshold_secs`; SQL interval changed from string-concat to `make_interval(secs => $2)`; TOCTOU window documented with concurrency-safety comment; `SideBySideBuffer::push` wildcard arm replaced with `log_warn`; two unit tests added for threshold floor and SQL query construction; `#[tokio::test]` → `#[test]` on synchronous OAuth cookie tests; scaffolding `#[allow(dead_code)]` upgraded to `#[expect(dead_code)]` where cross-module references permitted. Crawl engine: `prepare_crawl_output_dir` extracted from `run_crawl_once`; `enqueue_robots_sitemaps` added for `robots.txt` sitemap discovery; `save_partial_cancel_result` added for graceful-cancel partial persistence.

- **Services layer migration complete + contract tests (`ca7831c0` window)** — all CLI commands, MCP handlers, and web sync modes route through `crates::services::*`; dead-code exports (`run_evaluate_native`, `run_suggest_native`) removed; `watch` CLI command migrated to service layer; MCP contract parity tests hardened; map migration and scrape contract tests added.

- **Session lifecycle hardening + tooling cleanup (`b39e83a0`)** — ACP/web/MCP lifecycle behavior and developer tooling were hardened in a single branch-head commit.

- **Web performance/a11y hardening + ACP reliability follow-through (post-`e1e612c6`)** — landed five branch-head commits: web performance and accessibility improvements (`fb7a9f87`, `14d8edd3`), ACP session persistence through WebSocket disconnects (`4663ce65`), and shell/session UX reliability fixes for streaming/session list behavior (`80a7e21d`, `356ea87a`).

- **Branch head sync (post-`5682daa2`)** — documented two previously missing branch-head commits: ACP session/config persistence hardening (`bbc1684b`) and GitHub TEI batch embedding performance improvements (`e1e612c6`).
- **Assistant mode in Reboot sidebar and ACP path isolation (v0.18.0)** — added `assistant` rail mode with dedicated session list (`/api/assistant/sessions`), `useAssistantSessions` hook, and shell wiring for separate assistant session continuity; pulse chat now accepts `assistant_mode` and resolves CWD to `$AXON_DATA_DIR/axon/assistant` (fallback `~/.local/share/axon/axon/assistant`) with per-agent+mode ACP connection scoping.
- **MCP config path alignment (v0.18.1 window)** — normalized config-path expectations to `mcp.json` across web/server/docs flows to remove path drift between UI settings and backend resolution.

- **Verification hardening + pre-existing gate cleanup** — fixed pre-existing failing tests/clippy issues (`await_holding_lock`, `collapsible_if`, env-coupled health assertions, refresh DB test skip behavior) so `just verify` passes cleanly; aligned web lint configuration for upstream PlateJS-derived components via scoped Biome overrides and removed stale suppressions.

- **Reboot UI cutover — chat-first interface promoted to root route (v0.16.0)** — `AxonShell` (the reboot UI) promoted from `/reboot` to `/`; legacy dashboard preserved at `/legacy`; `AppShell` sidebar guard updated to `hideAppSidebar` covering `/`, `/legacy`, `/reboot`; sidebar page links updated (removed duplicate `/reboot` entry, added `/legacy`); Docker stats wired to NeuralCanvas intensity via `canvasRef` + `useAxonWs` subscription (`command.done`/`command.error` pulse, CPU-normalized idle intensity); message edit and retry callbacks implemented (trim-and-resubmit pattern); settings dialog added with canvas profile picker (current/subtle/cinematic/electric/zen) persisted to localStorage; `useLogStream` hook extracted from `AxonLogsDialog` to eliminate SSE duplication with `logs-viewer.tsx`; TypeScript build errors from Plate.js untyped APIs resolved; 771 tests passing, Next.js build clean

- **GraphRAG scaffolding** — `crates/core/neo4j.rs`, `crates/jobs/graph.rs`, `crates/jobs/graph/worker.rs`, `crates/services/graph.rs` stubs added; `ServiceResult` gains graph-related variants; `rust-toolchain.toml` updated

- **MCP transport/docs alignment + shell completions/CORS/crawl output hardening (v0.15.0)** — `feat(mcp)` adds stdio + dual transport support (`a3c1f18e`), docs/env alignment for MCP transport settings (`ef2c4fad`), and feature-level CLI/web hardening for shell completions, CORS/origin handling, and crawl output path behavior (`3d3f9d98`); includes ingest progress fix baseline in this unreleased window (`e462931f`)

- **GitHub ingest progress display fixes (v0.14.2)** — three bugs fixed: (1) `Authorization: Bearer` → `Authorization: token` for classic GitHub PATs (`ghp_`) in `files.rs` and `wiki.rs`; (2) added unauthenticated clone fallback for public repos; (3) final progress send (`5/5 tasks, chunks_embedded`) added after `tokio::join!` completes in `github.rs`; (4) `ingest_metrics_suffix()` completed branch in `metrics.rs` now handles `tasks_total` — `axon status` shows `5/5 tasks | N chunks` for completed GitHub ingests

- **GitHub code-aware chunking + git clone performance + Qdrant tuning (v0.14.1)** — `embed_code_with_metadata()` added to `crates/vector/ops/tei.rs` — tries tree-sitter AST-aware chunking (Rust, Python, JS, TS, Go, Bash) with fallback to 2000-char prose; unified `GitHubPayloadParams` builder in `crates/ingest/github/meta.rs` produces 31 `gh_*` structured metadata fields per chunk; `--no-source` flag (source code included by default); GitHub repo re-ingest via refresh schedules gated on `pushed_at`; **performance**: replaced per-file HTTP API fetches with `git clone --depth=1` — 10K+ individual requests → single clone operation (biomejs/biome: 30+ min → seconds); live progress tracking via `UnboundedSender<serde_json::Value>` channel from `embed_files()` → DB writer task in `process.rs`; progress displays task-level phase, file-level counts, and final chunks in both `axon ingest list` and `axon status`; Qdrant `production.yaml` config added (on-disk payload + vectors + HNSW, memmap threshold 20KB); docker-compose gains Qdrant memory limits (1G–4G); `ssh_auth.rs` test cleanup (base64_encode moved inside test module)

- **Web auth hardening + Pulse workspace improvements + CLI cleanup (v0.14.0)** — SSH key auth (`crates/web/ssh_auth.rs`) validates SSH public keys from `~/.ssh/authorized_keys` or `AXON_SSH_AUTHORIZED_KEYS`; dual-auth mode (`AXON_REQUIRE_DUAL_AUTH`) requires both Tailscale identity AND API token; Tailscale auth module hardened with configurable allowed users/networks; Pulse workspace gains dedicated logs/MCP/terminal panes (`pulse-logs-pane.tsx`, `pulse-mcp-pane.tsx`, `pulse-terminal-pane.tsx`); mobile pane switcher improved; `use-split-pane` rewritten for new pane layout; proxy middleware updated; `axon.subdomain.conf` deleted (superseded by Tailscale auth); CLI: `spider_capture.rs` dead code deleted; `map.rs`/`scrape.rs`/`screenshot.rs` cleaned up; crawl runtime DB helpers expanded; AMQP channel improvements; `suggest.rs` simplified; `vector/ops/input` split into module; web download handler hardened; new `.env.example` entries for auth settings; `auth/` docs added

- **Sync dispatch refactor + session guard scaffold (v0.13.2)** — `dispatch_service` split into focused per-mode helpers (`dispatch_query_modes`, `dispatch_acp_modes`, etc.) to keep the top-level router concise; `session_guard.rs` added as `pub(crate)` module under `crates/web/execute/` — polls `~/.claude/projects/` for `{session_id}.jsonl` after a Pulse turn completes (100ms × 50 retries); `#![allow(dead_code)]` suppresses warnings while the call site is wired; `AcpConn` type alias simplifies signatures in `acp_adapter.rs`; `subprocess.rs` restructured for cleaner fallback path; `pulse_chat.rs` session-file integration points added; ACP WS event tests updated to cover new event shapes


- **Ingest progress display + embed list polish + crawl batch resilience (v0.13.1)** — `axon status` now shows live YouTube ingest progress (`videos_done/total`, `enumerating…` placeholder) via `result_json` COALESCE merge on completion; `axon embed list` displays rich per-job rows (target, metrics, collection, age, error) reusing `status/metrics` helpers (made `pub(crate)`); `crawl_batch` downgrades excluded-URL errors to warnings and only hard-fails if all URLs are excluded; `find_excluded_prefix` replaces `is_excluded_url_path` with clearer error message; `YoutubeVideoMeta` gains `video_id` + `thumbnail` fields stored as `yt_video_id`/`yt_thumbnail` Qdrant payload

- **Multi-agent sessions sidebar (v0.13.0)** — `/reboot` sessions sidebar now surfaces Claude, Codex, and Gemini sessions with colored agent badge pills (CX green, G blue); `codex-scanner.ts` walks `~/.codex/sessions/{year}/{month}/{day}/*.jsonl`, `gemini-scanner.ts` walks `~/.gemini/tmp/{hash}/chats/session-*.json`; `session-utils.ts` extracted to break circular Turbopack module dependency; `codex-jsonl-parser.ts` + `gemini-json-parser.ts` parse history for the detail view; `[id]/route.ts` branches on `session.agent` to select the correct parser; per-agent representation guarantee (≥3 from each agent type) in the list route prevents all-Claude results when Claude sessions are most recent; `axon-shell.tsx` auto-switches agent selector to Codex/Gemini when a non-Claude session is clicked

- **Unified `axon ingest` + structured metadata (v0.12.0)** — replaced three separate ingest commands (`axon github`, `axon reddit`, `axon youtube`) with a single `axon ingest <target>` that auto-detects source from input (GitHub slug/URL, YouTube URL/@handle/bare ID, Reddit r/name or URL); `crates/ingest/classify.rs` added with 17 tests; `gh_*` structured metadata added to all GitHub Qdrant chunks (repo, issue, PR) via new `crates/ingest/github/meta.rs`; `reddit_*` metadata added to all Reddit chunks (both subreddit listing and thread URL paths) via new `crates/ingest/reddit/meta.rs`; `regex` crate moved from `[dev-dependencies]` to `[dependencies]` (was breaking MCP compilation); `AntiBotTech.as_ref()` removed in collector.rs (spider updated enum from `Option<AntiBotTech>` to `AntiBotTech`)

- **ACP performance + scalability fixes + modern Rust (v0.11.2)** — all 19 findings from the ACP performance/scalability analysis addressed: `crates/services/acp.rs` split from 2060-line monolith into a proper module (`acp/bridge.rs`, `acp/adapters.rs`, `acp/config.rs`, `acp/mapping.rs`, `acp/runtime.rs`, `acp/session.rs`); `Arc<Mutex<AcpRuntimeState>>` replaced with `OnceLock` + `RefCell` (no lock on streaming token hot path); `Arc<Mutex<HashMap>>` permission map replaced with `DashMap`; double `serde_json::to_value`+`to_string` on every streaming token replaced with direct `to_string` + string-concat envelope (FINDING-5); `tokio::runtime::Builder` with configurable `max_blocking_threads` replaces `#[tokio::main]` default (FINDING-6); `AdapterGuard` RAII kills subprocess on drop covering all error paths; `select! { biased; }` drains events before checking process exit; MCP server config TTL cache added; ACP session concurrency semaphore (`AXON_ACP_MAX_CONCURRENT_SESSIONS`, default 8); FINDING-14 fully fixed: exit watcher `drop(exit_tx)` on clean exit instead of `send(String::new())` — receiver `Err` = clean shutdown, `Ok(msg)` = crash; `mod.rs` → `.rs` files (`acp/mod.rs` → `acp.rs`, `types/mod.rs` → `types.rs`) per Rust 2018 module conventions; all clippy warnings resolved with `#[expect]` (not `#[allow]`)

- **dev-setup bootstrap script (v0.11.1)** — `scripts/dev-setup.sh` auto-detects arch, installs `just` prebuilt, auto-generates secrets on first `.env` creation, prompts for `AXON_DATA_DIR`, pre-creates container data directories, starts test infra and populates test env URLs; `Justfile` gains `test-infra-up`/`test-infra-down` recipes; hook script paths made portable via `git rev-parse`

- **Test coverage expansion — web app + Rust crates (v0.11.1)** — 914 new tests across 18 files: 6 new TypeScript test files (`api-fetch`, `api/cortex-routes`, `api/sessions-routes`, `api/workspace-route`, `pulse-chat-api-lib`, `pulse-session-store`) + 5 expanded TS test files; Rust tests added to `crates/web/` (execute/args, execute/cancel, execute/files, execute/overrides, download/archive, docker_stats, pack) and `crates/services/` (acp, events, query, search, system, types); two bugs fixed: `pushCapped` array spreading via `items.concat(item)` → `[...items, item]`, `window.localStorage` SSR guard added via `getLocalStorage()` helper with `typeof window !== 'undefined'` check; zip-slip vulnerability documented in `build_zip` (entry path stored verbatim); `LogLevel` case-sensitivity documented (`"WARN"` → `Info`); XML single-quote escaping gap documented in `pack.rs`

- **AxonShell real ACP/session wiring + UI polish (v0.11.0)** — `useAxonSession` hook added for JSONL session history fetch; `useAxonAcp` hook added for real ACP WebSocket prompt submission with `randomUUID` message IDs; `useAxonSession` behavioral tests added; `AxonShell` wired to real session data and ACP WebSocket; `AxonSidebar` wired to real `SessionSummary` list with repo/branch filter; git enrichment hoisted to outer project loop in sessions ingest; `SessionFallback` event emitted on failed session resume and handled in Pulse stream pipeline; `Reboot*` components renamed to `Axon*`, `REBOOT_` constants renamed to `AXON_`; `onTurnComplete` wrapped in `useCallback`; history sync guarded during streaming; timestamp display fixed; loading/error states added to `AxonMessageList`; `AxonPromptComposer` submit disabled during streaming with spinner; sessions fix: `apiFetch` injects `x-api-key` on session load; biome dep warning suppressed in shell-server.mjs; Rust: services `events.rs`, MCP `config.rs`/`server.rs`, crawl engine, ingest, jobs, vector ops, web crate all hardened/refactored; new `align-kit.tsx` editor plugin; `mcp-config.tsx` component added

- **Reboot UI shell + logs SSE fix + infra repairs (v0.9.0)** — reboot section fully redesigned: deleted legacy `data.ts`, `lobe-shell.tsx`, `reboot-home.tsx`, `reboot-scene.tsx`, `workflow-shell.tsx`; added `reboot-message-list.tsx`, `reboot-prompt-composer.tsx`, `reboot-sidebar.tsx`, `reboot-terminal-pane.tsx`, `reboot-pane-handle.tsx`, `reboot-logs-dialog.tsx`, AI element components (`chain-of-thought.tsx`, `confirmation.tsx`, `prompt-input.tsx`, `tool.tsx`); hooks `use-copy-feedback.ts`, `use-mcp-servers.ts`, `use-workspace-files.ts` added; logs SSE viewer fixed: three bugs eliminated (premature stream close when stopped containers finished, wrong default service `axon-web`→`all`, `EventSource` replaced with `fetch()` + `Authorization: Bearer` to satisfy proxy auth gate); `next.config.ts` gains `allowedDevOrigins: ['axon.tootie.tv']` silencing cross-origin dev warning; `AXON_WEB_ALLOWED_ORIGINS` already included `https://axon.tootie.tv` covering API routes and shell WebSocket; reboot page routes and reboot-frame/reboot-shell/reboot-pane-handle layout wired; Justfile `dev` target updated; Dockerfile updated

- **Zed alignment + ACP permission plumbing (v0.8.0)** — 5 parallel agents implemented Zed-aligned patterns: session list/resume (`use-pulse-sessions.ts`, `session-store.ts`), tool call terminal rendering (`tool-call-terminal.tsx`), permission modal UI (`permission-modal.tsx`), process exit monitoring, targeted entry updates; `PermissionResponderMap` type wired through WS handler → execute bridge → ACP bridge client using `std::sync::Mutex` + `tokio::sync::oneshot` for cross-runtime communication; `permission_response` WS message type added with `tool_call_id`/`option_id` fields; 60s auto-approve timeout fallback prevents session hangs; `AXON_ACP_AUTO_APPROVE` env var controls behavior (default `true`); 3 pre-existing TS build errors fixed (`route.ts` model type, `claude-stream-types.ts` model lookup, `pulse-chat-helpers.ts` agent type); reboot page scaffolding added; shadcn accordion/collapsible/hover-card/button-group components added

- **CSS selector scoping + markdown cleanup (v0.7.5)** — new `--root-selector` and `--exclude-selector` CLI flags thread `SelectorConfiguration` through all crawl/scrape/embed/sitemap/refresh paths; `build_selector_config()` constructs the config from `Config` fields; `clean_markdown_whitespace()` collapses excessive newlines (3+→2) and horizontal spaces (2+→1) post-transform, applied in collector, cdp_render, thin_refetch, and to_markdown; MCP `ScrapeRequest` gains `root_selector`/`exclude_selector` fields; Pulse debug logging added to omnibox execution, handlePrompt, and workspace prompt dispatch

- **ACP comprehensive review fixes (v0.7.4)** — 30 unique findings fixed across security, performance, and code quality: model argument injection guard (`validate_model_string`), env allowlist in `spawn_adapter` (env_clear + 12 vars), 5-minute adapter lifecycle timeout, `LogLevel` enum replacing raw strings (30+ call sites), `try_send` event loss logging, double mutex → single lock, `std::fs` → `tokio::fs`, dead code removal, duplicate function merge, `Serialize` derives on all ACP types with serde rename, hand-rolled JSON → `serde_json::to_value`, channel capacity 32→256, `toolsRestrict` regex tightened to match backend `TOOL_ENTRY_RE`, `--dangerously-skip-permissions` gated behind `AXON_ALLOW_SKIP_PERMISSIONS`, `response.body!` null guard, localStorage Zod validation, `handlePrompt` split 268→155 lines, dual config state unified, config probe caching (60s TTL), 5 localStorage effects consolidated to 2

- **Regression tests for ACP env isolation (v0.7.3)** — `tests/services_acp_spawn_env.rs` (3 tests) locks in `spawn_adapter()` env stripping: `CLAUDECODE`, `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` must never leak to child process; uses process-level `Mutex` to serialize env mutations; `#![allow(unsafe_code)]` at file scope with `#[allow(clippy::await_holding_lock)]` per test; credentials staged into `axon-web` via `16-materialize-agent-credentials` cont-init.d

- **Pulse Chat local dev fixed (v0.7.2)** — two root causes identified and fixed: (1) `CLAUDECODE` env var inherited from parent Claude Code session blocked `claude-agent-acp` from spawning the `claude` CLI ("Claude Code cannot be launched inside another Claude Code session") — fixed by `command.env_remove("CLAUDECODE")` in `spawn_adapter()`; (2) `acp.rs` was double-wrapping `assistant_text` in a JSON object before passing it as `AcpTurnResultEvent.result`, causing `parseClaudeAssistantPayload` to extract raw JSON instead of the assistant's text — fixed by passing `assistant_text` directly; added `17-materialize-claude-credentials` cont-init.d for Docker credential staging; `docker-compose.yaml` mounts host Claude credentials read-only into workers container; `constants.rs` updated with Pulse Chat WS mode constant

- **Services layer refactor complete (v0.5.0)** — `crates/services/` is now the single source of business logic; CLI/MCP/WS are thin transport adapters; `crawl`/`extract`/`embed` modes use fire-and-forget direct service enqueue (no subprocess); `github`/`reddit`/`youtube` remain on subprocess fallback due to `!Send` constraint; `polling.rs` deleted; 971 tests passing
- **PR review threads fully resolved (v0.7.1)** — all 154 review threads on `feat/services-layer-refactor` addressed across 10 batches; fixes cover security hardening (env mutation serialization, port binding to localhost), stale React ref cleanup (`isBackgroundRef` on background error), `AbortController` dedup via `tabsRef`, trivial wrapper removal (`map_map_payload` inlined), and a range of typed errors, fail-fast mappers, probe uniqueness, MCP error sanitization, and flag validation
- **Pulse ACP agent selection + routing (v0.7.0)** — Pulse UI now supports selecting `claude`/`codex`; selection persists in workspace state/localStorage; `/api/pulse/chat` forwards `agent` to ws flags; `pulse_chat` sync mode resolves per-agent ACP adapter env overrides (`AXON_ACP_CLAUDE_ADAPTER_*`, `AXON_ACP_CODEX_ADAPTER_*`) with fallback to shared `AXON_ACP_ADAPTER_*`; replay cache key now includes `agent` to prevent cross-agent replay collisions
- **Scrape/embed stabilization** — fixed scrape page selection and constrained embed operations to the current run for deterministic indexing behavior
- **Release v0.6.0** — web workspace/sidebar updates landed with TEI retry behavior hardening and release/documentation refresh
- **Editor tab bar + tabs hook** — new `apps/web/components/editor-tab-bar.tsx`, `apps/web/hooks/use-tabs.ts`, `apps/web/lib/pending-tab.ts`, `apps/web/lib/result-to-markdown.ts` for multi-tab editor UX
- **CmdK palette improvements** — `CmdKOutput`, `CmdKPalette`, `cmdk-palette-dialog.tsx`, `cmdk-palette-types.ts` updated for better JSON/output display
- **MCP common.rs expansion** — `crates/mcp/server/common.rs` (+99 lines) with shared helpers; `handlers_system.rs` updated
- **Scripts + docker hardening** — `scripts/cache-guard.sh`, `scripts/check_docker_context_size.sh`, `scripts/check_dockerignore_guards.sh` added; `docker-compose.yaml`, `.dockerignore`, `scripts/rebuild-fresh.sh`, `lefthook.yml`, `Justfile` updated
- **Docs updated** — `docs/MCP-TOOL-SCHEMA.md`, `docs/OPERATIONS.md`, `docs/TESTING.md`, `README.md`, `.env.example` refreshed
- **Post-v0.4.0 stabilization** — fixed MCP OAuth smoke env handling and serialized crawl DB tests to reduce flakes; fixed 4 failing CI checks; pinned Vitest timezone (`TZ=UTC`) and refreshed snapshots for deterministic test output
- **Release prep + execution hardening (v0.4.1)** — updated web/container/docs env wiring and token guidance (`AXON_WEB_API_TOKEN`/`NEXT_PUBLIC_AXON_API_TOKEN`), refreshed Docker/compose defaults, and fully hardened the services-layer refactor execution plan with strict preflight, safety rails, and parallel-worker dispatch protocol
- **Full codebase security & quality review (v0.4.0)** — comprehensive 5-phase review covering 244 Rust + 424 TypeScript files; 40 Phase 1 findings (3 Critical, 7 High, 17 Medium, 13 Low) + 17 CodeRabbit findings all addressed; WS OAuth bearer token gating added; all `format!` SQL → parameterized queries (H-03); `Secret<T>` wrapper with `[REDACTED]` debug; `ConfigOverrides` + sub-config scaffolding (A-H-01); `Config::test_default()` (CR-Q); ANTHROPIC_API_KEY + CLAUDE_* passthrough in child env allowlist (H-02/CR-D); `spawn_blocking` replaces `block_in_place` in MCP ask handler (CR-E); token rotation race fixed (CR-F); OAuth state capacity caps (H-05/CR-K); `apply_overrides` returns new `Config` (CR-M); `ServiceUrls` Debug redacts secrets (CR-L); migration table for `axon_session_ingest_state` (CR-B); arch docs for A-H-01/A-M-01/A-M-04/A-M-08
- **Evaluate page + cortex suggest API** — new `/app/evaluate/page.tsx` for RAG evaluation UI; new `/api/cortex/suggest/route.ts` server route; `apps/web/lib/api-fetch.ts` typed fetch utility; v0.3.0 (minor bump)
- **Image SHA verification** — `docker/s6/cont-init.d/00-verify-image-sha` and `docker/web/cont-init.d/00-verify-image-sha` added to both worker and web containers; `scripts/check-container-revisions.sh` for CI; `scripts/rebuild-fresh.sh` and `scripts/test-mcp-oauth-protection.sh` added
- **CLI help contract test** — `tests/cli_help_contract.rs` verifies `axon --help` exit code and output structure; `scripts/check_mcp_http_only.sh` ensures HTTP transport is correctly gated
- **Sidebar simplification** — `SidebarSectionId` pruned to `'extracted' | 'workspace'`; `recents-section`, `starred-section`, `templates-section` removed; `workspace-section.tsx` and `file-tree.tsx` updated
- **Docs reorganization** — `commands/axon/`, `commands/codex/`, `commands/gemini/` skill command stubs deleted; 20+ `docs/commands/*.md` reference files added covering all CLI subcommands; new `docs/CONTEXT-INJECTION.md`, `docs/schema.md` added; `scripts/check_no_mod_rs.sh` and `scripts/check_no_next_middleware.sh` added for CI
- **Module consolidation** — `mod.rs` indirection pattern replaced with single-file modules across `crates/core/config/cli.rs`, `crates/core/config/types.rs`, `crates/core/http.rs`, `crates/jobs/common.rs`, `crates/jobs/ingest.rs`, `crates/jobs/refresh.rs`, `crates/jobs/worker_lane.rs`, `crates/web/execute.rs`, `crates/web/download.rs`, `crates/ingest/reddit.rs`; deleted corresponding `mod.rs` files
- **Map migration tests** — `crates/cli/commands/map_migration_tests.rs` added (TDD red phase): `map_payload_returns_unique_urls_without_cli_side_dedup`, `map_payload_reports_sitemap_url_count_consistently`, `map_autoswitch_only_falls_back_when_no_pages_seen`; wired via `#[cfg(test)] mod map_migration_tests` in `map.rs`
- **CLI/config refactor** — `crates/cli/commands/crawl.rs`, `map.rs`, `mcp.rs`, `research.rs`, `search.rs`, `youtube.rs` updated; `crates/core/config.rs`, `config/parse/build_config.rs`, `config/parse/helpers.rs`, `config/types/config.rs`, `config/types/config_impls.rs`, `config/types/enums.rs` updated; `crates/cli/commands/crawl/runtime.rs` updated
- **Web/Docker updates** — `apps/web/lib/axon-ws-exec.ts` updated; `apps/web/middleware.ts` deleted; `docker-compose.yaml`, `docker/Dockerfile`, `docker/web/Dockerfile` updated; image SHA verification scripts added to s6 cont-init
- **CI improvements** — `.github/workflows/ci.yml` updated; `lefthook.yml` refined; `Justfile` updated
- **MCP HTTP transport + Google OAuth** — `rmcp` upgraded 0.16→0.17 with `transport-streamable-http-server` feature; `run_http_server()` added alongside existing `run_stdio_server()`; new `crates/mcp/server/oauth_google/` module (8 files: config, handlers_broker, handlers_google, handlers_protected, helpers, state, tests, types) implements Google OAuth2 flow with PKCE, session management, and MCP-native auth middleware; s6 `mcp-http` service for Docker; `crates/mcp.rs` replaces `crates/mcp/mod.rs` with `#[path]` attributes
- **Screenshot CDP→Spider migration** — hand-rolled CDP WebSocket screenshot client deleted; replaced with Spider's `screenshot()` API; contract tests verify full-page capture behavior; scrape migration coverage added
- **Engine-level sitemap backfill** — `append_sitemap_backfill()` moved from CLI robots loop into `engine.rs`; fires automatically after every crawl; `discover_sitemap_urls_with_robots()` characterization tests; SSRF-safe `build_client` enforced; CLI robots backfill loop removed
- **API middleware + server-side extraction** — new Next.js `middleware.ts` (125L) with Bearer token auth (`AXON_WEB_API_TOKEN`), origin allowlist (`AXON_WEB_ALLOWED_ORIGINS`), and insecure dev bypass; `lib/server/url-validation.ts` (212L) extracts SSRF guards + URL sanitization from inline route code; `lib/server/api-error.ts` standardizes error responses; `lib/server/pg-pool.ts` centralizes Postgres pool creation; all API routes refactored to use shared server utilities
- **Omnibox hook extraction** — monolithic `omnibox-hooks.ts` (506→~200L) split into 3 focused hooks: `use-omnibox-execution.ts` (command dispatch), `use-omnibox-keyboard.ts` (key handlers), `use-omnibox-mentions.ts` (@ mentions); `omnibox-types.ts` relocated from component dir to `lib/`
- **Pulse workspace hook** — new `use-pulse-workspace.ts` (336L) consolidates workspace state management from `pulse-workspace.tsx`; `pulse-error-boundary.tsx` adds React error boundary; `use-timed-notice.ts` hook for auto-dismissing UI notices
- **Utility extractions** — `lib/debounce.ts`, `lib/storage.ts` (typed localStorage wrapper), `lib/command-options.ts` centralize shared logic previously duplicated across components
- **10 new test suites** (1250L) — `api-error.test.ts`, `axon-ws-logic.test.ts`, `jobs-route.test.ts`, `pg-pool.test.ts`, `pulse-op-confirmation.test.ts`, `replay-cache-eviction.test.ts`, `url-validation.test.ts`, `use-timed-notice.test.ts`, `workspace-persistence.test.ts`, `ws-messages-handlers.test.ts`
- **Existing test updates** — connection-buckets, terminal-history, omnibox-snapshot, replay-cache, ws-messages-runtime, ws-protocol tests updated for module extraction imports
- **Inline Chrome thin-page recovery** — new `cdp_render.rs` module renders thin pages inline via raw CDP WebSocket (`Page.setContent()` — no second HTTP request) while the HTTP crawl continues; `thin_refetch.rs` provides both inline (concurrent semaphore-gated) and batch fallback (spider-based post-crawl) re-fetch paths; `CollectorConfig` gains `chrome_ws_url`, `chrome_timeout_secs`, `output_dir`; `process_page()` extracted as pure function returning `PageOutcome` enum; collector spawns `JoinSet` of Chrome render tasks capped at `THIN_REFETCH_CONCURRENCY=4`
- **Custom HTTP headers (`--header`)** — new `--header "Key: Value"` repeatable CLI flag; `Config.custom_headers: Vec<String>` threaded through crawl/scrape/extract/Chrome re-fetch paths; headers applied to spider `Website` config and to standalone reqwest calls
- **Streaming sources dedup** — `check_sources_repetition()` in `streaming.rs` detects and truncates duplicate `## Sources` sections in LLM streaming responses; tracks first occurrence position and truncates at the second
- **Spider feature flags documentation** — new `docs/spider-feature-flags.md` inventorying all spider/spider_agent feature flags with observable behavior notes
- **Monolith enforcer improvements** — `enforce_monoliths_helpers.py` and `enforce_monoliths_impl.py` refined; `.monolith-allowlist` updated
- **CI enhancements** — `.github/workflows/ci.yml` updated with additional service container config
- **Web test improvements** — new/updated vitest tests for pulse mobile pane switcher; vitest config updates; 14 new web test files for various utilities
- **Integration/proptest test suite** — new integration tests for AMQP channel/queue (`amqp_integration.rs`), Redis pool (`redis_integration.rs`), heartbeat (`heartbeat.rs`), Postgres pool (`pool_integration.rs`), refresh job scheduling (`schedule_integration_tests.rs`), and WS protocol/allowlist/ANSI stripping (`ws_protocol_tests.rs`); proptest suites for `is_junk_discovered_url` (`url_utils_proptest.rs`), HTTP SSRF validators (`proptest_tests.rs`), and vector input chunking (`input_proptest.rs`); CI adds Redis 8.2, RabbitMQ 4.0, and Qdrant 1.13.1 service containers with health checks + `AXON_TEST_REDIS_URL` / `AXON_TEST_AMQP_URL` / `AXON_TEST_QDRANT_URL` env vars
- **MCP typed schema** — `crates/mcp/schema.rs` introduces fully-typed `AxonRequest` enum (tagged union, `snake_case`, `schemars::JsonSchema`) covering all 22+ actions (status/crawl/extract/embed/ingest/query/retrieve/search/map/doctor/domains/sources/stats/help/artifacts/scrape/research/ask/screenshot/refresh and more) with per-action request structs
- **Ask context heuristics module** — budget helpers and supplemental-injection logic extracted to `crates/vector/ops/commands/ask/context/heuristics.rs`; `push_context_entry` respects `max_chars` budget; `should_inject_supplemental` gates domain-boost on coverage gaps; `SUPPLEMENTAL_CONTEXT_BUDGET_PCT` / `SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE` / `SUPPLEMENTAL_RELEVANCE_BONUS` constants
- **Qdrant utils + tests expanded** — `crates/vector/ops/qdrant/utils.rs` (+229 lines) and `crates/vector/ops/qdrant/tests.rs` (+366 lines): test helpers, scroll utilities, source display improvements, additional coverage for search and facet paths
- **Sidebar simplified** — removed `recents-section.tsx`, `starred-section.tsx`, `templates-section.tsx`; `SidebarSectionId` reduced to `'extracted' | 'workspace'`; `StarredItem`, `RecentItem`, `TagDef`, `TaggedItem` types removed from `types.ts`
- **Web deprecation cleanup** — deleted creator dashboard + route (`/api/creator`, `/creator`), tasks dashboard + route (`/api/tasks`, `/tasks`), and all associated components (`task-form.tsx`, `tasks-dashboard.tsx`, `tasks-list.tsx`, `creator-dashboard.tsx`)
- **CmdK palette — no raw JSON** — `CmdKPalette` tracks `jsonCount` separately; `command.output.json` events increment the counter instead of `JSON.stringify`-ing into the log lines array; `CmdKOutput` shows a "N data objects received — see results panel" badge; `classifyLine` drops the `json` case; `formatToolArg` in `tool-badge.tsx` renders tool call inputs as human-readable labels (arrays as `[N items]`, objects as `{key, key, …}`) instead of raw `JSON.stringify`
- **Integration tests: vector + cancel** — `resolve_test_redis_url` + `resolve_test_qdrant_url` helpers added to `common/mod.rs` (skip-not-fail if env var unset); `poll_cancel_key` integration test in `process.rs`; `ensure_collection` idempotency test in `qdrant_store.rs`; new `crates/vector/ops/qdrant/tests.rs` (search + url_facets); new `crates/vector/ops/tei/tests.rs` (empty-input short-circuit + 429 retry via httpmock); `resolve_test_pg_url` no longer falls through to `AXON_PG_URL` production DB
- **`--include-subdomains` default changed to `false`** — was accidentally `true`; default is now documented and matches the CLAUDE.md gotcha note
- **MCP as `axon mcp` subcommand** — `mcp_main.rs` and `scripts/axon-mcp` deleted; `crates/cli/commands/mcp.rs` added; `CommandKind::Mcp` wired through config stack; MCP server is now a first-class CLI subcommand rather than a separate binary entry point
- **CLI `common.rs` expansion** — shared `JobStatus` trait + status display helpers extracted from crawl/extract/ingest subcommands, reducing duplication; URL glob expansion now logs a warning at `MAX_EXPANSION_DEPTH`
- **Smart dotenv loading** — `main.rs` discovers `.env` by walking ancestors from exe path and CWD; `AXON_ENV_FILE` env var for explicit override; graceful fallback chain with per-error warnings
- **Mobile omnibox fix** — three-bug root-cause chain: (1) sidebar auto-collapses on mobile viewports (<768px) when no stored preference, preventing it from consuming 260px of a 390px screen; (2) textarea auto-resize uses `height: '1px'` instead of `'auto'` before reading `scrollHeight` — `'auto'` in a flex layout returns the stretched layout height rather than intrinsic content height; (3) `ResizeObserver` added so height recalculates after sidebar collapse reflows the layout (the `[input]`-dep effect fired once on mount while sidebar was still 260px and never re-ran)
- **CmdK palette** — new `apps/web/components/cmdk-palette/` component with `CmdKPalette` and `CmdKOutput`; wired into `AppShell`
- **xterm.js terminal enhancements** — WebGL GPU renderer (`@xterm/addon-webgl`) with context-loss fallback; search decorations (amber highlights + active-match blue) via `allowProposedApi: true`; overview ruler lane (`overviewRulerWidth: 8`) shows match positions in scrollbar; copy-on-select via `onSelectionChange`; visual bell via `onBell` opacity flash; `attachCustomKeyEventHandler` for Ctrl+Shift+C (copy) / Ctrl+Shift+V (paste); all clipboard calls guarded with `?.` for HTTP contexts
- **Cortex layout refactor** — `app/cortex/layout.tsx` rewritten with proper sidebar integration; Cortex API routes standardised; doctor/status/stats/sources/domains dashboards updated for new layout
- **Plate.js editor enhancements** — slash commands (`/`), block drag-and-drop, callout blocks, collapsible toggles, table of contents, multi-block selection, block context menu, AI menu, inline comments, suggestion mode, export (HTML/PDF/image/markdown); 15 new plugin kit files wired into `copilot-kit.tsx`; mobile-responsive compact toolbar; `@ai-sdk/gateway@1.0.15` pinned for `ai@5` compatibility; `@platejs/ai` command route rewired with `generateText` for `ai@5` breaking changes (`Output.choice`, `partialOutputStream` removed); `useSearchParams` Suspense guard on `/cortex/sources`
- **Plate.js editor expansion** — 15 additional `@platejs/*` plugins (callout, caption, combobox, comment, date, emoji, indent, layout, math, mention, resizable, selection, suggestion, toc, basic-styles), supporting packages (`@ai-sdk/react`, `ai`, `@ariakit/react`, `date-fns`, `cmdk`, `lowlight`, etc.), `tailwind-scrollbar-hide` plugin, and new shadcn/ui components (dialog, popover, cursor-overlay)
- **Cortex dashboard review fixes** — AbortController on all polling dashboards (status/doctor/stats) cancels in-flight fetches on unmount and before each new poll; `disabled={loading || spinning}` on all 5 Refresh buttons; `Object.keys(data).length` badge fix in sources-dashboard; `useSearchParams` seeds filter from `?q=` param so domain drill-down links work; `local_ingest_jobs ?? []` guard in SummaryBar; `AXON_BIN` env var wires the pre-built binary path for Docker (routes were silently broken without it); missing `--sidebar-w` CSS update in `handleNavClick`; `aria-label` + `aria-current="page"` on Cortex sub-links; `target?: string` added to `JobEntry` interface
- **Cortex virtual folder in sidebar** — collapsible "Cortex" folder appended after PAGE_LINKS with Brain icon; 5 sub-links (Status, Doctor, Sources, Domains, Stats); open/closed state persists to `localStorage`; clicking Brain icon while collapsed auto-expands sidebar; active route highlighting on `/cortex/*` paths; 5 API routes (`/api/cortex/*`) spawn the axon binary with `--json`; 5 server component pages under `/app/cortex/`; 5 client dashboard components with loading skeletons, error banners, and refresh buttons; Status polls every 5s with collapsible job cards, Doctor polls every 15s with service health grid + pipeline chips, Sources uses `@tanstack/react-virtual` for virtualized URL table with search filter, Domains renders relative CSS bar chart with clickable domain→sources links, Stats polls every 30s with 6 large metric cards + payload fields + command count table
- **Jobs dashboard UX overhaul** — color-coded type badges (crawl=sky, embed=amber, extract=violet, ingest=rose), stats summary bar with live counts per status, sortable column headers (type/target/collection/status/started), relative timestamps ("5m ago") with absolute on hover, smart URL truncation (last 2 path segments), row hover actions (cancel/retry/view), animated ping ring + shimmer progress bar for active jobs; API extended with `StatusCounts` from parallel DB queries
- **Pulse 3-panel collapsible layout** — chat panel left, editor right, chevron strips to collapse/expand; `showChat`/`showEditor` booleans replace `DesktopViewMode`/`DesktopPaneOrder`; `use-split-pane` rewritten for 3-panel chevron layout
- **Pulse autosave optimization** — `updatePulseDoc` skips file read when client caches `createdAt`/`tags`/`collections` from last save response; pre-deletes stale Qdrant vectors before re-embed; save response now includes `createdAt`, `tags`, `collections`
- **Editor UX** — `loadedDocRef` tracks loaded doc param so re-navigation to a different `?doc=` reloads content; `SaveStatusBadge` wrapped in `memo`; `Suspense` fallback skeleton added
- **Z-index fix** — sidebar `z-[2]`, main content `z-[1]` — prevents NeuralCanvas/floating elements from bleeding over the sidebar
- **Job Detail Pages (`/jobs/[id]`)** — clickable job rows on `/jobs` now navigate to a dedicated detail page showing status, pages crawled/discovered, markdown created, timing, config, and raw result JSON; live-polls every 3s for running jobs
- **Knowledge Base (`/docs`)** — new page listing every scraped/crawled page from the axon output directory, grouped by domain, with markdown content viewer; backed by filesystem manifest.jsonl reads (no Qdrant calls)
- **PTY Shell** — real interactive shell at `/terminal` via `portable-pty` + dedicated `/ws/shell` WebSocket
- **Sidebar nav** — "Files" replaced with "Docs" → `/docs`; AXON logo made a home link; section-tab architecture with extracted/starred/recents/templates/workspace content panels

### Commit Summary (main..HEAD)

| Commit | Type | Message |
|---|---|---|
| `ca7831c0` | fix | OAuth __Host- cookie HTTP bug, orphaned pending re-enqueue, youtube helper, evaluate dead code |
| `a3120774` | chore | remove orphaned run_evaluate_native and run_suggest_native dead code |
| `f508977f` | feat | add watch service module, migrate watch CLI command through services layer |
| `c4dcb115` | fix | strengthen MCP contract parity tests from tautological to real assertions |
| `c9e5c468` | fix | restore artifact path test, fix OAuth redirect URI normalization, fix MCP issuer |
| `fddf8374` | fix | fix worker lane exit bug, Reddit ingest flags, and inverted routing test |
| `b0db2244` | fix | restore auto-inline and artifacts param docs in MCP-TOOL-SCHEMA.md |
| `5c298b29` | fix | fix next.config.ts typos, URL validation, and page.tsx re-export |
| `01143928` | chore | stabilize branch and make all quality gates green |
| `4fffcb68` | test | harden crawl fallback and oauth error contracts |
| `3f8214ae` | chore | finalize service-layer migration task 9 |
| `afe1ef60` | chore | finalize service layer migration v2 with guards and verifications |
| `51775607` | refactor | route web async ingest modes through direct services |
| `57ce5057` | refactor | split refresh schedule and route watch/scheduler through services |
| `5d1960cf` | refactor | route cli lifecycle and system commands through services |
| `318eae23` | refactor | complete mcp lifecycle and screenshot rewires to services |
| `eb2895e9` | refactor | route mcp embed ingest handlers through services layer |
| `84d0736f` | feat | add service-owned ingest target classification |
| `5f91f82c` | feat | add service lifecycle wrappers for crawl extract embed ingest refresh |
| `67808fd5` | test | add migration guardrails for CLI MCP and web ingest routing |
| `68db1231` | chore | checkpoint current changes |
| `b6149f31` | feat(web) | refresh shell mission control and provider branding |
| `b39e83a0` | feat(acp,web,mcp) | harden session lifecycle and developer tooling |
| `356ea87a` | fix(web) | make session list loading reliable |
| `80a7e21d` | fix(web) | clear streaming flag on message when result arrives |
| `14d8edd3` | feat(web) | performance/accessibility audit fixes + density feature + state split |
| `4663ce65` | feat | ACP session persistence — survive WebSocket disconnects |
| `fb7a9f87` | perf | web performance & accessibility improvements |
| `e1e612c6` | perf(ingest) | batch GitHub TEI embeddings across documents |
| `bbc1684b` | feat(acp) | persist MCP config and harden session scanning |
| `5682daa2` | fix(mcp) | align config path to mcp.json across web/api/docs |
| `98e7b96e` | feat(release) | ship assistant mode and stabilize verification gates (v0.18.0) |
| `93537231` | feat(web) | wire assistant mode sessions through shell and ACP |
| `aef2014f` | test(web) | fix cortex route mock arg typing |
| `c54de559` | feat(web) | render assistant session list in sidebar |
| `17a6d231` | feat(web) | add assistant rail mode to config |
| `e7271b23` | feat(web) | add assistant sessions API route and scanner |
| `c2d414c8` | feat(web) | use assistant CWD when assistant_mode=true |
| `9c7e6a5f` | feat(web) | add assistant_mode to DirectParams and extract from flags |
| `05d13ba5` | test(services) | align scrape payload contract assertion |
| `df0f0ffe` | feat(web) | add assistant_mode to ALLOWED_FLAGS |
| `4fdc70be` | feat | complete GraphRAG rollout and prune reboot remnants |
| `4e107038` | feat | add graph worker, services layer, artifact context isolation, and toolchain bump (v0.16.0) |
| `61568562` | dev-setup | arch-aware just prebuilt install with binary verification |
| `c8ba34a8` | dev-setup | always backfill .env entries and data dirs on rerun |
| `fea465cc` | Update scripts/dev-setup.sh | Update scripts/dev-setup.sh |
| `b645b204` | Update scripts/dev-setup.sh | Update scripts/dev-setup.sh |
| `fc197755` | Update scripts/dev-setup.sh | Update scripts/dev-setup.sh |
| `60c50870` | dev-setup | fix die newlines and sed -i portability on macOS |
| `8ea30464` | dev-setup | fix local-outside-function bug, drop dead just fallback |
| `e900e335` | just | add test-infra-up/down recipes; use them in dev-setup |
| `c308205f` | dev-setup | start test infra, populate test env URLs, fix stale summary |
| `c762a652` | feat(dev-setup) | pre-create container data directories |
| `f6098774` | feat(dev-setup) | auto-generate secrets on first .env creation |
| `bc3dbc6b` | feat(dev-setup) | prompt for AXON_DATA_DIR on first .env creation |
| `86062089` | fix(dev-setup) | fast just install + clarify entrypoint |
| `08e35097` | feat | add dev-setup.sh bootstrap script |
| `5179cba0` | fix | make hook script paths portable via git rev-parse |
| `48d372d9` | fix | correct stale hook script paths in .claude/settings.json |
| `706e84b7` | fix(sessions) | suppress biome dep warning, format shell-server.mjs |
| `b488a20a` | feat(reboot) | add loading/error states to AxonMessageList |
| `a83b1901` | feat(reboot) | disable AxonPromptComposer submit during streaming, add spinner |
| `96120f43` | fix(sessions) | add repo/branch to SessionSummary type |
| `8a0ada40` | fix(reboot) | add repo/branch to sidebar filter and card display |
| `a2a252bb` | feat(reboot) | wire AxonSidebar to real SessionSummary list |
| `dc51e2ed` | fix(reboot) | guard history sync during streaming, fix timestamp display |
| `c0ffbf59` | fix(reboot) | wrap onTurnComplete callback in useCallback |
| `9ce7c25a` | feat(reboot) | wire AxonShell to real session data and ACP WebSocket |
| `eca13f44` | fix(hooks) | use randomUUID for message IDs + add ACP types to WsServerMsg |
| `863cdee7` | test(hooks) | add behavioral tests for useAxonSession |
| `ba85c64e` | refactor(reboot) | rename remaining REBOOT_ constants to AXON_ |
| `ee1e5403` | refactor(reboot) | rename Reboot* components to Axon* |
| `e3f2ae1c` | fix(pulse) | forward session_fallback through route handler + fix types |
| `c1367c35` | feat(pulse) | handle session_fallback event in stream pipeline |
| `bc2d691f` | style(sessions) | use template literals in git-metadata (biome) |
| `26273571` | feat(sessions) | add git-metadata helper for repo/branch enrichment |
| `adff1e2f` | merge | integrate feat/sidebar into main |
| `3e8d7778` | chore(config) | update mcporter axon transport endpoint shape |
| `5405832a` | fix(docker) | unblock worker/web healthchecks in local compose |
| `fb91fadd` | chore(release) | v0.4.1 — stabilize web token/docs and prep services refactor execution |
| `555ade14` | feat | add evaluate page, cortex suggest API, image SHA verification, CLI help contract; consolidate modules and expand command docs (v0.3.0) |
| `460c8e30` | refactor | unify scrape response shaping and fetch pattern |
| `cd831c88` | Merge pull request #7 from jmagar/add-claude-github-actions-1772591515488 | Merge pull request #7 from jmagar/add-claude-github-actions-1772591515488 |
| `5472d9f3` | "Claude Code Review workflow" | "Claude Code Review workflow" |
| `604d2d67` | "Claude PR Assistant workflow" | "Claude PR Assistant workflow" |
| `cd8d172c` | feat(mcp) | add HTTP transport with Google OAuth + cleanup |
| `4f71971d` | fix(web) | resolve TypeScript build errors from Plate.js untyped APIs |
| `65a74309` | fix(web) | remove unused LogEntry import from axon-logs-dialog |
| `a42cf681` | refactor(web) | extract shared log stream hook from AxonLogsDialog |
| `4dcf1746` | feat(web) | add settings dialog with canvas profile to reboot shell |
| `f7f60573` | feat(web) | wire message edit and retry in reboot chat |
| `88cf67e3` | feat(web) | wire Docker stats and NeuralCanvas intensity into reboot shell |
| `8dbbb1f1` | feat(web) | update reboot sidebar page links for root route |
| `fdf06eee` | feat(web) | promote reboot UI to root route, move legacy dashboard to /legacy |
| `163998b4` | feat | finalize mcp transport and review hardening (v0.15.0) |
| `3d3f9d98` | feat | add shell completions, CORS guards, and crawl output paths |
| `ef2c4fad` | docs(mcp) | align transport docs and env example |
| `a3c1f18e` | feat(mcp) | support stdio and dual transport modes |
| `e462931f` | fix(ingest) | GitHub clone auth + progress display fixes (v0.14.2) |
| `1a4ded20` | fix(ingest) | GitHub clone auth + progress display fixes (v0.14.2) |
| `0c8f2b57` | chore | Qdrant tuning + ssh_auth test cleanup (v0.14.1) |
| `81e6a874` | fix(ingest) | display task-level and phase progress for GitHub ingest |
| `17782382` | perf(ingest) | replace per-file GitHub API fetches with git clone --depth=1 |
| `fa11b4a3` | feat(ingest) | add live progress tracking for GitHub repo ingestion |
| `d29b1f4a` | docs | update all docs for GitHub code-aware chunking feature |
| `ed336b16` | feat(refresh) | GitHub repo re-ingest schedules with pushed_at gating |
| `31db768b` | feat(cli) | source code included by default in GitHub ingest |
| `bdd687d1` | feat(ingest) | unified GitHub payload builder + code-aware chunking |
| `61a0f387` | feat(vector) | add embed_code_with_metadata with AST chunking fallback |
| `69f673c0` | feat(web) | web auth hardening + Pulse workspace improvements + CLI cleanup (v0.14.0) |
| `0401eaa0` | feat(deps) | add text-splitter + tree-sitter grammar crates |
| `717d37cc` | fix(review) | address 114 CodeRabbit threads + remove dead run_*_native functions (v0.13.3) |
| `2f53720f` | fix(review) | address 14 CodeRabbit/cubic-dev-ai PR comments |
| `b0f9ad34` | refactor(web) | sync dispatch helpers + session guard scaffold (v0.13.2) |
| `775111dc` | fix(ingest) | progress display + embed list polish + crawl batch resilience (v0.13.1) |
| `2cf2a067` | feat(web) | multi-agent sessions sidebar — Claude + Codex + Gemini (v0.13.0) |
| `031af077` | feat(ingest) | unified axon ingest + structured metadata + MCP artifacts (v0.12.0) |
| `a4ceffd7` | feat(acp) | wire `<axon:editor>` XML blocks to PlateJS editor |
| `cbaa1eab` | feat(web) | add editor_update WS message type to protocol |
| `175b0454` | fix(acp) | address all PR review comments + implement SEC-7 session-scoped permission routing |
| `f6d9bace` | fix | address Codex + Copilot PR review comments |
| `5279f7ad` | refactor(acp) | performance/scalability fixes + modern Rust idioms (v0.11.2) |
| `e2a503c7` | chore | merge dev-setup script PR (#9) (arch-aware just install, secret gen, data dirs, test infra) |
| `5fcbad02` | fix | patch zip-slip, LogLevel case-sensitivity, XML single-quote escaping |
| `e012ce34` | test | expand coverage across web app + Rust crates (+914 tests, 18 files) |
| `470ad642` | fix(ci) | resolve mcp-smoke and test job failures |
| `47e62592` | fix | address misc/infra PR review comments (threads 1,3,4,5,7,11,34,36,37,40,41,43,51) |
| `05152d9d` | fix | address Rust backend PR review comments (threads 10,17,19,22,26,27,31,47) |
| `b5690063` | fix | address reboot + terminal component PR review comments (threads 42,44,46,49,50,52,53) |
| `bbf962b3` | fix | address AI elements component PR review comments (threads 23,28,29,32,33,35,45) |
| `197f4975` | fix | address Pulse component PR review comments (threads 9,13,15,24,25,54,55) |
| `59df81cb` | fix | address API route PR review comments (threads 2,6,12,14,20,39) |
| `7b0af2fe` | feat(reboot) | wire AxonShell to real ACP/session data, add hooks + UI polish (v0.11.0) |
| `31cf6299` | fix(sessions) | use apiFetch to inject x-api-key on session load |
| `45a19e59` | feat(hooks) | add useAxonSession for JSONL session history |
| `9bb93bce` | feat(hooks) | add useAxonAcp for real ACP WebSocket prompt submission |
| `9726772f` | feat(acp) | emit SessionFallback event on failed session resume |
| `02e26020` | refactor(sessions) | hoist git enrichment to outer project loop |
| `489c5435` | feat(sessions) | enrich session list with git repo/branch metadata |
| `85518db6` | feat | reboot UI shell + logs SSE fix + CORS config + biome cleanup (v0.9.0) |
| `e596f3e6` | feat | Zed alignment patterns + ACP permission plumbing (v0.8.0) |
| `24e25081` | feat | add --root-selector/--exclude-selector + clean_markdown_whitespace (v0.7.5) |
| `9c38b0fa` | refactor | split monolith-violating files (route.ts, use-pulse-chat.ts) |
| `8d4603b7` | feat | address all ACP review findings (v0.7.4) |
| `4d3d2a9a` | feat | address all ACP review findings (v0.7.4) |
| `edabb90a` | test | regression tests for ACP env isolation (v0.7.3) |
| `7368ddb7` | fix | stage claude/codex credentials into axon-web container |
| `107d2a6c` | fix | remove pulse_chat direct-dispatch flags from ALLOWED_FLAGS |
| `a017bb28` | chore | v0.7.1 — address all PR review threads (batches 1-10) |
| `2ae80ede` | fix | address PR review batch 10 — thread-safety, stale ref, and cleanup |
| `98f0d817` | fix | address remaining CodeRabbit review comments (batch 9) |
| `b464c3ab` | fix | address frontend PR review comments (batch 8) |
| `cb708b2a` | fix | decouple services layer from CLI commands (screenshot + map) |
| `68ff42c9` | fix | bind infra ports to localhost, fix nginx CORS, pin TEI retry env vars in tests |
| `e2f8bd90` | fix | address PR review batch 5 — typed errors, fail-fast mappers, probe uniqueness |
| `e933160c` | fix | address PR review feedback (batch 4 - frontend) |
| `5359faba` | fix | address PR review feedback (batch 3) |
| `2ad79b93` | fix | address PR review feedback (batch 2) |
| `6fde4d77` | fix | address PR review threads — dead code, render modes, service hardening |
| `54075260` | fix(review) | arrow fns, session id, proxy headers, pulse chat, chunk fix, dispatch split |
| `b787c7ba` | fix(review) | mode ref routing, log visibility, facet limit clamps |
| `e7b3e249` | fix(review) | address PR comments — MCP error sanitization, event field names, cancel safety, flag validation |
| `477f44a0` | fix(pr) | address review comments — security, correctness, and flag propagation |
| `de90c337` | feat(release) | v0.7.0; Pulse agent selector (claude/codex), ACP adapter routing, ws/api wiring, replay-key hardening |
| `baf24e5e` | fix(scrape) | select requested page and scope embed to current run |
| `4d5b0cb5` | feat(release) | v0.6.0 — web workspace/sidebar updates + TEI retry fixes |
| `f90d123a` | feat(release) | v0.5.0 — services-layer refactor complete + editor tabs + CmdK + scripts |
| `4e5144a3` | chore(web) | remove dead code from services layer refactor |
| `14b62d49` | feat(web) | fire-and-forget async dispatch and cancel via services |
| `476ad35b` | feat(web) | replace sync subprocess execution with direct service dispatch |
| `fe83d0a9` | fix(web) | replace dead Some(other) arm with unreachable! in render_mode match |
| `ed2bd90d` | refactor(web) | plumb base Config and ws override mapping for direct service dispatch |
| `dae2b0b1` | test(mcp) | pin map_retrieve_result data contract — chunk_count in wrapper element |
| `e93df53e` | fix(mcp) | correct retrieve chunk_count and research error class |
| `fb485043` | fix(mcp) | preserve sources wire contract — urls remains string[] in MCP response |
| `03996f72` | fix(mcp) | use option mapper helpers in system and query handlers |
| `38f0a53d` | refactor(mcp) | rewire handlers to use services layer |
| `d146571f` | refactor(mcp) | add request-to-service option mappers |
| `e4f81653` | fix(services) | address quality review issues from Wave 2 |
| `7f91caf2` | refactor(cli) | route system/stats/doctor/status handlers through services |
| `196ab300` | refactor(cli) | route query scrape search lifecycle and ingest handlers through services |
| `a802ff87` | feat(services) | implement query services (query/retrieve/ask/evaluate/suggest) |
| `c76fe394` | feat(services) | implement scrape/map/search/research services |
| `5a6f0393` | feat(services) | implement system services (sources/domains/stats/doctor/status/dedupe) |
| `475aa3da` | feat(services) | scaffold services module and events/types base |
| `cd42ee57` | docs(plan) | record baseline verification for services refactor |
| `58c66e29` | fix(docker) | expose service ports and restore external MCP reachability |
| *(prev)* | chore(release) | v0.4.1; stage pending web/docker/docs updates; harden services-layer refactor execution plan and dispatch safety |
| `b71fd7fd` | test | fix mcp-oauth-smoke missing env vars and serialize crawl DB tests |
| `25e2287f` | fix(ci) | fix 4 failing CI checks |
| `05238113` | fix(web) | set TZ=UTC in vitest config and update snapshot timestamps |
| `9eddd039` | chore(release) | v0.4.0 — full codebase review complete; 40+17 findings fixed; changelog updated |
| *(this commit)* | feat+chore | v0.4.0; full codebase review — 40 + 17 CR findings fixed; WS OAuth gating; SQL parameterization; Secret<T>; ConfigOverrides; env allowlist hardening |
| `18c6e6ae` | fix(test) | add #[serial] to extract DB tests to eliminate race condition |
| `54ced213` | fix(jobs) | fix doctest annotation in status.rs |
| `79cca7ba` | fix(config) | add Config::test_default() for stable test helpers (CR-Q) |
| `cf178f6e` | docs,feat | add arch docs (A-H-01, A-M-01, A-M-04, A-M-08) and scrape/evaluate module files |
| `da712968` | fix(jobs) | H-03 SQL parameterization in ingest/ops.rs |
| `b6671081` | fix(jobs,mcp,web) | H-03 SQL parameterization (extract/ingest/crawl), spawn_blocking, ANTHROPIC_API_KEY allowlist, sitemap tests |
| `ee330e95` | fix(jobs,mcp,web) | H-03 SQL params in process.rs, spawn_blocking safety, ? operator cleanup, CLAUDE_* env passthrough |
| `d95938ce` | fix(web,mcp) | add ANTHROPIC_API_KEY to env allowlist, fix block_in_place panic risk (CR-D, CR-E) |
| `e3134ef7` | feat(security) | gate /ws with OAuth bearer token; fix cancel mode injection, shell IPv4-mapped loopback, clock sentinel |
| `61169198` | fix(config) | wire modules, fix Secret timing, align defaults, expand ConfigOverrides, fix Debug (CR-A, CR-G, CR-H, CR-I, CR-L, CR-M) |
| `57c0250e` | fix(oauth) | fix token rotation race and add pending_state capacity cap (CR-F, CR-K) |
| `09d15d26` | fix(migrations,docs) | add missing tables/indexes to migration, fix scaling.md network (CR-B, CR-C, CR-N) |
| `72e7742d` | fix(deps) | bump aws-lc-sys 0.37.1 → 0.38.0 via aws-lc-rs 1.16.1 |
| `012cdcf4` | fix(ingest) | address 3 code review findings (C-02, M-04, L-06) |
| `e7238085` | fix | use raw sitemap url count in MapResult and remove shadow test |
| `4fff3661` | docs | record map command engine unification |
| `4eea6b93` | test | lock map payload schema after engine unification |
| `0186de11` | fix(compile) | add missing log crate dependency for web execute module |
| `b2f4c124` | fix(oauth) | address 8 code review findings (C-01, C-03, H-05, M-02, M-05, M-07, M-09, L-04) |
| `ddf4e830` | fix(cli) | restore stable JSON schemas for status/cancel/list/errors |
| `f9c26621` | fix(scrape) | redact headers in debug, fix failure propagation, dedup markdown, CDP timeout, schedule tier |
| `d2ade357` | fix(omnibox) | exec_id guard, suggestion staleness, useCallback deps, isProcessing sync, empty content |
| `66fd1ed6` | fix(ssrf) | block IPv6 enum bypass, 0.0.0.0, and redirect SSRF |
| `f35ce379` | fix(pulse) | auto-scroll MAX_LINES, Enter double-fire, clipboard fallback, empty text guard, unreachable boundary, allowlist expiry |
| `e63f6473` | fix(web) | api-fetch header merge, token scope, permissionLevel default, CSP, loopback, eviction order |
| `6f172dbd` | test | add map migration coverage |
| `3466ddf0` | test | serialize DB-touching integration tests with #[serial] to prevent race conditions |
| *(this commit v0.3.0)* | feat+chore | v0.3.0; evaluate page; cortex/suggest API; image SHA verification cont-init; CLI help contract test; command docs expansion (20+ files); module consolidation; sidebar simplification; script additions |
| *(this commit v0.23.1)* | fix(web)+fix(jobs) | complete crates/web review remediation (31 findings); embed worker crash fix; subtle token comparison; process-wide rate limit; ws_handler refactor |
| b387bf95 | fix(web) | shell WS msg size gate, ACP mode constant, markdown formatting |
| 57c33133 | fix(web) | session ownership gate, auth consistency, and compilation fixes |
| ae3382d4 | fix(web) | address TypeScript PR review issues (threads 2,9,12,15) |
| 6c6b3837 | fix(web) | address P1/P2 Rust PR review issues (threads 4,5,6,7,11,13,14,16,17,19,20,21,22) |
| f2a7b3b2 | fix(web) | web integration security, protocol, and performance fixes (v0.23.0) |
| 7fb1100d | feat(mcp)+chore | MCP HTTP transport + Google OAuth; rmcp 0.17; screenshot CDP→Spider migration; engine sitemap backfill; cleanup |
| `62bdae5e` | test | add scrape migration contract coverage |
| `2d004e27` | docs | record screenshot migration to spider api |
| `426cac65` | test | verify full-page screenshot behavior after migration |
| `0e45780c` | chore | delete hand-rolled screenshot cdp client |
| `e6ca9ddf` | feat(screenshot) | replace CDP client with Spider screenshot capture |
| `22310087` | test(screenshot) | add migration contract tests for CDP→Spider transition |
| `370ee1af` | docs | record engine-only backfill architecture |
| `147b9ca5` | chore | remove cli robots backfill loop |
| `c38dfb5f` | refactor | remove double validate_url + add TODO for http_client singleton |
| `209b86a1` | feat(crawl) | add engine-level append_sitemap_backfill and wire into sync_crawl |
| `2862eb9d` | test(crawl) | add failing contract tests for engine-delegated sitemap backfill |
| `c9ebd58b` | fix | use SSRF-safe build_client + add max_sitemaps TODO in engine sitemap |
| `817160bd` | test(sitemap) | characterization tests for discover_sitemap_urls_with_robots |
| `04559aed` | refactor(web)+test | API middleware + server-side extraction; omnibox/pulse module splits; 10 new test suites; utility extractions |
| `84cd8d2b` | feat(crawl)+refactor | inline Chrome thin-page recovery; CDP render module; custom headers; streaming sources dedup; spider feature flags docs |
| `129eb1fa` | test(rust)+refactor(web) | integration/proptest test suite; MCP typed schema; ask context heuristics; sidebar cleanup; CI service containers |
| `9428156c` | fix(ci) | remove invalid cargo-audit --deny flag; add Qdrant keyword indexes on collection init |
| `fa8ddc29` | revert | remove redundant .cargo/config.toml — sccache already in ~/.cargo/config.toml |
| `149325f0` | fix | restore sccache config; patch minimatch ReDoS (CVE high x2) |
| `edaafabf` | fix(web)+test(rust) | suppress raw JSON in CmdK palette; add vector/cancel integration tests; fix include_subdomains default |
| `959537ac` | refactor(mcp) | deduplicate DB queries in handle_status; fix artifacts action field |
| `76356b0e` | refactor(mcp+cli) | CLI command handlers, MCP wiring, and web fixes |
| `186a6936` | refactor(mcp+cli) | MCP as axon mcp subcommand; CLI common.rs JobStatus trait; smart dotenv loading; misc fixes |
| `d022c6f5` | fix(web) | mobile omnibox sizing — sidebar auto-collapse <768px, textarea ResizeObserver + height:1px fix; CmdK palette; web improvements |
| `27fc39f6` | feat(web) | xterm.js terminal enhancements — WebGL renderer, search decorations, overview ruler, copy-on-select, visual bell, Ctrl+Shift+C/V; Cortex layout refactor |
| `72d1f651` | fix(web) | wire AIKit into CopilotKit + address open items |
| `b2e2d61d` | fix(web) | address code review findings from Plate.js editor enhancements |
| `405e0945` | feat(web) | Plate.js editor enhancements — slash, DnD, callouts, toggles, TOC, block selection, AI menu, comments, export; ai@5 compat fixes |
| `f27cc810` | chore(deps) | Plate.js editor plugin expansion + dialog/popover/cursor-overlay UI components |
| `756a081e` | chore | wire AXON_BIN env var for Cortex routes in Docker — routes now fall back to pre-built release binary via /workspace mount |
| `f5d14901` | fix(web) | address Cortex dashboard review findings — AbortController, disabled state, binary path, accessibility |
| `51a2c9c8` | merge | feat/crawl-download-pack → main |
| `928ce7ba` | feat(web) | Cortex virtual folder in sidebar — status/doctor/sources/domains/stats diagnostic pages with API routes and dashboard components |
| `e2e5ee6b` | chore + fix | mcporter plate MCP entry; crawl worker output_dir uses worker root not job-serialized path |
| `5dee20a7` | fix(web) | pulse dual-hydration race + both-collapsed restore guard |
| `4e4633d9` | fix(web) | pulse workspace quality fixes — collapse guard, editor flex, aria |
| `a941173c` | feat(web) | jobs dashboard — color badges, stats bar, sort, relative time, smart truncation, hover actions, active progress |
| `61a1696e` | fix(web) | remove unused verticalDragStartRef from pulse-workspace destructure |
| `3359e863` | feat(web) | 3-panel collapsible layout — chat left, editor right, chevron strips |
| `cf1323ce` | fix(web) | remove unused showChatRef from use-split-pane |
| `50dd9473` | feat(web) | update use-pulse-persistence for showChat/showEditor |
| `f5c13206` | feat(web) | remove view-mode toggle buttons from PulseToolbar |
| `60cd01ed` | feat(web) | rewrite use-split-pane for 3-panel chevron layout |
| `1925a5bb` | feat(web) | replace DesktopViewMode/DesktopPaneOrder with showChat/showEditor booleans |
| `8ad11100` | fix(web) | pulse autosave update-in-place + editor hardening |
| *(2d32f42e)* | fix(web) | pulse autosave: skip file read, pre-delete stale vectors, editor doc-reload fix, z-index |
| `394917d5` | feat(web) | /jobs/[id] detail page — status, stats, timing, config, live polling |
| `ac294073` | feat(web) | /docs knowledge base page — filesystem-backed manifest reader |
| `9fdf8913` | feat(web) | terminal page — real PTY shell via useShellSession |
| `d7cff203` | feat(web) | useShellSession hook — dedicated /ws/shell WebSocket |
| `d357f088` | feat(web) | add /ws/shell route for PTY shell sessions |
| `e9011060` | feat(web) | PTY shell WebSocket handler in crates/web/shell.rs |
| `e55c4e00` | chore(deps) | add portable-pty for PTY shell support |
| `ac16331b` | feat(web) | xterm.js terminal emulator at /terminal — WS integration, design system theming, sidebar nav |
| `a31a58ea` | fix(docker) | install uvx for neo4j-memory MCP, add pnpm-dev finish script |
| `2a23d860` | feat(web) | hoist PulseSidebar to AppShell — visible on all pages |
| `a5dc786c` | fix(docker) | resolve inotify watch limit, EADDRINUSE port race, and node_modules ownership |
| `4e45fb38` | fix(web) | use ExtractedSection in results-panel instead of inline file list |
| `6b0619ed` | fix(web) | restore selectedFile/selectFile in results-panel with inline file list |
| `22a96263` | fix(web) | remove unused selectedFile/selectFile from results-panel destructure |
| `9235a534` | fix(web) | remove CrawlFileExplorer from results-panel, delete stub |
| `f3ca9641` | feat(web) | Logs page - Docker compose log viewer with SSE streaming |
| `7847680d` | fix(web) | jobs-dashboard Biome lint compliance - hook deps and unused imports |
| `7f7a49fa` | feat(web) | Tasks page - task scheduler dashboard with CRUD and manual run |
| `d91167a2` | fix(security) | resolve symlink traversal and path canonicalize bypasses |
| `d36e18d7` | chore | update changelog sha 8386d55 |
| `8386d55` | feat(pulse) | remove hard borders, glow shadow separators, word wrap fix in editor |
| `b7dd29e` | fix(jobs) | spawn_heartbeat_task helper, Redis cancel timeouts, async I/O fixes, 7 new unit tests |
| `1ec5513` | feat(web) | workspace virtual dirs, Claude folder, landing editor, header normalization |
| `b2d8a74` | feat(web+docker) | PlateJS editor integration, pnpm-watcher s6 service, chrome health fix |
| `8d85538` | fix(jobs) | address all P0/P1/P2 code review issues — 8-agent team landing |
| `5dc43f1` | chore | update changelog for UI overhaul + workspace explorer; misc Rust job fixes |
| `e73906a` | feat(pages) | modal delete dialogs, MCP single save, settings typography, empty states, layout improvements |
| `7ca6184` | feat(pulse) | motion, empty state, message alignment, tool badge discoverability, mobile pane labels, divider improvements |
| `e3a0c96` | feat(omnibox) | status bar persistence, @mention discovery tip, staggered suggestions |
| `4bdee4b` | feat(ui) | button/input hover micro-interactions, branded focus rings, scrollbar contrast fix |
| `e56c72d` | feat(web) | add CodeViewer component with line numbers and copy button |
| `648010c` | feat(web) | add /workspace file explorer page with tree + viewer |
| `b585aef` | feat(design) | establish design token foundation — fonts, palette, motion, atmosphere, shadows, a11y |
| `dcb077a` | feat(web) | add CodeViewer component with line numbers and copy button |
| `074ad72` | feat(web) | add workspace (FolderOpen) nav icon to omnibox toolbar |
| `63e71ff` | feat(web) | add /api/workspace route for AXON_WORKSPACE file browsing |
| `8e1f4e1` | fix(web) | prefix unused liveToolUses prop + update changelog sha |
| `bc62851` | fix(web) | fix duplicate tool badges and raw-JSON response text in Pulse chat |
| `b20a7a3` | fix | address all 12 PR review comments from cubic-dev-ai |
| `d9823b2` | feat(web+jobs+mcp) | SSRF hardening, AMQP reconnect backoff, multi-lane workers, expanded tests |
| `ebca63c` | fix(web) | add Settings2 icon import to omnibox + changelog update |
| `d3f8047` | fix(ci) | resolve sccache and cargo audit failures |
| `03b1ef3` | fix(web) | remove dangling useRouter() call from omnibox |
| `9d98e86` | fix(web) | replace !important with :root specificity for slate placeholder CSS |
| `054e262` | feat(web) | settings redesign, MCP config/agents pages, PlateJS theming, MCP status indicators, nav icons in header, 72 tests |
| `f6e5e11` | feat(web) | settings page, session cards, workspace persistence, PWA scaffold |
| `884af14` | fix(web) | fix Pulse chat 'Claude CLI exited 1' due to root-owned .claude dirs |
| `d7ad5bb` | fix(ask) | remove brittle Gate 5/6 URL heuristics; trust LLM citation grounding |
| `c246b22` | fix(rust) | address 5 PR review comments (env_bool fallback, authoritative_ratio, touch_running_job dedup, cancel exit 130) |
| `375e737` | fix(web) | use Number.isNaN instead of global isNaN (Biome lint) |
| `04d12e0` | fix(web) | address 6 PR review comments (JSON guard, timeout ref, block immutability, NaN split, stale comment, empty vector guard) |
| `93dd150` | fix(infra+docs) | address 4 PR review comments (pnpm sentinel gate, SSH mount opt-in, SERVE.md cleanup, crawl.md subcommands) |
| `7be0ba0` | refactor(web+pulse+ask) | pulse module splits + ask gates + omnibox/toolbar polish |
| `ddc19a0` | feat(web+docker+pulse) | pulse thinking blocks + empty bubble fix + claude hot-reload s6 + sccache |
| `aea1c5c` | fix(web+jobs+ci) | land review fixes, test env alignment, and changelog/session plumbing |
| `d6b01b2` | fix(pulse) | ensure Qdrant collection exists before upsert |
| `75d4ee7` | fix(pulse) | default save collection to AXON_COLLECTION / cortex instead of `pulse` |
| `ab79a0c` | docs(changelog) | update ccbccfd TBD sha references and session doc |
| `ccbccfd` | fix(docker+web) | dereference claude symlink for node user + path-traversal hardening in download.rs |
| `6f8f7c7` | feat(docker) | install AI CLIs in web image, non-root node user, AXON_WORKSPACE + ~/.ssh mounts |
| `f5eb415` | fix(docker) | pin codex cli package in web image |
| `93f51e8` | chore(docker+docs) | align web CLI mounts and refresh changelog |
| `4756caa` | feat(pulse+docker) | conversation memory fallback + claude binary mount |
| `4e4a9d2` | docs(changelog) | fix TBD sha → a3b3b76 |
| `a3b3b76` | fix(docker+test) | expose axon-web on 0.0.0.0, fix test pg_url normalization, update TS snapshots |
| `cec02a8` | docs(changelog) | fix a3b3b76 sha → 167ccb3 |
| `167ccb3` | feat(docker) | axon-web service + chrome Dockerfile move + web-server s6 worker |
| `6a65ead` | docs(changelog) | update unreleased section with 10 commits since last entry |
| `d1f20a4` | feat(web+crawl) | pulse workspace overhaul + refresh schedules + crawl download pack |
| `115e264` | feat(refresh) | add refresh job pipeline and command manifests |
| `3d547dd` | fix(ci) | disable strict predelete for fresh Qdrant in mcp-smoke |
| `0e4b3f2` | fix(ci) | create .env for docker compose in mcp-smoke job |
| `7b9d9ba` | fix(ci) | resolve remaining test failures for schema, ask, and web |
| `234989b` | feat(ask) | citation-quality gates + diagnostics enrichment |
| `c1d65e8` | fix(ci) | resolve all three failing CI checks |
| `d3e0c7f` | feat | harden crawl/mcp flows and resolve PR review threads |
| `9d2c182` | feat(status) | improve CLI diagnostics and refresh web accent mapping |
| `7b4c898` | feat(mcp) | hard-cutover actions and add mcporter CI smoke tests |
| `9ad2e24` | feat(mcp) | align status action parity and refresh docs |
| `6bdfa36` | feat(mcp) | add path-first artifact contract, schema resource, and smoke coverage |
| `2724a2a` | fix | Fix CI failures for websocket v2 tests and cargo-deny config. |
| `54a543b` | chore/fix | Finalize PR feedback fixes and docs updates. |
| `9d5cdd4` | fix(web) | address remaining PR review threads comprehensively |
| `6a02ad3` | feat(web) | refresh pulse UI styling and architecture docs |
| `3863d7c` | fix | address PR API review threads batch 1 |
| `4de7d94` | feat(web) | add omnibox file mentions and root env fallback for pulse APIs |
| `4ac2b46` | fix(web) | resolve pulse UI lint warnings and align renderer changes |
| `241e7ff` | feat(web) | ship Pulse workspace foundation with RAG and copilot API |
| `d15dede` | feat(web) | doctor report renderer, options reorder, result panel polish |
| `1dd74f2` | feat(web) | crawl download routes — pack, zip, and per-file downloads |

### Highlights

#### UI Design System Overhaul — 7-Agent Parallel Implementation (b585aef..e73906a)
33 design review issues addressed across 6 commits using a parallel agent team with zero file conflicts.

- **Design token foundation (`b585aef`):** Space_Mono (display) + Sora (body) fonts replace Outfit; 30+ CSS custom properties (`--axon-primary/secondary`, `--surface-*`, `--border-*`, `--shadow-sm/md/lg/xl`, `--focus-ring-color`, `--text-*`); 8 new `@keyframes` + 7 `@utility` Tailwind animation aliases; 3-radial + linear gradient body background with grain overlay via `body::before`; WCAG contrast fixes (`--axon-text-dim` 3.2:1 → 5.1:1, scrollbar pink 0.15 → blue 0.35).
- **UI primitives (`4bdee4b`):** Button hover scale (1.03/0.98) + primary glow; branded `--focus-ring-color` outline on all interactive elements (button, input, tabs, dropdown); scrollbar thumb WCAG fix; hardcoded rgba audit across `ui/` components.
- **Omnibox (`e3a0c96`):** Status bar persists 4 s post-completion with CheckCircle2/XCircle icons; dismissible `@mention` discovery tip backed by localStorage; staggered 35 ms suggestion reveals via `animate-fade-in-up`.
- **Neural canvas (`e3a0c96`):** New `zen` profile (brightness 0.3, density 0.4, 20 particles, high burstThreshold) for low-CPU focused-work mode; `useNeuralCanvasProfile` hook with localStorage persistence exported for parent consumers.
- **Pulse chat (`7ca6184`):** Asymmetric message alignment (user right 72%, assistant left 80%); ThinkingBlock word count + `animate-fade-in` reveal; radial-glow empty state with scale-in animation; 3-dot breathing loading indicator; labeled mobile pane switcher with `role="tablist"` ARIA; drag-handle divider with grip dots; unsaved title indicator dot.
- **Results panel (`e73906a`):** Virtual scrolling via `@tanstack/react-virtual` (threshold: 200 rows); top-N toggle for 1000+ row tables; failure-first service grouping in doctor report; asymmetric 2:1 metric grid; `animate-fade-in-up` stagger on table rows; focus rings on crawl-file-explorer and command-options-panel; copy button success state with `animate-check-bounce`.
- **Pages (`e73906a`):** Modal overlay delete confirmation (MCP + settings reset) replaces inline toggle; unified MCP save button (single sticky footer, dispatches to form/JSON tab handler); `font-display` section headers with icon container; improved empty states with contextual guidance; settings sidebar `border-r` accent, gradient `SectionDivider`, `border-l-2` left accent bars on sections, `max-w-[780px]`.

#### Workspace File Explorer (63e71ff..e56c72d)
- **`/api/workspace` route (`63e71ff`):** Serves AXON_WORKSPACE directory tree over HTTP; SSRF-guarded path traversal prevention.
- **Workspace nav icon (`074ad72`):** FolderOpen icon added to omnibox toolbar linking to `/workspace`.
- **`/workspace` page (`648010c`):** Full-page file explorer with tree sidebar + content viewer; directory navigation.
- **CodeViewer component (`dcb077a`, `e56c72d`):** Syntax-highlighted code viewer with line numbers and one-click copy.

#### Security Hardening + Worker Resilience (ebca63c..HEAD)
- **SSRF guards (web):** `validateAddDir()` in `buildClaudeArgs` checks `--add-dir` paths against `ALLOWED_DIR_ROOTS` (`/home/node`, `/tmp`, `/workspace`); `validateStatusUrl()` in `/api/mcp/status` blocks `localhost`, `127.x`, `10.x`, `192.168.x`, `172.16-31.x`, and IPv6 loopback/ULA ranges before probing MCP HTTP servers.
- **Input sanitisation (web):** `--allowedTools` / `--disallowedTools` values now filtered through `TOOL_ENTRY_RE` (`/^[a-zA-Z][a-zA-Z0-9_*(),:]*$/`) — malformed entries silently dropped. `PULSE_SKIP_PERMISSIONS` env var makes `--dangerously-skip-permissions` opt-out instead of hardcoded.
- **AMQP reconnect backoff (Rust):** `worker_lane.rs` adds exponential backoff (2 s → 60 s) on consecutive AMQP failures; resets on successful reconnect. Prevents thundering-herd against RabbitMQ on restart.
- **Dynamic multi-lane workers (Rust):** `loops.rs` replaces hardcoded `tokio::join!(lane1, lane2)` with `join_all(1..=WORKER_CONCURRENCY)` — lane count is now driven by config, not compile-time constants.
- **`claim_delivery()` helper (Rust):** extracts semaphore-acquire + DB claim + ack/nack into a single unit; prevents job leaks on ack failure.
- **MCP response cleanup (Rust):** `respond_with_mode` removed from crawl `status`/`list` and `domains` handlers — always inline; `#[allow(dead_code)]` + comment on `response_mode` struct fields clarify intent.
- **New test coverage:** sessions scanner/parser tests (`__tests__/sessions/`), expanded `build-claude-args.test.ts`, `mcp/route.test.ts`, `agents/parser.test.ts`.
- **New helpers:** `error-boundary.tsx`, `lib/agents/parser.ts`, `scripts/axon-mcp` launcher.

#### PR Review Batch (93dd150..c246b22)
- **Rust (5 fixes):** `env_bool()` now falls back to `default` for unknown/typo env values (not `false`); `authoritative_ratio` returns 0.0 when domain list is empty; `touch_running_extract_job` / `touch_running_ingest_job` removed — replaced with shared `common::job_ops::touch_running_job`; `handle_cancel` emits exit code 130 (SIGINT convention) instead of 0 so UI doesn't log canceled jobs as successful.
- **TypeScript (7 fixes):** `tool-badge.tsx` guards `JSON.stringify` undefined before `.slice`; `use-pulse-autosave` clears `setTimeout` ref on unmount; `use-pulse-chat` block update is now immutable (spread instead of mutation); `workspace-persistence` NaN-safe `parseSplit()` helper; pulse/chat route stale comment removed; pulse/save route guards empty embedding response before `ensureCollection`.
- **Infra / Docs (4 fixes):** `20-pnpm-install` sentinel touch gated on successful install (exits 1 on failure); `docker-compose.yaml` SSH mount commented out (opt-in); `docs/SERVE.md` legacy browser-UI instructions removed; `commands/axon/crawl.md` `errors`/`worker` subcommands added to argument-hint.

#### MCP Config, Agents, Status Indicators, Nav Icons (`054e262`, `9d98e86`)
- **MCP configuration page** (`/mcp`): full CRUD for `~/.claude/mcp.json` — form-based (stdio command+args / HTTP URL) and raw JSON editor tab, delete confirmation, glass-morphic design. Accessible directly from the omnibox Network icon.
- **MCP server status indicators**: `/api/mcp/status` probes each server on page load — HTTP via `AbortSignal.timeout(4s)` fetch, stdio via `which <command>`. Cards show animated status dot (green glow = online, red = offline, yellow pulse = checking).
- **Agents listing page** (`/agents`): parses `claude agents` CLI output into grouped card grid with source badges (Built-in/Project/Global). Shimmer skeleton loading and empty state with actionable message.
- **Omnibox nav buttons**: Network (→ `/mcp`), Bot (→ `/agents`), Settings2 (→ `/settings`) icons in every omnibox instance. Previously only Settings was one-click accessible.
- **Settings redesign**: NeuralCanvas background bleeds through glass-morphic panels; all 3-option card selectors replaced with `<select>` dropdowns; 3 new CLI flags wired end-to-end (`--add-dir`, `--betas`, `--tools`).
- **PlateJS Axon theme**: `.axon-editor` CSS scope, `axon` CVA variants, toolbar hover/active/tooltip colors aligned to design system.
- **72 new tests**: `build-claude-args.test.ts` (49), `agents/parser.test.ts` (11), `mcp/route.test.ts` (12).

#### Pulse Settings Page + Session Cards (f6e5e11)
- **Settings full page** (`/settings`): replaced popup panel with a proper Next.js route — sticky header with back button and "Reset to defaults", sidebar nav on lg+, 8 sections: Model, Permission Mode, Reasoning Effort, Limits, Custom Instructions, Tools & Permissions, Session Behavior, Keyboard Shortcuts.
- **5 new CLI flags** wired end-to-end through the entire settings → API stack: `--allowedTools`, `--disallowedTools`, `--disable-slash-commands`, `--no-session-persistence`, `--fallback-model`. Each passes from `usePulseSettings` → `usePulseChat` → `chat-api.ts` → `route.ts` → `buildClaudeArgs`.
- **Session cards**: `extractPreview()` in `session-scanner.ts` reads the first 4 KB of each JSONL file to extract the first real user message (≤80 chars) as a preview. "tmp" project label hidden; UUID filename capped at 20 chars as fallback. Limited to 4 cards.
- **Workspace persistence**: `workspaceMode` now lazy-initializes from `localStorage('axon.web.workspace-mode')` and syncs on every change. Workspace restores correctly after page reload.
- **New Session button**: "New" button (Plus icon) in `PulseToolbar` clears all chat/doc state and wipes the localStorage persistence key so blank state survives reload.
- **Handoff message chip**: session handoff messages (`I'm loading a previous Claude Code session…`) now render as a compact inline chip ("Loaded session: project · N turns") instead of the raw multi-line dump.
- **Omnibox**: settings gear always visible and navigates to `/settings` via `router.push`; controlled `input` cleared when leaving Pulse workspace.
- `settings-panel.tsx` deleted (no remaining consumers).

#### Pulse Module Splits (7be0ba0)
- Broke three over-limit files into 13 focused modules — no behavioral changes, zero re-exports:
  - `route.ts` (562→388 lines) split into `replay-cache.ts`, `claude-stream-types.ts`, `stream-parser.ts`
  - `pulse-workspace.tsx` (1093→342 lines) split into `hooks/use-pulse-chat.ts`, `use-pulse-persistence.ts`, `use-split-pane.ts`, `use-pulse-autosave.ts`, `lib/pulse/workspace-persistence.ts`, `lib/pulse/chat-api.ts`
  - `pulse-chat-pane.tsx` (952→450 lines) split into `components/pulse/tool-badge.tsx`, `doc-op-badge.tsx`, `message-content.tsx`, `chat-utils.ts`
- `ChatMessage` interface relocated from `pulse-workspace.tsx` to `lib/pulse/workspace-persistence.ts` (canonical location); all consumers updated in place.
- `computeMessageVirtualWindow` relocated to `chat-utils.ts`; test import updated directly (no shim).
- All 110 tests pass, TSC clean, Biome clean.

#### Ask / Strict Gates (d7ad5bb)
- Added `ask_strict_procedural` and `ask_strict_config_schema` config fields (both default `true`) — allow disabling Gate 5 (official-docs source check) and Gate 6 (exact-page-citation check) via env vars `AXON_ASK_STRICT_PROCEDURAL` / `AXON_ASK_STRICT_CONFIG_SCHEMA` without code changes.
- `crates/vector/ops/commands/ask.rs` extended with corresponding gate logic.

#### Pulse / Thinking Blocks + Empty Bubble Fix (ddc19a0)
- Wired Claude extended thinking (`type: 'thinking'` stream blocks) end-to-end through all four layers: `route.ts` captures them and emits `thinking_content` stream events; `chat-stream.ts` adds the event type; `types.ts` adds `PulseMessageBlock` thinking variant; `pulse-workspace.tsx` handles events and builds thinking blocks in real-time; `pulse-chat-pane.tsx` renders a collapsible `ThinkingBlock` component (violet-themed, shows char count, expands to monospace reasoning text).
- Fixed empty bubble bug: the assistant draft message was added to `chatHistory` eagerly (before any content arrived), creating a blank bubble above the "Claude thinking…" indicator. Now uses a `draftAdded` flag + `ensureDraftAdded()` helper — the bubble only appears when the first real content event (`thinking_content`, `assistant_delta`, or `tool_use`) fires.
- `groupBlocksForRender` updated to handle `thinking` blocks alongside `tool_use` and `text`; `MessageContent` now fires the structured-block render path for both `tool_use` and `thinking` blocks.

#### Docker / Hot Reload (ddc19a0)
- `axon-web` now runs three s6-overlay services: `pnpm-dev` (Next.js), `claude-session` (persistent Claude REPL with `--continue --fork-session`), and `claude-watcher` (inotifywait loop). When agents, skills, hooks, commands, or settings change on the host, `claude-watcher` restarts `claude-session` so the web app always loads the latest config without a container restart.
- `claude-session` uses `script -q -e /dev/null` to allocate a pseudo-TTY (required for interactive mode without a real terminal) and `--dangerously-skip-permissions` (container sandbox). Workspace trust dialog bypassed via `cont-init.d/10-trust-workspace` which patches `~/.claude.json` at boot.
- Watcher uses an explicit path whitelist (agents, commands, hooks, plugins, skills, settings, CLAUDE.md, .mcp.json) — runtime-written paths (`~/.claude/projects/`, `~/.claude/statsig/`, `~/.claude.json`) intentionally excluded to prevent restart loops.
- `docker/Dockerfile` builder stage now installs sccache prebuilt binary (arch-aware: `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`) so `.cargo/config.toml`'s `rustc-wrapper = "sccache"` resolves correctly during `cargo build`.
- `docs/CLAUDE-HOT-RELOAD.md` added: architecture diagram, watched paths table, setup instructions, verification commands, troubleshooting section, design decisions table.

#### CI / Test Env (aea1c5c)
- Review fixes: test env alignment across `common/tests.rs`, `crawl/runtime/tests.rs`, `embed/tests.rs`, `extract/tests.rs`; changelog and session doc plumbing.

#### Pulse / Runtime
- Fixed Pulse persistence path to ensure the target Qdrant collection exists before upserts, eliminating first-write failures when collection bootstrap lagged (`d6b01b2`).
- Fixed Pulse save default collection selection to use `AXON_COLLECTION` (fallback `cortex`) instead of hardcoded `pulse` (`75d4ee7`).
- Changelog hygiene pass replaced leftover TBD SHA references from prior branch notes and refreshed linked session metadata (`ab79a0c`).
- Fixed: `spawn claude EACCES` in Pulse chat — `docker/web/Dockerfile` now dereferences the symlink (`readlink -f`) when copying the claude binary so `node` user can execute it without traversing `/root/.local/` (700 perms) (`ccbccfd`).
- `AXON_SERVE_HOST=0.0.0.0` moved to `.env`/`.env.example` (removed from inline docker-compose env) per single-source-of-truth policy (`ccbccfd`).
- Security: `download.rs` hardened with `is_safe_relative_manifest_path()` + `canonicalize()`-based path traversal prevention (`ccbccfd`).
- `axon-web` now runs as non-root `node` user; Claude, Codex, Gemini CLIs installed from official sources inside the image (`6f8f7c7`).
- `AXON_WORKSPACE` env var mounts host workspace dir at `/workspace` inside the container (`6f8f7c7`).
- `~/.ssh` and `~/.claude.json` bind-mounted into `axon-web` for key-based git ops and Claude auth (`6f8f7c7`).
- `docker/web/Dockerfile` switched to `node:24-slim`; legacy static web UI files removed (`6f8f7c7`).
- Fixed: pinned `@openai/codex` to `0.105.0` to avoid broken `@latest` tarball (`f5eb415`).
- Aligned web runtime mounts to `/home/node/.claude*` and refreshed commit-driven changelog coverage for branch history (`93f51e8`).
- Added conversation-memory fallback for favorite-color recall in Pulse chat when upstream Claude CLI path fails, ensuring turn continuity for the common “what is my favorite color?” follow-up (`4756caa`).
- Updated Docker web image/runtime to include `claude` binary mount behavior used by the Pulse chat API subprocess path (`4756caa`).

#### Pulse Workspace (latest pass)
- Pulse workspace full overhaul: streaming tool-use blocks, model selector, source management (`d1f20a4`).
- Pulse chat pane: multi-block messages, citations, op-confirmations (`d1f20a4`).
- Pulse toolbar: model picker, permission controls, editor toggle (`d1f20a4`).
- New primitives: `pulse-markdown.tsx`, `claude-response.ts`, `prompt-intent.ts`, `/api/pulse/source` route (`d1f20a4`).
- WS protocol: `PulseSourceResponse`, `PulseToolUse`, `PulseMessageBlock` types (`d1f20a4`).
- Hooks: `use-axon-ws` additions, `use-ws-messages` streaming improvements (`d1f20a4`).

#### Refresh / Schedules
- Refresh job pipeline: `RefreshSchedule` table + schedule-claim lease (300s) (`115e264`, `d1f20a4`).
- Refresh command: full schedule CRUD — list/add/remove/enable/disable/run (`d1f20a4`).
- Command artifact manifests for axon, codex, and gemini workflows (`115e264`).
- `docs/commands/refresh.md` reference added (`d1f20a4`).

#### Ask / RAG
- Citation-quality gates: min score threshold, per-citation diagnostic fields (`234989b`).
- Diagnostics enrichment: ask command surfaces citation metadata in structured output (`234989b`).

#### MCP
- Hard-cutover to strict action parser; added mcporter CI smoke tests with resource checks (`7b4c898`).
- Hardened crawl/MCP safety and response behavior; restored compatibility paths (`d3e0c7f`).
- Added MCP artifact contract and schema-resource support (`6bdfa36`).
- Status action parity + related docs refresh (`9ad2e24`).

#### CLI / Status
- Status command: extended job table output, improved CLI diagnostics (`9d2c182`, `d1f20a4`).
- Scrape command: `--output-file` flag added (`d1f20a4`).
- Web accent palette updated (pink/blue → new interface palette) (`9d2c182`).

#### Docker / Infrastructure (latest)
- `axon-web` port binding changed from `127.0.0.1:49010` → `0.0.0.0:49010` so reverse proxies (SWAG/Tailscale) can reach the Next.js UI (`a3b3b76`).
- Fixed `docker-compose.yaml` `dockerfile:` path for `axon-web` — was relative to context (`apps/web`), now uses `../../docker/web/Dockerfile` (`a3b3b76`).

#### Tests / Rust
- Applied `normalize_local_service_url()` to all `pg_url()` test helpers across `common/tests.rs`, `crawl/runtime/tests.rs`, `embed/tests.rs`, `extract/tests.rs`, `refresh.rs` — Docker hostnames now rewrite to `127.0.0.1:PORT` when running `cargo test` from the host (`a3b3b76`).
- Updated `.env.example` comment for `AXON_TEST_PG_URL` to document auto-normalization fallback (`a3b3b76`).

#### Web / Pulse
- Regenerated stale snapshots for `pulse-chat-pane-layout.test.ts` after component rewrite; all 85 TS tests passing (`a3b3b76`).

#### Docker / Infrastructure
- Added `axon-web` service: Next.js dev UI with hot reload on port `49010`, bind-mounted source + anonymous volumes for `node_modules`/`.next` cache.
- Moved Chrome Dockerfile from `docker/Dockerfile.chrome` → `docker/chrome/Dockerfile`; updated compose reference.
- Added `web-server` s6-overlay service in `axon-workers`; healthcheck updated to include it.
- Exposed `axon-workers` port `49000` (`axon serve` HTTP + WebSocket) on localhost.
- Added `docker/web/Dockerfile` for the Next.js container build.
- `.env.example` updated with new service env vars (`AXON_BACKEND_URL`, `NEXT_PUBLIC_AXON_PORT`, etc.).

#### Web / Pulse Workspace (earlier pass)
- Added Pulse workspace foundation with RAG and copilot API (`241e7ff`).
- Added crawl download routes for pack/zip/per-file downloads (`1dd74f2`).
- Added omnibox file mentions and root env fallback for Pulse APIs (`4de7d94`).
- Applied UI/renderer polish and lint/review follow-up fixes (`d15dede`, `4ac2b46`, `6a02ad3`, `9d5cdd4`).

#### CI Stability
- Fixed strict predelete on fresh Qdrant in mcp-smoke (`3d547dd`).
- Fixed `.env` provisioning for docker compose in CI (`0e4b3f2`).
- Resolved schema, ask, and web test failures (`7b9d9ba`).
- Resolved security, crawl schema, and mcp-smoke CI checks (`c1d65e8`).
- Fixed CI failures for websocket v2 tests and cargo-deny config (`2724a2a`).

#### Stability and Review Follow-up
- Hardened crawl/MCP flows; tightened API error handling and docs alignment (`d3e0c7f`).
- Landed multiple PR feedback batches and docs updates (`3863d7c`, `54a543b`).

### Notes
- This changelog entry is commit-driven and branch-scoped to avoid stale migration guidance from unrelated historical branches.
- For file-level detail, inspect `git log --name-status main..HEAD`.
