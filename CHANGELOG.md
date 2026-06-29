# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [6.2.0] - 2026-06-29

### Added

- Add generic lifecycle store
- Seal source ledger payload fields
- Register local sources with source ledger
- Commit source ledger generations after embed
- Track git branches in source ledger
- Add refresh coalescing and redacted status

### Fixed

- Harden refresh lifecycle
- Verify SourceLedger Qdrant publish and cleanup visibility before clearing ledger state
- Keep local code-index watch roots queued after refresh failures
- Exclude uncommitted SourceLedger points from URL/full-document retrieval

## [6.1.5] - 2026-06-28

## [6.1.4] - 2026-06-28

### Fixed

- Restore axon crate layering boundaries

## [6.1.3] - 2026-06-27

# Changelog

## [6.1.2] - 2026-06-27

### Changed
- Release version bump.

## [6.1.1] - 2026-06-27

### Changed
- Release version bump.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [6.1.0] - 2026-06-26

### Added

- Parse freshness schedules
- Define safe replay snapshots
- Persist freshness schedules
- Run schedules through services
- Expose freshness schedules

### Fixed

- Align fresh history limit parsing
- Honor due batch tuning

## [6.0.2] - 2026-06-25

### Fixed

- Satisfy sqlite hardening release gates

## [6.0.1] - 2026-06-25

### Changed

- Split palette integration monoliths

### Fixed

- Harden SQLite IOERR diagnostics and recovery reporting.

## [6.0.0] - 2026-06-23

### Added

- Replace codex app-server spawn-per-completion with persistent process pool
- Add project metadata read/write to store
- Move page-cap policy to the services layer; unify CLI/MCP/HTTP at a 5k default+cap

### Changed

- Extract axon-authz micro-crate (epic axon_rust-23dw)
- Seed axon-api crate, break vector<->services cycle (epic axon_rust-23dw)
- Remove 2 of 4 core upward deps (epic axon_rust-23dw.3)
- Make src/core import-clean (epic axon_rust-23dw.3)
- Extract axon-core crate (epic axon_rust-23dw.3)
- Break vector->ingest cycle 2 (epic axon_rust-23dw)
- Extract axon-crawl crate (epic axon_rust-23dw.10)
- Break vector->jobs and vector<->code_index edges (epic axon_rust-23dw)
- Extract axon-vector crate (epic axon_rust-23dw.5-8)
- Move IngestSource DTOs to axon-api, break ingest<->jobs (epic axon_rust-23dw)
- Extract axon-ingest crate (epic axon_rust-23dw.12)
- Extract axon-extract crate (epic axon_rust-23dw.9)
- Move diff DTOs to axon-api (epic axon_rust-23dw .11 prep)
- Move ArtifactHandle to axon-api::contract (epic .2/.11 prep)
- Move ScrapeResult/IngestResult/ExtractSyncResult to axon-api (epic .11)
- Move ServiceEvent/emit/progress channel to axon-core::events (epic .11 enabler)
- Move ingest orchestration to axon-ingest::orchestrate (epic .11)
- Move predict_crawl_output_dir to axon-crawl (epic .11)
- Move compute_diff/extract_links_from_payload to axon-api::diff (epic .11)
- Move services::artifacts to axon-core::artifacts (epic .11 prep)
- Move extract_sync to axon-extract::sync (epic .11)
- Move single-URL scrape to axon-extract::scrape, break jobs/watch->services edge (epic .11)
- Move ServiceJob + JobStatus to axon-api, sever last jobs->services edge (epic .11)
- Repoint final watch test DTOs to axon-api, src/jobs now fully services-free (epic .11)
- Extract axon-jobs crate (epic axon_rust-23dw)
- Invert ServiceContext->cfg+pool, break code_index<->services cycle (epic axon_rust-23dw)
- Extract axon-code-index crate (epic axon_rust-23dw)
- Move mcp::schema DTOs to axon-api::mcp_schema, break services->mcp cycle3 (epic axon_rust-23dw)
- Extract axon-services crate (epic axon_rust-23dw)
- Relocate unified-server bootstrap to cli, break mcp<->web cycle (epic axon_rust-23dw)
- Extract axon-mcp crate (epic axon_rust-23dw)
- Extract axon-web crate (epic axon_rust-23dw)
- Extract axon-cli crate, root becomes thin binary (epic axon_rust-23dw)
- Trim 735 unused crate deps (cargo machete); narrow service_job_conv; doc-hide token reader (lavra review)

### Fixed

- Inherit product version across workspace crates; bump syncs [workspace.package] (epic axon_rust-23dw.17)
- Enforce [workspace.package]==[package] version + guard half-bumps; doc lossy ServiceJob From impls (PR review)

## [5.19.0] - 2026-06-22

### Added

- Replace codex app-server spawn-per-completion with persistent process pool
- Add project metadata read/write to store

## [5.18.0] - 2026-06-21

### Added
- `AXON_MAX_JOB_ATTEMPTS` (default 5, `0` = unlimited): the job watchdog now
  dead-letters a stale `running` job — marking it `failed` — once it has been
  reclaimed that many times, instead of re-queueing it forever. Bounds a job that
  crashes or hangs on every attempt from cycling running→pending indefinitely.

### Changed
- Hardened the SQLite job runtime against connection-pool poisoning: a single
  `ImmediateTx` RAII guard now owns every `BEGIN IMMEDIATE` transaction (enqueue,
  claim, reclaim, cleanup), so an early `?` return or a panic can no longer leave
  a connection in the pool mid-transaction and starve the worker lanes. Replaces
  four duplicated commit/rollback implementations.

### Fixed
- Crawl jobs now anchor their wall-clock timeout at job claim (before URL
  validation) and bound the DNS-resolving validation step by the same deadline,
  so a slow or hung lookup counts against `crawl_job_timeout_secs` instead of
  parking the worker lane until the process is restarted.

## [5.17.0] - 2026-06-21

### Added
- Worker-lane liveness safeguards: a panic in a job runner is caught and recorded
  as a job failure instead of silently killing the lane; the job watchdog now runs
  a starvation detector that loudly logs and re-kicks any queue holding pending
  jobs with nothing running. New `worker_starvation_secs` (default 120) and
  `crawl_job_timeout_secs` (default 7200) tuning knobs.

### Fixed
- Crawl worker lanes could silently stop claiming jobs — a runner panic, a leaked
  SQLite transaction starving the connection pool, or a wedged crawl engine — with
  no error logged, recovering only on restart. Added a connection-pool
  ROLLBACK-on-release hook plus best-effort commit/rollback on the claim path, a
  `pool.acquire()` slow-path warning, and the panic guard, starvation detector,
  and per-job crawl timeout above.

## [5.16.6] - 2026-06-20

### Added
- Prometheus `/metrics` endpoint on the unified server (`axon serve`/`axon mcp`),
  exposing ask-path request, latency, and retrieval metrics. Unauthenticated,
  like the `/healthz` and `/readyz` probes.
- Codex app-server backend: ephemeral threads, developer instructions, and a
  reasoning-effort hint (summarize requests run at `low`, the evaluate judge at
  `high`).
- `axon doctor` surfaces a Codex capability probe (available models + rate-limit
  headroom) when the configured LLM backend is `codex-app-server`.

### Changed
- Release version bump.
- Crawl page bodies are now capped at 4 MiB by default (previously unlimited);
  pages over the cap are skipped. Set `scrape.max_page_bytes = 0` in
  `~/.axon/config.toml` to restore unlimited.
- Uncapped crawls (`--max-pages 0`) of a deep (≥2 path-segment) start URL are
  now allowed — they auto-scope to the path subtree. Only uncapped root or
  single-segment crawls remain rejected.

### Fixed
- Added crawl memory guardrails for unscoped uncapped crawls, oversized page
  bodies, broadcast buffering, and queued HTML-owning fallback tasks.
- JSON/YAML/TOML files that exceed the per-file chunk cap now log a warning
  naming the file and the dropped chunk count instead of silently truncating.

## [5.16.5] - 2026-06-20

### Changed
- Release version bump.

## [5.16.4] - 2026-06-20

### Changed
- Release version bump.

## [5.16.3] - 2026-06-20

### Changed
- Release version bump.

## [5.16.2] - 2026-06-18

### Changed
- Release version bump.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.16.1] - 2026-06-16

### Changed

- Bumped CLI, palette, and Android component versions after the palette registry,
  Aurora drift guard, and Android theme follow-up changes landed on `main`.

## [5.16.0] - 2026-06-15

### Added

- **Fast local release profile.** Added `release-fast`, which keeps release
  optimization but disables LTO and increases codegen units for quicker local
  deployable builds.

### Changed

- **Crawl cache is opt-in by default.** `--cache` now defaults to `false`; use
  `--cache true` when cache reuse is desired. `--etag-conditional` now requires
  explicit `--cache true`.
- **Local build wrapper installs only the local binary.** The repo-local Cargo
  rustc wrapper now copies completed `axon` binaries only to `~/.local/bin/axon`
  and no longer updates the plugin bundle binary path.

### Documentation

- Expanded `.env.example` / `config.example.toml` for recently shipped runtime,
  compose, LLM, Qdrant, and TEI settings.
- Removed stale plugin-binary provenance docs for the old checked-in binary
  distribution path.

## [5.15.0] - 2026-06-15

### Added

- **Structured-data chunking for JSON, YAML, and TOML.** `.json`, `.yaml`/`.yml`,
  and `.toml` files now route through the declaration-driven chunker via new
  tree-sitter grammars instead of generic prose splitting. Top-level object keys
  (JSON/YAML) and tables / array-of-tables / top-level pairs (TOML) are captured
  as `SymbolKind::Key` chunks; nested keys stay inside their parent's chunk and
  the shared residual-sweep / oversized-split / 2000-char cap apply. Keys are
  searchable but excluded from the code-symbol authority boost (config keys like
  `port`/`server` are common words). Keyless files (top-level array/scalar/
  sequence) fall back to non-empty prose. The embed routing predicate
  (`should_chunk_as_code`) now derives from `language_for_extension` so it can
  never drift from the registered grammar set.

## [5.14.0] - 2026-06-15

### Changed

- **Declaration-driven code chunking (query-capture).** Reworked the code chunker
  from size-window splitting (`text-splitter` `CodeSplitter`) to declaration-driven
  chunking, where a named declaration is the chunk unit. A data-driven tree-sitter
  query registry drives per-language declaration extraction across all 8 grammars
  (Rust, Python, JS, JSX, TS, TSX, Go, Bash), closing the prior TS/JS parity gap
  (name-bound arrow-fns, function-expressions, exported consts, enums). Adds an
  axon-original residual-gap sweep and a zero-declaration whole-file prose fallback
  so import/glue/barrel files never drop to zero chunks, plus oversized-declaration
  line-boundary splitting (multibyte-safe). Drives the symbol-less code-chunk
  fragment rate below 1% (from 5% axon / 11% lab) and eliminates anonymous AST
  slivers (`() =>`, `;`, `{`) as standalone chunks. No `PAYLOAD_SCHEMA_VERSION`
  bump — boundary-only change; re-ingest reaps orphans via upsert + stale-tail
  cleanup. (epic axon_rust-8rpa)
- **Fixed `.tsx` parsing to use the JSX grammar.** `.tsx` files now route to a
  dedicated `Extractor::Tsx` (tree-sitter `LANGUAGE_TSX`) instead of the plain
  TypeScript grammar. Parsing JSX with the non-JSX grammar forced error recovery
  that fabricated spurious `method` declarations from statement-level calls and
  `if` blocks (`slice(0,3)`, `if (...)` captured as methods — bead axon_rust-2ykl)
  and also degraded brace-less direct-JSX arrow components
  (`const C = () => <jsx/>`) to symbol-less prose (bead axon_rust-gnpr). Both are
  now fixed: real React components capture as Functions and no statement-level
  node leaks in as a method. Method/method-signature query rules are additionally
  scoped to `class_body` / `interface_body` as defense-in-depth.
- **Bounded residual prose chunks to the chunk cap.** A large non-declaration
  span between captured declarations (a big top-level object/array const, a test
  file's `describe(...)` blocks) was emitted by the residual sweep as one
  unbounded prose chunk (observed up to ~7 KB), producing coarse, low-quality
  retrieval units. Oversized residual gaps now split at line boundaries (with
  overlap) through the same `MAX_CODE_CHUNK_CHARS`-bounded splitter the code path
  uses, so no chunk — declaration, prose-fallback, or residual — exceeds the cap.
- **Recovered container bodies and per-spec Go declaration names.** Container
  declarations (struct/enum/class/impl/trait/mod) emitted only a header chunk and
  advanced the cursor past the whole body, silently dropping body content not
  captured as a child leaf — struct fields, enum variants, class fields, and
  their doc comments. The cursor now advances only to the header end so the
  residual sweep recovers that content. Separately, Go `const`/`var`/`type` names
  were nulled for backward-parity (dragging Go symbol coverage to ~73%); `@decl`
  is now anchored on the spec node so grouped declarations get per-spec names.

## [5.13.0] - 2026-06-15

### Added

- **`AXON_CODEX_LOAD_USER_CONFIG`** — opt-in flag (default `false`) for the
  `codex-app-server` LLM backend. When `true`, Axon runs `codex app-server`
  against the user's real `CODEX_HOME` with the full inherited environment so
  MCP servers, skills, and hooks load, instead of the isolated stripped home.
  Surrenders synthesis isolation; intended as the escape hatch toward
  tool-enabled (agentic) Codex use. Implemented as a passthrough spawn branch in
  `src/core/llm/codex_app_server.rs`.
- **Codex CLI in the container image** — `config/Dockerfile` now installs
  `@openai/codex` in both the production and dev runtime stages, so the
  `codex-app-server` backend works in-container against the container's **fresh**
  `~/.codex` (no host MCP servers/skills/hooks) — fast by default, MCP-capable
  only when the container's own codex home is configured. The previous host-only
  restriction in `validate_codex_cmd` is removed. Codex child cleanup now sends
  SIGKILL to the process group via the `kill(2)` syscall instead of shelling out
  to a `kill` binary, so it works in slim images that ship no `procps`. `ESRCH`
  (the child already exited) is treated as success, and a cleanup failure on an
  otherwise-successful turn is logged rather than discarding the completion.
- **Persistent container codex home** — `docker-compose.prod.yaml` sets
  `CODEX_HOME=/home/axon/.axon/codex`, so the container's codex home (config.toml
  with MCP servers, auth.json, refreshed OAuth tokens, sessions) lives inside the
  already-mounted `~/.axon` and survives container recreates. Fresh/empty by
  default → fast init, and fully separate from the operator's host `~/.codex`.
  Seed it once on the host with `CODEX_HOME=~/.axon/codex codex login` (or copy
  `~/.codex/auth.json` into `~/.axon/codex/`); `OPENAI_API_KEY` works without
  seeding. Replaces the earlier read-only `auth.json` bind mount.

## [5.12.0] - 2026-06-14

### Added

- **`--exclude-path` for git ingest** — repeatable flag that skips files whose
  repo-relative path contains any supplied substring (e.g.
  `--exclude-path docs/references/`). A file is excluded when its path matches.
  Useful for repos that vendor third-party doc mirrors; re-ingesting `jmagar/lab`
  with `--exclude-path docs/references/` cut it from ~247k to ~34k chunks.

### Fixed

- **Crawl cache TTL lowered from 24h to 1h** — a cached crawl manifest younger
  than the TTL short-circuits the crawl and skips re-embedding, which silently
  left a collection empty while reporting success after a Qdrant wipe. 1h is a
  safer default for re-index workflows.

## [5.11.1] - 2026-06-14

### Fixed

- **RAG pipeline review remediation** — resolved findings from a comprehensive
  review of the ask/retrieve/synthesis path: cache `mise which` resolution and
  move Gemini home-dir prep off the async reactor (was blocking a Tokio worker
  twice per completion); redact the streaming `/v1/ask/stream` error path before
  emit/log; validate the web `/v1/ask` collection override at the handler;
  integrity-check the resolved Gemini program path.
- `release.yml`: gate the optional signing step on job-level `env` instead of
  the `secrets` context in a step `if:` (the latter is forbidden by GitHub
  Actions and had invalidated the workflow file).

### Changed

- **Typed service boundary** — `ask`/`evaluate`/`query` commands return typed
  result structs directly; the `*_payload()` JSON shims are wire-identical.
- **Synthesis context-window capability** gains an explicit
  `AXON_SYNTHESIS_HIGH_CONTEXT` override; the model profile is the auto-detect
  fallback.
- **Backend-aware LLM completion concurrency** default (Gemini 4 /
  OpenAI-compat 16; codex keeps its dedicated knob).

### Performance

- Dropped the per-ask 208-field `Config` deep clone and the ~1 MB/ask candidate
  clone on the ask path; replaced O(n²) shingle-Jaccard dedup with MinHash
  signatures; halved the secondary dual-search prefetch arm; concurrent TEI 413
  split-drain.

### Removed

- `once_cell` and the redundant `futures` umbrella crate dependencies.

## [5.11.0] - 2026-06-14

### Changed

- **Release pipeline now ships the Tauri palette.** The `palette-linux` and
  `palette-windows` jobs in `.github/workflows/release.yml` build
  `apps/palette-tauri` via `pnpm` + `tauri build --no-bundle` and package the
  portable `axon-palette-tauri[.exe]` binary. The published artifact names are
  unchanged (`axon-palette-linux-x86_64.tar.gz`,
  `axon-palette-windows-x86_64.zip`), so `axon palette install` keeps working,
  but the GitHub Release now contains the polished Aurora-styled palette instead
  of the old GPUI build.
- **`axon palette` CLI repointed to the Tauri app** — the resolved/installed
  binary is now `axon-palette-tauri[.exe]`, and `axon palette install --method
  build` builds from `apps/palette-tauri` with `pnpm` + `tauri build
  --no-bundle`.

### Removed

- **Deleted the GPUI desktop palette (`apps/desktop`)** and its
  `.github/workflows/desktop.yml` CI workflow. The Tauri palette in
  `apps/palette-tauri` is the sole desktop palette. Removed the
  `exclude = ["apps/desktop"]` entry from the root `Cargo.toml` workspace.

## [5.10.1] - 2026-06-14

### Security

- **Unified secret redaction** — added `core::redact` with a single regex-based
  redactor (`redact_secrets`) that operates on the full string (not whitespace
  tokens) and is a superset of every redactor it replaces: Google `AIza...` API
  keys, Google `ya29.` OAuth tokens, token-anchored `sk-` OpenAI keys,
  `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_` GitHub tokens, `atk_` tokens,
  `Authorization:` header values, the `API_KEY`/`TOKEN`/`SECRET` (`=` or `:`)
  marker rules, and a Shannon-entropy-gated 32+ char high-entropy run (benign
  repeated padding is left intact). Prefix rules are `\b`-anchored to avoid
  mid-word false positives. Replaces the divergent token-splitting redactors in
  `core::llm::headless::common` and `core::llm::openai_compat` (the headless
  JSON-aware structural redaction is preserved), closing a Google-API-key leak
  path through the Gemini subprocess stderr tail (surfaced on the
  `/v1/ask/stream` SSE error event and in `~/.axon/logs/axon.log`).

## [5.10.0] - 2026-06-13

### Added

- **WARC archive output for crawls** (`--warc <path>`) — write every fetched
  page of a crawl to a WARC 1.1 archive. HTTP and Chrome render paths both
  archive identically (spider `warc` feature enabled, wired in
  `src/crawl/engine/runtime.rs`). Useful for archival/compliance and
  re-embedding from disk without re-fetching. The setting round-trips through
  the crawl job config snapshot. Ported from spider's `warc`/`warc_chrome`
  examples.
- **Chrome web-automation scripts for crawls** (`--automation-script <path>`) —
  a JSON file mapping URL path prefixes to ordered automation steps
  (click/scroll/wait/fill/evaluate/screenshot) that spider runs against each
  matching page during a Chrome crawl before capture. Unlocks crawling sites
  that need interaction (cookie banners, "load more", infinite scroll). New
  `src/crawl/automation.rs` parses the file into spider's `AutomationScriptsMap`;
  applied only on Chrome render paths. Ported from spider's
  `chrome_web_automation` example.
- **RSS / Atom / JSON feed ingest source** — `axon ingest <feed-url>` (or the
  explicit `rss:` / `feed:` / `atom:` prefix) auto-classifies feed URLs and
  embeds one document per entry (HTML content converted to markdown, with
  title/link/published metadata). Backed by `feed-rs`; new `src/ingest/rss.rs`.
  Participates in `axon refresh` via `seed_url`. Available on the CLI
  (auto-classified), MCP (`source_type=rss`), and REST ingest paths. Ported
  from spider's `rss` example, adapted to axon's ingest framework.

## [5.9.2] - 2026-06-13

### Added

- **Ask quality smoke** — added `scripts/smoke-ask-quality` to verify live ask
  quality against the configured Axon stack, including full-doc context count,
  citation validation, and final source ordering.
- **Indexed token statistics** — `axon stats --json` now reports sampled
  average chunk/doc token estimates from indexed Qdrant payloads.

### Changed

- **Ask synthesis context** — high-context Gemini/Claude/GPT/Codex-family
  models now receive at least four selected full documents, and selected full
  documents are prioritized ahead of loose chunks in the final context order.
- **Ask citation validation** — ask JSON now includes structured
  `citation_validation` metadata, canonical URL variants are deduped before
  counting citations, and one non-streaming repair retry runs when the first
  answer fails citation validation.
- **Gemini headless backend** — Axon now resolves the real Gemini CLI behind
  mise shims, uses argument transport for small prompts and stdin for large RAG
  prompts, and preserves process stderr/status when stdin write failures occur.

## [5.9.1] - 2026-06-11

### Fixed

- **Palette operation results** — removed redundant source-only rows from scrape
  output, filtered empty markdown bullets, and moved document metadata into the
  result header so scraped content starts cleanly.
- **Palette code blocks** — replaced the bordered purple code-block treatment
  with syntax-highlighted Shiki rendering and quieter operation-reader styling.

## [5.9.0] - 2026-06-10

### Added

- **Unified file-ingest engine** (`src/vector/ops/file_ingest.rs`) — a single
  shared `collect_files` walker and `chunk_file` adapter that replaces four
  divergent copies in GitHub, GitLab, generic Git, and `embed <dir>`. All git
  providers now produce tree-sitter AST-aware chunks with canonical
  `code_*`/`symbol_*` Qdrant payload fields; previously only GitHub did.
- **Symbol metadata for GitLab file ingest** — `gitlab_file_chunk_payload` added
  to `src/ingest/gitlab/embed.rs`; closes `axon_rust-wavn`.
- **Symbol metadata for generic Git / Gitea file ingest** — `file_docs` in
  `src/ingest/generic_git.rs` now emits one `PreparedDoc` per `CodeChunk` with
  `line_start/end`, `chunking_method`, `symbol_name/kind`, and
  `symbol_extraction_status`.
- **Symbol metadata for local `embed <dir>` code files** — `prepare_embed_docs`
  in `src/vector/ops/tei/prepare.rs` now routes local code files through the
  shared `chunk_file` engine and attaches `code_*`/`symbol_*` extra payload per
  chunk, aligning `axon embed` output with the ingest path.

### Changed

- `src/ingest/git_files.rs` — removed the now-redundant `collect_repo_files`
  walker (superseded by `file_ingest::collect_files`); `embed_docs` wrapper
  retained.

## [5.8.1] - 2026-06-10

### Changed

- **Split `apps/web/app/page.tsx`** (1467 → 460 lines) into five sibling modules
  to satisfy the monolith policy (≤500 lines): `panel-types.ts` (type defs),
  `command-format.ts` (command/result formatting + `commandExamples`),
  `job-helpers.ts` (job/doctor data helpers), `panel-components.tsx`
  (presentational components), and `use-panel-data.ts` (state + handlers hook).
  Pure structural extraction — no behavior change. Deduped a duplicate
  `formatBytes` and tightened the `ArtifactHandle` type to carry
  `kind`/`display_path`/`line_count`.

## [5.8.0] - 2026-06-10

### Added

- **Web panel artifact rendering** — commands that return artifact handles now
  render them in the panel: screenshot artifacts display inline as `<img>`, other
  kinds show concise file-metadata rows (kind badge, filename, size, download
  link). New `GET /api/panel/artifact/{*path}` route serves files from
  `cfg.output_dir` with MIME detection, path-containment validation, and
  panel-auth gating. (Ported from the closed #197 affinity branch.)

## [5.7.9] - 2026-06-10

### Changed

- **Toolchain: pin Rust 1.96.0** (`rust-toolchain.toml` 1.94.0 → 1.96.0,
  current stable). Recent branches were already building and passing the full
  test suite under 1.96; the pin now matches. The sccache wrapper's toolchain
  canonicalization keys `stable` and `1.96.0` to the same compiler path, so
  dist cache archives stay consistent.

## [5.7.8] - 2026-06-10

Collapse PR #197 (`feat/qdrant-affinity-tei-burst`) into this branch by
cherry-picking its four unique commits, so #196 merges to main as a single PR
with no stack.

### Added

- **`docker-compose.prod.yaml`** — shield `axon-qdrant` from the global OOM
  killer (`oom_score_adj -500`) and raise its CPU limit from 4 to 8.

### Changed

- **`src/web/server/routing.rs`** — split the monolithic `router()` into scoped
  route-table builders (`read_routes`/`write_routes`/`large_write_routes`/
  `panel_routes`). Preserved this branch's `/v1/artifacts/{*path}` read route;
  dropped the affinity branch's `/api/panel/artifact` route (its `panel_artifact`
  handler is not part of this branch).

## [5.7.7] - 2026-06-10

### Changed

- **Monolith policy: sitemap split + allowlist cleanup.** Split
  `src/crawl/engine/sitemap.rs` (721 lines) into
  `sitemap/{discover,backfill,filter}.rs` with the module root keeping the
  shared bounded-HTTP fetch helpers and re-exports — no public-surface or
  behavior change. Pruned 7 stale `.monolith-allowlist` entries: 3 web files
  that no longer exist (`job-detail-ui.tsx`, `axon-shell-state.ts`,
  `ws-messages/provider.ts`) and 4 Rust files now under the cap or
  pattern-exempt (`crawl/scrape.rs`, `core/config/parse.rs`,
  `core/config/types/config.rs`, `services/system.rs`,
  `services/types/service.rs`). Pre-commit hooks pass again without
  `--no-verify`.

## [5.7.6] - 2026-06-10

### Fixed

- **MCP: per-action fields now visible to schema-flattening clients.** The
  `axon` tool's `tools/list` `inputSchema` kept all per-action request fields
  (`query`, `url`, `job_id`, `response_mode`, …) inside `oneOf` branches, so
  clients that render callable parameters from top-level `properties` only
  (Codex, mcporter signatures, Labby Code Mode `.d.ts` consumers) saw just
  `{action, subaction}`. `enrich_tool_input_schema` now lifts a merged,
  all-optional superset of every per-action field into top-level `properties`,
  each annotated with an `Applies to action(s): …` description prefix and an
  `x-axon-actions` array; fields with conflicting shapes across actions (e.g.
  `limit`) publish an `anyOf` union. The strict `oneOf` validation contract and
  serde parsing are unchanged. (`src/mcp/server/tool_schema.rs`)

## [5.7.5] - 2026-06-09

Consolidates the multi-lane RAG code-review hardening (P1/P2/P3) plus the two
critical findings reserved for the lead pass. Tree verified green: `cargo fmt`,
`cargo check --all-targets`, and `cargo clippy --workspace --all-targets -- -D
warnings` all clean.

### Changed

- **A-C1 — services↔vector dependency cycle broken.** Relocated the shared leaf
  code out of the `services` layer so `vector` depends *down* on `core` instead
  of cyclically on `services`:
  - `src/services/llm_backend{.rs,/}` → `src/core/llm{.rs,/}` (`crate::core::llm`)
  - `src/services/error{.rs,/}` (`ServiceError` + taxonomy) → `src/core/error{.rs,/}`
    (`crate::core::error`)
  - `AskExplain*` / `CorpusHealthKind` / `CorpusHealthDiagnostic` trace and
    diagnostic types moved from `services::types::service::query` to
    `crate::core::ask_explain`, re-exported through `services::types` for the
    wire/CLI surface (`AskResult`/`AskTiming` remain service-layer wire
    contracts). The `vector` layer now imports zero `crate::services` paths in
    production code.
- **Q-H3 — `PreparedDoc::ingest()` constructor** added to `src/vector/ops/tei.rs`;
  all 22 ingest-source `PreparedDoc { .. }` literals migrated to it, removing the
  repeated `content_type: "text", extractor_name: None, structured: None` tail.

### Fixed

- **O-C1 — ask-quality regression gate no longer silently passes.**
  `scripts/test-ask-quality-regressions.sh` now runs every `cargo test` filter
  through `scripts/cargo_test_filter_guard.py`, so a filter that matches zero
  tests hard-fails instead of exiting 0. Removed four filters whose target tests
  do not exist (citation-grounding / authoritative-allowlist / five-query
  fixture) — flagged as follow-up coverage to author with their underlying
  policy code.
- Removed an unused `SetupMethod` import, fixed `unused_qualifications` warnings
  in `src/jobs/watch/change_detect_tests.rs`, `collapsible_if` lints in
  `src/cli/commands/{palette,setup}.rs`, and an orphaned `src/services/crawl/audit.rs`
  stub surfaced during the green-tree pass.

## [5.7.4] - 2026-06-09

Fix compile errors from parallel lanes (routing type, Vec<String> test
assertions, doc comment lint) and add screenshot support to desktop app.

### Fixed

- **`src/web/server/routing.rs`** — `panel_routes()` return type changed from
  generic `Router<S>` to concrete `Router<(AppState, Arc<Config>)>`; the
  `setup_targets` handler requires `State<(AppState, Arc<Config>)>` so the
  generic bound was never satisfiable at the call site.
- **`src/vector/ops/token_policy_tests.rs`** — four `Vec<String>::contains("lit")`
  calls updated to `contains(&"lit".to_string())`; `Vec<T>::contains` requires
  `&T`, not `&str` (unlike `HashSet` which accepts `&str` via `Borrow`).
- **`src/ingest/github/files/prepare.rs`** — inserted blank `///` line before
  continuation paragraph to satisfy `clippy::doc_lazy_continuation`.

### Added

- **Desktop screenshot command** — new `screenshot` action in the palette;
  `RestClient::fetch_bytes()` retrieves the PNG artifact; `OutputSection` carries
  an `image` field so the rendered output body can display the screenshot inline.
- **`src/services/scrape.rs`** — artifact write logic moved from `generic_scrape`
  to the service layer so all callers (CLI, MCP, REST) share identical write
  semantics.

## [5.7.3] - 2026-06-09

