# Services Layer Gap Closure Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the remaining issue #13 services-layer migration gaps by moving the last direct CLI/MCP/web bypasses behind typed `crates/services/*` APIs, while preserving current user-facing behavior and transport-specific formatting.
**Architecture:** Keep business logic in `crates/services/*` with typed result structs in `crates/services/types/service.rs`. CLI, MCP, and web execution remain thin adapters that validate input, call services, and render transport-specific output only. Introduce new service modules only where there is still no shared boundary today, especially refresh lifecycle/scheduling and sync-only embed/scrape orchestration.
**Tech Stack:** Rust, Tokio, Axum/WebSocket bridge, RMCP MCP server, existing `crates/jobs/*`, `crates/vector/ops/*`, serde_json, cargo test/check/fmt/clippy.

---

## Scope

This plan covers only the remaining live gaps found on 2026-03-10.

Already complete and out of scope:
- `query`, `retrieve`, `ask`, `search`, `research`, `map`, `doctor`, `stats`, `sources`, `domains`, `dedupe`, `extract`, most ingest sync flows, and web direct sync dispatch already call services.
- `crates/services/query.rs` already contains `evaluate()` and `suggest()` service functions, so the issue text claiming those wrappers are missing is stale.
- Web direct dispatch already handles `suggest` and `evaluate` through service wrappers, so any old subprocess-only assumption there is stale.

Remaining migration items in scope:
1. CLI `evaluate` still calls `run_evaluate_native()` directly instead of `services::query::evaluate()`.
2. CLI `suggest` still calls `run_suggest_native()` directly instead of `services::query::suggest()`.
3. CLI `crawl` async enqueue still calls `jobs::crawl::start_crawl_jobs_batch()` directly instead of `services::crawl::crawl_start()`.
4. CLI `scrape` still owns scrape execution and sync embed batching via `crawl::scrape::{build_scrape_website, fetch_single_page, select_output}` and `vector::ops::embed_path_native()`, bypassing services.
5. CLI `embed` sync path still calls `embed_path_native()` directly; only async enqueue routes through `services::embed`.
6. Refresh lifecycle and scheduling have no shared service boundary yet:
   - `crates/cli/commands/refresh.rs`
   - `crates/cli/commands/refresh/schedule.rs`
   - `crates/cli/commands/watch.rs` (`run-now` refresh dispatch)
   - `crates/mcp/server/handlers_refresh_status.rs`
7. CLI `ingest` still performs source classification in the transport layer via `ingest::classify::classify_target()` instead of a service-owned dispatch boundary.
8. CLI `debug` still performs doctor collection + LLM troubleshooting directly instead of calling a service.

## Design Constraints

- Do not change successful transport output formats unless tests are updated intentionally.
- Keep job persistence, Qdrant calls, and external HTTP calls in existing lower layers; services should orchestrate them, not duplicate them.
- Prefer adding typed option/result structs over raw `serde_json::Value` where the shape is stable.
- Preserve the existing split between sync direct service calls and async enqueue service calls in web/MCP.
- Do not revert unrelated in-flight work from other agents.

## Task 1: Baseline service-gap regression coverage

### Files
- Create: `crates/cli/commands/services_migration_gap_tests.rs`
- Modify: `crates/cli/commands/mod.rs` or nearest test module registration point

### Steps
1. Add failing tests that codify the remaining expected service boundaries instead of implementation details.
2. Cover, at minimum:
   - `run_evaluate()` must delegate through the service layer.
   - `run_suggest()` must delegate through the service layer.
   - async crawl enqueue path must not call `jobs::crawl::start_crawl_jobs_batch()` directly from CLI.
   - refresh MCP handler must route through a refresh service module rather than direct jobs calls.
3. Use narrow seam tests where possible:
   - pure mapping/helper tests for new services
   - module-level migration tests for CLI command adapters
   - handler tests for MCP refresh paths
4. Run the new tests and confirm they fail before implementation.

### Verify failing state
```bash
cargo test services_migration_gap -- --nocapture
```

### Commit
```bash
git add crates/cli/commands/services_migration_gap_tests.rs crates/cli/commands/mod.rs
git commit -m "test(services): codify remaining migration gaps"
```

## Task 2: Finish CLI query-adapter migration for evaluate and suggest

### Files
- Modify: `crates/cli/commands/evaluate.rs`
- Modify: `crates/cli/commands/suggest.rs`
- Modify: `crates/services/query.rs`
- Modify: `crates/services/types/service.rs`
- Modify or create: focused tests near `evaluate.rs`, `suggest.rs`, and `query.rs`

