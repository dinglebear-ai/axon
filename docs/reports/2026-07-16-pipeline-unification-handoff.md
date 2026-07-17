# Pipeline Unification Refactor Handoff
Last Modified: 2026-07-16

## Purpose

This is the engineering handoff for the issue #298 pipeline-unification
refactor. It records the live branch, implemented behavior, proof already run,
work still in progress, and the exact remaining closeout sequence.

This report is a point-in-time execution record. The contracts under
`docs/pipeline-unification/` remain authoritative. Several old plan checkboxes
and the GitHub issue body are stale; do not infer missing implementation from
an unchecked historical task without checking the live tree.

## Live State

| Item | State |
|---|---|
| Checkout | `/home/jmagar/workspace/axon` |
| Branch | `codex/frfr-issue-298-closeout-wave` |
| Pushed HEAD | `9c360f40d6abf547eed0b394c5cdbe6ab2a573bc` |
| Pull request | [#442 Complete pipeline unification closeout](https://github.com/jmagar/axon/pull/442) |
| Base | `main` |
| PR state | open, blocked by checks from the old pushed head |
| Issue | [#298](https://github.com/jmagar/axon/issues/298), open and stale |
| Working tree | intentionally dirty; current closeout wave is not committed |
| Dirty tracked paths before this report | 125 |
| New untracked implementation paths before this report | 7 |
| Current diff before this report | 1,485 insertions, 1,879 deletions |
| Release build | pass from current dirty checkout |
| Release binary | `target/release/axon`, 112,375,208 bytes |
| Release SHA-256 | `9a04c7a63dab745dc92695a1b26fc743895ed395ec6c991be83a202500cd3c6c` |

Do not reset, clean, switch branches, or recreate this work from an old plan.
The dirty tree contains the latest resource parity, artifact-ID, web-option,
Android, generated-contract, and observability work.

## Pushed Closeout Commits

The current PR already contains these closeout commits:

| Commit | Purpose |
|---|---|
| `cd5089032` | close redaction write-boundary findings |
| `5960cf2c7` | integrate the issue #298 closeout wave |
| `c886f3573` | complete the primary pipeline-unification closeout |
| `d243a9358` | classify canonical pipeline environment keys |
| `f854f478e` | address first review findings |
| `2d50e7492` | close residual security and cancellation findings |
| `65e76e3c7` | refresh Aurora primitive inventory |
| `ae7e6235e` | restore disabled closeout CI gates |
| `9c360f40d` | split files that violated the monolith policy |

## Completed Refactor

### 1. Target Architecture And Ownership

- The root `axon` package is the thin binary/bootstrap surface.
- The product dependency graph has 23 product crates; Cargo metadata has 25
  workspace packages when the root package and `xtask` are included.
- Target boundaries exist for API, errors, authorization, routing, adapters,
  ledger, parsing, documents, embeddings, vectors, retrieval, LLMs, graph,
  memory, observation, pruning, jobs, services, CLI, MCP, and web.
- Dependency layering is acyclic and enforced by `cargo xtask check-layering`.
- Old `axon-vector`, `axon-crawl`, `axon-ingest`, `axon-code-index`, and
  `axon-source-ledger` crates are gone.
- `axon-extract` intentionally remains. It owns the restored vertical
  extractor implementations, while adapter routing and parser-facing facts
  live in `axon-adapters` and `axon-parse` respectively.
- Source trees use sibling module files rather than `mod.rs`; crate agent docs
  and symlink rules are enforced.

### 2. Shared Contracts, Stores, And Providers

- `axon-api::source` owns the transport-neutral typed IDs, requests, results,
  manifests, stage results, jobs, events, graph, vectors, watches, reset, prune,
  artifacts, providers, and lifecycle DTOs.
- `axon-error` owns the shared error taxonomy, stages, retry/cooling semantics,
  visibility, and redacted context.
- Durable boundaries and strict fakes exist for ledger, graph, memory, vector,
  embedding, LLM, artifact, job, watch, config, credential, cache, health,
  rate-limit, search, fetch, render, network-capture, and security policy.
- Provider reservations, cooling, health, backpressure, interactive reserve,
  and starvation protection are implemented.
- Generated capability schemas and crate layering snapshots are in place.

### 3. One Source Request And Routing Model

- Canonical ingestion starts from `SourceRequest` and passes through
  `SourceResolver` and `SourceRouter` before acquisition.
- Routes carry canonical URI, source ID, source kind, adapter, scope, authority,
  options, and capability validation.
- Unsupported scopes fail before acquisition.
- The canonical route covers web, local, git, feed, YouTube, Reddit, sessions,
  registry/package, upload, CLI tool, MCP tool, and the narrow memory source.
- The source-family matrix and fixtures cover resolve, auth, degraded behavior,
  manifests, metadata, provider failure, source documents, and source jobs.

### 4. Adapter-Owned Acquisition And Shared Publication

- Non-web families use the shared path:
  discover -> ledger diff -> acquire -> normalize -> enrich -> prepare ->
  embed/upsert -> graph -> generation publish.
- Git, feed, YouTube, Reddit, session, registry, upload, CLI tool, MCP tool, and
  memory materialization lives behind adapters rather than service-owned
  pre-acquisition bridges.
- Temporary workspaces used by adapters are internal materialization details,
  not old public ingest pipelines.
- Manifest generations, publication config snapshots, leases, interrupted
  generation cleanup, document statuses, and cleanup debt are shared.
- `embed=false` still discovers, acquires, normalizes, prepares, graphs, and
  publishes ledger state while skipping collection creation and vector writes.

### 5. Web, Scrape, Crawl, And Map

- Bare `axon <url>` routes by URI and scope through the source pipeline.
- `axon scrape <url>` remains the single-page convenience surface requested by
  product: page scope, no crawl, embedded by default, cleaned content returned
  according to output policy.
- Site/docs crawl acquisition runs through `WebSourceAdapter`, ledger,
  preparation, embedding, and publication. There is no service-owned
  crawl-to-disk pre-pass before indexing.
- Map is discover-only and uses the bounded in-memory map strategy; tests forbid
  the old `map_with_sitemap`, crawl configuration, output directory, and
  `manifest.jsonl` path.
- The adapter still uses the relocated web engine internally for discovery,
  which is allowed by the contract because acquisition ownership is the
  adapter's.
- HTTP/Chrome auto-switch, conditional ETag reuse, 304 document reuse, sitemap
  discovery, per-item isolation, and shared clients are implemented.
- The current dirty wave adds canonical `headers`, `respect_robots`,
  `cache_policy`, and per-extractor `vertical_cache_ttl_secs` routing.
- Cache policies use the frozen vocabulary: `bypass`, `use`, `revalidate`, and
  `offline`; offline changed-item acquisition fails closed on a cache miss.

### 6. Tool Sources

- CLI and MCP tool sources are live source kinds, not unsupported parser-only
  fixtures.
- CLI execution and MCP call modes are routed through adapters.
- Allowlists, execution authorization, auth snapshots, local secret/path
  protection, artifact capture, redaction, audit records, and graph facts are
  implemented.
- Tool output is bounded and projected into normal source documents before the
  shared preparation/vector path.

### 7. Vertical Extractors, Parsing, Chunking, Metadata, And Graph

- The vertical extractor set was restored rather than dropped during
  unification.
- Adapter-owned vertical routing invokes extractor implementations without
  giving `axon-extract` ownership of canonical route or parse contracts.
- Tree-sitter parsing exists for Rust, Python, JavaScript, TypeScript, and TSX.
- AST symbol ranges feed document chunking; Markdown, sessions, schemas,
  package/config/tool outputs, Docker/Compose, env, and plain text have parser
  and chunk routes.
- `GraphCandidate` construction, merge-key validation, evidence, source ranges,
  baseline graph writes, and cross-store source/generation metadata are wired.
- Source metadata is sanitized and projected through the shared vector payload
  builder rather than family-specific payload builders.

### 8. Unified Jobs, Watches, And Cancellation

- Family job tables and active crawl/embed/ingest bridges were removed.
- Canonical jobs use the durable `jobs` model with attempts, stages, events,
  heartbeats, artifacts, reservations, status transitions, cursors, recovery,
  watchdogs, panic handling, cancellation, and provider cooling.
- `JobKind` has canonical final variants rather than old Crawl/Embed/Ingest
  variants.
- Source watches use canonical source-watch IDs and tables; the old
  `axon_watch_defs` scheduler is absent.
- CLI, REST, and MCP watch create/get/update/pause/resume/delete/exec/history/
  artifacts paths are implemented.
- Watch execution routes a source request and no longer owns a parallel URL-only
  indexing pipeline.

### 9. Memory Integration

- Memory lifecycle, decay, reinforcement, review, supersession, forgetting,
  vector namespaces, graph links, and retrieval are implemented.
- The current contract intentionally has a narrow `memory://mem_*` source
  adapter for projecting memory records through the source preparation and
  publication pipeline.
- Memory source documents use `DocumentPreparer`, canonical vector payloads,
  source generation metadata, and graph candidates.
- Some older plan and crate documents still say memory must never be a source
  adapter. Those statements predate the narrow-adapter decision and must be
  reconciled; they are not evidence that the implementation should be removed.

### 10. Security, Authorization, Redaction, And Audit

- Route-time and execution-time authorization checks are implemented.
- Local-source classification is lexical and protected against URL/local path
  ambiguity and local secret-path dispatch.
- SSRF policy covers private IPs, redirects, DNS rebinding, loopback,
  link-local, and local/file schemes across network/render paths.
- Auth snapshots are captured and rechecked for delayed execution.
- Public vector, event, artifact, graph, memory, CLI JSON, MCP, REST, and trace
  write boundaries have redaction gates and failure tests.
- Artifact/event persistence validates bounded payloads and redacts before
  durable writes.
- Destructive reset/prune execution is admin-gated and confirmation/plan based.
- Review fixes tightened cancellation, cooling bounds, secret handling, and
  redaction behavior.

### 11. Prune, Reset, And Empty-Store Cutover

- Public purge/dedupe surfaces are removed; their implementation ownership is
  behind `axon-prune`.
- Prune has typed plans, execution, selectors, receipts, cleanup-debt order,
  generation fences, collection threading, and dry-run behavior.
- Reset inventories stores, computes a plan/checksum, requires confirmation,
  recreates target SQLite/Qdrant state, and writes receipts.
- Terminal migrations drop old job/watch/freshness storage and enforce final
  kind constraints.
- Startup/doctor/preflight detect incompatible old stores.
- Empty-store reset and reindex is the supported clean-break migration path.

### 12. CLI, REST, MCP, And Resource Parity

- Removed CLI tokens fail before bare-source dispatch; no compatibility aliases
  silently revive old commands.
- Canonical source, jobs, watches, map, extract, memory, graph, prune, reset,
  providers, capabilities, collections, artifacts, uploads, chat, status,
  doctor, preflight, smoke, config, and help surfaces exist.
- REST source/job/SSE and lifecycle routes use shared DTOs and success/error
  envelopes.
- MCP remains one `axon` tool with action/subaction dispatch and generated
  schema.
- The current dirty wave completes previously missing CLI resource commands,
  MCP artifact handling, chat service parity, and resource/action matrices.
- Artifact retrieval now uses opaque artifact IDs instead of exposing local
  filesystem paths across CLI, REST, MCP, web, and Palette boundaries.
- Cross-surface fixtures currently report no known CLI/REST/MCP operation
  divergence after regeneration.

### 13. Apps And Clients

- Web and Palette removed stale purge/dedupe and old crawl/ingest assumptions.
- Web and Palette consume canonical action/resource output and opaque artifact
  identifiers.
- Palette's Tauri bridge and artifact preview paths were aligned with the new
  server contract.
- Android source/job flow is present and generated API contract checks were
  green on the prior pushed head.
- The current dirty wave changes Android web options from legacy
  `custom_headers` arrays to canonical `headers` objects and renames the
  remaining internal embed UI/model language to source indexing.
- Chrome extension and presentation/Aurora inventory gates are enabled.

### 14. Generated Contracts, Documentation, And CI

- `xtask schemas` generates API, CLI, OpenAPI, MCP, config, event, error,
  database, graph, vector-payload, provider, and adapter families from declared
  inputs.
- Valid/invalid fixtures, snapshots, checksums, JSON, and Markdown artifacts are
  tracked.
- The final 110-file documentation tree exists.
- Most recently completed local docs run passed:
  - `cargo xtask schemas generate --check`
  - `cargo xtask docs generate --check`
  - `cargo xtask docs check`
  - 504 Markdown files with no broken relative links
  - 115 contract docs with no removed-surface references
- CI's previously disabled full gates were restored. There are no `false &&` or
  `if: false` bypasses. The only `continue-on-error` is an informational
  whole-repo monolith report; the changed-file monolith gate is blocking.
- CodeQL, Android, Android OpenAPI, Palette, web, REST parity, MCP OAuth,
  Windows binary, release, and related checks passed on the old pushed head.

## Current Dirty Wave

The uncommitted wave is not incidental. It contains:

- CLI resource command implementations and argument/config dispatch modules.
- MCP artifact action parity and artifact response/path hardening.
- Opaque artifact-ID migration across core, services, web, MCP, CLI, web app,
  Palette, retrieval citations, and tests.
- Chat service and cross-surface resource/action parity.
- Canonical web adapter option routing and generated adapter scope updates.
- Android source-index terminology and canonical web header options.
- Generated CLI/MCP/API/error/adapter schemas and snapshots.
- Release-version test corrections and CI gate updates.
- Source progress emitter work for structured source identity, counts,
  generation, current item, warnings, and errors.

Never discard this wave to make the tree clean. It must be verified, committed,
and pushed as the next PR #442 commit.

## Verification Already Completed On The Dirty Tree

| Command | Result |
|---|---|
| `cargo test -p axon-route --lib -- --test-threads=1` | pass, 83 tests |
| focused `axon-services` web-option tests | pass, 11 tests |
| `cargo test -p axon-adapters web:: --lib -- --test-threads=1` | pass, 42 tests |
| `cargo xtask schemas generate --check` | pass before latest event edits |
| `cargo xtask docs generate --check` | pass before latest event edits |
| `cargo xtask docs check` | pass before latest event edits |
| `cargo fmt --all -- --check` | pass after event edits |
| `cargo check -p axon-services` | pass after event edits |
| `cargo check --bin axon` | pass after event edits |

Generated checks must be rerun after the final observability and docs edits.

Release build evidence:

- First `cargo build --release --bin axon` attempt failed because a generated
  `utoipa-swagger-ui` embed file retained an absolute path to the deleted
  `.worktrees/pipeline-unification-impl` target directory.
- `cargo clean --release -p utoipa-swagger-ui` removed only the stale package
  artifacts.
- The retry passed in 18m 49s.
- `target/release/axon --help` launched successfully and rendered the canonical
  command inventory.
- Artifact: 112,375,208 bytes; SHA-256
  `9a04c7a63dab745dc92695a1b26fc743895ed395ec6c991be83a202500cd3c6c`.

## Work In Progress

### Structured Progress Production

Transport projection is already capable of carrying full
`SourceProgressEvent` data through REST SSE and MCP. The remaining defect was
producer-side: portions of the shared source pipeline emitted a second minimal
event with empty counts and no item warning/error.

Current uncommitted progress:

- `SourceEventEmitter` now supports source ID, canonical URI, adapter, scope,
  generation, counts, current item, structured warning, and structured error.
- Non-web discovery, diff, acquisition, normalization, publication, warnings,
  item failures, and terminal failures now project structured events.
- New focused emitter tests were added but have not yet been run.
- The new helpers were split into `source/non_web/progress.rs` to keep
  `non_web.rs` below the repository file-size limit.

Still required for this item:

1. Run and fix the new emitter tests.
2. Add equivalent structured completion/warning/error projection to the web
   source path; its running-phase events are still intentionally minimal.
3. Verify no duplicate terminal events create inconsistent sequences.
4. Rerun services, jobs, web SSE, and MCP progress tests.

## Remaining Work To Finish The Refactor

The following list is the real remaining work. It separates implementation,
verification, and delivery so a stale checklist does not inflate the scope.

### A. Remaining Implementation

1. Finish web-source structured progress projection described above.
2. Fix any compile/test findings from the new progress tests.
3. Reconcile stale memory contract statements with the newer narrow
   `memory://mem_*` adapter decision in:
   - `docs/pipeline-unification/plans/finish-unification-metaplan.md`
   - `docs/pipeline-unification/plans/2026-07-04-phase-9-tool-source-families-matrix.md`
   - `docs/pipeline-unification/sources/adapter-scopes.md`
   - `docs/pipeline-unification/crates/axon-memory/CLAUDE.md`
4. Reconcile `delivery/implementation-checklist.md` and old phase-plan status
   text with the live REST/MCP/resource/prune/app implementations.
5. Address only genuine findings uncovered by the final gates or PR review.

No other known core source-family implementation is currently missing.

### B. Required Local Verification

Run one Rust command at a time:

1. `cargo fmt --all -- --check`
2. focused event/progress tests
3. `cargo xtask check`
4. `cargo xtask schemas generate --check`
5. `cargo xtask docs generate --check`
6. `cargo xtask docs check`
7. `cargo xtask check-api-parity`
8. `cargo xtask check-openapi-drift`
9. `cargo xtask check-android-api-contract`
10. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
11. `cargo test --workspace --all-features -- --test-threads=1`
12. security/redaction/reset/prune focused suites if not covered by the full run
13. web unit/build checks
14. Palette frontend and Tauri checks
15. Android compile/test after the current Kotlin rename
16. release binary launch: `target/release/axon --help`

Do not start multiple workspace Rust builds in parallel. Other repos may be
compiling on this host, but this Axon checkout should have one Cargo writer.

### C. Commit And PR Delivery

1. Inspect final `git diff --check`, file sizes, and status.
2. Stage all intended Axon closeout files, including this report and generated
   artifacts. This report lives under the intentionally ignored `docs/reports/`
   tree, so stage it explicitly with
   `git add -f docs/reports/2026-07-16-pipeline-unification-handoff.md`. Do not
   stage unrelated user changes if any appear.
3. Commit the dirty wave.
4. Push `codex/frfr-issue-298-closeout-wave`.
5. Wait for PR #442's new-head checks. Current failures are from old head
   `9c360f40d` and include Windows xtask, schema sync, version sync, clippy,
   test, security, MCP smoke, and aggregate CI gate; do not treat those as
   current after the new push.
6. Run the requested Lavra/PR review toolkit review on the new head.
7. Address every actionable finding and rerun affected checks.
8. Push final review fixes and require a green blocking CI set.

### D. Merge, Deploy, And Live Proof

1. Merge PR #442 into `main` without dropping the dirty-wave commit.
2. Sync local `main` and verify the merge commit/history.
3. Build the final release binary from merged `main`.
4. Sync/install the binary to the user's PATH and the Axon Incus/container
   runtime according to the live deployment layout.
5. Restart/recreate the Axon service as required.
6. Verify `axon.dinglebear.ai` and the server health endpoint.
7. Run a live CLI smoke across every canonical command group, including source
   families, jobs, watches, map, scrape, extract, memory, graph, resources,
   prune/reset dry-run, providers/capabilities, status/doctor/preflight, config,
   completions, MCP startup, and serve health.
8. Record works/does-not-work evidence; fix any runtime-only failures.

### E. Tracker And Repository Closeout

1. Replace issue #298's stale 2026-07-04 audit with current evidence.
2. Update the canonical implementation checklist from live proof.
3. Post the final gate, review, merge, deployment, and smoke summary.
4. Close issue #298 only after merged-main and live-runtime proof exist.
5. Delete stale merged branches and disposable worktrees while preserving the
   intentional `marketplace-no-mcp` long-lived branch.
6. Verify all remaining worktrees are clean and no unique commits are stranded.

## Known Stale Claims: Do Not Reimplement

These historical findings have been disproven by the live tree:

- Old MCP Crawl/Embed/Ingest/Dedupe/Purge DTO variants still exist.
- `/v1/extract/*` family lifecycle routes still exist.
- Fresh/refresh public commands remain.
- MCP watch exec/history is missing.
- The old `axon_watch_defs` scheduler still runs.
- Map still calls `map_with_sitemap`.
- Non-web acquisition is still service-owned.
- CLI/MCP tool sources are unsupported.
- Vertical extractors are missing or `axon-extract` still owns parse facts and
  canonical routing.
- JavaScript/TypeScript AST parsing and tree-sitter chunking are absent.
- Memory bypasses `DocumentPreparer` or is unsupported as a source kind.
- Purge/dedupe logic still lives in `axon-services/src/system`.
- CI remains disabled behind `false &&` gates.
- The final documentation tree is missing.

Recheck these only as regression assertions, not as implementation projects.

## Immediate Resume Point

At handoff, resume in this order:

1. Run `cargo test -p axon-services source::events -- --nocapture` and adjust the
   exact filter if the test module name differs.
2. Finish web-source structured event output and tests.
3. Reconcile stale memory/checklist docs.
4. Run the serialized verification sequence.
5. Commit and push the complete dirty wave to PR #442.
6. Review, green CI, merge, deploy, full live CLI smoke, tracker closeout, and
   branch/worktree cleanup.

## Completion Definition

The refactor is fully complete only when all of the following are true:

- no known contract gap remains in the live tree;
- all generated and repository checks are green;
- web, Palette, Android, and Rust gates are green;
- PR #442 review findings are resolved;
- PR #442 is merged into `main`;
- the release binary is built from merged `main` and deployed;
- the canonical live CLI smoke passes or every failure is fixed;
- issue #298 and canonical checklists match the merged implementation;
- stale branches/worktrees are cleaned without losing unique work.

## Closeout Addendum (2026-07-17)

This session completed the remaining implementation and local verification
from the sections above. Point-in-time record; the PR and tracker carry the
authoritative final state.

### Implementation completed

- Web-source structured progress projection landed: the shared helpers moved
  to `crates/axon-services/src/source/progress.rs` and now drive both the
  non-web and web pipelines (discovered/diffed/published completions with
  generation + stage counts, publish-time warnings, item errors, and exactly
  one terminal failure event per run). The web emitter now carries source
  identity/canonical URI. New tests:
  `source/events_tests.rs`, `source_web_events_tests.rs`, and the
  status-aware `source_observability_tests.rs` spine assertion.
- Memory narrow-adapter reconciliation in the four stale docs
  (adapter-scopes, metaplan, phase-9 plan, axon-memory CLAUDE.md) and the
  delivery implementation checklist rewritten from live proof.
- MCP help payload gained the wave's `artifacts` and `chat` actions; three
  MCP source-handler tests were updated to the deliberate lazy-provider
  contract (empty qdrant/tei no longer means "no runtime").
- The seven flattened CLI resource commands (`artifacts`, `uploads`,
  `collections`, `graph`, `providers`, `capabilities`, `chat`) were added to
  the curated help sections, the xtask CLI registry (`cli_registry/part4.rs`),
  and the generated CLI schema artifacts.
- Crate-contract specs synced to the live module surface (axon-api/-prune/
  -services); the retired pipeline crates are now forbidden deps for
  axon-services.
- Workspace clippy cleaned (`--all-targets --all-features -D warnings`),
  including removal of the deleted `map_screenshot_result` tests from
  `tests/services_lifecycle_services.rs`.
- Versions bumped for the restored release gate: cli 7.0.0 (major — opaque
  artifact IDs and canonical web options are breaking), palette 5.14.3,
  android 1.6.2 (versionCode 18), chrome 0.3.2, with changelog headings and
  release-please manifest parity. `check-release-versions --mode pr` passes.

### Verification results (this session)

- `cargo fmt --all -- --check` clean; `git diff --check` clean.
- `cargo xtask` gates green: layering, crate-contracts, api-parity (regenerated),
  schemas generate --check, docs generate --check, docs check, public-api
  (regenerated), dep-graph, redaction-logs, sqlite-migrations, secrets,
  mcp-http, env-staged, unwraps, repo-structure, symlinks, doc-links,
  doc-contracts, android-api-contract, version-sync.
- `check-openapi-drift` regenerates deterministically; it reports drift only
  because the regenerated artifacts are intentionally uncommitted on the
  dirty tree.
- Workspace: full `cargo test --workspace --all-features --no-fail-fast`
  serialized run green except two findings, both fixed (curated help sections
  and the CLI registry missing the resource groups); axon-core (761) and
  xtask (338) suites green after the fix. A final full run at 7.0.0 was
  started after the version bump.
- Apps: web vitest 53/53 + next build; palette vitest 456/456 + tsc + tauri
  release bundle + src-tauri cargo tests 159/159; android compileDebugKotlin
  + testDebugUnitTest green (requires `AXON_AURORA_ANDROID_PATH=
  ~/workspace/aurora/android`).
- One caution reproduced and confirmed: running any other cargo/pnpm build
  concurrently with a workspace `cargo test` in this checkout corrupts the
  interleaved build (phantom compile errors); serialize all cargo writers.

### Remaining after this session

Delivery steps C3-C8 (commit, push, PR review, green CI), section D
(merge, deploy, live smoke — includes migrating the live `~/.axon/config.toml`
to the 20-section shape before restart), and section E (tracker closeout).
Beads: closeout wave tracked as `axon_rust-uew6e`; follow-ups
`mutates_if` scope upgrade and live-config migration are beaded.