Fix desktop palette health check endpoint and add offline test coverage for
watch change-detection branches.

### Fixed

- **Desktop palette** — health-check dot now probes `/healthz` (unauthenticated,
  lightweight) instead of `/v1/doctor`; adds disconnected-server UX warning before
  submitting commands.

### Tests

- **`src/jobs/watch/change_detect`** — added `WatchFetcher` injection seam
  (`LiveFetcher` production path + `StubFetcher` for tests); four offline
  `#[tokio::test]` cases now cover the 304 short-circuit, probe-failure fallback,
  hash-equal skip, and first-seen (seed) branches deterministically without any
  live HTTP.

### Internal

- **`src/services/llm_backend/concurrency`** — tighten `acquire_completion_permit_for_key`
  visibility to `pub(crate)`.

## [5.7.2] - 2026-06-09

Documentation accuracy: remove stale `CHROME_URL` workaround instructions.

### Documentation

- **`src/crawl/CLAUDE.md`** — updated `NoResponse` troubleshooting row: `CHROME_URL`
  in `.env` is a stale alias (deleted by `axon config migrate`); `runtime.rs` already
  pins the connection via `with_chrome_connection` so the spider `CHROM_BASE` fallback
  never fires.
- **`docs/guides/configuration.md`** — marked `CHROME_URL` as a stale alias in the
  Chrome browser env-var table; added clear "do not set" instruction pointing to
  `AXON_CHROME_REMOTE_URL`.

## [5.7.1] - 2026-06-09

Bug fixes and documentation accuracy corrections.

### Fixed

- **`--etag-conditional` without `--cache` lost 304-skipped pages** (`src/crawl/engine/dir_ops.rs`) —
  `prepare_crawl_output_dir` now archives `markdown/` to `markdown.old/` when either
  `cfg.cache` OR `cfg.etag_conditional` is set; previously `etag_conditional=true` with
  `cache=false` left no recycling bin, so `reconcile_unmodified()` could not relink
  unchanged pages and they were silently dropped.
- **SearXNG search wrongly constrained by Tavily's 100-result cap** (`src/services/search.rs`) —
  `enforce_pagination_window()` is now applied only on the Tavily code path; SearXNG
  queries above 100 results no longer fail with a misleading Tavily cap error.
- **`--no-source` / `--include-source` help text said `(GitHub only)`** (`src/core/config/cli.rs`) —
  these flags apply to all Git providers (GitHub, GitLab, Gitea, generic-git); the
  redundant `--include-source` flag (inverse of `--no-source`) has been removed.

### Documentation

- **`src/ingest/CLAUDE.md`** — corrected two stale claims: GitHub file ingest now uses
  `git clone --depth=1` (not reqwest tree-fetch); YouTube playlist ingest is a sequential
  `for` loop capped at 500 videos (not N=5 concurrent via `FuturesUnordered`).

## [5.7.0] - 2026-06-09

Multi-lane RAG-pipeline hardening: Qdrant/TEI accuracy fixes, ingest unification,
jobs ops improvements, documentation correctness sweep, CI hygiene, and plugin cleanup.

### Security

- **Prompt-injection defence in ask context** (`ask/context/build/appenders.rs`) —
  every retrieved chunk body is now wrapped in a
  `<retrieved_content trust="evidence_only">` XML trust boundary so the synthesis
  model can distinguish indexed content from axon-generated scaffolding.  Structural
  markers (`## Sources`, `## Source Document`, `## Top Chunk`, `## Supplemental Chunk`)
  and forged citation keys (`[S{n}]`) inside chunk bodies are defanged with
  U+200B zero-width spaces, preventing indexed content from injecting forged
  citations or misleading section headers into the answer context. (S-H2)

### Added

- **`xtask check-version-sync`** — new pre-commit and CI check that verifies
  `Cargo.toml`, `README.md`, and `CHANGELOG.md` all carry the same version string
  and that `plugin.json` does NOT contain a `version` key.
- **`scripts/axon-backup.sh`** — Qdrant snapshot API + `sqlite3 .backup` script
  with SHA256 checksums and restore instructions.
- **`benches/chunking.rs`** — criterion benchmarks for `chunk_text` and `chunk_markdown`.
- **Prompt-injection regression tests** (`ask/context/build/appenders_tests.rs`) —
  T-C2 tests verify `## Sources`/`[S{n}]` blocks injected into retrieved content
  are defanged before reaching the synthesis prompt. (T-C2)
- **Budget-fitting and truncation tests** (`ask/context/build/appenders_tests.rs`) —
  T-H4 tests cover: XML overhead in the minimum-budget guard, unicode-boundary
  truncation of oversized excerpts, the multi-doc skip-and-continue path, and
  separator accounting. (T-H4)
- **Malformed-judge output parse tests** (`evaluate/scoring_tests.rs`) — T-M5
  sidecar covers empty/garbage judge responses, partial axis detection, winner
  resolution, `rag_underperformed`, `build_suggestion_focus`, and source URL
  extraction edge cases. (T-M5)
- **CI path filtering** — `dorny/paths-filter@v3` gates `test-infra` and `live-qdrant`
  jobs to only run when relevant source paths change; new `version-sync` job in
  `production-gate`.

### Changed

- **`ask_payload_with_delta_handler` decomposed** into three focused functions
  (`resolve_answer_and_timing`, `assemble_ask_payload`, trimmed orchestrator)
  each ≤60 lines; eliminates a 124-line mixed-concern function. (Q-M2)
- **`fetch_full_docs` returns `Arc<Vec<QdrantPoint>>`** — doc-cache hits now
  share the allocation without cloning the entire point vector; `FetchedDoc` type
  alias reflects the Arc wrapper throughout `fetchers.rs` / `build.rs` /
  `appenders.rs`. (P-M4)
- **One candidate-count integer instead of a cloned pool** — `AskRetrieval` now
  stores `candidate_count: usize` rather than `candidates: Vec<AskCandidate>`.
  The pool was materialised only to call `.len()` two lines later; eliminates
  a full `candidates_only()` clone on every ask request. (P-M5)
- **`RepeatGuardStop` is a typed error struct** in `streaming.rs` instead of a
  `const &str` sentinel; `Display` still emits `"repeat_guard_stop"` so LLM
  backend error-string detection remains stable. (B-M4)
- **`build_timing_json` slice-iterates the diagnostics fields** — replaces 10
  repeated `if let Some(v) = e.field { obj.insert(...) }` blocks with a single
  `&[(&str, Option<u128>)]` loop. (L2)
- **Inline test blocks → sidecar files** in `ask/output.rs`,
  `ask/context/retrieval/build.rs`, and `ask/context/retrieval/dispatch.rs`
  per the project's sidecar `_tests.rs` convention. (B-H1)

### Fixed

- **TEI retry documentation corrected** — 6 attempts (not 5), backoff includes 16s tier,
  triggers on 429 + any 5xx only (not transport errors), worst-case budget ~213s.
- **`PAYLOAD_SCHEMA_VERSION` corrected to 7** throughout CLAUDE.md files.
- **VectorMode cache reconciliation** — `axon migrate` restart note clarified:
  restart only required when the destination collection name differs from the source.
- **BM42 collision math** corrected in `src/vector/CLAUDE.md` (12% at 100 terms, 24% at 200).
- **Plugin SessionStart hook removed** — the axon plugin no longer runs
  `setup plugin-hook` on every session start; no session side-effects on startup.
- **Two-Tier Signature Convention documented** in `src/services/CLAUDE.md`.

## [5.6.1] - 2026-06-09

Remediation of the 10 surviving findings from the four-PR code review of
#185 / #186 / #188 / #192.

### Fixed

- **Server-side embed no longer fails on `node_modules` symlinks** — the
  MCP/REST embed validator (`src/services/embed.rs`) now prunes
  VCS/dependency/build directories **before** its symlink check.
  `node_modules/.bin/*` is symlinks by design and the reader never visits
  pruned subtrees, so validation no longer rejects JS projects over files that
  are never read. Symlinks outside pruned dirs are still rejected.
- **Concurrent same-repo ingest race eliminated** — ingest job claims
  (`src/jobs/ops/lifecycle.rs`) now serialize per `(source_type, target)`: a
  pending job whose target has a running sibling stays queued (other targets
  are claimed past it). Previously two jobs for the same repo on parallel
  lanes could race, letting one job's repo-scoped stale cleanup delete points
  the other had just upserted.
- **`axon refresh` replays the original job's config** — each re-enqueued
  origin now applies the most recent stored config snapshot for that crawl URL
  / ingest target (max-depth, page caps, subdomain scoping, headers,
  include-source, …) instead of current process defaults, which silently
  widened or narrowed previously scoped crawls. Collection and service
  endpoints always follow the current process config.
- **`axon refresh` exits nonzero on partial failure** — when origins fail to
  enqueue (e.g. the pending-job cap is hit mid-loop), the per-origin failures
  were printed but the command still exited 0; scripted `--yes` runs could not
  detect that most origins were never enqueued.
- **Prose-fallback chunk line numbers are exact on files with repeated
  content** — GitHub ingest's fallback chunker re-discovered each chunk's byte
  offset by substring search, which locks onto the first duplicate occurrence
  and emits wrong `code_line_start`/`code_line_end` + `#L` fragments. The
  chunker (`chunk_text_with_offsets`) now reports true byte offsets.
- **CLI embed root-symlink policy made explicit** — POSIX-style (like `du` /
  `find -H`): a symlink named explicitly as the embed target is followed,
  symlinks encountered during traversal are skipped; now documented and
  regression-tested (the server path still rejects symlinked roots).
- **Local embed no longer drops empty docs silently** — docs that are empty or
  chunk to nothing are counted and reported via a single
  `skipped_empty_docs count=N` warning (per-file detail at debug level).
- **`axon embed <host path>` no longer embeds the literal path string** — a
  fire-and-forget local-path embed landed on the axon container's worker
  (shared jobs DB), where the host path doesn't exist; the reader's free-text
  fallback then "successfully" embedded `/home/<user>/docs` as a one-chunk
  document. Two fixes: path-shaped inputs that don't resolve are now a hard
  error in the reader (both claim directions fail loudly instead of corrupting
  the index), and the CLI runs local-path embeds in-process even without
  `--wait` — a local path can only be embedded by a process that shares its
  filesystem, so queueing it was never serviceable.
- **Local embed file-size cap (10 MB, matching the server validator)** — a
  31 MB machine-generated JSON ground the prose chunker for minutes (release)
  to hours (debug) and would have flooded the collection with junk chunks.
  Directory walks skip oversized files with a `skip_oversized_file` warning;
  an explicitly named oversized file is a hard error naming the cap.
- **Payload-index assertion no longer fails embeds or re-PUTs every index** —
  `ensure_payload_indexes` previously fired ~46 concurrent index PUTs on every
  embed and treated a single timeout as fatal, so a slow/overloaded Qdrant
  failed the whole embed during collection init. It now reads the collection's
  `payload_schema` (from the GET ensure_collection already performs) and only
  asserts missing indexes — zero PUTs on a warm collection — and index
  failures log a warning instead of aborting (missing indexes retry on the
  next embed).

### Changed

- **Ask full-doc budget fitting renders once, not up to 7×** — the top-k
  ladder in `fit_full_doc_entry_to_budget` now ranks chunks once and walks the
  ladder on length arithmetic, rendering only the rung that fits (previously
  each rung cloned all points and re-scored + re-rendered the document).
- **Single source of truth for re-ingestable source types** —
  `RE_INGESTABLE_SOURCE_TYPES` now lives in `src/jobs/ingest/types.rs` next to
  `source_type_label`; `axon refresh` imports it instead of duplicating the
  provider list (adding a provider previously required touching both).

### Documentation

- New CLAUDE.md gotcha: **GitHub stale cleanup only sees schema-v7+ points** —
  file chunks indexed before payload schema v7 (5.5.0) lack the `git_*` filter
  fields and are invisible to `qdrant_delete_stale_repo_file_urls` until their
  repo is re-ingested at v7+.

## [5.6.0] - 2026-06-09

### Added

- **Android APK release workflow** (`.github/workflows/android-release.yml`).
  Pushing an `android-v*` tag whose version matches `versionName` in
  `apps/android/app/build.gradle.kts` builds the release APK, signs it
  (zipalign + apksigner from release-keystore secrets, falling back to an
  unsigned APK when secrets are absent), checksums it, and publishes a GitHub
  Release with the APK + SHA256 attached (`make_latest: false`).
  `workflow_dispatch` runs the same build as a dry-run that uploads the APK as a
  run artifact without creating a Release. The Aurora design-system composite is
  resolved via an optional `AURORA_REPO` checkout. Mirrors the
  `chrome-extension-release` pattern so the Android app versions independently of
  the main axon `v*` releases.

## [5.5.5] - 2026-06-09

### Changed

- **Renamed the `axon` usage skill to `using-axon`** so the skill directory and frontmatter `name` match the plugin's `dir == name` convention (the skill is now invoked as `axon:using-axon`). Updated the skill reference in `plugins/axon/README.md` and the global agent memory pointer.

### Documentation

- **`plugins/axon/README.md` skills inventory corrected** from a stale "Skills (16)" per-action list to the actual "Skills (2)" (`using-axon`, `axon-rag-synthesize`) after the per-action skills were consolidated into the unified usage skill.

## [5.5.4] - 2026-06-09

### Changed

- **Qdrant container memory cap 12G → 16G.** The `axon` collection's resident working set (int8 `always_ram` quantization + in-RAM HNSW index) stably sits ~12.5G and was OOM-killed against the old 12G `deploy.resources.limits` cap (206 restarts, dmesg `CONSTRAINT_MEMCG`). Raised to 16G for ~28% headroom over the stable working set; the `MemoryMax` ceiling was never hit (`oom_kill=0`). To trade RAM for latency instead, flip the collection's quant `always_ram:false` + `hnsw.on_disk:true`.

### Documentation

- **Config / env reference sync.** Documented previously-undocumented env vars and config knobs across `.env.example`, `config.example.toml`, and `docs/guides/configuration.md`: `AXON_RESEARCH_FULL_CONTENT`, `AXON_LLM_COMPLETION_CONCURRENCY` / `_TIMEOUT_SECS`, the `GOOGLE_API_KEY` alias, the synthesis/chat model split (`AXON_SYNTHESIS_*` / `AXON_CHAT_*` with legacy `AXON_OPENAI_MODEL` / `AXON_HEADLESS_GEMINI_MODEL` aliases), `AXON_WATCH_TICK_SECS` / `AXON_WATCH_LEASE_SECS`, `AXON_MCP_TRANSPORT`, `AXON_LOG_FULL_QUERIES`, standard `NO_COLOR` (replacing `AXON_NO_COLOR`), and the `endpoints` / `suggest` / Codex-backend discovery vars. Updated `ask.chunk-limit` (default 20 → 24, clamp 3–64, model-tier-derived) and `authoritative-domains` / `authoritative-boost` defaults.
- **`using-axon` skill gains a Configuration section** pointing at the three authoritative config references (`config.example.toml`, `.env.example`, `docs/guides/configuration.md`) instead of duplicating the env surface.

## [5.5.3] - 2026-06-09

### Fixed

- **Plugin `userConfig` missing from `plugin.json`.** The HTTP MCP transport in `.mcp.json` referenced `${user_config.server_url}` but no `userConfig` block existed — users were never prompted for the server URL, leaving the MCP connection URL malformed. Added `server_url` (required, default `http://localhost:8080`) and `api_token` (sensitive) fields.
- **Plugin HTTP MCP auth not wired.** Added `headers: { Authorization: "Bearer ${user_config.api_token}" }` to `.mcp.json` so token-protected axon instances can authenticate the MCP connection.
- **README accuracy + completeness pass.** Corrected `/v1/actions` (a removed 404 stub) to reference the live `/v1/*` REST routes, and fixed the Claude plugin manifest path to `plugins/axon/.claude-plugin/plugin.json`. Replaced the non-matching `env_file_` test selector with `load_dotenv`.
- **README coverage gaps closed.** Added the previously undocumented `brand`, `diff`, `endpoints`, `monitor jobs`, `refresh`, and `train` commands to the CLI Map; documented the SearXNG search backend (`AXON_SEARXNG_URL`, Tavily fallback), `GITLAB_TOKEN`/`GITEA_TOKEN`, the full ingest source list, and a new "Notable Capabilities" section covering hybrid RRF search, vertical extractors, and the `axon serve` web panel.
- **CLAUDE.md drift fixes.** Synced `payload_schema_version` to 5 in `src/vector` and `src/extract`, corrected the `CommandKind` count/list in `src/core`, and fixed the version-bump plugin-manifest path in the root `CLAUDE.md`.

## [5.5.1] - 2026-06-09

### Changed

- **Axon plugin skill renamed `axon` → `using-axon`.** The packaged skill under
  `plugins/axon/skills/` moved from `axon/` to `using-axon/` (SKILL.md plus the
  `async-job-lifecycle` and `mcp-response-protocol` references), and the plugin
  MCP manifest moved from `plugins/axon/mcp.json` to `plugins/axon/.mcp.json`
  (HTTP transport pointing at `${user_config.server_url}/mcp`).

### Fixed

- **MCP query-family errors now carry their cause.** `logged_internal_error`
  (`src/mcp/server/common.rs`) previously returned a bare `"<context> failed"`
  to the client while discarding the real error — only logging it server-side.
  Callers (and the Labby gateway) saw opaque messages like `ask '...' failed`
  with no actionable detail. It now appends the error's top-level message
  (`"<context> failed: <cause>"`) for `ask`/`query`/`retrieve`/`evaluate`, while
  the full source chain (with any nested DSNs/paths) still goes only to the
  server log. This restores informative errors without leaking deep internals.
- **MCP error source-chain walk is now bounded against cyclic sources.**
  `logged_internal_error` (`src/mcp/server/common.rs`) guards the error
  `source()` traversal so a self-referential or cyclic error chain can no longer
  spin indefinitely when building the server-side log. The log line now also
  appends a `… (source chain truncated at 16)` marker when the cap is hit, so a
  clipped chain can't be mistaken for a terminated one.
- **`logged_internal_error` doc-comment corrected.** It now describes the helper
  as general (every MCP handler routes through it) and states that callers are
  responsible for passing a client-safe top-level message, rather than claiming
  the forwarded message is always safe.

### Tests

- **Unit coverage for `logged_internal_error`** (`src/mcp/server/common_tests.rs`):
  top-level cause appears in the client message, only the top-level `Display` is
  forwarded (deeper chain stays out of the response), and a self-referential
  `source()` terminates via the depth cap.

### Removed