### Steps
1. Add or extend typed helpers if current `EvaluateResult` / `SuggestResult` are insufficient for CLI formatting parity.
2. Rewrite `run_evaluate()` to:
   - resolve question from CLI input
   - call `query::evaluate()`
   - print JSON/human output without calling vector-op native CLI helpers
3. Rewrite `run_suggest()` to:
   - resolve optional focus from CLI input
   - call `query::suggest()`
   - preserve current JSON/human formatting
4. Remove obsolete “Phase 2” comments claiming service extraction is still pending.
5. Keep `vector/ops` helpers private to the service boundary where practical.

### Verify
```bash
cargo test run_evaluate
cargo test run_suggest
cargo test query::tests
```

### Commit
```bash
git add crates/cli/commands/evaluate.rs crates/cli/commands/suggest.rs crates/services/query.rs crates/services/types/service.rs
git commit -m "refactor(cli): route evaluate and suggest through services"
```

## Task 3: Route CLI async crawl enqueue through `services::crawl`

### Files
- Modify: `crates/cli/commands/crawl.rs`
- Modify: `crates/services/crawl.rs`
- Modify: `crates/cli/commands/runtime_migration_tests.rs` or nearest crawl migration tests

### Steps
1. Replace direct `start_crawl_jobs_batch()` usage in `run_async_enqueue_multi()` with `crawl_start()`.
2. If the CLI still needs URL-to-job-id pairing for display, extend `CrawlStartResult` to carry that structure instead of collapsing to job IDs only.
3. Keep sync crawl behavior unchanged for this task; only move async enqueue orchestration behind the service.
4. Update tests to assert CLI formatting still prints job IDs and status hints correctly.

### Verify
```bash
cargo test crawl
```

### Commit
```bash
git add crates/cli/commands/crawl.rs crates/services/crawl.rs
git commit -m "refactor(crawl): use services layer for async enqueue"
```

## Task 4: Extract shared sync scrape orchestration into `services::scrape`

### Files
- Modify: `crates/services/scrape.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/cli/commands/scrape.rs`
- Modify: `crates/cli/commands/scrape/scrape_migration_tests.rs`

### Steps
1. Introduce typed service-level result structs for:
   - one scraped page payload
   - multi-URL scrape batch aggregation
   - optional embed batch metadata when `cfg.embed` is enabled
2. Move scrape orchestration out of CLI:
   - URL normalization/validation
   - single-page fetch orchestration
   - output selection payload assembly
   - batch embed directory creation and embed invocation boundary
3. Leave terminal presentation and file emission in CLI only if transport-specific; otherwise centralize shared output data in the service.
4. Ensure the SSRF guard remains ahead of any network activity.
5. Preserve current behavior for `--output`, `--json`, multi-URL scrape, and `--embed`.

### Verify
```bash
cargo test scrape_migration_tests
cargo test run_scrape
```

### Commit
```bash
git add crates/services/scrape.rs crates/services/types/service.rs crates/cli/commands/scrape.rs crates/cli/commands/scrape/scrape_migration_tests.rs
git commit -m "refactor(scrape): move sync scrape orchestration into services"
```

## Task 5: Add sync embed service and migrate CLI embed wait-mode