- **Repo cleanup.** Dropped stray/leftover files: the empty `=12.2` artifact,
  the unused `benches/dom_extraction.rs` benchmark (and its `[[bench]]` entry in
  `Cargo.toml`), the duplicate top-level `bin/axon` wrapper, and a stale
  `research-output.md`. Added `mempalace.yaml` / `entities.json` to `.gitignore`
  (MemPalace per-project files, issue #185).

## [5.5.0] - 2026-06-08

### Added

- **Symbol-aware code chunking for GitHub ingest.** Code files are chunked with
  tree-sitter (new `src/vector/ops/input/code/{chunk,extract,postprocess}.rs`),
  emitting per-chunk `symbol_name`/`symbol_kind`/`symbol_extraction_status` and
  `code_line_*`/`code_chunking_method` metadata via a canonical `git_*`/`code_*`
  payload (`src/ingest/git_payload.rs`). Adds the `tree-sitter` dependency.
- **Code-aware retrieval ranking.** Query/ask retrieval factors code-symbol and
  line-range metadata into ranking and trace output
  (`src/vector/ops/commands/retrieval/*`, `query.rs`).

### Changed

- GitHub file ingest (`src/ingest/github/files/*`) reworked around the new code
  chunker; payload-index schema extended for the `code_*`/`symbol_*` fields.

> Rebuilt from PR #187 onto a proper base off `main` — the original branch was an
> orphan with no shared history with `main` (unmergeable). No drift: `main`'s
> #188 (`select.rs`) and `be78e629` work are untouched.

## [5.4.2] - 2026-06-08

### Fixed

- **Binary sync targets the in-repo plugin bundle, not the plugin cache.** `just
  sync-container`, `just link-bin`, `just install-debug`, and the `scripts/axon`
  background auto-sync now `install` the freshly built binary into
  `plugins/axon/bin/axon` (the LFS-tracked bundle the plugin ships) instead of
  symlinking it into `~/.claude/plugins/cache/jmagar-lab/axon`. The old glob
  hardcoded a marketplace name that no longer matches every install (e.g.
  `labby-marketplace`), so the plugin-sync step silently did nothing; it also
  wrote into the plugin manager's cache — the wrong layer. The new behavior
  matches what `scripts/cargo-rustc-wrapper` already does on every link, and
  refreshes the bundle even when the binary is already current (no rebuild).
  Runtime pickup still requires a plugin reinstall/refresh.
- **Admin panel build.** `apps/web/lib/axon-client.ts` referenced the removed
  `WatchCreateRequest` OpenAPI schema; corrected to `WatchDefCreateRequest` (the
  `POST /v1/watch` request body), unblocking `next build` / `just web-build`.
- **Release pipeline repaired.** `release.yml` never produced 5.x binary assets
  because every build job failed: the non-cone `sparse-checkout` omitted root
  `Cargo.toml`/`Cargo.lock` (axon jobs) and `apps/desktop` (palette jobs), and
  the axon jobs never created the `apps/web/out` folder the binary RustEmbeds.
  All four jobs now do a full checkout, and the axon jobs add the empty
  `apps/web/out` placeholder (mirroring `ci.yml`). The Windows axon build also
  disables the bash `rustc-wrapper` (which can't run as a Windows rustc wrapper —
  os error 193), matching the palette-windows job.
- **`install.sh` asset contract aligned.** The installer fetched a bare
  `axon-<rust-triple>` asset, but `release.yml` publishes a
  `axon-linux-x86_64.tar.gz` tarball, so installs 404'd against real releases.
  `install.sh` now downloads, checksum-verifies, and extracts the tarball.
## [5.4.1] - 2026-06-08

### Added

- **Chrome extension packaging.** `apps/chrome-extension/package.sh` (and the
  `just package-extension` target) builds a distributable
  `dist/axon-<version>.zip`. The `assets/` directory in the
  extension is a symlink into the repo's top-level `assets/`; the script copies
  only the referenced icons as real files (no symlinks, as required by the
  Chrome Web Store) and omits dev-only files. The version is read from
  `manifest.json` so the two cannot drift.
- **Chrome extension releases.** A `chrome-extension-release` workflow publishes
  a GitHub Release (zip + SHA256) on its own `chrome-ext-v*` tag, independent of
  the axon `v*` releases. The tag version must match `manifest.json` or the
  workflow fails; `workflow_dispatch` builds a dry-run artifact without releasing.

### Fixed

- **Chrome extension README.** Corrected the stale "Load unpacked" directory
  reference (`chrome-page-scraper-extension` → `apps/chrome-extension`) and
  documented the packaging workflow.

## [5.4.0] - 2026-06-08

### Changed

- **Local directory embed (`axon embed <dir>`) now recurses, filters, and
  AST-chunks code.** Previously a directory embed read only the top level
  (subdirectories were silently ignored), embedded every file regardless of
  type, and **failed the entire job** on the first non-UTF-8 file. The reader in
  `src/vector/ops/tei/prepare.rs` now walks the tree recursively, prunes
  VCS/dependency/build directories (`.git`, `node_modules`, `target`, `dist`,
  `.venv`, …), skips known-binary extensions, and skips (rather than aborts on)
  any file that fails to decode as UTF-8 or any subdirectory that can't be read
  (e.g. permissions) — only an unreadable top-level target is fatal. File
  selection lives in the shared `src/vector/ops/input/select.rs` predicates.
  Symlinked entries are now skipped (previously a symlink-to-file was followed
  and embedded); this also removes a symlink-escape vector on the directory path.
- **Code files are chunked with tree-sitter AST splitting.** Local source files
  with a supported grammar (Rust, Python, JS/JSX, TS/TSX, Go, Bash) route through
  `chunk_code` (run on a blocking thread) and are tagged `content_type = "text"`,
  matching the GitHub-ingest path. Markdown/docs keep the prose splitters
  (`chunk_markdown`, with the control-character `chunk_text` fallback) tagged
  `content_type = "markdown"`. Crawl-output directories are unaffected — their
  manifest URLs keep prose chunking.
- **Server-side embed validator reconciled with the reader.** The MCP/REST
  directory validator (`src/services/embed.rs`) now prunes the same junk
  directories and skips the same binary extensions the reader does, so it no
  longer rejects an embed for a dotfile or oversized blob buried in a directory
  the reader never reads. The server-only security sandbox (allowed roots,
  symlink/secret/size rejection) is unchanged and remains CLI-exempt.

### Fixed

- **Local directory embed no longer silently drops a file when tree-sitter
  chunking panics.** The `spawn_blocking` code-chunk path previously collapsed a
  `JoinError` to an empty chunk list with no log, dropping the document
  downstream without it counting as a failure (over-reporting success). It now
  logs the error before dropping, matching the other skip-on-error paths.
- **Manifest-skip misses are now attributable.** A `canonicalize` failure in the
  directory reader silently defeated the crawl-output `changed == false` skip and
  dropped the source URL / structured payload; it now logs a warning.

## [5.2.0] - 2026-06-08

### Added

- **Origin tracking (`seed_url` payload field).** Every indexed chunk now records
  a `seed_url` — the crawl start URL or ingest target that originated it, distinct
  from the chunk's own page `url`. Crawl chunks carry the crawl start URL; ingest
  chunks carry the re-ingestable target (e.g. `owner/repo`, `r/rust`); direct
  `embed`/`scrape` fall back to the doc's own URL. The field is an indexed keyword
  (faceted), and `PAYLOAD_SCHEMA_VERSION` bumped to `5`. Existing chunks indexed
  before this release carry no `seed_url` and are unaffected (default retrieval
  applies no version filter).
- **`axon refresh [FILTER]` command.** Re-enqueues crawl and ingest jobs for
  previously indexed origins by faceting the collection on `seed_url`. Classifies
  each origin (web URL → crawl, ingest target → ingest, sessions/non-URL →
  skipped), prints a plan, and confirms before enqueuing (respects `--yes` /
  non-TTY via `confirm_destructive`). Optional `FILTER` narrows by `source_type`
  (e.g. `github`) or a `seed_url` substring (e.g. a domain). Facet breadth is
  bounded by `AXON_REFRESH_FACET_LIMIT` (default 10,000). Only content indexed
  with `seed_url` participates — re-crawl/re-ingest once to backfill the marker.

## [5.1.2] - 2026-06-06

### Added

- The orange "live crawl" status dots (the TAILING indicator and the collapsed
  tray dot) now breathe with a subtle pulse while a crawl is active, so an
  in-flight crawl reads as alive at a glance. Flattened under
  `prefers-reduced-motion`.

## [5.1.1] - 2026-06-06

### Fixed

- Collapsed crawl tray now blends into the command bar as one cohesive panel:
  the command bar squares its bottom and drops its own shadow there while the
  tray carries the panel's outer shadow over a faint internal divider, the mode
  pill is cleared on minimize (clean default placeholder), and the tray chevron
  is muted instead of accented.

## [5.1.0] - 2026-06-06

### Added

- **Android direct Chat mode and shared desktop palette support.** Added direct LLM chat endpoints (`POST /v1/chat`, `POST /v1/chat/stream`) that bypass RAG retrieval and synthesis prompts, wired Android Ask/Chat mode switching, and exposed the same direct chat command in the Rust desktop palette and Tauri palette.
- **Split synthesis and chat model configuration.** Added `AXON_SYNTHESIS_OPENAI_MODEL`, `AXON_CHAT_OPENAI_MODEL`, `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL`, and `AXON_CHAT_HEADLESS_GEMINI_MODEL`, plus `[llm]` `config.toml` model fields. Legacy `AXON_OPENAI_MODEL` and `AXON_HEADLESS_GEMINI_MODEL` remain synthesis aliases.
- **Live crawl job view in the Tauri palette.** Running a `crawl` from the
  palette now opens a real-time job surface that polls `GET /v1/crawl/{id}`
  (and the handed-off embed job) ~once/sec and renders real backend data:
  FETCHED / QUEUED / DOCS·EMBEDDED / DEPTH stat cards, a progress bar with
  client-derived percent + ETA, a TAILING/stalled liveness indicator driven by
  the worker heartbeat, a tailing per-URL log, a rate-limit banner, and
  View-partial-result / Cancel-job actions. Also a collapsed minimal tray
  (`Crawling <host>` + progress) when minimized. Honest two-phase model: the
  third card shows docs-written during the crawl and embedded docs afterwards.
- **Crawl event stream in `result_json`.** The crawl progress persister now
  emits a bounded ring of per-page events (`events`: timestamp, URL, HTTP
  status, outbound link count), plus `queued`, `depth_max`, and `rate_limited`
  hosts — fed from the collector. `Website::with_return_page_links(true)` is
  enabled so the collector can compute a real discovered/queued backlog and
  per-page link counts. No new endpoint: the palette reads the richer
  `result_json` from the status poll it already performs.

### Changed

- **Android mock-alignment and operation surfaces continue moving to live production data.** The Android app now parses action and job output into human-readable UI surfaces, exposes expanded settings for `.env` and `config.toml`, and moves rail/sidebar data into dedicated app screens.
- The palette's HTTP calls route through a single `invoke` wrapper
  (`src/lib/invoke.ts`): the Tauri IPC bridge in production, or a same-origin
  relative fetch (vite proxy) in browser-dev — never an absolute cross-origin
  URL.

## [5.0.1] - 2026-06-04

### Fixed

- **Classified host-side cargo rustc wrapper environment knobs in the env matrix.** Added `AXON_RUSTC_WRAPPER_DELEGATE`, `AXON_RUSTC_WRAPPER_LOCAL_BIN`, `AXON_RUSTC_WRAPPER_NO_SCCACHE`, and `AXON_RUSTC_WRAPPER_PLUGIN_BIN` as script/test-only, non-runtime variables so the env/config boundary check stays current without treating wrapper controls as production configuration.

## [5.0.0] - 2026-06-03

### Changed

- **BREAKING: CLI and MCP actions always run in-process.** The CLI/MCP client-forwarding path has been removed entirely. `axon` no longer routes commands to a remote `axon serve` over HTTP — every command runs locally against Qdrant and TEI. The `axon serve` HTTP server (`/v1/*`, MCP-over-HTTP) is unchanged and remains the way to expose Axon for API access; deploy the container only when you need that.
  - Removed the `AXON_SERVER_URL` environment variable and the `--local` / `AXON_LOCAL_MODE` flag — execution is always local.
  - Removed the CLI server-mode client (`ServerClient`/`RestClient`), the command-routing layer, and the MCP "thin client" forwarder.
  - `axon setup` no longer scaffolds `AXON_SERVER_URL` into generated `.env` files.
  - `doctor` JSON output: the `mode` object no longer includes `client`, `server_url`, `route`, or `fallback`; it now reports only `local_runtime`.

### Removed

- **BREAKING: the `artifacts` MCP action is removed** (`head|grep|wc|read|list|delete|clean|search`). The artifact-first response mode is unchanged — large outputs still persist to `~/.axon/artifacts/<context>` and the `path` field points at them. Because the MCP server runs in-process, read those files directly from disk, or request `response_mode=inline`/`auto_inline` to get payloads in-band.

## [4.20.3] - 2026-06-03

### Fixed

- **`ask` timing line now shows `streamed=yes/no` and `ttft=Xms` in normal (non-diagnostics) mode.** `set_streamed` and `set_ttft` were no-ops when `AskTiming::Disabled` (the default path without `--ask-diagnostics`). Promoted `streamed` and `llm_ttft_ms` to the `Disabled` variant so they are always captured and emitted.

## [4.20.2] - 2026-06-03

### Fixed

- **`ask` streaming tokens now go to stdout (not stderr) with explicit `flush()` per token.** Previously tokens were `eprint!`-ed to stderr and the full answer was then re-printed on stdout — the answer appeared twice. Now the consumer does `stdout.write_all()` + `stdout.flush()` per token, and `print_ask_human` skips reprinting the answer when stream mode is active.
- **`summarize` and `research` streaming tokens moved from stderr to stdout** with the same `write_all` + `flush` pattern, making the output pipe-safe and visually progressive in all three streaming commands.

## [4.20.1] - 2026-06-03

### Fixed

- **`ask` streaming now works correctly by default.** `ask_stream=true` was the default but the CLI passed `None` for the event channel, so no tokens were streamed and `print_ask_human` skipped printing the answer entirely. Fixed: `run_in_process_ask` now sets up an mpsc channel + consumer task when `!json_output`, passes `Some(tx)` to the service, and `print_ask_human` always prints `result.answer` to stdout.
- **`summarize` now streams LLM tokens as they arrive.** Previously used `complete_text` (blocking). Now uses `complete_streaming` with a `SynthesisDelta` delta handler, matching the research streaming pattern. The CLI handler sets up a channel + consumer that forwards tokens to stderr while the full summary is always printed to stdout.

## [4.20.0] - 2026-06-02

### Added

- **`research` and `search` can use a self-hosted SearXNG instance instead of Tavily.** Set `AXON_SEARXNG_URL` (e.g. `https://searx.example.com`) and both commands query SearXNG's JSON API (`/search?format=json`); when unset they fall back to Tavily as before. SearXNG must have the `json` output format enabled in `settings.yml`. New `src/services/search/searxng.rs` client (with httpmock-backed tests for the success / `403`-when-json-disabled / blank-url-filter paths); requests go through the SSRF-guarded shared HTTP client.
- **`AXON_RESEARCH_FULL_CONTENT` toggle.** Defaults to `true` (full-page synthesis); set `false`/`0`/`no`/`off` to make `research` synthesize over search snippets only — much faster when you don't need deep sourcing.

### Changed

- **`research` now synthesizes over full page content, not just search snippets.** Previously the LLM only saw each result's short search excerpt (snippet-starved). It now fetches the top sources' full pages (HTTP render, vertical extractors disabled so Reddit/YouTube URLs yield raw text, per-URL failures tolerated and fall back to that source's snippet) and synthesizes over them, truncated per-source to a slice of the model-aware context budget. On Gemini, a representative query went from ~150-char snippets to multi-thousand-char full pages per source (e.g. the canonical plugins-reference page contributed ~55KB). Adds latency (one page fetch per source); bounded to the top `RESEARCH_FETCH_MAX_URLS` sources, fetched concurrently.
- **Review fixes (PR #158):** `searxng_search` now runs the parse-time `validate_url` SSRF guard (literal private IPs like `http://127.0.0.1` were not caught by the connect-time resolver); research full-content is keyed by the *input* URL (scrape-normalized return URLs no longer cause silent snippet fallback); research synthesis is capped to the top-N fetched sources and the per-source budget floor can no longer overflow the total budget; MCP `search`/`research` and the CLI `search` preflights accept `AXON_SEARXNG_URL` *or* `TAVILY_API_KEY` (SearXNG-only deployments no longer error); `AXON_RESEARCH_FULL_CONTENT` parsing reuses the shared `env_bool` helper. SearXNG search now walks `pageno` pages (with cross-page URL dedup, bounded by `MAX_SEARXNG_PAGES`) to satisfy `offset`+`limit`, instead of querying page 1 only. SSRF httpmock tests use a panic-safe RAII guard for the loopback allowance.
- **`summarize` context budget now scales with the model.** It already reused `ask_max_context_chars` but clamped it to a fixed 120k ceiling; that cap is removed so big-context models (Gemini/Claude 1M, Codex 400k) can summarize larger pages / more documents in one pass, with an 8k floor retained.

## [4.19.0] - 2026-06-02

### Changed

- **`ask` context quality: chunk-cap raised, mirror sources demoted, near-duplicate collapse.** Diagnosis via `ask --explain` showed good index coverage but a starved LLM context: only 6 chunks / ~7 KB of a 20 KB budget reached the model, and a low-rerank GitHub *mirror* of the canonical docs page was force-promoted to the `#1` full-doc slot by the dominant-host bonus (github.com "dominates" a mirror-heavy index). Three coordinated fixes:
  - **Chunk limit.** `AXON_ASK_CHUNK_LIMIT` default raised 20 → 24, clamp widened 3–40 → 3–64. Previously-dropped canonical chunks (`plugins-reference`, `plugin-hints`) now reach the LLM; context fills toward the budget instead of leaving ~64% empty.
  - **Mirror demotion in full-doc selection** (`src/vector/ops/commands/ask/context/build/selection.rs`). A VCS-mirror URL (generic `/blob/`, `/tree/`, `/raw/`, `*usercontent*` markers — no host/repo hardcoding) is excluded from host-dominance and never wins the canonical full-doc slot. For the motivating query the planned full-doc flips from a GitHub mirror to the canonical `code.claude.com/docs/en/plugins`.
  - **Near-duplicate collapse** (`src/vector/ops/commands/ask/context/dedup.rs`, new). Before context selection, reranked candidates are clustered by normalized shingle-Jaccard and collapsed to a single canonical representative (authoritative-domain > docs-path > not-a-mirror-blob > shallower-path > rerank), so identical mirror chunks can't each consume a slot. Conservative threshold (0.50): a defensive net for truly near-identical chunks — it does not collapse distinct sections of the same page. Drops are surfaced in `--explain` warnings.
  - No regression on the tracked retrieval fixture sweep (8/9 before and after; the one miss is a pre-existing docs.rs corpus gap).
- **`ask` retrieval depth now scales with the configured LLM's context window** (`tuning.rs`). A single model tier — derived from the backend/model (`gemini-headless` or a `gemini`/`gemma`/`claude` model → Large ≈1M tokens; `codex` → Medium ≈400k; anything else → Small, assume <50k) — drives four defaults so larger-window models receive proportionally more context:

  | knob | Large | Medium | Small |
  |------|------:|-------:|------:|
  | `ask_max_context_chars` | 1,000,000 | 400,000 | 40,000 |
  | `ask_chunk_limit` | 50 | 28 | 10 |
  | `ask_candidate_limit` | 250 | 150 | 60 |
  | `ask_hybrid_candidates` | 200 | 120 | 60 |

  All four remain overridable by their existing env/TOML knobs (explicit values still win). Effects on Gemini for a representative query: chunks injected 13 → 49, context ~14KB → ~93KB, and large authoritative full documents now fit (e.g. the 21KB canonical plugins page enters as a complete full-doc instead of a single supplemental chunk). Trade-off: a deeper candidate pool raises retrieval latency on heavy queries — dial the knobs down via env/TOML if needed. No fixture-sweep regression (8/9; top-domain coverage improved).
- **Full documents get context-budget priority over individual chunks** (`build.rs`). Planned full-docs are now inserted before top chunks, so an authoritative complete page survives budget pressure instead of being dropped (it previously had to fit in whatever budget the chunks left). Final context order is still re-sorted by score, so this changes only what is dropped when the budget is tight, not display order. A full-doc larger than the *entire* `ask_max_context_chars` budget intentionally falls back to chunk coverage (its top chunk re-enters via supplemental backfill) to preserve diversity across distinct authoritative pages. No fixture-sweep regression.

## [4.18.6] - 2026-06-02

### Changed

- **Plugin SessionStart hook no longer deploys — it is now probe-only.** `axon setup plugin-hook` previously ran preflight + `docker compose pull`/`up` on the down-path (and on any prerequisite-check failure). Deploying from a session-start hook was the wrong place for it; provisioning now belongs solely to the `/axon-deploy` slash command. The hook now only probes `/readyz`:
  - **up** → exit silently (success), as before;
  - **down** → print a one-line advisory `axon stack not reachable on /readyz — run /axon-deploy to start it` and exit success (non-blocking). It never runs preflight or compose.

  This removes the preflight/compose/report machinery from the hook path (`build_plugin_hook_report`, `PluginHookReport`, exit-policy classification, the 360s timeout wrapper, and the `--no-setup` flag's effect). `axon setup`, `axon preflight`, `axon compose …`, and `axon smoke` are unchanged and remain the explicit ways to provision/inspect the stack.

## [4.18.5] - 2026-06-01

### Fixed

- **Plugin-hook `/readyz` probe respects the configured MCP HTTP bind.** The 4.18.4 fast-path probed a hardcoded `127.0.0.1:8001`; it now resolves the host/port from `AXON_MCP_HTTP_HOST` / `AXON_MCP_HTTP_PORT` in the environment (populated from `~/.axon/.env` at startup, the same knobs `setup init` writes), so a non-default port is probed correctly instead of always falling through to a redeploy. The env is read directly rather than via `cfg.mcp_http_port`, because the config layer gates those host/trusted-bootstrap keys for the `setup plugin-hook` command (leaving `cfg.mcp_http_port` at its `8001` default) — reading the env matches how the `setup`/`preflight` readiness check already resolves the axon URL. Bind-all hosts (`0.0.0.0`/`::`) are probed over loopback and IPv6 literals are bracketed. Also de-hardcodes the same `:8001` assumption in the readiness check. URL building is unit-tested via `axon_readyz_url`.

## [4.18.4] - 2026-06-01

### Fixed

- **Plugin SessionStart hook no longer redeploys an already-healthy stack.** `axon setup plugin-hook` now probes `/readyz` (single-shot, 3s) before doing anything: if the axon server answers (which itself asserts qdrant + tei are ready), the hook short-circuits to success and stays **silent** in human mode — no preflight, no `compose pull`/`up`. Previously any failed preflight prerequisite check (e.g. a missing `nvidia-smi`) set `needs_setup = true` and forced a full compose redeploy on every session start, producing spurious `compose-pull`/`compose-up` blocking failures on hosts where the stack was already up. The auto-deploy-when-down behavior is preserved: an unreachable `/readyz` still falls through to the normal preflight + setup path.

### Added

- **`/axon-deploy` slash command.** Explicit on-demand deploy/restart/rebuild of the axon stack (`axon compose up|restart|rebuild` + `axon doctor`), as the manual counterpart to the now-silent session-start hook. Registered via `"commands": "./commands/"` in the plugin manifest.

## [4.18.3] - 2026-06-01

### Fixed

- **Qdrant cold-read `ask` timeouts.** Bake `quantization.scalar.always_ram = true` + `hnsw_config.on_disk = false` into the `ensure_collection` create body (raw vectors stay `on_disk`). New/recreated collections now pin the int8 quantized vectors and HNSW graph in RAM instead of leaning on the OS page cache. Previously the `always_ram = false` default let cache eviction (e.g. a concurrent crawl) turn vector searches into 20–30s cold disk reads, tripping the 30s `internal_service_http_client` timeout and hard-failing `ask`. Warm searches drop from seconds to ~40–60ms.

## [4.18.2] - 2026-06-01

### Changed

- **Documentation: comprehensive accuracy refresh + aggressive restructure.**
  - Verified all ~110 living/reference docs against current source. Fixed the default
    collection (`cortex` → `axon`) across command/config/ingest/MCP docs, corrected MCP
    action tables and payload schema version (v4), the watch auto-fire scheduler, security
    port-binding claims, the spider `firewall` flag (NOT enabled), and stale `axon_rust`
    naming / removed-AMQP/lite-mode references.
  - Added 6 missing command references: `diff`, `brand`, `config`, `train`, `monitor`, `sync`.
  - Restructured `docs/` into intent-based sections — `guides/`, `reference/`, `architecture/`,
    `operations/`, `contributing/` — with filenames normalized to lowercase-kebab. Dated
    historical records (sessions, reports, plans, archive, superpowers, perf snapshots) left in
    place. All moves via `git mv` (history preserved); every internal link and code/CI/script
    reference updated; link-checked clean. Rewrote `docs/README.md` and `docs/CLAUDE.md` as
    navigation hubs. Audit trail under `docs/reports/2026-06-01-stale-docs-refresh/`.

## [4.18.1] - 2026-06-01

### Added

- `axon setup install`: copies the running axon binary into `~/.local/bin/axon` so it is callable as a bare command in your own terminal, independent of Claude Code. The plugin SessionStart hook self-installs each session (survives `/plugin update`). The plugin now bundles the release binary at `plugins/axon/bin/axon` via Git LFS.

### Changed

- `ask.retrieve` tracing span no longer records the `timing` argument as a field.

### Removed

- Dropped the committed root `bin/axon`; the plugin ships its binary at `plugins/axon/bin/axon`.
## [4.18.0] - 2026-05-31

### Added

- URL change-detection watch: the `watch` task type (replacing the stateless `refresh` task) now detects content changes per URL each scheduler tick and crawls only the changed subtrees. Per URL: a cheap conditional ETag/Last-Modified probe (304 short-circuits), then scrape → normalize + `ignore_patterns` noise filter → SHA-256 fast-equal skip → reuse `services::diff::compute_diff` against the stored snapshot → a meaningfulness threshold (`change_threshold_words`, link changes always count). Meaningful changes get a best-effort Gemini AI summary and a `url-change` artifact (unified diff + summary + link/word deltas), are clustered by common path prefix, and one depth-bounded crawl is enqueued per cluster (skipping clusters whose prior crawl is still in flight). New `axon_watch_url_state` table (migration `0007`) holds the latest snapshot + validators. `task_payload` gains `max_depth`, `ignore_patterns`, `change_threshold_words`, and `summarize`; payloads are validated at create time (non-empty `urls`, compilable `ignore_patterns`) across CLI and both HTTP create paths.

### Changed

- `SUPPORTED_TASK_TYPES` is now `["watch"]`; the `refresh` task type is removed.

## [4.18.0] - 2026-06-01

### Added

- **Conditional re-crawl (ETag / If-Modified-Since)** — opt-in via `--etag-conditional` (bead axon_rust-hiyf). Re-crawls seed spider 2.51's per-`Website` ETag cache from a persisted `etag.json` sidecar so unchanged pages return `304 Not Modified`. Because spider drops 304 responses from its broadcast stream entirely, the crawl engine **reconciles** those silent skips back into the manifest: every URL that was seeded with validators and did not arrive this run is re-emitted as a reused (`changed=false`) entry, relinked from the `markdown.old` recycling bin. The reconciliation set is `{seeded ∩ previous_manifest − arrived ∩ visited}`, gated on spider's visited set (`Website::get_links()`): a genuine 304 skip is recorded in `links_visited` because spider's 304 short-circuit runs inside the per-URL fetch task, whereas a page that is no longer discovered is never scheduled and is therefore excluded — so the feature cannot resurrect deleted pages as zombie content. Validators are carried forward across runs (spider's 304 path returns before re-storing, so the previous sidecar is overlaid with freshly-stored validators). Independent of `--cache`. Wired on the crawl path only; single-page `scrape` is intentionally excluded (a 304 there would drop the page the user asked for, with no reconciliation seam).
- **Per-path crawl budgets** — repeatable `--budget PATH=N` flag (bead axon_rust-37zv) wired to spider's `Website::with_budget`, capping pages crawled under each path prefix (`*` = all paths). Unset = current behavior.

### Changed

- **Dead-host retry classification** (bead axon_rust-6i30) — sitemap fetch retries now exclude spider 2.51's permanent synthetic status codes `525` (DNS/NXDOMAIN) and `526` (host/TLS unreachable) so the retry budget is not burned re-resolving hosts that will never resolve. Transient `52x` (connection refused / timeout) and genuine upstream `5xx` remain retryable.
- **Media-asset link filtering** (bead axon_rust-mk95) — discovered links are now dropped via spider's `is_media_asset_url` classifier in the `set_on_link_find` guard chain, so images, fonts, audio, video, archives, and PDFs are never queued, fetched, or embedded. HTML/extensionless doc routes pass through unaffected.
- Enabled the `etag_cache` spider feature.

### Security

- **SSRF redirect guard — confirmed, no change** (bead axon_rust-4rdf). spider 2.51 adds an always-on `is_ssrf_redirect` guard that refuses redirect hops to loopback/private/link-local addresses under the default redirect policy. This complements — and does **not** replace — axon's existing three-layer SSRF defense (`validate_url` parse-time check + `SsrfBlockingResolver` connect-time DNS guard + the spider blacklist patterns). No axon security code was removed; demonstrated equivalence was not established, so both layers are kept.

### Notes

- **Adaptive concurrency (AIMD)** (bead axon_rust-6y3c) was investigated and **deferred**: spider 2.51 ships an `AIMDController` type but exposes **no public `Website`/`configuration` method to attach it** (the type is referenced nowhere else in the crate). The only wireable adaptive primitive is `auto_throttle`, which is latency-based per-domain *delay* — not the 429/403 status-driven concurrency backoff the bead specified, and enabling it by default would throttle the high-concurrency performance profiles. Documented as not-wireable per the bead's own fallback clause; bead left open with a deferral note.

## [4.17.0] - 2026-06-01

### Added

- `llms.txt` probing: crawl and `map` now fetch `/llms.txt` at the site root, parse its markdown links, and union them (dedup, no blanket truncation) into the sitemap-backfill candidate set (config: `scrape.discover-llms-txt`, default on; `scrape.max-llms-txt-urls`, default 512 — bounds the llms.txt fan-out only; sitemap-URL backfill stays uncapped as on prior releases). Raw `.md`/`.markdown`/`.txt` targets pass through without the HTML transform. `fetch_text_with_retry` caps the `/llms.txt` discovery document at 512 KB and `sitemap.xml` at 50 MB (the sitemap-spec ceiling); HTML page backfill stays uncapped and charset-aware. Request surfaces (MCP `crawl`, REST `/v1/crawl`, server-mode plan, action API) accept `discover_llms_txt` / `max_llms_txt_urls` overrides.

## [4.16.0] - 2026-05-31

### Added

- `--probe-rpc-subdomains` flag and MCP candidate synthesis: with `--probe-rpc`, axon now probes well-known MCP paths (`/mcp`, `/api/mcp`) on the target host, and (with `--probe-rpc-subdomains`) on the derived `mcp.<registrable-apex>` host. Confirmed servers appear in `endpoints` as `synthesized_mcp`; every attempt is recorded in a new `mcp_candidates` report field. The initial page fetch is now non-fatal under `--probe-rpc`, so bare MCP endpoints (which serve no HTML) can be probed directly. Exposed across CLI, MCP (`endpoints` action), and the web `/v1/endpoints` API — `probe_rpc` is now settable over MCP/HTTP too.

### Changed

- Version sync: brought `apps/web` (package.json + lockfile) up from a stale
  4.14.1 to match the crate, and regenerated `apps/web/openapi/axon.json` so its
  embedded `info.version` matches. (Folds in the superseded v4.15.2 sync PR.)

## [4.15.1] - 2026-05-31

### Changed

- Finished the plugin split: removed the leftover `plugins/axon-mcp/` manifest,
  `.mcp.json`, and monitors, and moved monitors + `.mcp.json` under `plugins/axon/`.

### Docs

- Added the llms.txt-probe implementation plan
  (`docs/superpowers/plans/2026-05-31-llms-txt-probe.md`) — a 10-task TDD plan to
  probe `/llms.txt` during crawl and `map` and merge its links into sitemap backfill
  (tracked as epic `axon_rust-6s51`). Plan only; no implementation yet.

## [4.15.0] - 2026-05-31

### Added

- **Watch scheduler now auto-fires recurring watches.** A new in-process
  scheduler loop (`src/jobs/workers/watch_scheduler.rs`) is spawned by
  `spawn_workers` (so it runs under `axon serve` / `axon mcp`). Each tick
  atomically leases every enabled watch whose `next_run_at` has passed via the
  new `lease_due_watches` (`UPDATE ... RETURNING`), runs it through
  `run_watch_now_with_pool` (which records a run, advances `next_run_at` by
  `every_seconds`, and clears the lease), and is crash-safe through the existing
  `reclaim_stale_watch_leases` sweep. Previously watches only ran via manual
  `watch run-now` or the HTTP `/v1/watch/{id}/run` endpoint.
  - Tuning: `AXON_WATCH_TICK_SECS` (sweep interval, default 15, min 1) and
    `AXON_WATCH_LEASE_SECS` (lease TTL, default 300, min 1).

### Fixed

- **`axon watch create` now validates `task_type` at create time.** The CLI
  previously accepted unsupported or whitespace-padded task types, persisting
  watches that could never run. Validation is now centralized in
  `jobs::watch::validate_task_type` and shared by the CLI and both HTTP create
  handlers (removing two duplicated `SUPPORTED_TASK_TYPES` constants).
- **`run_watch_now_with_pool` guards the COMPLETED finalize write.** The write
  previously short-circuited the function via `?` on failure, leaving the run row
  stuck in `running` (nothing reclaims stale `axon_watch_runs` rows). Task
  execution and finalization are now separated, and a failed COMPLETED write
  falls back to a best-effort FAILED finalize so a row-level write failure no
  longer wedges the run.

## [4.14.1] - 2026-05-29

### Changed

- **CI: `windows-build` now cross-compiles `axon.exe` from Linux instead of
  building natively on a GitHub-hosted Windows VM.** The job runs on
  `ubuntu-latest` using the `mingw-w64` (`x86_64-pc-windows-gnu`) toolchain.
  A prior `cargo-zigbuild` attempt was blocked because zig does not bundle the
  Windows platform headers (`sched.h` / `windows.h`) that `aws-lc-sys`'s
  jitterentropy code needs; `mingw-w64` ships those headers, so `aws-lc-sys`
  and `ring` cross-build cleanly. The resulting `axon.exe` was smoke-tested on
  Windows 11 (runs, full CLI, native `C:\` path resolution). Faster than a cold
  Windows VM and avoids 2x Windows-minute billing. The `cfg(windows)`
  `AXON_DATA_DIR` path-regression test is no longer executed in CI (it would
  require running the windows-gnu test binary under wine); it remains in
  `src/core/paths_tests.rs` and was confirmed passing on Windows 11 against the
  cross-compiled binary.

## [4.14.0] - 2026-05-29

### Changed

- **BREAKING:** Renamed the `axon stack` command to `axon compose`
  (`axon compose up|down|restart|rebuild`). There is no backward-compatible
  alias — `axon stack` is no longer recognized. Internal identifiers
  (`CommandKind::Stack` → `Compose`, `StackAction` → `ComposeAction`,
  `StackArgs`/`StackSubcommand` → `Compose*`) and setup phase keys
  (`stack-up` → `compose-up`, etc.) were renamed to match.

### Fixed

- **Ingest failures now report their real cause instead of a useless label.**
  The ingest error wrappers formatted the underlying error with `{e}` (plain
  `Display`), which on an `anyhow::Error` prints only the outermost context and
  discards the `.source()` chain. A failed GitHub ingest therefore surfaced as
  `github ingest failed for <repo>: GitHub` (octocrab's top-level `Display`),
  hiding the actual `404 Not Found` / rate-limit / auth detail. Switched all six
  provider wrappers (github, gitlab, gitea, generic git, reddit, youtube) to the
  alternate formatter `{e:#}`, which walks the full error chain — e.g.
  `github ingest failed for rust-lang/serde: GitHub: Not Found`.

## [4.13.2] - 2026-05-29

### Fixed

- **`palette-tauri` CI job no longer fails at `cargo check`** — the job checked out with `sparse-checkout-cone-mode: false` and a pattern (`apps/palette-tauri`) that excluded the tracked root `.gitignore` from the working tree. Combined with `actions/checkout`'s blobless partial clone, cargo's gix file-walker could not build its excludes stack while fingerprinting the Tauri build script (`failed to determine package fingerprint for build script … Failed to update the excludes stack to see if a path is excluded`), so the job had never passed since it was added. Switched the job to cone-mode sparse checkout (matching the green `check`/`clippy` jobs), which always includes root-level files.

## [4.13.1] - 2026-05-29

### Fixed

- **`--probe-rpc` concurrency now actually honors `AXON_ENDPOINT_PROBE_CONCURRENCY`** — the probe semaphore was acquired once per discovery session and held for its whole duration, so the env var limited *concurrent sessions*, not per-endpoint fan-out (which was a hardcoded 4). The permit is now acquired per-endpoint inside the `buffer_unordered` stream (mirroring the bundle-fetch pattern), and the stream width is driven by the same cap, so the env var governs global in-flight probes as documented.
- **Probe timeout no longer inflated by the performance profile** — `probe_rpc_endpoints` routed through the shared `timeout_secs` helper, so a configured `request_timeout_ms` (e.g. 20s on `high-stable`) overrode the advertised 3s probe budget across up to five sequential requests per endpoint. The timeout is now clamped to a hard 3s ceiling (a lower configured value can still shorten it).
- **MCP tool enumeration works against stateful Streamable-HTTP servers** — `probe_mcp` now captures the `Mcp-Session-Id` returned by `initialize`, sends the required `notifications/initialized`, and replays the session id on `tools/list`. Previously the `tools/list` POST carried no session context and was rejected by spec-compliant servers, yielding empty tool lists.
- **MCP-over-SSE servers are now fully parsed** — POST probes send `Accept: application/json, text/event-stream`, and `text/event-stream` responses are parsed incrementally (first complete `data:` frame), so Streamable-HTTP servers that reply over SSE surface their `serverInfo` and tools instead of degrading to a bare `transport: sse` label. The incremental reader stops at the first frame so a kept-open stream cannot stall the probe.
- **Probe response bodies are now byte-bounded** (256 KiB cap) instead of an unbounded `resp.text()`, closing a memory/DoS surface against hostile endpoints.
- Removed the dead `error` field from `RpcProbeResult` (documented as "error message if probing failed" but never populated). `protocol` and `transport` are now typed enums (`RpcProtocol`, `RpcTransport`) instead of free-form strings; wire values are unchanged.
- Bumped the MCP `initialize` `protocolVersion` to `2025-06-18`.
- Added unit-test coverage for the probe ladder (MCP + session-id replay, MCP-over-SSE, OpenRPC, `system.listMethods`, `-32601` fingerprint, SSE transport, and a non-RPC negative case).
- **Version sync** — `apps/web/package.json` was left at `4.12.3` (the `version_bearing_files_stay_in_sync` contract test caught the drift against `Cargo.toml`); bumped to `4.13.1`.
- **Env/config boundary matrix** — classified the previously-unlisted `AXON_ENDPOINT_PROBE_CONCURRENCY`, `AXON_LLM_BACKEND`, and `AXON_OPENAI_{BASE_URL,MODEL,API_KEY}` keys (the `env_config_boundary` checker was failing); the `OPENAI_COMPAT_SECRET` redaction-test fixture is now ignored as a non-env token.
- **Build determinism** — added a repo-local `.cargo/config.toml` `[build] rustc-wrapper = ""` to disable the global sccache wrapper for this repo; its shared-across-worktrees cache was serving stale sibling-worktree artifacts and producing phantom compile errors on cache miss.

## [4.13.0] - 2026-05-28

### Added

- **`--probe-rpc` flag on `axon endpoints`** — opt-in JSON-RPC 2.0 / MCP / ACP protocol fingerprinting for discovered endpoints. Probes each candidate with: MCP `initialize` + `tools/list` handshake, OpenRPC `rpc.discover`, `system.listMethods` introspection, JSON-RPC -32601 error-code fingerprinting, and SSE transport detection. Results surface as `rpc_probe` on each `DiscoveredEndpoint` in both CLI and REST output. Concurrency controlled via `AXON_ENDPOINT_PROBE_CONCURRENCY` (default: 4) with a 3-second per-probe timeout.

## [4.12.4] - 2026-05-28

### Fixed

- **CDP Page.navigate deadlock** — `Fetch.enable` with `requestStage: Request` intercepts the navigation request before dispatch; the inline navigate wait loop now replies to `Fetch.requestPaused` events so Chrome never stalls, fixing the `timeout waiting for Chrome response to Page.navigate` error in `--capture-network` mode.
- **Endpoint noise filtering** — `is_noise_value` now rejects `w3.org`, `json-schema.org`, `schema.org`, `example.com/org/net` hosts and 20 static asset extensions (`.js`, `.css`, `.png`, `.woff2`, `.pdf`, `.map`, etc.).
- **Minifier-garbage host rejection** — `is_valid_absolute_host` now rejects single-label domains, single-char TLDs, and empty hosts that flood results from bundled JS.
- **Registrable-domain first-party detection** — `host_is_first_party` now correctly treats `api.example.co.uk` as first-party to `www.example.co.uk` via registrable-domain comparison covering 20 multi-label TLDs (`.co.uk`, `.com.au`, etc.).

## [4.12.3] - 2026-05-28

### Added

- **Android redesign specs** — added `docs/architecture/specs/android-redesign.md` and
  implementation plans for Android phase 3 completion and the full rail redesign.

### Changed

- style(palette): Crystalline visual design — darker near-black surfaces, cyan accent system replacing rose, ghost chip mode pill with × dismiss

## [4.12.2] - 2026-05-27

### Added

- **build-windows.sh** — new cross-compile script that builds the Palette Tauri
  or `axon.exe` Windows executable on dookie and ships it to Steamy's Desktop
  via `scp`, replacing the old `build-on-steamy.sh` repo-sync approach.

### Fixed

- **Android: 22 code-review findings from PR #142** — CRLF injection in
  `joinHeader`, `askStream` skipping `ModeOptionsApplicator`, `security-crypto`
  downgraded to stable, URI scheme validation in four screens, `EncryptedTokenStore`
  race condition, backup exclusion rules, `JobsViewModel` backoff, connection poll
  flow termination, `DocumentViewModel` concurrent fetch cancellation,
  `RecentJobsRepository` Set-of-JSON dedup bug, `HeadersField` delete index
  shifting, `ModeOptionsRepository` dependency inversion (`*FormKeys` moved to
  `data.repository.options`), `ConnectionState` name collision renamed to
  `TestConnectionState`, `QueryRequest.limit` caller-wins precedence, answer text
  capped at 500 chars, `OperationMode` ProGuard safety, `data object` sealed
  states.

## [4.12.1] - 2026-05-27

### Fixed

- **Android: job cancellation races** — `SystemViewModel`, `SearchWebViewModel`,
  `SummarizeViewModel`, and `QueryViewModel` now cancel in-flight coroutine jobs
  before launching new ones, preventing stale results from overwriting fresh state.
- **Android: `JobsViewModel` tab-switch flicker** — emit `Resource.Loading`
  immediately at the start of each `flatMapLatest` block so the UI transitions
  to a loading state before the first poll result arrives.
- **Android: lazy `ToolsViewModel` creation** — moved `viewModel()` call inside
  the `Scrape/Crawl/Map/Research` branches of `ModeContentHost` so the VM is not
  eagerly constructed for `Ask/Query/Summarize/Search/Ingest` modes.
- **Android: form reset not reflected in UI** — `ModeOptionsRepository.resetKeys()`
  now increments a `resetVersion` counter; `rememberPersistedState` and
  `rememberOptionalPersistedState` re-read DataStore when the counter changes,
  restoring default values in the UI after a reset.
- **Android: `MapOptionsForm` allows negative values** — `limit` and `offset`
  fields now clamp input to `coerceAtLeast(0)`.
- **Android: URL-encoded job IDs in HTTP paths** — `AxonClient.crawlStatus`,
  `getJob`, and `cancelJob` now percent-encode the job ID before embedding it
  in the request URL path.
- **Android: `AxonRepository.retrieve` accepts invalid `tokenBudget`** — added
  `require(tokenBudget > 0)` guard before the network call.
- **Android: `IngestViewModel` persistence failure masks success** — wrapped
  `recentJobs.add()` in `runCatching` so a DataStore I/O failure during
  persistence does not prevent the `Submitted` state from being set.
- **Android: token migration failure silently swallowed** — `AxonApp.onCreate`
  now logs a warning when the `migrateTokenToEncrypted` migration fails.
- **Android: `SummarizeScreen` submits blank input** — added `trim().isNotEmpty()`
  guard in `onSend` so whitespace-only strings never reach the ViewModel.
- **Android: `IngestScreen` submit-enabled for whitespace** — changed `isNotBlank()`
  to `trim().isNotEmpty()` to match the actual trimmed payload being submitted.
- **Android: `DocumentViewModel` logs PII URLs** — removed the URL from the
  `Log.w` call in the error path to avoid leaking document URLs in logcat.
- **Android: `StringChunking` drops separator at chunk boundary** — separator
  is now appended to the *outgoing* buffer before `flush()`, preserving it in
  the emitted chunk and enabling lossless reassembly via `joinToString("")`.
- **Android tests: missing HTTP method/path assertions** — added `assertEquals`
  calls for method and path in the `doctor`, `suggest`, and `domains` test cases
  of `AxonClientPhase2Test`.
- **Android tests: missing reassembly assertion** — paragraph and line split
  paths in `DocumentChunkingTest` now verify `chunks.joinToString("") == original`.
- **Docs: session log filename missing time component** — renamed
  `2026-05-27-android-pager-fab-shell.md` to
  `2026-05-27-16-02-android-pager-fab-shell.md`.

## [4.12.0] - 2026-05-27

### Added

- **Android: pager + FAB shell** — bottom-pager navigation and draggable FAB shell
  wired into the main nav graph, enabling swipe-between-tabs UX alongside the
  existing destination-based routing.
- **Android: complete operation mode coverage** — `Map`, `Research`, `SearchWeb`,
  and `Summarize` modes added to `OperationMode`; matching option forms
  (`MapOptionsForm`, `ResearchOptionsForm`, `SearchWebOptionsForm`,
  `SummarizeOptionsForm`) and nav graph destinations registered.
- **Android: `options` form-keys package** — all per-mode form key constants
  extracted from the repository layer into
  `data/repository/options/{Ask,Crawl,Ingest,Map,Query,Research,Scrape,SearchWeb,Summarize}FormKeys.kt`,
  giving UI forms a stable, typed constant surface.

### Changed

- Android: options forms (`AskOptionsForm`, `CrawlOptionsForm`, `IngestOptionsForm`,
  `QueryOptionsForm`, `ScrapeOptionsForm`, and the new forms) simplified — form key
  references now come from the dedicated keys package rather than inline strings.
- Android: `AxonRepository`, `ModeOptionsRepository`, `RecentJobsRepository`, and
  `EncryptedTokenStore` updated to use the new form-keys constants.
- Android: `DocumentScreen`, `SourcesScreen`, `MapTab`, `ResearchTab` wired into
  the expanded nav graph.
- Android: `libs.versions.toml` dependency updates for new form/nav requirements.
- `scripts/build-on-steamy.sh`: minor fixes to sync paths.

## [4.11.0] - 2026-05-27

### Security

- **Android: encrypt user-supplied HTTP headers** — header values entered in the
  Crawl options form (Authorization, Cookie, X-Api-Key, Proxy-Authorization,
  X-Auth-Token) previously persisted to the plaintext mode-options DataStore.
  New `EncryptedHeadersStore` (AES-256-GCM via AndroidX security-crypto) stores
  the entire crawl header list encrypted at rest; the legacy
  `CrawlFormKeys.HEADERS` DataStore key is removed.
- Android: `data_extraction_rules.xml` now excludes the encrypted headers prefs
  file plus the `settings` and `mode_options` DataStore protobufs from both
  cloud backup and Android 12+ device-to-device transfer.

### Fixed

- **Android: Ask mode no longer hangs and force-closes after repeated taps.**
  Root cause was a leaked OkHttp IO thread in `AxonClient.askStream`:
  `BufferedReader.readLine()` blocked for up to 300s after coroutine
  cancellation. Fix captures the `Call` reference and installs
  `invokeOnCompletion { call.cancel() }`, and `AskViewModel` now tracks the
  in-flight `Job` so a second `ask()` cancels the prior stream cleanly.
- Android: `DraggableFab` could be dragged off-screen / into the system nav bar.
  `clampOffset` now caps `maxX`/`maxY` to `0f` so the FAB cannot escape its
  visible anchor.
- Android: `JobsViewModel.cancel()` and `IngestViewModel.cancel()` discarded the
  `Result` — failed cancels were invisible to the user. Both now surface
  errors (toast in Jobs, error state in Ingest) and log to logcat.
- Android: `AskViewModel` fallback for truncated SSE streams now sets a
  `historyWarning` flag instead of silently presenting partial bytes as a
  completed answer.
- Android: `EncryptedTokenStore` now logs every keystore failure path
  (`Log.w`), surfaces commit failures via boolean returns, and `SettingsRepository.save()`
  throws `IllegalStateException` if encrypted persistence fails so the UI is
  never left in a "looks saved but isn't" state.
- Android: token migration (`SettingsRepository.migrateTokenToEncrypted`) now
  uses `try { write } finally { remove }` and refuses to remove the plaintext
  copy when the encrypted write returns false — bounded the
  plaintext-exposure window if a process is killed mid-migration.
- Android: `AxonClient.execute` and `askStream` now log network failures
  (one-line `Log.w` with method + path) so field reports have a logcat
  breadcrumb.

### Changed

- Android: `AxonRepository`'s `applicator` constructor parameter is now
  **required** — the silent `NoopModeOptionsApplicator` default removed a
  class of test-fixture bugs where forgotten injection silently bypassed
  user preferences.
- Android: extracted `ConnectionStatusEngine` from `ConnectionStatusViewModel`
  so the polling / refresh / cancellation contract is unit-testable under
  `runTest` (5 new tests).
- Android: extracted `HeadersReducer` pure-function reducer from
  `HeadersField`; row-add/delete/set-key/set-value/serialize logic is now
  unit-tested (10 new tests).
- Android: `KnowledgeViewModel` collapses the 4 near-identical `loadX()`
  bodies into one generic `loadSection<T>(state, cachedAt, force, label, fetch)`
  helper — 60+ LOC saved, identical behaviour.

### Added

- Android: `AxonClientErrorPathTest` — 401/403/404/500/503 + malformed JSON +
  empty body + socket disconnect + abort coverage for phase-2 endpoints (15
  new tests).
- Android: `UrlValidatorTest` — 9 new tests covering `hostOrNull`,
  ipv4-literal, userinfo-stripping, and the `github.com.attacker.com`
  lookalike regression.

## [4.10.0] - 2026-05-27

### Added

- Android: per-mode options screen (9 forms — Ask, Query, Summarize, Research,
  Scrape, Crawl, Search, Map, Ingest). Each form persists overrides to
  Preferences DataStore (one file `mode_options`) with file-private keys and
  defaults. `Reset to defaults` clears only that mode's keys.
- Android: `ModeOptionsApplicator` decorator boundary on `AxonRepository`.
  Every request flowing through the repository is run through `applicator.apply(req)`
  before reaching `AxonClient` so the repository stays ignorant of which fields
  exist per mode. Call-site values always win over persisted overrides.
- Android: `EncryptedTokenStore` (EncryptedSharedPreferences with `@Volatile`
  cache) replaces the legacy plaintext DataStore entry. Tolerates AndroidKeyStore
  invalidation by deleting the prefs file and forcing re-auth. Synchronous
  `.commit()` writes survive immediate process kill.
- Android: idempotent boot-time migration moves any legacy plaintext token
  into `EncryptedTokenStore` and wipes the plaintext entry. Runs on every
  `AxonApp.onCreate()`.
- Android: `data_extraction_rules.xml` excludes `axon_secrets.xml` from cloud
  backup and device-transfer (Android 12+).
- Android: `ModeOptionsScreen` applies `FLAG_SECURE` to its host window for the
  lifetime of the composition so sensitive header values cannot bleed via the
  recent-apps thumbnail.
- Android: `HeadersField` (used by `CrawlOptionsForm`) masks sensitive
  Authorization/Cookie/X-Api-Key/Proxy-Authorization/X-Auth-Token values with
  `PasswordVisualTransformation` plus a show/hide toggle.
- Android: new `LocalOpenModeOptions: (OperationMode) -> Unit` composition
  local provided by `AxonNavGraph`; `OperationsScreen` cog now navigates to
  the real options screen.

### Changed

- Android: wire DTOs (`CrawlRequest`, `ScrapeRequest`, `AskRequest`,
  `ResearchRequest`, `MapRequest`, `QueryRequest`) extended to mirror the
  matching `RestXxxRequest` fields the server already exposes so the
  applicator has fields to merge into.

### Removed

- Android: `StubModeForm` deleted — all 9 modes now route to real screens.

## [4.9.0] - 2026-05-27

### Added

- Android: `HorizontalPager`-based shell with four swipe pages (Operations,
  Jobs, Knowledge, System); bottom navigation bar removed; top app bar carries
  the settings gear on every page.
- Android: FAB on the Operations page opens an Aurora-styled mode picker with
  nine operations (Ask, Summarize, Research, Query, Scrape, Crawl, Ingest,
  Search, Map); the active mode persists in `OperationsViewModel`.
- Android: Tapping a Query result now opens an in-app `DocumentScreen` that
  renders the full assembled document via `/v1/retrieve` (chunk count, matched
  URL, truncated/warning callouts), with an explicit "Open source URL" escape
  hatch.
- Android client: `retrieve(...)` wired through `AxonClient` and
  `AxonRepository` with the long-timeout HTTP client.
- Android nav: `LocalAxonNavController` `CompositionLocal` so deep children
  (e.g. result cards) can navigate without prop-drilling the controller.

### Changed

- Android: Renamed `ui/search/` → `ui/query/` (the screen always called
  `/v1/query`); the "Search" mode label is reserved for the future real
  web-search wiring (`/v1/search`).

## [4.8.2] - 2026-05-26

### Changed

- Palette CI now runs Tauri crate tests in addition to cargo check, and compose
  smoke validates the Steamy build safety helper.

### Fixed

- Palette startup now falls back to default settings if persisted settings
  cannot be read.
- Palette GitHub ingest requests use the REST `repo` field expected by the
  server.
- Job config snapshots now reject invalid serialized LLM backend values instead
  of silently falling back to the worker default.

## [4.8.1] - 2026-05-26

### Changed

- Tauri palette command entry now has clearer mode state, clearable input, hover
  selection, and run-state badges for output results.
- New Qdrant collections now use scalar int8 quantization with
  `always_ram = false` to reduce memory pressure while keeping the existing
  quantile and HNSW defaults.

### Fixed

- Tauri palette action lists now show an explicit empty state and better action
  hints when no command matches.

## [4.8.0] - 2026-05-26

### Added

- OpenAI-compatible LLM backend support for llama.cpp-style endpoints via
  `AXON_LLM_BACKEND=openai-compat`, `AXON_OPENAI_BASE_URL`,
  `AXON_OPENAI_API_KEY`, and `AXON_OPENAI_MODEL`.
- Tauri palette deployment helper `scripts/build-on-steamy.sh` for building a
  Windows executable and placing it on Steamy's desktop.
- Job progress presentation helpers and additional ask/query logging around
  retrieval, context assembly, and LLM synthesis.

### Changed

- Tauri palette output formatting now unwraps REST payloads for search/research
  results and renders human-readable summaries instead of raw JSON blobs.
- Tauri palette layout keeps the input bar fixed while answers scroll, hides on
  blur, and aligns action/status colors more closely with Aurora.
- Ask/RAG defaults and logging are tuned for the local Gemma/llama.cpp path.

### Fixed

- Research responses shown in the Tauri palette now prefer summary/result text
  over raw payload JSON.
- Qdrant ask-path diagnostics now include enough stage detail to distinguish
  retrieval stalls from LLM synthesis latency.

## [4.7.0] - 2026-05-25

### Added

- Live per-page crawl progress for `axon crawl --wait`: Spider's broadcast page
  stream is wired into an indicatif spinner that updates every 250 ms with the
  running counts — `Crawling… N pages · M markdown · K thin`. Gated on stderr
  TTY + `!--json` + `!--quiet`; finishes with `✓ Crawled N pages · M markdown`
  on completion.

### Fixed

- Aurora styling applied to `evaluate`, `map`, `sync`, and `common_jobs` CLI
  output — all human-readable output now uses `primary`/`accent`/`muted` tokens.
- ANSI padding misalignment in `evaluate` score rows — raw string padded before
  wrapping in color helpers to avoid ANSI byte count skewing `{:<N}` widths.
- Double-redaction in `parse_claude_jsonl` test helper — per-item redaction was
  applied and then the joined block was redacted again; now matches production
  single-redaction behavior.
- Double-serialization in sessions server-mode batch loop — `post_json<T:
  Serialize>` now receives the struct directly instead of a pre-serialized
  `serde_json::Value`.
- `axon status --watch` idle-exit reduced from 5 → 3 ticks to fix a flaky
  timeout in the integration test suite under parallel load.
- Removed incorrect aggregate total-text cap in prepared-session validation —
  the per-doc limit is the correct boundary; batching handles large counts.

### Changed

- Sessions server-mode now batches prepared-session docs in chunks of
  `MAX_PREPARED_SESSION_DOCS` (256) and collects all returned job IDs.
- Claude and Codex session parsers now stream JSONL files line-by-line via
  `BufReader`, stopping at `max_text_bytes` without loading the full file into
  memory.

## [4.6.0] - 2026-05-25

### Added

- CLI palette aligned with the Aurora design system (`src/core/ui.rs`). Primary
  shifts to Aurora rose (#F9A8C4), accent to Aurora cyan (#29B6F6); success,
  warn, error, info, muted, and subtle map to their `--aurora-*` semantic
  token equivalents.
- `--color=auto|always|never` global flag (`Config::color_choice`) that flows
  through `core::ui::color_enabled()` and `core::logging::should_use_ansi()`.
- `--watch` global flag and `axon status --watch` live MultiProgress view of
  running/pending jobs.
- OSC 8 hyperlinks helper (`core::ui::hyperlink`) — URLs in `sources` are now
  clickable in supported terminals (kitty, iTerm2, wezterm, vscode, Windows
  Terminal). Falls through to plain text otherwise.
- Aurora bordered summary panel helper (`core::ui::panel`) used by
  `crawl --wait` to render a final completion card.
- `comfy-table`-backed Aurora table renderer (`core::ui::aurora_table`) used by
  `sources`, `domains`, and the job list views.
- Unicode sparkline helper (`core::ui::sparkline`) for future inline trend
  displays.
- Tracing-subscriber formatter now prefers 24-bit truecolor when
  `COLORTERM=truecolor` is set, falling back to the existing ANSI-256 Aurora
  palette otherwise.

### BREAKING CHANGES

- Removed the legacy graph/Neo4j request surface and dead runtime code. The
  CLI `--graph` flag, MCP `ask.graph`, `/v1/ask.graph`, internal
  `Config::ask_graph`, `timing_ms.graph`, and the unused Neo4j client module
  are no longer part of the production contract.
- Removed `--embed <bool>` from the CLI. Scrape and crawl still index by
  default when TEI and Qdrant are configured; use `--skip-embed` to fetch/save
  without writing to Qdrant.
- Renamed `--cache-skip-browser` to `--cache-http-only` so the flag describes
  the real behavior: cached crawl flow stays on the HTTP path and suppresses
  Chrome runtime/bootstrap.
- Removed noisy/deep tuning flags from the CLI surface; these now live in
  `~/.axon/config.toml` or `~/.axon/.env` as appropriate:
  `--chrome-remote-url`, `--chrome-proxy`, `--chrome-user-agent`,
  `--respect-robots`, `--min-markdown-chars`, `--drop-thin-markdown`,
  `--discover-sitemaps`, `--sitemap-since-days`, `--max-sitemaps`,
  `--delay-ms`, `--request-timeout-ms`, `--fetch-retries`,
  `--retry-backoff-ms`, `--bypass-csp`, `--accept-invalid-certs`,
  `--chrome-network-idle-timeout`, `--auto-switch-thin-ratio`,
  `--auto-switch-min-pages`, `--url-whitelist`, `--max-page-bytes`,
  `--redirect-policy-strict`, `--concurrency-limit`,
  `--crawl-concurrency-limit`, `--backfill-concurrency-limit`,
  `--watchdog-stale-timeout-secs`, `--watchdog-confirm-secs`,
  `--watchdog-sweep-secs`, and `--sqlite-path`.
- Removed `--server-url`, `--log-level`, and `--start-url` from the CLI.
  `AXON_SERVER_URL` and `AXON_LOG_LEVEL` remain the supported runtime knobs.

### Changed

- Rebuilt `axon help` from the actual Clap command surface and tightened
  command-specific help so subcommands no longer inherit unrelated global
  crawl/Chrome/vector flags.
- Kept `--local` as the explicit CLI override for bypassing `AXON_SERVER_URL`,
  but only advertises it on the top-level global help.
- Scrape preamble output now reports `indexing: enabled|skipped` instead of the
  internal `embed: true|false` boolean.

### Tests

- Added CLI help contract coverage to ensure removed flags are rejected by Clap,
  not merely hidden from generated help.
- Added config parsing coverage for `--skip-embed` mapping to
  `Config::embed = false`.

## [4.5.0] - 2026-05-23

### Added

- New `apps/palette-tauri/` desktop palette: Tauri 2 + React/Vite frontend
  with Aurora-styled UI components, axon HTTP client bindings, action
  registry, and shell-quoted command parsing.

### Fixed

- `dev_to` vertical: fetch full article body via the per-article detail
  endpoint (`/api/articles/{id}`) instead of falling back to the short
  `description` returned by the author listing. Adds `get_json` helper,
  `select_article_body`, and a debug trace for `body_markdown`/`description`
  lengths. Coverage expanded in `dev_to_tests.rs`.

## [4.4.2] - 2026-05-21

### Fixed

- `brand` and `diff` MCP actions: promote `url` (brand), `url_a`, `url_b` (diff)
  from `Option<String>` to `String` in the request schema so MCP clients receive
  accurate required-field information. Serde now enforces presence at parse time;
  the now-redundant `ok_or_else` runtime guards in both the MCP handler
  (`handlers_query/brand_diff.rs`) and the action-API dispatcher
  (`dispatchers_brand_diff.rs`) have been removed. Schema doc regenerated.

## [4.4.1] - 2026-05-21

### Fixed

- Fixed `BUNDLE_FETCH_SEMAPHORE` to acquire one permit per individual bundle
  HTTP fetch rather than one permit per endpoint-discovery session. Previously
  the cap limited concurrent *bundle-fetch phases* (sessions), allowing up to
  `cap × 8` total concurrent bundle requests across the process. Now
  `AXON_ENDPOINT_BUNDLE_CONCURRENCY` (default 8) is a true global cap on
  simultaneous bundle HTTP fetches process-wide.

## [4.4.0] - 2026-05-21

### Added

- Added global process-wide semaphores for bundle fetches (default 8), Chrome
  capture (default 1), and verification probes (default 16) to limit
  concurrent outbound I/O across all simultaneous endpoint discovery requests.
  Override via `AXON_ENDPOINT_BUNDLE_CONCURRENCY`, `AXON_ENDPOINT_CHROME_CONCURRENCY`,
  and `AXON_ENDPOINT_VERIFY_CONCURRENCY`.
- Added CDP `Fetch.enable` pre-dispatch SSRF blocking for Chrome network capture.
  Private, loopback, link-local, `.local`, and `.internal` targets are now
  rejected at the network level before Chrome dispatches them, not filtered
  from results after capture.

### Fixed

- Fixed MCP scope: `endpoints` action now requires `axon:write` (was incorrectly
  mapped to `axon:read`). Endpoint discovery performs active outbound network I/O
  and must not be accessible with read-only tokens.
- Fixed REST routing: `/v1/endpoints` moved from `read_routes` to `write_routes`
  to match the MCP scope change.
- Fixed verification constants to match bead w2wf.4 acceptance criteria:
  `MAX_VERIFY_PROBES` corrected from 40 to 100, `VERIFY_TIMEOUT_SECS` from 4s
  to 2s, `VERIFY_CONCURRENCY` from 5 to 4.

### Documentation

- Added `## Security and Scope` section to `docs/reference/commands/endpoints.md` covering
  `axon:write` scope requirement, anonymous probe behavior, and CDP pre-dispatch
  blocking.
- Added `## Resource Controls` table to `docs/reference/commands/endpoints.md` with exact
  constants, semaphore defaults, and environment override knobs.

## [4.3.0] - 2026-05-21

### Added

- `axon diff <url-a> <url-b>` — port of webclaw's diff tool. Fetches two URLs,
  compares their markdown content with a unified diff, reports metadata changes
  (title, description, author, etc.), added/removed links, and word-count delta.
  Status is now `Changed` when links differ even if text is identical.
  Exposes a matching `diff` MCP action and service function (`services::diff::diff`).
- `axon brand <url>` — port of webclaw's brand tool. Analyzes a URL's HTML/CSS
  for brand identity: up to 10 dominant colors (with usage classification), brand
  font families, primary logo URL, favicon URL, og:image, and all logo variants.
  Pure DOM/CSS extraction (no LLM), with SSRF guard and HTTP status validation.
  Exposes a matching `brand` MCP action and service function (`services::brand::brand`).
- New dependencies: `similar = "2"` (unified diff), `scraper = "0.22"` (HTML DOM/CSS parsing),
  `once_cell = "1"` (lazy regex compilation).

## [4.2.0] - 2026-05-19

### Added

- `AXON_USER_AGENT` env var — general-purpose UA for all HTTP requests
- `AXON_CHROME_USER_AGENT` falls back to `AXON_USER_AGENT`
- docs.rs vertical extractor fetches rustdoc JSON directly
- crates.io extractor pulls rustdoc JSON + README concurrently
- New verticals: hackernews, stackoverflow, arxiv, github_issue, github_pr, docs_rs
- YouTube extractor using ytInitialPlayerResponse (no yt-dlp needed)
- Chrome path for Amazon/eBay extraction when configured
- `cargo xtask check-secrets` pre-commit secret scanner
- Sparse checkout on all CI jobs (significant checkout time reduction)

### Fixed

- All vertical extractors use correct UA (browser vs API)
- crates.io 429 retry with Retry-After backoff
- Various vertical content gaps (npm readme, PyPI description, etc.)

## [4.1.0] - 2026-05-18

### Added

- **REST API: dedicated per-resource routes (`/v1/{resource}`).** Replaces the
  generic `POST /v1/actions` envelope dispatcher with HTTP-native routes.
  Implemented as Families 1–4 of epic `axon_rust-2qva`:
  - **Family 1** — `GET /v1/{sources,domains,stats,doctor,status}` (read scope).
  - **Family 2** — `POST /v1/{query,retrieve,suggest,map,search,research,scrape}`
    (read scope for query/retrieve/map; write for the rest). `/v1/ask` already
    existed and is kept. `/v1/evaluate` is intentionally NOT exposed yet — see
    Known Limitations.
  - **Family 3** — async jobs for crawl/embed/extract/ingest:
    `POST /v1/{kind}` returns `202 Accepted` + `JobStartOutcome`; `GET
    /v1/{kind}/{id}` returns the current status (or 404); `POST
    /v1/{kind}/{id}/cancel` cancels the job.
  - **Family 4** — admin/destructive: `POST /v1/migrate` and `POST /v1/dedupe`
    (unconditional auth — required even in `LoopbackDev`, matching the
    `/v1/actions` invariant); watch scheduler CRUD at `GET /v1/watch`, `POST
    /v1/watch/create`, `GET /v1/watch/{id}`, `POST /v1/watch/{id}/run`.

### Deprecated

- **`POST /v1/actions` envelope.** Still functional and supported, but every
  response now carries the `Deprecation: true` and `Link: …; rel="successor-version"`
  headers per RFC 8594 guidance. New integrations should use the dedicated
  `/v1/{resource}` routes added in this release. A removal date will be
  announced before the next major bump.

### Changed

- `src/jobs/watch.rs::run_watch_now` outcome error type narrowed from
  `Box<dyn Error>` to `Box<dyn Error + Send + Sync>` so it can be invoked from
  a multi-thread axum handler (`POST /v1/watch/{id}/run`).

### Known Limitations

- `POST /v1/evaluate` is not yet exposed via a dedicated REST route.
  `services::query::evaluate` and the underlying `vector/ops/commands/evaluate`
  pipeline hold non-`Send` `Box<dyn Error>` values across `.await` points, and
  the multi-thread axum runtime requires `Send` handler futures. Tracked as a
  separate follow-up bead. Callers can still reach evaluate via
  `POST /v1/actions { "action": { "action": "evaluate", ... } }`.

## [4.0.0] - 2026-05-18

### BREAKING CHANGES

- **Auth scope: `ask`, `evaluate`, `suggest`, `research` promoted from `axon:read` to `axon:write`.**
  These actions trigger Gemini headless completions (external process, API quota consumption) and
  must not be reachable with read-only tokens. If you have automations or integrations using
  `axon:read` tokens to call these actions via the HTTP API, re-issue those tokens with
  `axon:write` scope.

### Security

- **F1 — `required_scope` catch-all hardened.** The wildcard arm in `required_scope()` previously
  returned `None`, which caused `authorize_action` to return `Ok(())` and bypass auth entirely for
  any unrecognised `AxonRequest` variant. It now returns `Some("axon:write")` as a secure default.
- **F5 — Migrate and Dedupe unconditionally require auth.** `action:migrate` and `action:dedupe`
  are now auth-gated regardless of `LoopbackDev` mode. A server with no token configured
  (development loopback) can no longer trigger these destructive operations without a credential.

## [3.0.1] - 2026-05-18

### Changed

- **HTTP API auth**: Promoted `ask`, `research`, `evaluate`, `suggest`, and
  `debug` action scopes from `axon:read` to `axon:write` because they can
  trigger Gemini completions or other cost-bearing side effects. Existing
  read-only tokens must be re-issued with `axon:write` before calling these
  actions through `/v1/actions` or the dedicated REST API.
- **palette/ci**: desktop CI now runs the `apps/desktop` unit test suite on
  Linux and Windows before producing release binaries.
- **palette/docs**: added dedicated desktop build, run, platform, hotkey,
  binary-resolution, smoke-test, and markdown-link behavior documentation.

### Security

- **HTTP API auth**: Unknown action variants now fail closed to `axon:write`
  instead of falling through to unauthenticated dispatch, and `migrate` /
  `dedupe` require auth even in loopback development mode.
- **SSRF**: `scrape` and `map` service entry points now use DNS-aware URL
  validation with a two-second fail-closed timeout before handing URLs to
  Spider-backed fetch paths.

## [3.0.0] - 2026-05-17

### BREAKING CHANGES

- Removed the entire OpenAI-compatible LLM client path. All LLM operations
  (`ask`, `evaluate`, `suggest`, `research`, `debug`, and `extract` LLM
  fallback) now run exclusively through the Gemini headless backend
  (`AXON_HEADLESS_GEMINI_*`).
- Removed env vars: `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL`.
  They are no longer read at startup. `axon setup repair --migrate-env`
  scrubs them from existing `~/.axon/.env` files (registered as
  `Delete`/`DeleteOnMigration` in `env_registry/migration.rs`).
- Removed CLI flags: `--openai-base-url`, `--openai-api-key`, `--openai-model`.
- Removed `Config` fields `openai_base_url`, `openai_api_key`, `openai_model`
  (and the matching fields on `ExtractWebConfig` and `ServiceUrls`). Test
  struct literals that set these fields will fail to compile until updated.
- Removed the `openai` service entry from the `doctor` JSON output and the
  associated `probe_openai`, `resolve_openai_model`,
  `openai_diagnostics_enabled`, and `openai_service_json` helpers.
- Removed `estimate_llm_cost_usd` and the `estimated_cost_usd` field on
  `ExtractionMetrics`, `FallbackResponse`, the extract aggregation, and
  the `extract` summary JSON. The estimator was OpenAI-pricing-only and
  returned `0.0` for every other model — including all Gemini models.
- Removed dead `build_openai_chat_request` helper in
  `src/vector/ops/commands/streaming.rs`.
- Removed the `gemini_compatible_model` / `gemini_compatible_openai_model`
  shims: `LlmBackendConfig.gemini_model` is now sourced exclusively from
  `headless_gemini_model` (no fallback through `openai_model`).

### Migration

- Run `axon setup repair --migrate-env` to scrub the three removed env
  vars from `~/.axon/.env`.
- If you relied on a non-Gemini OpenAI-compatible endpoint for ask /
  research / evaluate / extract, install the Gemini CLI and set
  `AXON_HEADLESS_GEMINI_CMD` and (optionally) `AXON_HEADLESS_GEMINI_MODEL`.
- Closes bead `axon_rust-6yxz` (env_registry misclassified OPENAI_BASE_URL
  / OPENAI_API_KEY as `WarnAndIgnore`) — obsolete after removal.

## [2.7.0] - 2026-05-17

### Added

- ask: four new session-management flags complement the existing `--follow-up`/`--session`/`--reset-session` surface:
  - `--new-session` — force a fresh thread for an explicit or auto-generated session name (auto names use `auto-YYYY-MM-DD-HHMMSS`). Wipes prior turns and runs without follow-up context. Mutually exclusive with `--follow-up`, `--reset-session`, and `--resume` (clap-enforced).
  - `--list-sessions` — print all local ask sessions (name, turn count, last-modified absolute + relative, and a `*` for `latest`). Sorted by last-modified descending. Pair with `--json` for a machine-readable array. Cannot be combined with a query argument.
  - `--resume <NAME>` — shorthand for `--follow-up --session <NAME>`. Mutually exclusive with `--session` (redundant) and `--new-session`.
  - `--continue` / `-c` — clap aliases for `--follow-up`, more discoverable for "keep going" usage.
- ask: `~/.axon/ask-sessions/` is now enumerable via `axon ask --list-sessions` without spinning up retrieval or the LLM; the renderer skips `latest`, `.tmp-*` temp files, and any non-`.jsonl` entries.

### Fixed

- ask (PR #103 review): `--resume` now has a clap-enforced conflict with `--new-session`, `--reset-session`, and `--session` so contradictory invocations are rejected at parse time instead of silently reinterpreting `<name>` as a reset target.
- ask (PR #103 review): `--list-sessions` validation also rejects the global `--query` flag, matching the documented "no query argument" contract. Previously `axon ask --list-sessions --query "..."` parsed and silently ignored the query.
- ask (PR #103 review): `--list-sessions` no longer hides session files whose names start with `.` (e.g. `.team.jsonl`). The previous dotfile filter was too broad; temp files written by `write_atomic` are still filtered by the `.jsonl` suffix check.

### Documentation

- docs/reference/commands/ask.md: documented the seven session-related flags together, added a "Session Lifecycle" table covering each flag's effect on session selection, history loading, and wipes, plus runnable examples for the new combinations.

## [2.6.0] - 2026-05-17

### Added

- palette: cross-restart persistence for the in-memory ask conversation. On startup the palette reads `~/.axon/ask-sessions/latest` and the pointed-to JSONL; if the last turn is within the 30-minute idle window, the live conversation (turn count + last-turn timestamp) is restored so reopening the palette picks up the existing `--follow-up` chain instead of starting fresh. Only turn timestamps are deserialized — prompt/answer content never enters palette memory. Stale, missing, corrupt, or path-traversal-y session pointers fall back to a fresh state without crashing.

## [2.5.0] - 2026-05-17

### Changed

- **release**: Re-version PR #100's feat-level work as `2.5.0` to comply with the repo policy that `feat`-prefixed changes bump the minor (not patch). The original `2.4.0` bump was correct for the feature commit, but subsequent review-fix patch bumps (`2.4.1`, `2.4.2`) shipped under the same feature PR — per policy, the rolled-up release tag for this PR is a minor. Supersedes `2.4.2`. (PR #100 review feedback)

## [2.4.2] - 2026-05-17

### Fixed

- **palette/ui**: Stale-conversation idle-timeout sweep now runs on every submit instead of only `ask` submits, so non-ask commands after a long idle still clear the stale conversation state. (PR #100 review feedback)
- **.env.example**: Removed `AXON_LITE=` template entry. The migration matrix classifies the key as `delete-on-migration`; runtime still accepts it as a backward-compat no-op, but it has no place in an env template. Aligns env-template with migration policy. (PR #100 review feedback)
- **docs/reference/env-matrix.toml**: Added `env-template` to `AXON_LOG_PATH` surfaces so migration metadata reflects that the key is present in `.env.example`. (PR #100 review feedback)

## [2.4.1] - 2026-05-17

### Fixed

- **palette/ui**: `submit()` now gates the `ask-reset` sentinel behind the running-command guard. Previously a user could submit `ask-reset` while an `ask` subprocess was still in flight; when that subprocess finished and called `finalize_result()`, it would recreate the conversation we just cleared, contradicting the "Next ask will start a fresh session." notice.
- **palette/render**: Conversation hint footer slot now has an explicit `w(px(180.0))` so it actually is a fixed-width slot — its appearance/disappearance no longer shifts surrounding footer elements.
- **.env.example**: Removed `GOOGLE_API_KEY` / `GOOGLE_APPLICATION_CREDENTIALS` (Gemini subprocess env allowlist scrubs them — setting them has no effect). Reverted `AXON_LOG_DIR` / `AXON_LOG_FILE` back to the actually-honored `AXON_LOG_PATH`.

## [2.4.0] - 2026-05-17

### Added

- **palette**: Minimal-on-launch window sizing. The palette now opens at the height of the prompt input row only (~91px including chrome) instead of the previous fixed 560px and grows content-driven as the user types, runs a command, or sees output. Hysteresis: clearing the query collapses the action list but keeps the most recent output card; dismissing the output card collapses fully. `apps/desktop` bumped to 0.3.0.
- palette: auto-continue `axon ask` conversations. The first `ask` of a session shells out plain; every subsequent `ask` while the conversation is "live" prepends `--follow-up` so the CLI threads it onto the same session. State is in-memory only (no cross-restart persistence). Conversations idle-time out after 30 minutes of inactivity, matching the ACP session cache TTL.
- palette: new "Reset ask conversation" action (aliases `reset-ask`, `new-chat`, `fresh-ask`) that clears the live conversation without shelling out. Surfaces a transient "Conversation reset" notice so the next ask starts fresh.
- palette: footer hint slot showing `· conversation: N turn(s)` when a follow-up chain is active. Layout space is always reserved so the hint's appearance does not shift surrounding footer elements.
- research: `--research-depth <N>` is now wired into the synthesis pipeline. When set, it overrides `--limit` as the number of Tavily sources synthesized over; falls back to `--limit` (default 10) when unset. Capped at 100 together with `--offset`.
- research: typed `ResearchPayload` replaces the untyped `serde_json::Value` payload at the service boundary. Adds `summary_source: "llm" | "fallback" | "none"` so callers can distinguish an LLM-produced summary from the deterministic fallback substituted on synthesis failure, and from the no-extractions case.
- research/search: bounded Tavily retry (3 attempts, 750ms exponential backoff) on transient provider failures, matching the resilience pattern used by `tei_embed`.
- research/search: pagination window enforcement — `limit + offset > 100` is now rejected at the service boundary with a clear error, instead of silently returning a truncated page.
- research/search: extended secret-redaction heuristic in log previews (AWS, JWT, Slack, Stripe, Google API keys, Tavily). Splits on `=`/`&`/`;`/`?`/`,` so `?key=sk-…` query-string forms are caught.
- research: XML-attribute escaping (`"`, `<`, `>`, `&`, control chars) on URLs and titles inside the synthesis prompt's `<untrusted_source>` framing, hardening prompt-injection defenses.

### Changed

- **palette**: `apps/desktop/src/ui.rs` refactored — `Render for Palette` impl moved to a `ui_render.rs` sidecar declared via `#[path]` to keep `ui.rs` under the 500-line monolith cap. New modules `apps/desktop/src/anim.rs` (shared easing/lerp helpers) and `apps/desktop/src/layout.rs` (pure height compute, no side effects).
- research: human-readable summary now labels fallback summaries explicitly (`=== Summary === (fallback — LLM synthesis unavailable)`).
- research: extraction preview no longer JSON-encodes the snippet (was showing `"\nescaped\n"`); now takes the raw text and char-truncates to 200.
- research: query validation runs before Tavily prereq check so a user with neither set gets the cheaper error first.
- research: synthesis-delta drop warnings rate-limited to one per session (was one per dropped token).
- research: consumer-drain timeout raised from 5s to 10s, with a clearer warning message.
- research/search: `map_research_payload` accepts a typed `ResearchPayload` (was `serde_json::Value`). MCP handler serializes the payload to JSON at the wire boundary.

### Removed

- research: `pub use synthesis::research_payload` re-export (only `research` is consumed by external callers). `research_payload` remains accessible inside the synthesis module.
- research: duplicate Tavily prereq check in the CLI handler (`validate_research_prereqs`). The service layer remains the single source of truth.

### Fixed

- **palette**: ANSI stripper terminator discipline — DCS/APC/PM/SOS now terminate only on ST (`ESC \`), not BEL. Per ECMA-48, only OSC accepts BEL as a shortcut terminator; embedded BEL bytes inside DCS/APC/PM/SOS payloads are content and must not short-circuit stripping. (PR #101 review.)
- **palette**: `step_toward(current, target, 0.0)` now returns `current` instead of `target`. A zero step means "no movement this tick" — the previous behaviour caused an unintended instant jump. (PR #101 review.) `apps/desktop` bumped to 0.3.1.
- research: misleading test name `test_run_research_allows_gemini_without_adapter` → `test_run_research_does_not_require_openai_model`.

## [2.3.3] - 2026-05-17

### Fixed

- **palette/output**: `consume_until_string_terminator` no longer treats a bare `ESC` as a String Terminator. Malformed OSC/DCS/APC payloads that contain a bare ESC (without a following `\`) would previously exit the strip loop early and leak the remaining payload bytes to rendered output. The ESC is now swallowed and stripping continues until a proper BEL or ESC `\` terminator is seen.
- **.env.example**: Removed `GOOGLE_API_KEY` / `GOOGLE_APPLICATION_CREDENTIALS` (both are scrubbed by the Gemini subprocess env allowlist — setting them has no effect). Reverted the logging keys from the unimplemented `AXON_LOG_DIR`/`AXON_LOG_FILE` pair back to the actually-honored `AXON_LOG_PATH`.

## [2.3.2] - 2026-05-17

### Fixed

- **palette**: Drop the repeating `pulsating_between` animation on the launch-time health-check status dot. The auto-spawned `axon doctor --json` probe combined with `Animation::new(...).repeat()` could keep GPUI re-rendering every frame on slower compositors and starve key-event dispatch, producing the user-visible "window opens but won't accept typing" freeze. The dot now changes color only (grey/checking → green/connected → red/disconnected). The footer and output-card pulsing dots, which only render once a command is selected or running, are unaffected.

## [2.3.1] - 2026-05-17

### Fixed

- cli(config): `axon config get` now accepts `--env` / `--toml` overrides, matching `set`/`unset`. Previously, keys written with a forced target that didn't match the auto-detect heuristic (e.g. lowercase keys forced into `.env`) were unreadable via `get`.
- cli(config): `set`/`unset`/`get` flag-scan is now scoped to positional args **after** the value, so a literal value of `--env` or `--toml` (e.g. `axon config set MY_KEY -- --toml`) is no longer misinterpreted as a target flag.
- cli(config): underscore-prefixed env keys (`_MY_VAR`) are now recognized by auto-detection, matching what `set_env_entry` already accepts on the write path.

## [2.3.0] - 2026-05-17

### Added

- cli: new top-level `axon config` command for reading/writing entries in `~/.axon/.env` and `~/.axon/config.toml`. Subcommands: `list [--env|--toml] [--reveal]`, `get <key> [--reveal]`, `set <key> <value> [--env|--toml]`, `unset <key> [--env|--toml]`, `path`. Auto-routes by key shape (UPPER_SNAKE → `.env`, dotted lowercase → `config.toml`); secrets are redacted by default and revealed only with `--reveal`. File-IO logic lives in `src/services/config.rs`; the CLI handler in `src/cli/commands/config.rs` is a thin router. Atomic write with 0o600 permissions; respects `AXON_ENV_FILE` and `AXON_CONFIG_PATH` overrides.

## [2.2.2] - 2026-05-16

### Fixed

- `AXON_COLLECTION` env var now correctly wins over TOML default when the user passes `--collection axon` explicitly (was comparing the value to the sentinel literal instead of checking clap's value source)
- Desktop palette: commands now pass `--local` to force in-process execution, fixing connection failures when `AXON_SERVER_URL` is set
- Desktop palette: ANSI escape codes stripped from stderr output before display

### Added

- Desktop palette: markdown rendering for `scrape`, `ask`, and `research` stdout via `pulldown-cmark`
- Desktop palette: clickable hyperlinks in markdown output open in the system browser (`xdg-open` / `cmd /c start`)
- Desktop palette: Tab key locks a command (badge mode), Backspace from empty argument unlocks
- Desktop palette: connection status dot with click-to-reconnect
- Desktop palette: "clear ✕" dismiss button for command output
- Desktop palette: Aurora design system token alignment across typography and spacing

## [2.2.1] - 2026-05-16

### Removed

- chore: remove `AXON_LITE` / `--lite` compat shim. The CLI flag, env var reading, `cfg.lite_mode` field, doctor `lite_mode` JSON output, and the dead `postgres`/`redis`/`amqp` doctor render branch are all gone. SQLite/in-process is the only runtime — has been for months. The migration registry still scrubs `AXON_LITE` from legacy `~/.axon/.env` files via `axon setup migrate-env` (see bd `axon_rust-kbad` for deferred final cleanup).
- chore: remove `GOOGLE_API_KEY` and `GOOGLE_APPLICATION_CREDENTIALS` from the Gemini headless env allowlist and runtime env registry. Listed in the migration registry as `Delete` so legacy `.env` files get scrubbed on next `axon setup migrate-env`.

### Changed

- chore: hard-rename `AXON_LOG_DIR` + `AXON_LOG_FILE` → single `AXON_LOG_PATH` env var (full path to active log file; rotated siblings live in the same directory). Default unchanged: `$AXON_DATA_DIR/logs/axon.log`. Legacy vars listed in migration registry for cleanup.
- chore: rename default Qdrant collection `cortex` → `axon`. Affects `Config::default().collection`, the clap `--collection` default, and the `cfg.collection != "cortex"` "is user-customized?" checks in `build_config.rs` and `src/ingest/sessions.rs::resolve_collection`.
- chore: rename `Config::default_lite()` → `Config::default_minimal()` and `apply_default_lite_tuning()` → `apply_default_minimal_tuning()`. Test fn names `*_in_lite_mode` → `*_with_lite_backend`.
- docs: scrub `AXON_LITE` / `--lite` / `cortex` references across CLAUDE.md, docs/guides/configuration.md, docs/contributing/testing.md, .env.example, config.example.toml. Root CLAUDE.md updated to point at `axon setup migrate-env` for the env scrub (was misleadingly described as "auto-scrubs"). The OPENAI_* docstring corrected — those vars are active wire types for the extract pipeline, not legacy compat.
- docs: add `src/extract/CLAUDE.md` (vertical extractor framework + 13 verticals). Refresh `src/core/CLAUDE.md` content/ map (extract_ladder, extraction, markdown, filename, url_parsing + sidecars). Refresh `src/crawl/CLAUDE.md` collector/ map with per-page passes (antibot detect, structured-data, DOM ladder). Add `vertical_scrape` action (discovery-only) to `src/mcp/CLAUDE.md`. Update all sub-CLAUDE.md `Last Modified` headers.
- env: restructure `.env.example` into labeled sections (Data + URLs, MCP, Web panel, Ingest, Gemini, Logging, Compose). Add missing actively-read vars (`AXON_HOME`, `AXON_COLLECTION`, `AXON_MCP_HTTP_HOST/PORT`, `AXON_WEB_ALLOWED_ORIGINS`, `AXON_WEB_API_TOKEN`).
- docker: refactor docker-compose.yaml with `x-common-service` and `x-gpu-service` YAML anchors to reduce duplication. Dockerfile: pin container env defaults (`AXON_HOME`, `AXON_IN_CONTAINER`, `AXON_MCP_HTTP_HOST=0.0.0.0`, `CLICOLOR_FORCE`).
- desktop: tighten command-palette match logic; return empty matches early on empty query.

## [2.1.1] - 2026-05-16

### Changed

- refactor: migrate all inline `#[cfg(test)] mod tests { ... }` blocks to sibling `_tests.rs` sidecar files (epic `axon_rust-lon7`). Production source files are now free of test code. The `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` pattern preserves private-item access. 143 source files migrated; adds `scripts/migrate_test_sidecars.py` for bulk migration. CLAUDE.md updated with pattern docs, footgun notes, and worked examples. vendor/lab-auth excluded.

## [2.2.0] - 2026-05-16

### Added

- vector: emit typed JSON-LD / `__NEXT_DATA__` / SvelteKit structured-data fields (`structured_kind`, `structured_type`, `structured_id`, `structured_blob`) on Qdrant payloads alongside markdown chunks. New `src/core/structured` module ports webclaw's three extractors plus `sanitize_json_newlines` (Bluesky-style raw-newline fallback). Per-chunk blob is capped at `cfg.structured_data_max_bytes` (default 64 KiB) — oversized blobs are dropped, not truncated. Wired into the remote-URL embed path via `prepare_embed_docs`. Existing 3.79M points remain on the implicit-v1 schema with no filter applied by default. (bd axon_rust-xvu9, axon_rust-d5mb)

## [2.1.0] - 2026-05-16

### Added

- retrieval: add typed corpus-health diagnostics and richer `ask --explain` selection metadata so ranking, full-document selection, and corpus coverage failures can be separated without scraping logs.
- eval: add a tracked retrieval fixture harness for repeatable domain-quality sweeps, including strict expected-domain matching and regression coverage for script pass/fail semantics.
- vector: every new Qdrant upsert now stamps `payload_schema_version` (integer, currently `2`) and optional `extractor_name` (keyword) on the payload. Existing pre-2.1.0 points (~3.79M) have no version field and are treated as implicit version `1`. `PAYLOAD_SCHEMA_VERSION` const lives in `src/vector/ops/qdrant/utils.rs` and is re-exported from `crate::vector::ops::qdrant`. (bd axon_rust-lu6a)
- vector: retrieval supports an optional `payload_schema_version >= N` filter via `VectorSearchRequest::with_payload_schema_version_min` and the new helper `qdrant::filter::build_schema_version_filter`. Default ask/query retrieval applies no filter — backward-compatible with existing points. Opt-in callers (vertical-aware queries from `xvu9`) pass `Some(N)` to scope results.
- cli: `axon sources --by-schema-version` adds a per-version chunk-count breakdown via collection scroll. Opt-in only — expensive on large collections. Breakdown is exposed in JSON output under `schema_version_breakdown`.
- vector: new integer payload index on `payload_schema_version` plus a keyword index on `extractor_name` so `/facet` and range filters work efficiently.

### Fixed

- retrieval: keep explain selection metadata attached to the right candidate after rerank reordering or duplicate URLs by using stable candidate keys instead of kept-index ordinals.
- retrieval: harden full-document selection with URL dedupe, configurable per-domain diversity, and fallback fill when a single domain is the only available source.

## [2.0.0] - 2026-05-15

### Added

- desktop: new `axon-palette` workspace member at `apps/desktop` — a GPUI command palette with a global hotkey (Ctrl+Shift+Space). Type to filter actions (Scrape/Crawl/Map/Ask/Search/Research/Ingest/Status/Doctor), space + arg, Enter spawns `axon` as a subprocess. Linux/X11 and Windows; Wayland depends on compositor.
- ci: new `windows-build` job cross-compiles `axon.exe` from `ubuntu-latest` via `cargo-zigbuild` targeting `x86_64-pc-windows-gnu` (no Windows runner required). Uploads `axon-windows-x86_64` artifact with `if-no-files-found: error`.
- ci: new `desktop.yml` workflow builds `axon-palette` for Linux + Windows on every PR touching `apps/desktop/**`.

### Removed (breaking)

- setup: removed `axon setup deploy` subcommand and the SSH-based remote Docker Compose orchestration. The `POST /api/panel/setup/deploy` web route, the "Remote Docker Deploy" panel in `apps/web`, and `src/services/setup/deploy.rs` are all gone. Use local Docker plus SSH tunnels or a proxy for cross-host setups. `axon setup targets` (informational listing of `~/.ssh/config` aliases) remains.
- deps: dropped `openssh` and `openssh-sftp-client` from `Cargo.toml`. These were the only Unix-only deps in the main crate; removing them is what makes the Windows cross-compile possible.
- spider: dropped the `firewall` spider feature — `spider_firewall`'s build script fetches GitHub blocklists unauthenticated and panics on rate-limit, which was breaking CI. `validate_url()` in `src/core/http/ssrf.rs` remains the primary SSRF guard; this was defense-in-depth on top.

## [1.11.3] - 2026-05-14

### Fixed

- Gemini CLI 0.41.2 compatibility: `tool_use` events renamed `"name"` field to `"tool_name"`; new built-in `update_topic` tool now whitelisted alongside `activate_skill`; removed unreliable `stats.tool_calls` count gate

## [1.11.2] - 2026-05-14

### Fixed

- logging: respect `FORCE_COLOR`/`CLICOLOR_FORCE`/`NO_COLOR` env vars so ANSI colors work in Docker where stderr is a pipe (not a TTY).
- docker: set `CLICOLOR_FORCE=1` in the axon container environment so colored log output is on by default; disable with `AXON_LOG_COLOR=0` in `~/.axon/.env`.
- docker: set container `TZ` from the host `TZ` env var, defaulting to `America/New_York`, so log timestamps show local time instead of UTC.

## [1.11.1] - 2026-05-13

### Fixed

- mcp: route HTTP MCP transports through the unified web server so the web panel, first-party APIs, and `/mcp` all share port 8001.
- auth: advertise the root-mounted OAuth protected-resource metadata URL while keeping `/mcp` as the canonical resource audience.

### Changed

- docs: align MCP transport guidance with the unified web/MCP listener model.

## [1.11.0] - 2026-05-12

Container now ships the Gemini CLI so `ask`/`evaluate`/`research` synthesis works without a host gemini binary. Dev binary auto-syncs to PATH and the container on every `axon` invocation. Fixes gemini 0.41+ compatibility and a recurring SQLite migration version-mismatch error.

### Added

- docker: install Node.js 22 LTS (via NodeSource) and `@google/gemini-cli@0.41.2` in the container runtime stage so `ask`/`evaluate`/`research` synthesis works in the served container without a host gemini binary.
- docker-compose: bind-mount `~/.gemini` read-only into the container so the Gemini CLI can access OAuth credentials for headless synthesis.
- scripts/axon: auto-rebuild the debug binary and sync PATH symlinks on every invocation; trigger an async `docker compose build + up` when source files are newer than `target/.container-built` so the container stays in sync automatically.
- just: add `link-bin` target that syncs `~/.local/bin/axon` and all plugin cache slots to the compiled release binary; wired into `just build` so every release build stays current.
- just: add `sync-container` target for synchronous release build + container rebuild + restart.

### Fixed

- gemini: set `GEMINI_CLI_TRUST_WORKSPACE=true` when spawning the Gemini headless process so gemini 0.41+ does not exit with code 55 in non-interactive mode.
- gemini: preserve the user's real `settings.json` in the isolated HOME instead of generating one from scratch; clear only MCP server, hook, and context fields to prevent side effects while keeping auth configuration intact across gemini versions.
- jobs/lite: wrap sqlx migration version-mismatch error with a human-readable hint pointing to `just install`.

## [1.10.1] - 2026-05-11

### Fixed

- cli: finish the server-mode renderer split so scrape, screenshot, crawl, extract, embed, ingest, and sessions reuse a shared typed response renderer instead of duplicating per-command formatting logic.
- cli: keep the command-level emit/render helpers aligned across local and server paths, and update the CLI contract smoke tests for the expanded retrieve option/result types.

## [1.10.0] - 2026-05-11

### Changed

- mcp: make `scrape` and `retrieve` inline-first document readers with a shared paged-content contract (`content`, `token_estimate`, `next_cursor`, `remaining_tokens_estimate`, `backend`) instead of forcing artifact-path responses.
- retrieval: promote `retrieve` to the canonical document reader by unifying Qdrant, stored source, and live scrape refresh backends behind one typed response with explicit warnings and refresh metadata.

### Fixed

- cli: surface retrieve backend, continuation, and refresh metadata in human and JSON output so large document reads can continue without falling back to raw artifacts.
- docs: reposition `artifacts` as a debug/admin surface and document the new content-first `scrape`/`retrieve` flows across MCP references.

## [1.9.5] - 2026-05-11

### Fixed

- ci: make the Claude review workflow best-effort so Anthropic quota or service outages warn instead of failing the PR.

## [1.9.4] - 2026-05-11

### Fixed

- dev: remove user-specific `sccache` wrapper paths and socket defaults from `Justfile` so local recipes stay portable across developers and machines.
- config: preserve an explicit `--output-dir` even when it matches the clap default, instead of letting `AXON_OUTPUT_DIR` or post-init default derivation override it.

## [1.9.3] - 2026-05-10

### Fixed

- http: route internal Qdrant and TEI calls through the internal service client so `AXON_SERVER_URL` does not accidentally bounce container-local maintenance traffic back through the public server endpoint.
- serve: initialize the unified server service context eagerly and clear `AXON_SERVER_URL` inside the Docker service so container-local commands stay on direct service URLs.
- dev: wire the local sccache wrapper defaults into `just` recipes for faster, consistent local builds.

## [1.9.2] - 2026-05-10

### Fixed

- status: make `axon status --server-url ...` reuse the same detailed human renderer as local mode instead of collapsing server responses to totals-only output.
- http: keep the shared client builder warning-free in test builds so clippy/hooks no longer fail on dead code and unused SSRF-guard plumbing.

## [1.9.1] - 2026-05-10

### Fixed

- config: ignore blank optional path env vars such as `AXON_OUTPUT_DIR`, `AXON_SQLITE_PATH`, and `AXON_LOG_DIR`, falling back to canonical `~/.axon` defaults instead of treating empty strings as real paths.
- docker: align the builder image with the pinned Rust `1.94.0` toolchain so cached Docker rebuilds avoid repeated Rust toolchain installation.

## [1.9.0] - 2026-05-09

### Changed

- config: make `~/.axon` the canonical appdata root for Compose, plugin setup, CLI wrappers, generated artifacts, and service state; keep Docker-host relocation behind `AXON_HOME` and publish MCP HTTP loopback-only by default.

### Fixed

- mcp: preserve adaptive ask diagnostics in typed service results, reject unavailable `ask.graph=true` requests, and route request-scoped MCP config changes through `Config::apply_overrides`.
- retrieval: add typed direct-retrieve metadata, bounded canonical-first URL lookup, malformed-point warnings, `retrieve --max-points`, shared query/ask dispatch diagnostics, and ask full-document cache config enforcement.
- retrieval: extract shared typed retrieval helpers for query and ask embedding, vector dispatch diagnostics, mode metadata, candidate construction, and scoring.
- retrieval: simplify the shared query/ask retrieval helpers without changing public result contracts.

### bd-teams/ask-perf-foundation

- ask: AXON_ASK_HYBRID_CANDIDATES default lowered 150 → 100 (Qdrant RRF rank-stable at 2x final K; ask_candidate_limit=50 → prefetch=100 is sufficient).

## [1.8.2] - 2026-05-07

### Fixed

- mcp: expose `evaluate` and `suggest` through the unified tool, reject unavailable `ask.graph=true`, preserve adaptive ask diagnostics in typed service results, and regenerate MCP discovery docs/help.

## [1.8.1] - 2026-05-07

### Fixed

- crawl: run sitemap backfill before embedding in both synchronous and lite-worker paths, while preserving primary crawl embedding when backfill fails.
- crawl: honor cancellation across primary crawl shutdown, sitemap backfill, and dependent embedding so canceled jobs do not enqueue stale embed work.
- crawl: restrict markdown cache reuse to safe `markdown.old` archive paths and reject absolute or traversal paths from previous manifests.
- chunking: share HTML-to-markdown conversion policy across primary crawl, Chrome refetch, and inline CDP render paths.

### Tests

- Added cache-reuse path validation and copied-archive coverage, crawl result JSON output-path coverage, sitemap timeout coverage, and inline Chrome selector-config coverage.

## [1.6.2] - 2026-05-07

### Fixed

- ask: forward per-request `ask_*` overrides through `--server-url`, use a longer server request timeout, and harden bearer-token handling.
- perf: tighten benchmark artifact validation to numeric-only values and require warm-mode benchmarks to use an explicit server URL.
- vector: remove disabled-path timing probes, repair evaluate's disabled ask timing locals, and split Qdrant payload-index setup under the Rust file-size cap.

## [1.6.1] - 2026-05-07

### Fixed

- http: cap shared `HTTP_CLIENT` `pool_max_idle_per_host` to 50 with a 60s idle TTL (bd axon_rust-wo1). reqwest defaults `pool_max_idle_per_host` to `usize::MAX`; under sustained dual-Qdrant + TEI load the idle pool grew unbounded. Audited `crates/vector/ops/commands/ask/context/retrieval.rs:252-352` and confirmed the dual-Qdrant `tokio::join!` fallback has no shared `Mutex`/`Semaphore`/`block_on` and `tei_embed_typed` returns vectors by value pre-join — parallelism is correct by construction.

## [1.6.0] - 2026-05-07

### Added

- ask: adaptive `ask_full_docs` per query complexity (bd axon_rust-721). Reuses the `AskQueryForms.use_dual` signal as a coarse `QueryComplexity{Simple|Complex}` hint. Simple queries default to 2 full-doc fetches; complex (use_dual=true) queries default to 3. The user's explicit override (`AXON_ASK_FULL_DOCS` env var) still wins; tracked via new `Config::ask_full_docs_explicit` field. Diagnostics surface `detected_complexity`, `resolved_full_docs`, and `full_docs_source` (`user_override` / `adaptive_simple` / `adaptive_complex`).

## [1.5.11] - 2026-05-07

### Fixed

- Aligned config docs and comments with runtime behavior for TEI retry attempts and ingest worker lane clamping.

## [1.5.10] - 2026-05-07

### Changed

- Made `Config::default()` pure by moving env/TOML tuning resolution into a shared config path used by `into_config()` and `Config::default_lite()`.
- Hardened config and secret loading: `~/.axon/.env` symlinks and directory/not-directory errors now hard-fail before repo `.env` fallback, and TOML config reads reject directory/not-directory cases and use no-follow opens on Unix.
- Added warnings for malformed numeric env tuning values and MCP artifact fallback diagnostics.

### Tests

- Added boundary clamp coverage for TEI, worker, search, and ask tuning knobs.
- Added integration coverage for the `~/.axon/.env` plus `~/.axon/config.toml` startup pipeline and hard-fail secret-loading cases.

## [1.5.9] - 2026-05-06

### Changed

- Documented the canonical `~/.axon/` layout across all docs and removed `[env-only]` framing from the TOML config reference (epic `axon_rust-2j9`).
- `config.example.toml`: dropped every `[env-only]` marker; every `[services]`, `[search]`, `[ask]`, `[tei]`, and `[workers]` key is now wired through `Config` and takes effect when set, with the env var still overriding (`axon_rust-2j9.4`).
- `.env.example`: documented the auto-load order (`AXON_ENV_FILE` → `~/.axon/.env` → repo-root `.env` ancestor walk; first match wins).
- `docs/guides/configuration.md`: added the canonical `~/.axon/` directory tree, replaced the Phase 1 / Phase 2 / "env-only" tables with a single wired-keys table, refreshed every `$AXON_DATA_DIR/axon/...` default-path example to the flat `$AXON_DATA_DIR/...` form, and added a migration note for users coming from `~/.local/share/axon` (`axon_rust-2j9.5`).
- `CLAUDE.md`, `README.md`, `docs/reference/mcp/overview.md`, `docs/operations/deployment.md`, `docs/operations/operations.md`, `docs/ACP.md`, `docs/reference/mcp/deploy.md`, `docs/reference/mcp/env.md`, `src/core/CLAUDE.md`, `src/jobs/CLAUDE.md`, `src/mcp/CLAUDE.md`: updated stale `~/.local/share/axon` and `$AXON_DATA_DIR/axon/...` references to the flat `~/.axon/` layout.

### Notes

- Companion to the `axon_rust-2j9` epic: bead `2j9.1` added `~/.axon/.env` autoload, `2j9.2` defaulted `AXON_DATA_DIR` to `~/.axon`, `2j9.3` flattened persistent default paths, and `2j9.4` wired every TOML key in `TomlConfig` through `Config`. This bead (`2j9.5`) is documentation only — no `.rs` source changes.

## [1.5.8] - 2026-05-06

### Changed

- Renamed `crates/` module directory to `src/` and moved `lib.rs`/`main.rs`/`crates.rs` into it — standard single-crate Rust layout
- Removed `[lib] path` and `[[bin]] path` overrides from `Cargo.toml`; Cargo now uses the default `src/lib.rs` and `src/main.rs`
- Eliminated the `crates.rs` re-export shim; module declarations now live directly in `src/lib.rs`

## [1.5.7] - 2026-05-06

### Changed

- Docker compose reorganized: `config/docker-compose.services.yaml` replaced by `docker-compose.yaml` at repo root (full stack — axon + qdrant + tei + chrome). Added `config/Dockerfile` (multi-stage Rust builder → debian:bookworm-slim runtime, tini PID 1, UID 1000).
- `.env.example` cleaned up: removed Docker-compose-only interpolation vars (`AXON_LITE`, `HF_TOKEN`, `HOST_HOME`, `AXON_WORKSPACE`, `HOST_WORKSPACE`, `AXON_BIN`, `TEI_*`, `AXON_EXTRACT_EST_COST_PER_1K_TOKENS`); added `SCREENSHOT_DIRECTORY`; fixed `AXON_ACP_AUTO_APPROVE` default comment.
- `mcporter.json`: pin `npm_config_cache` to `~/.cache/axon-mcporter-npm` for mcporter and context7 servers.
- `path.rs`: use `axon_data_base_dir()` helper instead of inline `AXON_DATA_DIR` env var read.
- `plugins/hooks/hooks.json`: fix structure — wrap `SessionStart` inside `hooks` key per plugin spec.

### Fixed

- `size_rotating.rs`: replace `vec![b'x'; N]` with `[b'x'; N]` slice literals (clippy::useless_vec).

## [1.5.6] - 2026-05-06

### Added

- Per-job heartbeat: 30 s tokio ticker bumps `updated_at` on every claimed job via new `crates/jobs/lite/workers/heartbeat.rs` (`HeartbeatGuard` RAII type) and `touch_heartbeat()` helper. Long blocking phases (single-page crawls, mid-batch embeds) no longer leave rows stale.
- Periodic stale-job watchdog: re-runs `reclaim_stale_running_jobs` every 60 s for the worker lifetime, alongside the existing startup-only sweep.
- `CancellationToken` plumbing for crawl, embed, and extract runners (previously only ingest had one). All four runners now respect `cancel_row` cleanly via `tokio::select!`.
- `SizeRotatingFile` writer in `crates/core/logging/size_rotating.rs` — supports `AXON_LOG_MAX_BYTES` (default 10 MiB) and `AXON_LOG_MAX_FILES` (default 3).

### Changed

- Logging now uses size-based rotation instead of daily/7-file. `<dir>/<file>` rolls when the active file exceeds `AXON_LOG_MAX_BYTES`; archives shift `<file>.{N-1} -> <file>.N`. `AXON_LOG_DIR` and `AXON_LOG_FILE` (bare filename) control location.
- `crates/jobs/CLAUDE.md`, `README.md`, `crates/ingest/CLAUDE.md`: replaced fictional "Tier 2 content-aware heartbeat" section (referenced non-existent constants) with accurate description of the new heartbeat + cancellation contract.
- `docs/operations/auth/api-token.md`: full rewrite. Documents the four real axon-side tokens (`AXON_MCP_HTTP_TOKEN`, web panel password, `AXON_ACP_AUTH_TOKEN`, `AXON_ACP_WS_TOKEN`); removes deleted-surface references (`/ws`, `/output/*`, `/download/*`, `AXON_WEB_API_TOKEN`).
- `docs/operations/security.md` §8: lists all three Chrome ports (6000 management, 9222 CDP proxy, 9223 raw DevTools), confirms loopback bind, adds cross-host deployment caveats. `config/docker-compose.services.yaml` annotated with intentional-loopback-bind comment.

### Security

- Bump `openssl` 0.10.78 -> 0.10.79 (and `openssl-sys` 0.9.114 -> 0.9.115) via `cargo update -p openssl` to address [GHSA-xp3w-r5p5-63rr](https://github.com/advisories/GHSA-xp3w-r5p5-63rr) / CVE-2026-42327 (high). `X509Ref::ocsp_responders` could construct a `&str` violating the UTF-8 invariant when a certificate's OCSP accessLocation contained non-UTF-8 bytes, causing undefined behavior. Pulled in transitively via `native-tls` (reqwest, sqlx, tokio-native-tls). No source changes required.

### Changed

- Comprehensive documentation refresh across all crates, docs/, and plugins/ — CLAUDE.md, README.md, and reference docs updated for accuracy and completeness

## [1.5.4] - 2026-05-06

### Added

- Plugin `userConfig` schema for Qdrant URL, TEI URL, collection name, LLM endpoint, API keys (sensitive), Tavily API key (sensitive), and Chrome remote URL — prompts users at enable time
- Plugin `mcp` field in `plugin.json` pointing to `plugins/axon/.mcp.json`
- MCP server entry in `.mcp.json` wires `axon mcp` stdio transport with `${user_config.*}` env var substitution for all service URLs and credentials

## [1.5.3] - 2026-05-06

### Fixed

- Crawl auto-embed silently dropped when embed queue at capacity. Now logs `tracing::error!`, prints `⚠ embed DEFERRED` to stderr, and exposes an `embed_deferred: <reason>` field in the result JSON so CLI/MCP/web callers can detect deferred indexing without parsing logs (PR #67 P1)
- Whole-repo monolith report walked into `target/`, `node_modules/`, `.worktrees/` before filtering — switched to `os.walk` with dirname pruning (PR #67)
- xtask `check-claude-symlinks` recursed into `.worktrees/`, surfacing failures from sibling checkouts. Added `.worktrees` to `SKIP_DIRS` (PR #67)
- xtask `check-mcp-http` matcher strengthened from bare `Both` substring to `McpTransport::Both =>` so comments/docs/unrelated enums cannot satisfy the gate (PR #67)
- xtask `check-unwraps` now treats `tests.rs` as a test filename and counts matching lines (not occurrences), restoring the original `grep -cE` semantics (PR #67)
- xtask `check-env-staged` uses `--diff-filter=AMR` so deletions of accidentally-tracked `.env` files are no longer blocked (PR #67)

### Changed

- Pre-commit `cargo test` scoped to `--lib` and `worker_e2e` skipped; redundant `cargo check` removed (clippy `--all-targets` already type-checks tests). Per-commit budget now under a minute (PR #67)
- ACP session cache exposes `pub(super) insert_with_cap` for testability of the `cap == 0` (unlimited) branch (PR #67)
- `Justfile` `taplo-check` / `taplo-fmt` recipes gated on `command -v taplo` with install hint (PR #67)

### Docs

- README clarified that `AXON_LITE` defaults to `false` (was incorrectly described as the build default) (PR #67)
- `docs/guides/configuration.md` deduplicated `AXON_NO_COLOR` row (PR #67)
- `docs/reference/mcp/env.md` documents unset semantics for `AXON_MCP_EMBED_MAX_LOCAL_BYTES` and `AXON_MCP_ALLOWED_ORIGINS` (PR #67)
- ACP session cache `evict_if_over_cap` doc rewritten to describe actual `min_by_key` tie-breaking (the previous "skips eviction on identical timestamps" claim was incorrect) (PR #67)

## [1.5.2] - 2026-05-06

### Added

- `xtask` workspace member with five enforcement checks ported from shell: `check-no-mod-rs`, `check-mcp-http`, `check-env-staged`, `check-unwraps` (warn-only), `check-claude-symlinks` (axon_rust-pp5.3–pp5.7)
- `taplo` TOML formatter wired into lefthook (staged-file glob) and CI (new `toml-fmt` job) (axon_rust-pp5.8, pp5.9)
- New CI `windows-check` lane validating xtask portability on `windows-latest`: `cargo check -p xtask`, `cargo test -p xtask`, `cargo xtask check-no-mod-rs`, `cargo xtask check-mcp-http` (axon_rust-pp5.9)

### Changed

- `lefthook.yml`: replaced 5 shell hooks with `cargo xtask check-*`; switched cargo gates to `--workspace` so the xtask crate is exercised by clippy/check/test (axon_rust-pp5.8)
- CI `mcp-transport-modes` and `no-mod-rs` jobs now install Rust toolchain + cache and call `cargo xtask`; `check`, `msrv`, `clippy`, `test` jobs upgraded to `--workspace` (axon_rust-pp5.9)
- Updated docs (`docs/contributing/guardrails.md`, `docs/contributing/repo/repo.md`, `docs/contributing/repo/rules.md`, `docs/contributing/repo/scripts.md`) and one source comment in `crates/core/config/parse/build_config.rs` to reference the new xtask commands (axon_rust-pp5.8)

### Removed

- `scripts/check_no_mod_rs.sh`, `scripts/check_mcp_http_only.sh`, `scripts/check_env_staged.sh`, `scripts/warn_new_unwraps.sh`, `scripts/check_claude_symlinks.sh` — superseded by `cargo xtask check-*` (axon_rust-pp5.8)

## [1.5.1] - 2026-05-06

### Fixed

- Negative-count wrap on i64→u64 cast in queue cap check (axon_rust-pkl.10.5)

### Changed

- Queue cap path: cache env vars in LazyLock; warn on unparseable env; introduce JobError::QueueCapacityExceeded domain error; inline four dead-weight wrappers; tighten table_name to &'static str (axon_rust-pkl.10.1-10.7)
- ACP session cache: document concurrent overshoot + O(N) eviction scan rationale; remove redundant cap==0 guard; demote routine eviction log to info; add at-most-one-victim contract test (axon_rust-pkl.11.1-11.4)
- MapResult.mapped_urls renamed to returned_url_count for clarity (JSON wire key preserved via serde rename) (axon_rust-pkl.34.3)

## [1.5.0] - 2026-05-06

### Added

- `--whole-repo` and `--include-allowlisted` flags to `enforce_monoliths.py` for auditing all production files (axon_rust-pkl.7)
- `just monolith-report` recipe and non-blocking informational CI step for whole-repo monolith visibility (axon_rust-pkl.7)
- Tests locking the canonical key set of crawl job result JSON (axon_rust-pkl.8)

### Changed

- Removed legacy aliases `pages_seen` and `markdown_files` from crawl job result JSON; consumers now read canonical `pages_crawled` and `md_created` only (axon_rust-pkl.8)

### Fixed

- CLAUDE.md crawl queue cap section now references the correct file path and function name (axon_rust-pkl.35)

## [1.4.0] - 2026-05-05

### Added

- Queue caps for embed, extract, and ingest jobs (AXON_MAX_PENDING_EMBED_JOBS, AXON_MAX_PENDING_EXTRACT_JOBS, AXON_MAX_PENDING_INGEST_JOBS, default 50)
- Global LRU session cap for ACP session cache (AXON_ACP_MAX_SESSIONS, default 100)
- taplo TOML formatter config and taplo-check/taplo-fmt Justfile recipes

### Changed

- MapResult migrated from serde_json::Value to typed struct with total: u64 field
- docs/guides/configuration.md designated as single authoritative env var reference
- Fixed MCP handle_map double-pagination bug

## [1.3.4] - 2026-05-05

### Fixed

- Fixed fresh-checkout compose path drift by making `config/docker-compose.services.yaml` read the repo-root `services.env`, validating that contract in CI and tests, and refreshing current infra docs to the tracked compose layout.

## [1.3.3] - 2026-05-04

### Fixed

- Carried PR #65 review feedback onto the stacked SSH deployment branch: CI smoke infra starts only services present in the compose file, setup no longer calls removed test-infra compose paths, build/install recipes honor `CARGO_TARGET_DIR`, explicit `AXON_CONFIG_PATH` I/O failures hard-fail, MCP transport guard checks the real resolver wiring, and GPU compose docs no longer reference a removed overlay.

## [1.3.2] - 2026-05-04

### Added

- Added the initial `xtask` workspace scaffold and cargo alias for portable repo checks.

### Changed

- Consolidated collection-name validation across config parsing, MCP, and Qdrant request paths.
- Updated docs and CI to use the tracked `config/docker-compose.services.yaml` infrastructure stack.

### Fixed

- Migrated screenshot service results from raw JSON pass-through to a typed `url`/`path`/`size_bytes` contract.

## [1.3.1] - 2026-05-04

### Fixed

- Hardened SSH remote deployment: `remote_dir` validation, complete compose asset upload, private-by-default service URLs, readiness-gated config writes, bounded SSH/SFTP/compose phases, `AXON_CONFIG_PATH` config writes, and explicit first-use host-key opt-in.
- Hardened MCP HTTP token middleware coverage and updated the smoke script to match the current bearer and `x-api-key` token contract.
- Tightened ACP adapter/path validation, unsupported ACP MCP responses, Axon data-dir fallback behavior, and local infrastructure port bindings.
- Refreshed testing, ACP, MCP, and maintainer docs for the current security and compose behavior.

## [1.3.0] - 2026-05-04

### Added

- Added `axon setup targets` and `axon setup deploy` for SSH-config target discovery and Docker Compose remote infrastructure deployment.
- Added web panel SSH target listing and remote deployment backed by shared `crates/services/setup/` logic.
- Bundled deployment compose/env templates into the binary and wired `[services]` config TOML URLs for remote Qdrant, TEI, and Chrome endpoints.

## [1.2.3] - 2026-05-04

### Fixed

- Rejected `HOME` values containing `..` path components when resolving `~/.axon`, closing the remaining traversal gap in config-home path validation.

## [1.2.2] - 2026-05-04

### Added

- `axon serve` unified web + MCP HTTP server with a bundled static Next.js admin panel.
- Browser-first setup flow that initializes `~/.axon/config.toml`, generates a 256-bit panel password, and exposes authenticated config/ops APIs.

### Fixed

- Hardened `~/.axon` file creation with private permissions, exclusive password creation, and `O_NOFOLLOW` on sensitive pre-create/open paths.
- Added Host header validation to the HTTP server path to block DNS rebinding.

## [1.2.1] - 2026-05-04

### Added

- **TOML config layer**: `~/.axon/config.toml` as a structured tuning-knobs config (safe to commit). 6 Config fields wired with `CLI > env > TOML > default` priority. See `config.example.toml`.
- `axon_home_dir()` / `axon_config_path()` in `crates/core/paths.rs` returning `~/.axon/` and `~/.axon/config.toml` (`None` when HOME is unset, no `/tmp` fallback).
- `env_bool_opt()`, `env_usize_opt()`, `env_f64_opt()` helpers for layered config wiring.
- `config.example.toml` at repo root with annotated Phase 1 fields, `[wired]`/`[env-only]` labels.

### Changed

- **`axon.json` + `axon.schema.json` deleted** — confirmed dormant (never read by the binary). Replace with `~/.axon/config.toml`.
- `build_config.rs` split: 9 helper functions moved to `helpers.rs` (971 → 681 lines).
- `env_bool` / `env_usize_clamped` / `env_f64_clamped` now delegate to their `_opt` variants (single parse path, no logic duplication).
- Malformed `AXON_HYBRID_SEARCH` (or any env bool) emits a warning before falling through to TOML/default.
- PermissionDenied on config file is now a hard fail (not warn+default).
- Docker infra cleanup: removed `docker/s6/` service scripts, CI scripts no longer needed for the current lite-mode stack.
- `docs/guides/configuration.md`, `CLAUDE.md`, `config.example.toml`: two-layer config system documented; wired vs env-only keys distinguished.

### Fixed

- `axon_config_path_env_var_overrides_home` test now acquires ENV_LOCK and saves/restores `AXON_CONFIG_PATH` unconditionally (panic-safe).
- HOME-mutating tests in `paths.rs` now use `#[serial_test::serial]` for crate-wide serialization.
- `check_mcp_http_only.sh` grep satisfied by comment in `build_config.rs` after `resolve_mcp_transport` moved to `helpers.rs`.
- Redundant secondary collection-name validation removed from `into_config()` (validate_collection_name is the authoritative check; secondary was unreachable).
- `AXON_ASK_AUTHORITATIVE_DOMAINS` now uses `parse_csv_env` helper (was inlined).

## [1.2.0] - 2026-05-04

### Added

- **Plugin skills**: 15 Claude Code skills under `plugins/axon/skills/` covering scrape, crawl, map, extract, embed, ingest, query, ask, search, retrieve, sources, domains, stats, status, and the top-level axon skill with full action reference.
- **Plugin agents**: researcher agent scaffold under `plugins/axon/agents/`.
- **Plugin MCP config**: `.mcp.json` added to `plugins/axon/` for MCP server wiring.

### Changed

- **Plugin manifest relocated**: `.claude-plugin/plugin.json` moved from `plugins/axon/` to the repo root `.claude-plugin/`.
- **Monolith splits**: `job_contracts`, `status/metrics`, `crawl/collector`, `crawl/map`, `ingest/github/files`, `jobs/lite/ops`, and `jobs/lite/workers/runners` each split into focused submodule files to comply with the 500-line file policy.

## [1.1.0] - 2026-05-03

### Added

- **Tracing and progress bundle**: lite job workers now persist richer progress snapshots, CLI status prints per-job progress summaries, and ingest/embed flows expose more detailed runtime metrics.
- **MCP plugin scaffold**: added the Axon Claude plugin package scaffold under `plugins/axon`.

### Changed

- **Operational docs and config**: updated MCP, command, ingest, and config references for the new observability and transport behavior.

### Fixed

- **MCP HTTP startup guard**: HTTP server startup now enforces the token policy before binding externally.

## [1.0.13] - 2026-05-03

### Changed

- **Retrieval dispatch contract**: query and ask now build typed `VectorSearchRequest` values, pass ask-specific hybrid candidate overrides without cloning `Config`, and keep dispatch/facet/retrieve/dedupe code in focused modules.
- **Typed embedding calls**: TEI embedding call sites now declare query vs document embedding intent with `EmbedKind`, preventing query-instruction omissions on new retrieval paths.

### Fixed

- **Ask context selection**: top chunk and full-document selections now use disjoint URL sets so the two diversity passes cannot select the same source twice.
- **Live Qdrant testing**: added a `live-qdrant` feature and CI job so live vector integration tests fail loudly when Qdrant configuration is expected but missing.

## [1.0.12] - 2026-05-03

### Fixed

- **VectorMode cache revalidation**: cached legacy `Unnamed` collection modes now re-probe live Qdrant when hybrid search is enabled, so long-running workers self-heal after migration instead of staying on dense-only paths until restart.

## [1.0.11] - 2026-05-03

### Fixed

- **Ask RRF rerank scale**: ask retrieval now skips cosine-calibrated rerank thresholds and additive BM25-style boosts only on the effective RRF path, while preserving cosine behavior for legacy, named-dense, and empty-sparse fallback searches.
- **RRF supplemental context**: supplemental ask candidates now use an optional score floor so RRF context backfill is gated by topical overlap and context budget instead of cosine-scale thresholds.

## [1.0.10] - 2026-05-02

### Fixed

- **Lite job replay review fixes**: versioned lite job snapshots now exactly replay submitted `None` option fields, preserve job-critical custom headers, Chrome proxy, and ACP adapter args, and omit credential-bearing endpoint URLs from public `config_json` while falling back to process config for those endpoints.
- **Monolith allowlist tracking**: the current extraction-sprint allowlist extension now references its review follow-up bead so the policy waiver remains auditable.

## [1.0.9] - 2026-05-02

### Removed

- **Ask authoritative allowlist**: removed the `AXON_ASK_AUTHORITATIVE_ALLOWLIST` configuration knob and strict retrieval/citation allowlist behavior. Authoritative domains and boost remain as reranking signals only, so `ask` and `query` no longer drop candidates through an ask-only allowlist.

## [1.0.8] - 2026-05-02

### Changed

- **RAG service contracts**: query, retrieve, ask, and evaluate service results now use typed structs at service boundaries with JSON serialization deferred to CLI/MCP entrypoints.
- **Shared RAG retrieval**: query and ask now share candidate construction, dedupe, scoring, and filtering helpers while preserving query-specific threshold behavior.
- **Lite job replay**: persisted lite job rows now carry non-secret config snapshots, and workers reconstruct effective crawl/embed/extract/ingest config from row data instead of relying on process defaults.

## [1.0.7] - 2026-04-30

### Added

- **`render_full_doc_filtered`**: optional `(query_tokens, top_k)` parameters score each chunk by query-token overlap, keep top-K, then re-sort by `chunk_index` for narrative coherence. Used by ask context build with `FULL_DOC_RENDER_TOP_K=24`.

### Changed

- **Ask context flattening**: `context_entries: Vec<(f64, String)>` and a final sort by `rerank_score` descending so the highest-scoring chunks appear first regardless of which bucket (top-chunks / full-docs / supplemental) they came from. Mitigates LLM proximity bias against the most relevant content.

## [1.0.6] - 2026-04-30

### Added

- **MCP per-request `hybrid_search` override**: `QueryRequest` and `AskRequest` now accept `hybrid_search: Option<bool>` to override `cfg.hybrid_search_enabled` per call (A/B comparison without restart).
- **Lite drain tests**: 3 tests covering `has_active_jobs` per-kind isolation, terminal-state transition, and bounded-time drain in the presence of unrelated pending rows.

### Changed

- **`build_scraped_at_filter`**: process-level `LazyLock<RwLock<HashMap>>` memoizes parsed `--since`/`--before` strings so dual-embed asks no longer re-parse chrono twice per question.
- **Hybrid search hot-path bodies**: replaced `serde_json::json!{...}` with typed `Serialize` structs (`HybridQueryBody`, `NamedDenseQueryBody`, `PrefetchArm`, `DenseParams`, `QuantizationParams`, `FusionSpec`). Eliminates per-request Map allocations.

## [1.0.5] - 2026-04-30

### Added

- **Score-distribution telemetry**: `vector.dispatch` tracing event now carries `top1_score` and `top10_avg_score` per arm so operators can detect threshold no-op (top-1 below `ask_min_relevance_score`) and arm-scale divergence (cosine vs RRF magnitudes).
- **`score_ask_candidates`**: ranks candidates without cloning, returning `(idx, score)` pairs sorted descending. Caller filters by threshold first; only survivors are cloned.

### Changed

- **`retrieve_ask_candidates`**: now scores → filters → clones (was clone-all → filter), avoiding ~1 MB of throwaway clones per ask. `compute_scored_indices` extracted as the shared inner loop between `score_` and `rerank_`.

## [1.0.4] - 2026-04-30

### Added

- **`validate_custom_headers`**: rejects malformed `--header K: V` entries (missing separator, empty name, RFC 7230 illegal token chars in name, CR/LF in value).

### Changed

- **Ask error-path diagnostics**: `dispatch_error` now always attaches `{stage, collection, qdrant_url, query_len, error}` JSON to failed retrieval errors. `cfg.ask_diagnostics` still gates verbose **success-path** payloads.

## [1.0.3] - 2026-04-30

### Added

- **`#[tracing::instrument]`** on retrieval hot path: `dispatch_vector_search`, `qdrant_hybrid_search`, `qdrant_named_dense_search`, `retrieve_ask_candidates`. Spans carry collection name, query length, sparse term count, candidate window, and filter presence.
- **Sparse term cap**: `MAX_TERMS_PER_VECTOR = 65,536` in `compute_sparse_vector` defends against pathological inputs.
- **Tests**: 5 unit tests for `merge_candidates` covering primary dedupe, cross-URL chunk parity, multibyte chunk-prefix boundary, empty inputs.
- **Vector docs**: env vars table (`AXON_HYBRID_SEARCH`, `AXON_HYBRID_CANDIDATES`, `AXON_ASK_MIN_RELEVANCE_SCORE`), Dual-Embedding for Ask section, Operational Caveats section (cache staleness, sparse fallback, threshold no-op, empty-return contract).

### Changed

- **`SparseVector`**: derives `serde::Serialize` and emits the Qdrant wire shape directly. Removed `to_json()`. Updated 4 call sites (`hybrid.rs`, `tei.rs`, `tei/pipeline.rs`, `services/migrate.rs`).
- **`COLLECTION_MODES` cache**: `OnceLock<RwLock<HashMap>>` → `LazyLock<RwLock<HashMap>>`. One fewer `Option` layer on every cache hit; `cache_vector_mode_key` no longer needs `get_or_init`.

## [1.0.2] - 2026-04-30

### Changed

- **`merge_candidates`**: Dedupes primary internally before merging secondary; a single chunk landing at slightly different RRF positions across prefetch arms no longer leaks duplicates into the ask context.
- **`compute_sparse_vector`**: Empty-result log promoted from `log_debug` → `tracing::warn!` with query character profile (`len`, `ascii_alnum`, `non_ascii`, `whitespace`, `other`); operators now see hybrid → dense-only fallback at default INFO.

### Docs

- **vector/CLAUDE.md Ranking Pipeline**: documented the score-scale mismatch — `ask_min_relevance_score` and `ask_authoritative_boost` are cosine-calibrated and don't transfer cleanly to RRF output.
- **vector/CLAUDE.md Query Instruction**: documented dual-embedding asymmetry (NL form gets `QUERY_INSTRUCTION`, keyword form does not).

## [1.0.1] - 2026-04-30

### Added

- **Observability**: Tracing logs across lite worker spawn, watchdog sweep, ACP session lifecycle, persistent-conn turn, replay buffer cap, MCP capability filter, AdapterGuard kill, ACP CWD validation, AXON_ACP_AUTH_TOKEN missing path, dispatch arm + per-arm latency in `dispatch_vector_search`.
- **Doctor**: Lite doctor probes Qdrant collection vector mode and warns when `unnamed` collection is paired with `hybrid_search_enabled=true` (silent dense-only fallback).
- **Stats**: SQLite-backed metrics (counts/durations/freshness/totals/longest crawl/most chunks) replace the no-op placeholder so `axon stats` populates Pipeline Stats / Freshness in lite mode.
- **Queue summary**: `spawn_queue_summary_logger` emits a periodic queue-depth event (env-gated `AXON_QUEUE_SUMMARY_SECS`, default 60s) from `new_with_workers` contexts.
- **SQLite retry**: `retry_busy()` helper retries `claim_next_pending`, `mark_completed`, `mark_failed` on transient lock contention with bounded exponential backoff.
- **Drain visibility**: `WorkerMode::InProcess` now carries `pending_at_start` + `elapsed_secs`; CLI prints both on completion.
- **Collection name guard**: `validate_collection_name()` rejects path-traversal / URL-injection in `cfg.collection` at dispatch entry.

### Changed

- **Lite crawl runner**: `result_json` now includes the field names the CLI status display reads (`pages_crawled`, `md_created`, `pages_discovered`, `thin_md`, `error_pages`, `waf_blocked_pages`).
- **ACP unsupported model warning**: Deduped per-process via `LazyLock<Mutex<HashSet>>`; warning now lists the adapter's available model options.
- **ACP fallback JSON parse**: Strips ```json fences and leading prose before `serde_json::from_str`; system prompt tightened to demand bare JSON output.
- **`extract --wait true`**: Returns non-zero exit when 0 items extracted across all URLs.
- **`suggest`**: Filters out malformed URLs (rejects single-label hosts like `https://next.js/`).
- **`load_status_jobs`**: Replaced `unwrap_or(0)` on `count_jobs` with `unwrap_or_else` that logs a tracing::warn! per JobKind.

### Docs

- Documented `LiteBackend::new()` (enqueue-only) vs `new_with_workers()` (spawns workers) in `crates/services/CLAUDE.md` and `docs/guides/configuration.md`.
- Removed stale `refresh` references from `crates/cli/CLAUDE.md` and `crates/mcp/CLAUDE.md` (refresh was deleted in commit 05da3b44).
- Added an inline rationale block to `main.rs` for the 8 MB Tokio worker stack.

## [0.35.1] - 2026-04-04

### Changed

- **Structured documentation**: Reorganized project documentation for clarity and consistency.

## [0.35.0] - 2026-04-03

### Fixed

- **OAuth discovery 401 cascade**: BearerAuthMiddleware was blocking GET /.well-known/oauth-protected-resource, causing MCP clients to surface generic "unknown error". Added WellKnownMiddleware (RFC 9728) to return resource metadata.

### Added

- **docs/AUTHENTICATION.md**: New setup guide covering token generation and client config.
- **README Authentication section**: Added quick-start examples and link to full guide.

Last Modified: 2026-03-31 (session: v0.34.1 — simplification pass: dedup helpers, fix service-context abstractions)

## [0.34.1] — simplification pass

### Highlights

- **Shared helpers extracted** — `read_env()` and `resolve_service_url()` deduplicated from `build_config.rs`; `query_wants_low_signal_sources()` extracted to `ranking.rs`; `is_low_signal_source_url()` wrapper deleted; `service_select_from()` SQL fragment removes 45 lines of copy-pasted queries in `lite/query.rs`.
- **ServiceContext construction unified** — `new()` and `new_with_workers()` share a private `build()` helper; `wait_for_pending_embed_jobs` uses `has_active_jobs()` (single EXISTS query) instead of fetching 50 rows per poll tick.
- **MCP Apps fixes** — redundant flat `"ui/resourceUri"` meta key removed; `serde_json::from_value(...).unwrap_or_default()` in capabilities replaced with direct Map construction; `ingest_count()` call that bypassed `ServiceContext` in lite mode removed.
- **Graph error types fixed** — `Box<dyn Error + Send + Sync>` propagated through graph/context, graph/schema, graph/worker — explicit `as Box<dyn Error>` casts eliminated.
- **lift_err / SQL / RAII cleanup** — `runners.rs` 13 verbose `.map_err` closures replaced with existing `lift_err`; `reclaim_stale_running_jobs` calls per-table helper; `watch.rs` opens pool once per operation instead of per sub-call.
- **doctor/lite dedup** — `build_browser_runtime` made `pub(super)` and shared with `doctor/lite.rs`; duplicate function deleted.

### Commits since v0.34.0

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | refactor | simplification pass — dedup helpers, fix service-context abstractions |
| 69700b9c | feat | WAF diagnostics, enqueue-only LiteBackend, serve preflight auto-terminate |

## [0.34.0] — WAF diagnostics + LiteBackend split

### Highlights

- **WAF diagnostics** — `WafDiagnostics` struct and `build_waf_diagnostics()` added to `engine.rs`; wired into crawl result builder and `crawl_sync`; surfaced in web UI job detail page (WAF Recovery section + remaining URLs list).
- **Enqueue-only `LiteBackend`** — `LiteBackend::new()` now creates an enqueue-only backend (no workers); `new_with_workers()` starts in-process workers. Prevents unnecessary worker startup for fire-and-forget CLI commands. `ServiceContext` gains `new_without_workers()` variant.
- **serve preflight auto-terminate** — `classify_nextjs_lock_state()` extracts lock state logic; active Next.js dev processes are now auto-terminated instead of erroring when a stale lock is found.
- **`jobs-models.ts` / job detail UI** — `WafDiagnostics` TypeScript interface; WAF Recovery section and remaining URLs list rendered in job detail view.

### Commits since v0.33.10

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | feat | WAF diagnostics, enqueue-only LiteBackend, serve preflight auto-terminate |
| c096677a | fix | address PR comments #1-2 - fix lite worker shutdown and changelog header |

## [0.33.10] — infra/gpu + CDI

### Highlights

- **GPU inlined into `docker-compose.services.yaml`** — `axon-tei` and `axon-ollama` now have `deploy.resources.reservations.devices` directly; no more `-f docker-compose.gpu.yaml` overlay needed.
- **`docker-compose.gpu.yaml` removed** — `just services-up` starts the full GPU-enabled stack with no extra flags.
- **CDI deadlock fix** — `nvidia-cdi-refresh.service`/`.path` masked; Docker `daemon.json` CDI feature flag removed; `nvidia-container-runtime` set to `legacy` mode. Eliminates `uvm_gpu_retain_by_uuid` deadlock on RTX 4070 + driver 590 + kernel 6.17.

### Commits since v0.33.9

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | chore | inline GPU into services compose; remove gpu overlay; fix CDI deadlock |

## [0.33.9] — feat/lite-mode

### Highlights

- **`lift_err`/`lift_ss` deduplicated** — shared error-lifting helper moved to `backend.rs` as `pub(crate) fn lift_err`; `full.rs` imports it directly, `runtime.rs` aliases as `lift_ss` preserving all downstream call sites.
- **`JobStatus::from_str` warns on corruption** — unknown DB status values now emit `tracing::warn!` instead of silently mapping to `Failed`, surfacing schema drift and corrupt rows in logs.
- **`wait_for_job` deadline check fixed** — moved deadline check to before the sleep, eliminating the up-to-500ms overshoot where the loop ran one extra poll after timeout expired.
- **`WorkerHandles::drop` aborts tasks** — added `Drop` impl that aborts all supervisor task handles so lite-mode background workers stop cleanly when `LiteBackend` is dropped (e.g. end of one-shot `axon scrape`).
- **Documentation fixes** — `list_ingest_jobs` default warns about post-filter correctness limitation; `from_summary` explains `updated_at = created_at` approximation.

### Commits since v0.33.8

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | refactor | PR #60 simplification — dedup lift_err, status warn, worker drop, deadline fix |
| be1eb00f | merge | integrate main into feat/lite-mode (resolve 36 conflicts) |
| 303a82ee | fix(mcp) | strip URL from internal_error on refresh.start partial failure |
| 3213b0ef | fix | address 11 PR #60 review findings |

## [0.33.8] — feat/lite-mode

### Highlights

- **Services-first contract complete** — dead `extract_status_raw`/`extract_list_raw` raw-backend functions removed from `services/extract.rs`; all callers use `ServiceContext.jobs` via the `ServiceJobRuntime` trait.
- **`FullServiceRuntime` cleanup** — inline `kind.table_name()` match replaced with `kind.table_name()` direct call; all `|e| e.to_string().into()` error maps normalized to `lift_ss`.

### Commits since v0.33.7

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | chore | remove dead extract raw fns; normalize lift_ss in FullServiceRuntime |
| fcc7ba5b | refactor | complete backend unification and services-first contract |

## [0.33.7] — feat/lite-mode

### Highlights

- **`LiteServiceRuntime::cancel_job` bug fixed** — now fires `CancellationToken` via `CancelStore::cancel()` so in-process workers are actually interrupted when cancel arrives through the service layer.
- **FullBackend Graph ops wired** — `enqueue`/`cancel`/`cleanup`/`clear` for `JobKind::Graph` no longer return runtime errors; routed to real Postgres functions.
- **Refresh lite-mode hardening** — 9 refresh service functions now return a clean error in lite mode instead of crashing on a missing Postgres connection.
- **Sync paths through services layer** — `migrate.rs` CLI 375→37 lines (new `services::migrate`), `sync_crawl.rs` 428→10 lines (new `services::crawl_sync`), `extract.rs` sync path wrapped in `extract_sync()` service function.
- **`LiteBackend::table_for` removed** — dead one-line wrapper replaced with direct `kind.table_name()` calls.
- **`ServiceJobRuntime` documented as canonical abstraction** — doc comments and both CLAUDE.md files updated to clarify that `JobBackend` is a 3-method delegate, not the real contract.

### Commits since v0.33.6

| SHA | Type | Description |
|-----|------|-------------|
| fcc7ba5b | refactor | complete backend unification and services-first contract |

## [0.33.6] — fix/mcporter-test-parity

### Highlights

- **mcporter smoke suite 152/152 pass** — all 12 previously failing cases resolved across four categories: help route parity, lite export guard, graph unavailable message wording, and refresh-schedule mode conditioning.
- **`refresh_schedule` exposed in help** — added `"refresh_schedule": ["create","delete","disable","enable","list"]` key to MCP help action map so `normalize_discovered_routes` matches the expected routes array.
- **Lite export guard** — `handle_export` now returns `invalid_params` with a descriptive message instead of a generic `-32603` error in lite mode.
- **Graph unavailable message corrected** — lite mode graph error message updated to match `run_error_case` expected substring.
- **Refresh schedule tests mode-conditioned** — test cases now use `run_error_case` in lite mode (consistent with export/graph handling) instead of expecting success.

### Commits since v0.33.5

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix | fix mcporter test suite — help routes, export guard, graph message, schedule conditioning |
| 5265c675 | refactor | remove prompt_clone redundancy; use from_runtime in tests |
| cd505952 | fix(test) | use :memory: sqlite in lite mode crawl service tests |

## [0.33.5] — fix/lite-mode-mcp-and-review-fixes

### Highlights

- **MCP handlers wired to service context** — crawl, extract, embed, and ingest MCP start handlers now call `_with_context` variants, routing through the shared `ServiceContext` so lite mode is respected end-to-end.
- **`crawl_start_with_context` early-return fix** — when `cfg.wait` is false the function now returns immediately after enqueueing instead of falling through to the blocking wait loop.
- **`embed_start_with_context` early-return fix** — same fix; non-blocking enqueue path now returns before calling `wait_for_embed_completion`.
- **PR #60 review batch complete (rs8.1–rs8.9)** — double pool open, SQLite PRAGMA application, migration 0003 data safety, monolith splits, Graph op errors, list_jobs pagination, MCP lite guards, and `Box<dyn Error>` Send+Sync boundaries all addressed and committed.

### Commits since v0.33.4

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix | wire MCP start handlers to service context; fix crawl/embed early-return |
| 4f6f5fd3 | refactor(rs8.4) | split oversized runtime.rs and workers.rs |
| 71ac0393 | fix(rs8.8) | fix Box<dyn Error> vs Box<dyn Error+Send+Sync> boundaries |
| c7ad4dc9 | fix(rs8.7) | add lite mode guards to graph and refresh MCP handlers |
| 48f5796e | fix(rs8.6) | fix hardcoded LIMIT 500 in lite list_jobs |
| e084b4e6 | fix(rs8.1) | remove double pool open in resolve_runtime |
| 0909b192 | fix(rs8.5+rs8.8) | fix Graph ops in FullBackend and update test mock bounds |
| 73d6ea11 | fix(rs8.3) | wrap migration 0003 in transaction and preserve unknown-status rows |
| 7b751c43 | fix(rs8.2) | apply SQLite PRAGMAs via SqliteConnectOptions |
| 438f9f7c | fix(ui+lite) | split shell/job detail state further and harden lite worker follow-ups |

## [0.33.4] — fix/shell-state-and-lite-followups

### Highlights

- **Shell state split continued** — Pulse shell connection, settings, and layout state now expose focused state/action bundles, reducing the monolithic `axon-shell-state.ts` surface and keeping the job detail UI split aligned with the extracted component module.
- **Lite worker hardening** — SQLite store setup now creates parent directories asynchronously, applies a `busy_timeout`, logs failed completion/failure transitions, and reconstructs ingest work from `config_json` instead of trusting stale denormalized columns.
- **CLI job JSON resilience** — `common_jobs.rs` no longer requires `Serialize` on status handlers and now emits explicit JSON error payloads instead of silently defaulting to `{}` when serialization fails.
- **PR follow-up batch landed** — the remaining lite-mode, refresh, screenshot, supervisor, watch, config, and diagnostics review threads from PR `#60` were fixed and resolved in four incremental commits.
- **Local tool artifact hygiene** — `.gitignore` now covers Beads/Dolt-generated local files so repo-local helper state stays out of commits by default.

### Commits since v0.33.3

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix(ui+lite) | split shell/job detail state further and harden lite worker follow-ups |
| 3d24c15e | fix | address remaining PR review follow-ups |
| 4cfd173c | fix | address PR comments #13-20 |
| 98005219 | fix | address PR comments #3-12 |
| 9c3751c6 | fix | address PR comments #1-2 - harden lite claims and sessions ingest |

## [0.33.3] — refactor/services-runtime-cutover

### Highlights

- **Shared service runtime cutover completed** — CLI remained on a single process-scoped `ServiceContext`, while MCP and web now route lifecycle/status calls through shared service-runtime plumbing instead of raw config-only paths.
- **MCP lifecycle handlers unified** — crawl, extract, embed, ingest, refresh, and status handlers now resolve through `ServiceContext` and the runtime-backed job services layer.
- **Web execution paths unified** — WebSocket cancel and sync status flows now reuse shared runtime context instead of reconstructing job backends per request.
- **Runtime defect cleanup** — `ServiceJobRuntime` now computes active jobs without nonexistent backend methods, and full-mode worker launch avoids non-`Send` future failures.
- **Test call-site cleanup** — graph/watch CLI tests and web sync-mode/context tests now match the new service-context signatures so the full test target compiles again.

### Commits since v0.33.2

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | refactor(services) | finish shared runtime cutover across CLI, MCP, and web |

## [0.33.2] — fix/embed-json-contract

### Highlights

- **Embed JSON contract cleanup** — `axon embed --json` now emits one machine-readable JSON object on `stdout`; human progress output moved off the JSON channel.
- **Stable embed metadata** — `axon embed status --json` now exposes top-level `collection`, `target`, and `source` fields so automation does not need to scrape nested blobs to find collection metadata.
- **Lite-mode embed parity** — lite embed workers no longer leak a second JSON payload during in-process completion; the start and status paths now present one coherent contract.
- **Warning cleanup in touched tests** — warning-only `.unwrap()` / `.expect()` additions were removed from the affected CLI, services, MCP config, watch-lite, and refresh-schedule test modules.
- **Embed command docs corrected** — `docs/reference/commands/embed.md` now documents the fixed local CLI JSON shape instead of the old leaked behavior.

### Commits since v0.33.1

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix(cli) | normalize embed JSON output contract and remove warning-only unwraps |

## [0.33.1] — fix/mcp-smoke-dual-mode

### Highlights

- **Dual-mode mcporter smoke harness** — `scripts/test-mcp-tools-mcporter.sh` now generates suite-local mcporter configs and validates the MCP surface in both full mode (`AXON_LITE=0`) and lite mode (`AXON_LITE=1`), including schema/help parity checks and route-level smoke coverage.
- **Lite/full contract enforcement** — lite smoke explicitly expects `export` and `graph:*` to be unavailable, while full smoke requires successful coverage for the full routed surface.
- **mcporter runtime normalization** — `config/mcporter.json` now uses the normalized local server name `axon`, stdio transport, and repo-local runtime paths for logs and SQLite state.
- **MCP config/help fixes** — `crates/mcp/config.rs` loads lite and graph env correctly; `action:help` now advertises `graph` and `refresh_schedule`, keeping the server-reported contract aligned with mcporter discovery.
- **Screenshot/export smoke stability** — screenshot capture now succeeds through MCP, and export tolerates missing watch history tables instead of failing hard.

### Commits since v0.33.0

| SHA | Type | Description |
|-----|------|-------------|
| 3fc64858 | feat(lite+retrieval) | lite mode backend + BM42/query retrieval improvements |

## [0.33.0] — feat/lite-mode

### Highlights

- **Lite mode** — `AXON_LITE=1` activates SQLite backend + in-process workers; no Postgres/AMQP/Redis required. `JobBackend` trait abstracts `FullBackend` (Postgres/AMQP) and `LiteBackend` (SQLite/in-process). `axon doctor` reports SQLite status in lite mode.
- **BM42 log-normalized TF** — `sparse.rs`: switch from raw term frequency to `ln(1 + count)`, preventing high-repetition documents from dominating sparse search regardless of term content. Mirrors BM25 TF saturation.
- **Low-signal URL filtering** — `ranking.rs`: `is_low_signal_url()` extended to catch `file://` URLs and `.jsonl` session exports; applied to both `query` and `ask` command paths via shared function.
- **Larger candidate pool** — `query.rs`: fetch_limit raised from 8x/500 to 16x/1000, giving reranker more candidates before selecting top results.
- **Title/URL-prepended embeddings** — `tei/pipeline.rs`: each chunk embedded as `[title] url\n\nchunk`, anchoring dense vectors to document identity. Payload still stores raw chunk text.

### Commits since v0.32.2

| SHA | Type | Description |
|-----|------|-------------|
| 81283de0 | feat(lite) | end-to-end smoke test + monolith compliance check |
| 5b7d0664 | feat(lite) | doctor checks SQLite in lite mode, update .env.example |
| 10dc66a1 | feat | wire Arc<dyn JobBackend> through lib.rs and async command handlers |
| cdf69b35 | feat(jobs/full) | FullBackend adapter wrapping existing Postgres/AMQP job functions |
| 7768b5f3 | feat(jobs/lite) | LiteBackend struct implementing JobBackend trait |

## [0.33.2] — refactor/cleanup

### Highlights

- **`print_list_footer` helper** — Extracted duplicated "Showing X of Y total" pagination footer into a shared `print_list_footer(shown, total, limit, offset)` function in `common.rs`. Removes ~60 lines of repeated code from crawl, embed, and ingest list handlers.
- **`filter_jobs_for_status_view` takes `&[T]`** — Signature changed from `Vec<T>` to `&[T]` + returns cloned slice, eliminating unnecessary `.clone()` calls at every call site.
- **PR #59 review fixes** — Two-pass cleanup addressing all 39+35 review threads: heartbeat kill threshold, test isolation for env-var races, stuck/dead job liveness docs.

### Commits since v0.33.1

| SHA | Type | Description |
|-----|------|-------------|
| 4e2e39d4 | fix(review) | address remaining 35 PR #59 review threads (second pass) |
| 86672db6 | fix(review) | address PR #59 review comments — all 39 threads |
| 7e67aa91 | docs | document two-tier liveness enforcement for stuck/dead job detection |
| dac0f14d | feat(jobs) | heartbeat kill threshold — cancel stuck jobs after 10min no progress via CancellationToken |
| 3d3d6ed0 | fix(tests) | isolate parse/build_config env var races — use CLI flags for QDRANT+TEI in parse tests |

## [0.33.1] — chore/cleanup

### Highlights

- **Sessions ingestion refactor** — `crates/ingest/sessions.rs` extracts shared `SessionDoc`, `SessionStateTracker`, `flatten_session_result`, `matches_project_filter`, `resolve_collection` abstractions; `claude.rs`, `codex.rs`, `gemini.rs` updated to use them. Reduces duplication across all three session ingestion paths.
- **ACP warm session in ask/evaluate/suggest/debug** — `run_streaming_completion` (renamed from `run_acp_streaming_completion`) now accepts a `WarmAcpSession` pre-warmed path. Cold-start overlap with upstream I/O reduces first-token latency. `WarmAcpSession::complete_text` added as non-streaming convenience.
- **Streaming refactor** — `StreamProcessorState` struct + `process_one_delta` extracted from inline streaming loop; reduces function complexity and enables warm/cold path branching.
- **gitignore** — `specs/.current-spec` and `**/.progress.md` added to suppress Ralph Specum transient files.

### Commits since v0.33.0

| SHA | Type | Description |
|-----|------|-------------|
| (this commit) | refactor | sessions ingestion refactor; ACP warm session path for ask/evaluate/suggest/debug |

## [0.33.0] — chore/cleanup

### Highlights

- **ACP MCP: SSE transport** — `AcpMcpServerConfig` gains `Sse { name, url, headers }` variant; `convert_mcp_servers` correctly maps to `McpServer::Sse` via `McpServerSse::new`. Previously SSE configs were silently dropped.
- **ACP MCP: HTTP headers** — `Http` variant gains `headers: Vec<(String,String)>`; headers forwarded via `HttpHeader::new` on session setup. Auth headers no longer silently lost.
- **ACP MCP: Capability gating** — `McpCapabilities` read from `InitializeResponse` after adapter init; `filter_sdk_mcp_servers` drops Http/Sse servers when adapter doesn't advertise support. Prevents adapter rejection of unknown transports.
- **ACP MCP: Fallback preservation** — Load-session fallback in both one-shot (`runtime.rs`) and persistent-conn (`turn.rs`) paths now correctly clones and threads MCP servers through to the fallback `NewSessionRequest`. Previously servers were lost on fallback.
- **mcp.json disk format** — `transport: "sse"` and `headers: [{name, value}]` fields now parsed from disk; unknown transport strings warn + fall back to Http.

### Commits since v0.32.4

| SHA | Type | Description |
|-----|------|-------------|
| 1549a85a | fix(acp) | expose spawn_adapter_skip_validation to integration tests |
| 0e299364 | docs(acp) | update gap analysis with full MCP support status |
| c8c5252e | fix(mcp) | warn on unknown transport in mcp.json disk loader |
| 0baee292 | feat(mcp) | support SSE transport and HTTP headers in mcp.json disk loader |
| d5d92c0e | fix(acp) | apply capability filter to per-turn MCP servers in persistent-conn path |
| 0b508e91 | fix(acp) | pass MCP servers through load_session and create_new_session in persistent mode |
| da582688 | chore(acp) | move INVARIANT comment; rename misleading test in session.rs |
| cece1794 | fix(acp) | preserve MCP servers on load-session fallback in one-shot path |
| dd5945f2 | chore(acp) | clarify dead_code annotation on filter_compatible_mcp_servers |
| 48b17f72 | feat(acp) | read McpCapabilities from InitializeResponse; filter unsupported MCP transports |
| bb4cc39c | fix(acp) | drop unknown MCP transports; add SDK SSE filter tests; assert header value |
| 67592aaf | feat(acp) | implement Sse + headers in convert_mcp_servers; add filter_compatible/sdk_mcp_servers |
| 8e9fe833 | fix(acp) | warn on SSE stub and dropped headers in mapping.rs |
| f8f3382a | feat(acp) | add Sse variant and headers to AcpMcpServerConfig |

## [0.32.3] — chore/cleanup

### Highlights

- **Security** — Debug auth bypass now requires explicit `AXON_WEB_ALLOW_INSECURE_DEV=true` (no longer implicit on missing token); startup warning emitted; shell PTY sessions emit structured audit log (session_id, duration_ms); `clear_collection_mode_cache()` called in migrate handler so workers pick up correct VectorMode without restart.
- **Performance** — Graph worker Stage 1 writes parallelized with `tokio::join!()` (latent entity-before-relationship bug fixed); embed worker shares Redis connection at startup via `Arc<Mutex>` (no more per-job TCP handshakes); progress Postgres UPDATEs debounced to 500ms; TEI env-var reads cached in `LazyLock` (eliminates per-batch global process lock); Qdrant retry jitter added to all 4 retry sites (prevents thundering herd).
- **CI/CD** — All CI jobs standardized to `rust-toolchain.toml` pin (`1.94.0`); weekly scheduled run for `#[ignore]` AMQP infra tests; `cargo-audit`/`cargo-deny` switched to `taiki-e/install-action` prebuilt binaries; Renovate `regexManagers` for 4 Dockerfile ARG binary versions; `services.env` protected by pre-commit env guard.
- **Code quality** — `open_amqp_channel()` deprecated + `pub(crate)`; `#[must_use]` on 26 public service entry-points; `thiserror` derive on `PayloadParseError`; named exports standardized in 2 TSX components; `criterion` dev-dep removed; `f64::EPSILON as f32` → `f32::EPSILON` in similarity test.
- **Testing** — 5 SQL safety tests (`JobTable`/`JobStatus` value validation); 2 collection mode cache tests; 2 auth bypass tests; CI schedule for infra tests.
- **Docs** — `docs/spider-feature-flags.md` corrected (`glob` removed, `hedge` documented, version updated); `.env.example` `AXON_COLLECTION` unified to `cortex`; `docs/operations/security.md` and `docs/operations/auth/api-token.md` updated; ACP session cache constants labeled in `docs/ACP.md`.

### Commits since v0.32.2

| SHA | Type | Description |
|-----|------|-------------|
| (pending) | chore | comprehensive review fixes: security, performance, CI/CD, quality (v0.32.3) |

## [0.32.2] — main

### Highlights

- **Graph similarity fix** — `similarity.rs`: adds `"using": "dense"` to Qdrant recommend requests (required for named-vector collections); replaces `.error_for_status()?` with explicit status check that logs a warning and returns empty results instead of propagating errors.

### Commits since v0.32.1

| SHA | Type | Description |
|-----|------|-------------|
| b118e0c1 | fix | graph: named-vector support and error resilience in compute_similarity |
| c90022bf | chore | bump version 0.32.0 → 0.32.1, add config files, update changelog |

## [0.32.1] — feat/pulse-shell-and-hybrid-search

### Highlights

- **Jobs infrastructure fixes** — `extract.rs` P1: removed process-wide `OnceLock` for schema init (multi-DB safety); `watchdog.rs`: `batch_mark_candidates` now reports `rows_affected()` + docstring corrected; `amqp.rs`: reused-channel publish errors fail fast to prevent job duplication.
- **Crawl & ingest fixes** — `collector.rs` P1: missing cache file now triggers fresh write instead of silent page drop; `dir_ops.rs`: `Semaphore(32)` caps concurrent `spawn_blocking` file copies; `reddit.rs`: drain task join errors propagated instead of `unwrap_or(0)`.
- **Services & health fixes** — `doctor.rs`: `build_client` failure handled per-probe, no longer aborts entire doctor report; `taxonomy.rs`: `taxonomy_path.trim()` before `from_path`.
- **Frontend & infra fixes** — `axon-shell-resize-divider.tsx`: `role="separator"` with live `aria-valuenow`; `message.tsx`: `useEffect` dep on full `childrenArray`; `docker-compose.yaml`: `group_add: ["981"]` restored for docker socket; CHANGELOG: removed duplicate commit row.
- **Config files** — `axon.json` and `axon.schema.json` added to repo.

### Commits since v0.32.0

| SHA | Type | Description |
|-----|------|-------------|
| e0a20b02 | fix | address frontend, docker, and changelog review issues |
| 3484e789 | fix | address services, health, and graph review issues |
| 24e7880a | fix | address ingest and crawl engine review issues |
| d564266e | fix | address PR review issues in jobs infrastructure |

## [0.32.0] — feat/pulse-shell-and-hybrid-search

### Highlights

- **Pulse shell WebSocket** — `crates/web/shell.rs` implements a persistent terminal WebSocket handler for interactive shell sessions from the Pulse UI.
- **Pulse chat connection management** — `crates/web/execute/sync_mode/pulse_chat/connection.rs` extracted as a dedicated module managing ACP-backed chat session lifecycle.
- **Performance sweep** — Batched watchdog DB updates (N individual queries → single `unnest()` UPDATE); concurrent reflink copies via `JoinSet`; O(n²)→O(n) link deduplication with `HashSet`; sitemap index detection from first 512 bytes instead of full document scan; selector config computed once before per-URL backfill loop.
- **Shared `resolve_input_text()` helper** — Consolidates 3+ duplicate query-text resolution functions across ask, query, evaluate, suggest, research, and search commands.
- **Code consolidation** — Removed duplicate `parse_search_time_range` + 2 orphan tests from research test module (identical copy of search.rs helpers testing a non-production code path); `debug.rs` now uses shared `report_bool`/`report_text` from `doctor/render.rs`.
- **`Arc<Config>` in session ingest tasks** — Replaces per-task `Config::clone()` with a single Arc shared across all spawned session ingest tasks.
- **Shared reqwest::Client for doctor probes** — Single 5s-timeout client passed to TEI and OpenAI probes instead of one per probe.
- **MCP schema cleanup** — `crates/mcp/schema.rs` reduced by ~500 lines; OAuth Google state consolidation in `oauth_google/state.rs`.
- **Logging restructure** — `crates/core/logging.rs` refactored for cleaner span-field collection.

### Commits since v0.31.0

| SHA | Type | Description |
|-----|------|-------------|
| 99067651 | refactor | fix clippy dead-code and style warnings across all changed files |

## [0.31.0] — feat/pulse-shell-and-hybrid-search

### Highlights

- **Persistent ACP session cache** — `crates/services/acp/session_cache/cache.rs` added; warm sessions are reused across calls, reducing cold-start latency.
- **`acp_llm` module split** — Warm-up logic extracted to `crates/services/acp_llm/warm.rs` for cleaner separation from the ACP session bridge.
- **Pulse chat events** — `crates/web/execute/sync_mode/pulse_chat.rs` emits structured `ServiceEvent` payloads for phase markers and synthesis deltas over the WebSocket connection.

### Commits since v0.30.1

| SHA | Type | Description |
|-----|------|-------------|
| 5752e125 | feat(acp) | persistent ACP session cache, acp_llm module split, pulse_chat events |
| a7567da8 | docs(env) | document AXON_ASK_HYBRID_CANDIDATES in .env.example |
| 7ebe1716 | obs(ask) | log candidate funnel after reranking (retrieved/score-filtered/selected) |
| aea6fae9 | obs(search) | add structured latency logging to all qdrant search paths |
| 50003e59 | feat(sparse) | bump SPARSE_DIM from 30_522 to 65_536, halving collision rate |

## [0.30.1] — feat/pulse-shell-and-hybrid-search

### Highlights

- **HNSW params on dense prefetch arm** — `hnsw_ef` and `quantization` rescore params now set on the dense prefetch arm of the hybrid query (not the top-level fusion stage, which has no HNSW traversal). Fixes ineffective search tuning.
- **PR review threads resolved** — All 147 review threads addressed: ACP warm-session degraded path, 300s completion timeout, `result_rx` drain on channel close, `synthesis_delta` in frontend WS handler, `MarkdownSplitter` control-char guard, research consumer timeout, doc comment fix.
- **`dispatch_vector_search` in evaluate** — `scoring.rs` now uses the dispatch path (hybrid-aware) instead of the raw `qdrant_search` function, matching query/ask behavior.
- **Error handling cleanup** — `.map_err(|e| anyhow!(e.to_string()))` replaced with `inspect_err` + `?` and `anyhow::Error::from(e)` across hybrid/search paths (preserves error chain).
- **New tests** — `qdrant_search_propagates_filter_when_some`, `qdrant_search_sends_oversampling_param`, `qdrant_hybrid_search_sends_hnsw_ef_on_dense_prefetch_arm`.

### Commits since v0.30.0

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix(vector) | move hnsw_ef to dense prefetch arm, use inspect_err, dispatch in evaluate |
| f3950bd5 | chore | document AXON_HNSW_EF_SEARCH and AXON_HNSW_EF_SEARCH_LEGACY in .env.example |
| 0e672072 | feat(vector) | add hnsw_ef + quantization rescore params to hybrid and named-dense search |
| d3b123aa | refactor(vector) | extract qdrant_search() to search.rs to restore monolith budget |
| 0a30c876 | feat(vector) | add HNSW config (m=32, ef_construct=256) and INT8 quantization to ensure_collection() |
| bfc4654a | fix | address PR review threads 1–18 (18 threads, previous session) |
| 758837ce | feat(research) | streaming synthesis, ACP eager warm-up, hybrid search fix, quality tests |

## [0.30.0] — feat/pulse-shell-and-hybrid-search

### Highlights

- **Streaming research synthesis** — `research` command streams LLM tokens to stderr in real time via `ServiceEvent::SynthesisDelta`; CLI renders inline with phase markers. Web handler forwards as `{"type":"synthesis_delta"}` WS frames.
- **ACP eager session warm-up** — `AcpConnectionHandle::spawn_eager` starts adapter subprocess cold-start in the background while the Tavily search runs, hiding the ACP latency from the critical path.
- **Hybrid search response shape fix** — `/points/query` returns `{"result":{"points":[]}}` (nested), not `{"result":[]}` (flat). Added `QdrantQueryResult`/`QdrantQueryResponse` types; 3 deserialization tests lock in the wire contract.
- **`chunk_markdown` proptests** — 4 property tests: no chunk >2000 chars, no empty/whitespace chunks, deterministic output, non-whitespace input yields ≥1 chunk. `markdown_safe_input()` strategy avoids control-character panics in `MarkdownSplitter`.
- **`prepend_query_instruction()` helper** — Consolidates 3 duplicated `format!("{}{query}", QUERY_INSTRUCTION)` sites in `query.rs`, `scoring.rs`, `retrieval.rs`.
- **Refactors** — `build_synthesis_context` uses `write!()` (no temp alloc per iteration); `parse_synthesis_response` uses typed struct; `try_send` failure now logged; dead `tx_for_deltas` rename removed.

### Commits since v0.29.0

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | feat | streaming research synthesis, ACP eager warm-up, hybrid search fix, embedding quality tests |
| a8812398 | fix(qdrant) | /points/query returns {result:{points:[]}} not {result:[]} |
| 79f8cf2f | feat(embed) | Tier 1 embedding quality — asymmetric encoding + semantic chunking |

## [0.29.0] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.28.0.

### Highlights

- **Export service v3** — `crates/services/export.rs` rebuilt with schema versioning (`EXPORT_SCHEMA_VERSION=3`), SHA-256 integrity checks, `ExportMetadata`/`ExportIntegrity` types, seed tracking (`GithubSeedExport`, `QuerySeedExport`, `ScrapeSeedExport`), watch export, settings snapshot, and a `verify` path (`ExportVerifyReport`/`ExportVerifyMismatch`). New `migrations/003_export_seed_tracking.sql` and golden test `tests/export_schema_v3_golden.rs`.
- **Hybrid search enhancements** — `qdrant_facet_filtered` added to `client.rs` (parameterised filter support); `commands.rs` wired for query diagnostics; `tests/query_diagnostics_error_contract.rs` validates the error contract.
- **Query / retrieval improvements** — `crates/services/query.rs` (+73 lines) and `crates/services/search.rs` (+145 lines) expanded for hybrid search service wiring; `crates/vector/ops/commands/query.rs` and `ask/context/retrieval.rs` updated.
- **Graph worker s6 service** — `docker/s6/s6-rc.d/graph-worker/` and `contents.d/graph-worker` added; graph worker now runs as a managed s6 service alongside crawl/embed/extract workers.
- **Services layer** — `crates/services/scrape.rs` extracted as a dedicated module (+71 lines); `crates/services/error.rs` added; `crates/services.rs` re-export updated; `services/acp/bridge.rs` expanded (+90 lines) for richer ACP session lifecycle.
- **MCP handler improvements** — All six handler files updated for cleaner URL validation, job-ID parsing, and error propagation; `schema.rs` minor correction; `handlers_system/screenshot.rs` cleaned.
- **Config parse** — `build_config.rs` (+63 lines) adds new fields; `cli.rs` (+48 lines) expands CLI flags; `config.rs`/`config_impls.rs` updated with new defaults.
- **Docs** — `docs/EXPORT.md`, `docs/GRAPH.md`, `docs/RESTORE.md`, `docs/reference/commands/README.md`, `docs/reference/commands/export.md`, `docs/reference/commands/graph.md` added; `docs/reference/mcp/overview.md`, `docs/reference/mcp/tool-schema.md`, `docs/SCHEMA.md`, `docs/reference/job-lifecycle.md` updated.
- **Deleted stale plans** — Seven completed superpowers plan docs removed (`2026-03-10` through `2026-03-13`).

### Commits since v0.28.0

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | feat | export service v3, hybrid search, graph worker, query diagnostics, docs |
| ebd54a66 | fix | address PR review thread 13 — clarify AXON_ACP_ADAPTER_CMD in README |
| ba913374 | fix | address PR review threads 1,4,5,6,10,14 — Rust security and correctness |
| e1845e27 | fix(copilot) | remove unused type import, use direct re-export for CopilotStreamEvent |
| eef95ee6 | fix | address PR review threads 2,3,7,8,9,11,15,16 — TS/web security and correctness |
| d24f3ea2 | feat(web) | Pulse shell UI, web server utilities, MCP/core refactoring, logging fixes |

---

## [0.28.0] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.27.2.

### Highlights

- **Pulse shell UI** — Full `apps/web/components/shell/` workspace with conversation pane, MCP pane, settings dialog, terminal pane, sidebar, and responsive mobile/desktop layouts. Shell state split into focused modules (`state-messages`, `state-session`, `state-layout`, `state-settings`, `state-tools`).
- **Web server utilities** — `cortex-proxy.ts` factory eliminates per-route duplication across five cortex endpoints; `openai-sse.ts` provides SSE streaming helpers; `mcp-config.ts` centralises MCP server registration.
- **New shared web lib** — `lib/pulse/` (chat-api, chat-helpers, chat-stream, claude-response, doc-ops, permissions, rag, session-store, types, workspace-persistence, etc.); `lib/error-utils.ts`, `lib/format.ts`, `lib/type-guards.ts`, `lib/url-utils.ts`.
- **MCP handler refactoring** — `parse_ingest_source` decomposed from struct mutation to positional params; `validate_mcp_urls` includes index in error messages; `parse_job_id` accepts `&str`; `artifacts/path.rs` cleanup.
- **Config parse improvements** — Generic `parse_csv_env` helper eliminates four identical split/trim/filter patterns across `build_config.rs`.
- **Logging TOCTOU fixes** — `SizeRotatingFile` log rotation eliminates TOCTOU races in `logging.rs`: `exists()`+`rename/remove` replaced with match-on-`NotFound` patterns; warns on unexpected remove errors.
- **ACP gateway migration** — `refactor(acp)`: extract/fallback/debug/research synthesis routed through ACP; suggest generation uses ACP gateway; ask/evaluate config validation aligned with ACP routing.

### Commits since v0.27.2

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | feat(web) | Pulse shell UI, web server utilities, MCP/core refactoring, logging fixes |
| 3a286289 | refactor(acp) | migrate extract fallback/debug/research synthesis and docs |
| 47207a8f | refactor(suggest) | use ACP gateway for suggestion generation |
| 042dd661 | fix(vector) | align ask/evaluate config validation with ACP routing |
| 3f3a1152 | refactor(vector) | route ask and evaluate LLM calls through ACP |
| 22b0d44a | fix(acp) | stream only assistant deltas in acp_llm gateway |

## [0.27.2] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.27.1.

### Highlights

- **Dependency security updates** — `apps/web` upgraded `next` to `16.1.7`, added `pnpm.overrides` for vulnerable transitive packages (`undici`, `hono`, `@hono/node-server`, `dompurify`, `express-rate-limit`), and refreshed `apps/web/pnpm-lock.yaml`.
- **CI security enforcement** — Web CI now runs `pnpm audit --prod --audit-level=high` before lint/test.
- **Crawl embed provenance fix** — crawl worker now enqueues embed jobs with `source_type = "crawl"` so downstream provenance metadata is preserved.
- **Rust dependency refresh** — lockfile updates include `quinn-proto` bump to `0.11.14` and transitive metadata refresh.

### Commits since v0.27.1

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix(security) | bump web dependencies, enforce CI audit, and set crawl embed source_type |
| 17a751ee | fix(embed) | propagate source_type from embed job config and harden tests |
| d76cbfea | feat(embed) | thread source_type through full embed pipeline (prepare→text_embed→worker) |
| 778a7884 | refactor | multi-crate security hardening, full-review remediation, shared utilities |
| 0f5ea1a4 | feat(embed) | add source_type field to EmbedJobConfig for provenance tracking |

## [0.27.1] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.27.0.

### Highlights

- **Security hardening** — SSRF guards added to all missing paths (Chrome re-fetch, screenshot, MCP handlers, post-redirect seed URL validation). `GoogleOAuthConfig` secrets redacted in `Debug`, removed from `Serialize`. DCR token and ACP auto-approve comparison use constant-time equality. MCP bind changed from `0.0.0.0` → `127.0.0.1`.
- **Full-review remediation** — Systematically addressed all critical issues across 8 crates from multi-agent `.full-review` reports: timing attack fixes, TOCTOU race elimination, unbounded scroll capping, subprocess timeout guards, async I/O migration.
- **Shared utilities extracted** — `crates/core/paths.rs` (`axon_data_dir`, `axon_data_base_dir`, `path_basename`), `crates/core/http/headers.rs` (`parse_custom_headers`), `crates/core/http/ssrf.rs` (`ssrf_blacklist_compact_strings`), `crates/ingest/subprocess.rs` (`run_command_with_timeout`) — each eliminating 3–7 duplicate implementations.
- **Monolith splits** — `pulse_chat.rs` (565→243 lines) split into `connection.rs` + `events.rs`; `ws_handler.rs` (543→386 lines) split into `acp_session.rs`; `schema.rs` split into `schema/tests.rs`; net -411 lines across 63 files.
- **`Arc<Config>` in job workers** — Config no longer deep-cloned per AMQP job; wrapped in `Arc` at worker startup and shared by reference. Concurrent `ensure_payload_indexes` (6 sequential PUTs → `join_all`).
- **IngestProgress phase reporter** — `PhaseReporter` wired across all ingest sources with per-task phase/progress tracking visible in `axon status`.
- **GitHub ingest limits** — `github_max_issues` and `github_max_prs` config caps prevent runaway API pagination on large repos.

### Commits since v0.27.0

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | refactor | multi-crate security hardening, full-review remediation, shared utility extraction |
| a3ac1acd | refactor | simplify review — fix PR label bug, use console strip_ansi, extract GITHUB_SUBTASK_COUNT, combine heartbeat queries |
| 0cf80dd9 | fix | address all 13 PR review comments from cubic-dev-ai |
| b1eabe32 | test(ingest) | integration tests for progress reporting and heartbeat |
| 61f796c7 | feat(ingest) | wire PhaseReporter across all ingest sources |
| 14b9f9ee | fix(prewarm) | eliminate silent fallbacks and error masking |
| bc519aa6 | feat(prewarm) | add tracing span for structured log correlation |
| 4c305c9d | feat(status) | show per-task phase and progress in ingest status |
| 44b0e0cc | feat(config) | add github_max_issues and github_max_prs limits |
| ec3b979a | refactor(prewarm) | switch to anyhow::Result with .context() chains |
| 217ae733 | feat | v0.27.0 — ACP prewarm, services routing, error context, docker split |

## [0.27.0] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.26.0.

### Highlights

- **ACP adapter pre-warming** — `spawn_prewarm_task()` on server boot spawns the default Claude ACP adapter and sends a ping turn to force `establish_acp_session()`, eliminating ~45-second cold start on first chat message. Controlled via `AXON_ACP_PREWARM` (default: true). Includes per-turn timeout, hung session detection/eviction, and liveness tracking (`mark_turn_started`/`mark_turn_completed`).
- **Services layer routing** — CLI commands (`embed`, `extract`, `crawl`, `refresh`, `status`) now dispatch through the services layer instead of calling job functions directly. Web cancel dispatch also routed through services.
- **Error context improvements** — Replaced `anyhow!(e.to_string())` anti-patterns with descriptive `.map_err()` context across vector ops, services, crawl engine, and CLI. Added `thiserror` dep with `HttpError` derive and `JobError` enum.
- **Structured tracing migration** — Converted `log::` macros to structured `tracing::` macros in ACP and web modules.
- **Docker Compose split** — Services (Postgres, Redis, Qdrant, RabbitMQ, TEI) extracted to `docker-compose.services.yaml`.
- **MCP HTTP transport** — MCP config now supports `http` transport mode alongside `stdio`.

### Commits since v0.26.0

| SHA | Type | Description |
|-----|------|-------------|
| f2eb21bd | fix | prevent mark_turn_completed skip on prewarm timeout/channel errors |
| cf81cdd5 | feat | wire ACP prewarm into server startup |
| fc8d0564 | feat | add ACP adapter pre-warming module |
| aad02369 | feat | add AXON_ACP_PREWARM config option (default: true) |
| 24b86975 | refactor | extract build_agent_key() helper from pulse_chat |
| bc268fd0 | fix | route sync_crawl embed job through services layer |
| 54e4ea4f | refactor | route web cancel dispatch through services layer |
| cdea4645 | refactor | route CLI refresh schedule through services layer |
| 4ef11251 | refactor | route CLI embed/extract/crawl subcommands through services |
| d8cff293 | fix | replace anyhow!(e.to_string()) anti-patterns with descriptive context |
| 26fc0229 | fix | add descriptive .map_err() context to crawl engine errors |
| 5719ffc2 | fix | convert log:: to structured tracing:: in web module |
| cfd8999d | fix | add descriptive .map_err() context to services error propagation |
| ddd38583 | refactor | extract update_latest_reflink + prepare_crawl_output_dir to dir_ops |
| 0bf1e05d | fix | replace Err(string.into()) with anyhow::anyhow! in CLI handlers |
| 78cee3b5 | fix | replace .map_err(\|e\| e.to_string().into()) with descriptive context |
| b5bf9a28 | feat | add thiserror dep, migrate HttpError to derive, add JobError enum |
| cc23b36a | fix | convert log:: to structured tracing:: in ACP module |
| 62be990f | feat | shared embed_with_retry, session refactors, MCP query filters |
| 67351322 | fix | error on malformed vector elements instead of silent zero-fill |
| b717e441 | docs | document migrate command in CLAUDE.md |

## [0.26.0] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.25.3.

### Highlights

- **Chrome stealth path for `axon extract`** — `--render-mode chrome` now routes single-URL extraction through Spider's `website.crawl()` with full stealth + fingerprint patching, `with_wait_for_idle_network0`, and CDP URL resolution. Previously Chrome mode fell back to plain reqwest. `engine/chrome.rs` extracted to satisfy monolith limit.
- **`render_mode` persisted in `ExtractJobConfig`** — async extract jobs now serialize `render_mode` (with `#[serde(default)]` for backward compat) so workers honour the originally requested render mode on dequeue.
- **Temporal vector search** — `--since` / `--before` flags on `query`/`retrieve`/`ask` filter Qdrant results by `indexed_at` timestamp, powered by new `VectorQueryFilter::temporal` path.
- **`spawn_blocking` for CPU-bound chunk ops** — `chunk_code`/`chunk_text` in GitHub ingest file batch now offload tree-sitter AST work to the blocking thread pool, preventing async runtime starvation during large repo ingest.
- **`build_client` user-agent support** — `build_client(timeout, user_agent: Option<&str>)` signature extended; `HTTP_CLIENT` LazyLock reads `AXON_CHROME_USER_AGENT` at init.
- **Local timestamp logging** — `log_info`/`log_warn`/`log_done` timestamps now use `chrono::Local::now().format("%H:%M:%S")` instead of UTC epoch math.
- **`list_ingest_jobs` source filter** — added `source_filter: Option<&str>` parameter with SQL `WHERE ($3::text IS NULL OR source_type = $3)` for filtering `sessions` separately.
- **Refresh schedule cascade delete** — `delete_refresh_schedule_with_pool` now also calls `delete_watch_def_with_pool`.
- **`context7` MCP server** — added `npx -y @upstash/context7-mcp` to `config/mcporter.json`.

### Commits since v0.25.3 (`3d9f3476`)

| SHA | Type | Description |
|-----|------|-------------|
| 3d9f3476 | feat | Chrome stealth extract, temporal search, spawn_blocking ingest, logging/client improvements |
| ff50c232 | fix | wire CDP URL resolution, network-idle wait, render_mode persistence in Chrome extract path |
| cb60b108 | refactor | extract Chrome path to engine/chrome.rs (monolith limit) |
| 516628ec | feat | temporal search via --since / --before flags |
| 8d58df42 | feat | add Chrome stealth path to single-URL extraction |
| f2150b2b | feat | add Chrome/rendering fields to ExtractWebConfig |
| 9b1291f4 | fix | address all 9 PR review comments from cubic-dev-ai |
| 4838a7cf | fix | re-push pre-acked job on DB claim error, cap preacked_ids |
| 64906291 | fix | drop response before permit on non-success TEI path, clean invalid indexes CONCURRENTLY |
| 5b47e5ec | fix | snap search_start to UTF-8 char boundary after chunk-overlap rewind |
| d72509c0 | fix | reset session generation counter on manual session switch |
| e2362a68 | fix | advance search_start by chunk_len minus overlap for correct line ranges |
| 12f1456b | fix | restore MCP card navigation after href removal |
| 7e94e987 | fix | add usage data fields to UsageUpdate wire event |
| cd210a48 | fix | restore positional URL for graph build and validate required args |
| 0a166796 | fix | replace misleading "unknown" errors with "not yet implemented" for watch subcommands |

## [0.25.3] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.25.2.

### Highlights

- **AMQP consumer_timeout eliminated** — saturation path in `run_amqp_lane` now polls `consumer.next()` during `semaphore.available_permits() == 0`. Deliveries that arrive while all slots are full are immediately pre-acked (clearing RabbitMQ's unacked count), stored as UUIDs in `preacked_ids`, and processed when a permit frees. Prevents the `PRECONDITION_FAILED - delivery acknowledgement on channel 1 timed out` channel close that fired after 30 min of continuous ingest. On lane exit, unstarted pre-acked jobs are re-enqueued to AMQP rather than waiting for the watchdog sweep.
- **`doc_concurrency` clamp corrected** — `pipeline.rs` thundering herd fix was `clamp(2, 16)` instead of the intended `clamp(2, 8)`. With 12 CPUs and 12 ingest lanes this queued 144 docs behind the 8-permit TEI semaphore, exceeding the 300s doc timeout. Now capped at 8.
- **`qdrant_upsert` retry** — bare `send().await?.error_for_status()?` had zero fault tolerance. Added 3-attempt exponential backoff (500ms, 1000ms) matching the TEI retry pattern.
- **AST chunking observability** — `github collect_start files_total=N batch_concurrency=M embed_batch_size=50` logged at the start of `collect_and_embed_batched` so the multi-minute tree-sitter CPU phase is visible in the log file (`$AXON_DATA_DIR/axon/logs/axon.log`).

### Commits since v0.25.2 (`f8f387bc`)

| SHA | Type | Description |
|-----|------|-------------|
| *(this commit)* | fix | AMQP consumer_timeout, doc_concurrency clamp, qdrant_upsert retry, AST chunking observability |

## [0.25.2] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.25.1.

### Highlights

- **`GraphArgs` proper subcommand struct** — `graph` CLI command upgraded from `TextArg` to a proper `GraphArgs` with `GraphSubcommand` enum (`Build`, `Status`, `Explore`, `Stats`, `Worker`); `--url`, `--domain`, and `--all` flags on `Build`; graph.rs handler updated accordingly.
- **`common/job_output.rs` + `common/url_inputs.rs`** — shared CLI utilities extracted: `JobStatus` trait with `impl_job_status!` macro for consistent job status/list/errors rendering across all job types; `url_inputs` collects positional + `--urls` CSV inputs into a single `Vec<String>`.
- **`qdrant_scroll_pages_while`** — new streaming scroll variant with an early-exit predicate; `system.rs` uses it for `summarize_detailed_domains_limited` with a `DEFAULT_DOMAINS_DETAILED_LIMIT` of 10k to cap unbounded scans.
- **Watch service additions** — `create_watch_run` added to `crates/services/watch.rs`; `WatchDefCreate` re-exported from service layer.
- **Ingest/jobs hardening** — `crates/jobs/crawl/runtime/worker/job_context.rs` and `result_builder.rs` updated; embed, extract, ingest schema, and refresh jobs hardened.
- **ws-messages tests** — comprehensive test suite added: `ws-messages-actions.test.ts`, `ws-messages-pulse.test.ts`, `ws-messages-subscription.test.ts`, `ws-messages-tracked.test.ts`; plus `axon-shell-state.test.ts` and `axon-shell.test.tsx` for shell component coverage.
- **GitHub ingest re-export + batch fix** — `crates/ingest/github.rs` re-exports, `files/batch.rs` hardening.

## [0.25.1] — feat/pulse-shell-and-hybrid-search

This section documents commits on `feat/pulse-shell-and-hybrid-search` since v0.25.0.

### Highlights

- **ACP session update cascade fix** — three confirmed root-cause bugs causing `"ACP load_session failed, falling back"` → new fallback session → 30s 404 poll loop eliminated.
- **SessionInfoUpdate + UsageUpdate SDK variants** — `agent_client_protocol` v0.10.0 `unstable` variants now handled in `map_session_update_kind()` and `map_session_notification_event()`; `Unknown` no longer hit for these cases.
- **emit() async with backpressure** — `emit()` in `crates/services/events.rs` changed from `try_send()` (silent drop on full channel) to `send().await` (blocks until receiver drains); 16 events were being silently dropped per 31-second probe.
- **session_info_update frontend handling** — new WS wire event `session_info_update` dispatched to frontend; `getCachedSessions()` extended with `forceRefresh` option bypassing 30s stale-while-revalidate cache when triggered by this event.
- **Shell component + ws-messages provider split** — `axon-shell.tsx` split into `axon-shell-desktop.tsx`, `axon-shell-mobile.tsx`, `axon-shell-conversation-pane.tsx`, `axon-shell-right-pane.tsx`, `axon-shell-sidebar-pane.tsx`; `provider.ts` split into `provider-actions.ts`, `provider-effects.ts`, `provider-runtime.ts`.

### Commits since v0.25.0 (`7b173bf8`)

| SHA | Message |
|-----|---------|
| f970f9ec | fix(services): make emit() async with backpressure — replace try_send drop with send().await |
| 370555fe | test(acp): add UsageUpdate wire-shape and e2e mapping tests |
| a5b372fa | feat(acp): map SessionInfoUpdate and UsageUpdate SDK unstable variants |
| 3142dfe8 | feat(web): handle session_info_update WS event and force-refresh cache bypass |

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
| 7b173bf8 | feat(web,vector): Pulse shell redesign, AI elements, hybrid search, new API routes (v0.25.0) |

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

This section documents commits on `feat/web-integration-review-fixes` relative to `main` (`fe11a78d`).

### Highlights

- **Secondary violation fixes (v0.23.3)** — six targeted fixes from follow-up review: (1) `rate_limiter.rs` O(N) `retain()` on every request replaced with amortized sweep (AtomicU64 gate, at most once per 60s window); (2) `handlers_elicit.rs` raw error detail `{e}` no longer forwarded to MCP client — logged server-side, generic `"elicitation failed"` returned; (3) `ws_send.rs` sentinel type hardcoded to `"log"` (was preserving original event type, producing malformed `command.*` messages without required `ctx` fields); (4) `config/mcporter.json` hardcoded `/home/jmagar/.local/bin/axon` replaced with portable `axon` (assumes PATH); (5) `docs/reference/mcp/tool-schema.md` corrected: `auto-inline` added to `ResponseMode` enum, `path` field marked required only for `head|grep|wc|read|delete` (not `list|search|clean`), `pattern` noted as required for both `grep` and `search`; (6) `elicit_demo` added to `action:help` discoverable action map in `handlers_system.rs`.

- **All PR review findings addressed (v0.23.2, `ebd54fd6`)** — complete batch of security, monolith, and test-hardening fixes: 14 Dependabot vulnerabilities resolved (13 npm via `pnpm.overrides`, 1 Rust `quinn-proto` via `cargo update`); `WEB-INTEGRATION-REVIEW.md` removed from branch git history via `git-filter-repo`; 15 PR review threads marked resolved; MCP elicitation wired (`rmcp` feature enabled, `ElicitDemoRequest`/`AxonRequest::ElicitDemo` defined, `handlers_elicit.rs` handler integrated); `ws_handler.rs` rate-limit extracted to `ws_handler/rate_limiter.rs`; `docker_stats.rs` tests split to `docker_stats/tests.rs`; `execute/cancel.rs` helpers extracted; `fingerprint_mcp_servers` uses SHA-256 instead of raw JSON; `session_cache::read_replay_buffer` delegates to `drain_replay_buffer`; worker lane env-var tests save/restore state; `axon-ws-exec.ts` cancel guarded on `backendJobId`; `axon-shell-state.ts` explicit rejection on non-auto-approve path; `ws-protocol.ts` named interfaces extracted.

- **`crates/web` comprehensive review — all 31 findings resolved (v0.23.1)** — complete remediation of all P0/P1/P2/P3 and security findings from the 2026-03-13 review. Security: loopback PTY auth bypass removed (SEC-1); ACP `enable_fs`/`enable_terminal` capability flags now threaded end-to-end (SEC-2); `DefaultHasher` replaced with JSON string key for ACP session cache (SEC-3); empty `session_id` filtered to `None` to prevent system-prompt bypass (SEC-4); `subtle::ConstantTimeEq` replaces hand-rolled token comparison to eliminate length side-channel (SEC-6). Critical/high: `MutexGuard` held across `.await` fixed; `dispatch_search_and_info_modes` split below 120-line hard limit; `ws_handler.rs` reduced 510→432 lines via test module extraction; all `.expect()`/`.unwrap()` in production paths replaced; `JoinSet` cleanup on WS disconnect; `session_ownership` DashMap cleanup on disconnect; rate-limit state moved to process-wide `AppState` `DashMap` keyed by client IP (bypassed reconnect); `docker_stats` task wrapped in restart loop. Medium: `handle_command` refactored to accept `ExecCommandContext` directly (removes 8-param lint suppress); `handle_ws`/`handle_ws_message`/`handle_pulse_chat` all reduced below 80-line warn threshold via helper extraction; `spawn_blocking` for `resolve_exe()` filesystem probes; `LazyLock` for env var caching; page-cache subtracted from Docker memory metrics. Low: `biased;` added to forward task `select!` (output prioritized over stats); `crawl_files` detection changed from fragile substring scan to typed JSON struct; rate-limit errors now use `WsEventV2::CommandError` envelope; `read_file` messages rate-limited; dead `WsEventV2::JobStatus`/`JobProgress` variants removed; `ASYNC_SUBPROCESS_MODES` empty constant removed; `POLL_INTERVAL_MS` corrected 1000→500ms. Also: `ws_handler.rs` new module `crates/web/ws_handler/` with extracted tests; 1266 lib tests passing.

- **Embed worker crash fix** — `poll_next_delivery` in `crates/jobs/worker_lane/amqp.rs` returned `Ok(None)` when an in-flight future completed via `FuturesUnordered::next()`, which `parse_delivery_result` correctly mapped to `DeliveryOutcome::Break` (consumer stream ended), terminating the lane. Fixed by returning `timeout(Duration::ZERO, pending())` → `Err(Elapsed)` → `Continue` instead. Regression test added in `worker_lane.rs`.

- **Web integration full-review fixes (v0.23.0)** — 5 critical and 12 high findings from a comprehensive `apps/web ↔ crates/web` integration review addressed across 20 files. Security: `check_auth()` now reads `Authorization`/`x-api-key` headers (tokens no longer forced into query strings / access logs); CORS preflight uses an explicit header allowlist instead of reflecting arbitrary client headers; ACP sessions are bound to originating WS connection (cross-session interference prevented); shell PTY input capped at 64 KB; debug-build auth bypass now emits a prominent `log::warn!`. Protocol: `acp_resume_result` field renamed `success` → `ok` to match TypeScript Zod schema (session resume was silently broken); `permission_request` ACP events fully wired through TypeScript WS handler; all `format!()`-based JSON replaced with `serde_json::json!()` (injection-safe); four ACP permission flags (`enable_fs`, `enable_terminal`, `permission_timeout_secs`, `adapter_timeout_secs`) wired through `ALLOWED_FLAGS` → `params.rs` (UI controls now functional). Performance: WS channel-full drops replaced with visible `[output truncated]` sentinel; sync-mode concurrency semaphore added (`AXON_MAX_SYNC_CONCURRENT`, default 16); per-connection WS execute rate limiting (120 req/60s); dead ACP adapter evicted from `SESSION_CACHE` on `run_turn()` error; `axon-ws-exec.ts` singleton sends abort-triggered cancel to server and caps pending map at 100. Code quality: `NO_JSON_MODES` updated to reflect service-layer routing; `pulse_chat_probe`/`mcp_refresh` documented as internal-only; editor system prompt extracted to named constant; 22 pre-existing TypeScript `noUncheckedIndexedAccess` test errors fixed; 862 tests passing.

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
- **Docs updated** — `docs/reference/mcp/tool-schema.md`, `docs/operations/operations.md`, `docs/contributing/testing.md`, `README.md`, `.env.example` refreshed
- **Post-v0.4.0 stabilization** — fixed MCP OAuth smoke env handling and serialized crawl DB tests to reduce flakes; fixed 4 failing CI checks; pinned Vitest timezone (`TZ=UTC`) and refreshed snapshots for deterministic test output
- **Release prep + execution hardening (v0.4.1)** — updated web/container/docs env wiring and token guidance (`AXON_WEB_API_TOKEN`/`NEXT_PUBLIC_AXON_API_TOKEN`), refreshed Docker/compose defaults, and fully hardened the services-layer refactor execution plan with strict preflight, safety rails, and parallel-worker dispatch protocol
- **Full codebase security & quality review (v0.4.0)** — comprehensive 5-phase review covering 244 Rust + 424 TypeScript files; 40 Phase 1 findings (3 Critical, 7 High, 17 Medium, 13 Low) + 17 CodeRabbit findings all addressed; WS OAuth bearer token gating added; all `format!` SQL → parameterized queries (H-03); `Secret<T>` wrapper with `[REDACTED]` debug; `ConfigOverrides` + sub-config scaffolding (A-H-01); `Config::test_default()` (CR-Q); ANTHROPIC_API_KEY + CLAUDE_* passthrough in child env allowlist (H-02/CR-D); `spawn_blocking` replaces `block_in_place` in MCP ask handler (CR-E); token rotation race fixed (CR-F); OAuth state capacity caps (H-05/CR-K); `apply_overrides` returns new `Config` (CR-M); `ServiceUrls` Debug redacts secrets (CR-L); migration table for `axon_session_ingest_state` (CR-B); arch docs for A-H-01/A-M-01/A-M-04/A-M-08
- **Evaluate page + cortex suggest API** — new `/app/evaluate/page.tsx` for RAG evaluation UI; new `/api/cortex/suggest/route.ts` server route; `apps/web/lib/api-fetch.ts` typed fetch utility; v0.3.0 (minor bump)
- **Image SHA verification** — `docker/s6/cont-init.d/00-verify-image-sha` and `docker/web/cont-init.d/00-verify-image-sha` added to both worker and web containers; `scripts/check-container-revisions.sh` for CI; `scripts/rebuild-fresh.sh` and `scripts/test-mcp-oauth-protection.sh` added
- **CLI help contract test** — `tests/cli_help_contract.rs` verifies `axon --help` exit code and output structure; `scripts/check_mcp_http_only.sh` ensures HTTP transport is correctly gated
- **Sidebar simplification** — `SidebarSectionId` pruned to `'extracted' | 'workspace'`; `recents-section`, `starred-section`, `templates-section` removed; `workspace-section.tsx` and `file-tree.tsx` updated
- **Docs reorganization** — `commands/axon/`, `commands/codex/`, `commands/gemini/` skill command stubs deleted; 20+ `docs/reference/commands/*.md` reference files added covering all CLI subcommands; new `docs/guides/context-injection.md`, `docs/schema.md` added; `scripts/check_no_mod_rs.sh` and `scripts/check_no_next_middleware.sh` added for CI
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
- `docs/reference/commands/refresh.md` reference added (`d1f20a4`).

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