### Files
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/cli/commands/embed.rs`
- Modify: embed-related tests near `crates/cli/commands/embed.rs`

### Steps
1. Add a sync service function, for example `embed_run(cfg, input, tx)`, that wraps `embed_path_native()` and returns a typed summary.
2. Keep `embed_start()` as the async enqueue path.
3. Update CLI `run_embed()` wait-mode to call the sync service instead of `embed_path_native()` directly.
4. Preserve current default-input resolution semantics and terminal output.

### Verify
```bash
cargo test embed
```

### Commit
```bash
git add crates/services/embed.rs crates/services/types/service.rs crates/cli/commands/embed.rs
git commit -m "refactor(embed): route sync embed through services"
```

## Task 6: Introduce a `services::refresh` boundary and migrate CLI + MCP refresh flows

### Files
- Create: `crates/services/refresh.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/cli/commands/refresh/schedule.rs`
- Modify: `crates/cli/commands/watch.rs`
- Modify: `crates/mcp/server/handlers_refresh_status.rs`
- Add/modify tests near refresh CLI and MCP handlers

### Steps
1. Define typed refresh service APIs for:
   - start/status/cancel/list/cleanup/clear/recover job lifecycle
   - schedule list/create/delete/enable/disable
   - due-schedule dispatch where shared
2. Move all direct `jobs::refresh::*` calls out of CLI and MCP handlers into `services::refresh`.
3. Preserve transport-specific validation in adapters only where appropriate:
   - e.g. MCP request-shape validation stays in handler
   - URL validation and schedule construction logic should live in service helpers if shared
4. Update `watch run-now` refresh dispatch to call the refresh service instead of `start_refresh_job()` directly.
5. Keep JSON response shapes stable for CLI and MCP.

### Verify
```bash
cargo test refresh
cargo test handlers_refresh_status
cargo test watch
```

### Commit
```bash
git add crates/services/refresh.rs crates/services.rs crates/services/types/service.rs crates/cli/commands/refresh.rs crates/cli/commands/refresh/schedule.rs crates/cli/commands/watch.rs crates/mcp/server/handlers_refresh_status.rs
git commit -m "refactor(refresh): add shared refresh services boundary"
```

## Task 7: Move ingest target classification behind a service-owned dispatch API

### Files
- Modify: `crates/services/ingest.rs`
- Modify: `crates/cli/commands/ingest.rs`
- Modify: `crates/cli/commands/ingest_common.rs`
- Add/modify ingest tests

### Steps
1. Add a service entry point that accepts raw target text plus config and returns either:
   - a classified `IngestSource`, or
   - a fully executed sync/enqueued result depending on the API shape chosen.
2. Remove direct CLI dependency on `ingest::classify::classify_target()`.
3. Keep variant-specific ingest execution in the service layer.
4. Preserve current user-facing error messages for unknown targets.

### Verify
```bash
cargo test ingest
cargo test classify
```

### Commit
```bash
git add crates/services/ingest.rs crates/cli/commands/ingest.rs crates/cli/commands/ingest_common.rs
git commit -m "refactor(ingest): move target classification into services"
```

## Task 8: Add debug service and migrate CLI debug

### Files
- Create or modify: `crates/services/debug.rs` or `crates/services/system.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/cli/commands/debug.rs`
- Add/modify tests around debug command and service mapping

### Steps
1. Extract doctor-report collection plus LLM troubleshooting request into a typed service.
2. Keep CLI rendering of human vs JSON output local.
3. Preserve required env validation and the exact OpenAI endpoint construction rules.
4. If web direct dispatch for `debug` is desired later, wire it only after the service boundary exists.

### Verify
```bash
cargo test debug
```

### Commit
```bash
git add crates/services.rs crates/services/types/service.rs crates/cli/commands/debug.rs
git commit -m "refactor(debug): route debug command through services"
```

## Task 9: Cleanup drift and enforce new source-of-truth boundaries

### Files
- Modify: `crates/web/execute.rs`
- Modify: `README.md`
- Modify: any services-layer docs that still claim migration is complete or missing in the wrong places
- Modify: issue #13 after implementation, not before

### Steps
1. Update stale comments/docs:
   - `crates/web/execute.rs` currently claims `suggest` and `evaluate` are still subprocess fallback, which is no longer true.
   - Any services-layer docs claiming MCP is fully migrated must be narrowed until refresh is actually moved.
2. Remove obsolete “Phase 2” comments in migrated CLI adapters.
3. Add a brief architecture note documenting the remaining transport adapters and service ownership lines.

### Verify
```bash
cargo test
cargo check --all-targets
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

### Commit
```bash
git add crates/web/execute.rs README.md docs
git commit -m "docs(services): align migration docs with live architecture"
```

## Final Verification Gate

Run the full verification suite before claiming issue #13 complete:

```bash
cargo fmt --all
cargo check --all-targets
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

If any new service module is added, also run targeted command smoke tests:

```bash
cargo run --bin axon -- evaluate "test question" --json
cargo run --bin axon -- suggest "rust async" --json
cargo run --bin axon -- crawl https://example.com --json
cargo run --bin axon -- refresh list --json
```

## Definition of Done

- No CLI, MCP, or web transport adapter calls lower-layer business logic directly for the scoped commands above.
- Shared orchestration lives in `crates/services/*`.
- Transport adapters only parse inputs, call services, and format outputs.
- Tests cover the new boundaries and preserve user-facing output shape.
- Issue #13 is updated to reflect the actual remaining scope and can be closed only after these tasks land.
