# Services Layer Refactor Implementation Plan

**Goal:** Extract shared business logic into `crates/services/` and rewire CLI, MCP, and Web to call services directly with no behavior regressions.

**Architecture:** Add a typed services layer (`crates/services/*`) with transport-agnostic return models and option structs. CLI/MCP/Web become thin adapters that map inputs to service calls and map service outputs to their existing transport contracts. Web execution drops subprocess spawning and uses direct service dispatch with fire-and-forget semantics for async modes.

**Tech Stack:** Rust, tokio, serde/serde_json, sqlx, axum WebSocket, existing jobs/vector/ingest modules

---

## Non-Negotiable Decisions

1. Preserve MCP request contract and behavior in this refactor.
- Keep schema fields and semantics for `limit`, `offset`, `max_points`, `search_time_range`, `response_mode`.

2. Web async semantics switch to fire-and-forget.
- For async modes, enqueue and return immediately with job IDs.
- No internal polling loop in web execution path.

3. No Agent Teams syntax.
- This plan is executable by Codex, Claude, or a human with regular parallel subagents/workers.

---

## Working Rules

- TDD required for every task:
  1. Write failing test.
  2. Run and confirm failure.
  3. Implement minimal fix.
  4. Run and confirm pass.
  5. Commit.
- Keep files under monolith limits. (Must be 500 lines or fewer, functions must be 120 lines or fewer).
- Prefer call-through wrappers over duplicated logic.
- Do not break `run_*` CLI signatures used by dispatcher.
- Do not mutate MCP schema shape in this plan.
- Modern Rust module layout only: do not create new `mod.rs` files during this refactor; prefer `foo.rs` module files.
- Blocker acceptance criterion: any non-transport business logic left in CLI/MCP/Web after Phase 6 blocks completion.

---

## Service API Contract (Exact)

Create option structs in `crates/services/types.rs`:

```rust
#[derive(Debug, Clone, Copy)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct RetrieveOptions {
    pub max_points: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum ServiceTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    pub limit: usize,
    pub offset: usize,
    pub time_range: Option<ServiceTimeRange>,
}

#[derive(Debug, Clone, Copy)]
pub struct MapOptions {
    pub limit: usize,
    pub offset: usize,
}
```

Service signatures (minimum):

```rust
// system
pub async fn sources(cfg: &Config, pagination: Pagination) -> Result<SourcesResult, Box<dyn Error>>;
pub async fn domains(cfg: &Config, pagination: Pagination) -> Result<DomainsResult, Box<dyn Error>>;
pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>>;
pub async fn doctor(cfg: &Config) -> Result<DoctorResult, Box<dyn Error>>;
pub async fn full_status(cfg: &Config) -> Result<StatusResult, Box<dyn Error>>;
pub async fn dedupe(cfg: &Config, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<DedupeResult, Box<dyn Error>>;

// query/search/map/retrieve
pub async fn query(cfg: &Config, text: &str, opts: Pagination) -> Result<QueryResult, Box<dyn Error>>;
pub async fn retrieve(cfg: &Config, url: &str, opts: RetrieveOptions) -> Result<RetrieveResult, Box<dyn Error>>;
pub async fn ask(cfg: &Config, question: &str, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<AskResult, Box<dyn Error>>;
pub async fn evaluate(cfg: &Config, question: &str) -> Result<EvaluateResult, Box<dyn Error>>;
pub async fn suggest(cfg: &Config, focus: Option<&str>) -> Result<SuggestResult, Box<dyn Error>>;
pub async fn search(cfg: &Config, query: &str, opts: SearchOptions, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<SearchResult, Box<dyn Error>>;
pub async fn research(cfg: &Config, query: &str, opts: SearchOptions) -> Result<ResearchResult, Box<dyn Error>>;
pub async fn discover(cfg: &Config, url: &str, opts: MapOptions, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<MapResult, Box<dyn Error>>;

// lifecycle
pub async fn crawl_start(cfg: &Config, urls: &[String], tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<CrawlStartResult, Box<dyn Error>>;
pub async fn crawl_status(cfg: &Config, job_id: Uuid) -> Result<CrawlJobResult, Box<dyn Error>>;
// ... cancel/list/cleanup/clear/recover for crawl/embed/extract

// ingest/screenshot
pub async fn ingest_github(cfg: &Config, owner: &str, repo: &str, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<IngestResult, Box<dyn Error>>;
pub async fn ingest_reddit(cfg: &Config, target: &str, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<IngestResult, Box<dyn Error>>;
pub async fn ingest_youtube(cfg: &Config, url: &str, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<IngestResult, Box<dyn Error>>;
pub async fn ingest_sessions(cfg: &Config, tx: Option<mpsc::Sender<ServiceEvent>>) -> Result<IngestResult, Box<dyn Error>>;
pub async fn screenshot_capture(cfg: &Config, url: &str) -> Result<ScreenshotResult, Box<dyn Error>>;
```

---

## Phase 0: Baseline and Branch Safety

### Task 0.1: Capture current baseline state

**Files:**
- Modify: `docs/plans/2026-03-03-services-layer-plan.md` (append baseline notes section while executing)

**Step 1: Run baseline checks**

Run:
```bash
cargo check && cargo test && cargo clippy && cargo fmt --check
```

Expected: all pass, or exact failures captured.

**Step 2: Record baseline functional probes**

Run:
```bash
./scripts/axon doctor
./scripts/axon sources --json
./scripts/axon query "baseline-test" --json
```

Expected: successful output from all three commands.

**Step 3: Commit baseline notes (if changed)**

```bash
git add docs/plans/2026-03-03-services-layer-plan.md
git commit -m "docs(plan): record baseline verification for services refactor"
```

---

## Phase 1: Foundation Scaffolding

### Task 1.1: Add services module and compile stubs

**Files:**
- Create: `crates/services.rs`
- Create: `crates/services/types.rs`
- Create: `crates/services/events.rs`
- Create: `crates/services/system.rs`
- Create: `crates/services/query.rs`
- Create: `crates/services/scrape.rs`
- Create: `crates/services/map.rs`
- Create: `crates/services/search.rs`
- Create: `crates/services/crawl.rs`
- Create: `crates/services/embed.rs`
- Create: `crates/services/extract.rs`
- Create: `crates/services/ingest.rs`
- Create: `crates/services/screenshot.rs`
- Modify: `crates.rs`
- Test: `tests/services_compile_services_smoke.rs`

**Step 1: Write failing test**

Create `tests/services_compile_services_smoke.rs`:
```rust
#[test]
fn services_module_exports_exist() {
    let _ = axon::crates::services::events::ServiceEvent::Log {
        level: "info".to_string(),
        message: "ok".to_string(),
    };
}
```

**Step 2: Run test to verify failure**

Run:
```bash
cargo test --test services_compile_services_smoke -- --nocapture
```

Expected: fails because `services` module does not exist.

**Step 3: Add minimal scaffolding implementation**

- Add `pub mod services;` to `crates.rs`.
- Add module exports in `crates/services.rs`.
- Add `ServiceEvent` enum + `emit()` helper in `events.rs`.
- Add empty compile-valid modules for each service file.

**Step 4: Run test to verify pass**

Run:
```bash
cargo test --test services_compile_services_smoke -- --nocapture
cargo check
```

Expected: pass.

**Step 5: Commit**

```bash
git add crates.rs crates/services.rs crates/services tests/services_compile_services_smoke.rs
git commit -m "feat(services): scaffold services module and events/types base"
```

---

## Phase 2: Implement Services (Parallel by File Ownership)

Parallel ownership:
- Worker A: `system.rs`, `query.rs`
- Worker B: `scrape.rs`, `map.rs`, `search.rs`
- Worker C: `crawl.rs`, `embed.rs`, `extract.rs`
- Worker D: `ingest.rs`, `screenshot.rs`

### Task 2.1: System services

**Files:**
- Modify: `crates/services/system.rs`
- Modify: `crates/services/types.rs`
- Test: `tests/services_system_services.rs`

**Step 1: Write failing tests**

Add tests for:
- pagination passthrough for sources/domains
- `stats` mapping shape
- `doctor` mapping shape

Example:
```rust
#[test]
fn maps_source_facets_to_sources_result() {
    // pure mapping helper test
}
```

**Step 2: Run to fail**

```bash
cargo test --test services_system_services -- --nocapture
```

**Step 3: Implement minimal service wrappers**

Call-through targets:
- `qdrant_url_facets`, `qdrant_domain_facets`
- `stats_payload` equivalent logic extraction
- `build_doctor_report` logic moved to service
- status aggregation from current status handlers

**Step 4: Run to pass**

```bash
cargo test --test services_system_services -- --nocapture
cargo check
```

**Step 5: Commit**

```bash
git add crates/services/system.rs crates/services/types.rs tests/services_system_services.rs
git commit -m "feat(services): implement system services (sources/domains/stats/doctor/status/dedupe)"
```

### Task 2.2: Query services

**Files:**
- Modify: `crates/services/query.rs`
- Modify: `crates/services/types.rs`
- Test: `tests/services_query_services.rs`

**Step 1: Write failing tests**
- query pagination mapping
- retrieve max_points passthrough
- ask result shape

**Step 2: Run to fail**
```bash
cargo test --test services_query_services -- --nocapture
```

**Step 3: Implement minimal wrappers**

Call-through targets:
- `query_results(cfg, text, limit, offset)`
- `retrieve_result(cfg, url, max_points)`
- ask chain: `build_ask_context` -> `ask_llm_answer` -> `normalize_ask_answer`
- evaluate/suggest wrappers from existing commands

**Step 4: Run to pass**
```bash
cargo test --test services_query_services -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/services/query.rs crates/services/types.rs tests/services_query_services.rs
git commit -m "feat(services): implement query services (query/retrieve/ask/evaluate/suggest)"
```

### Task 2.3: Scrape/map/search services

**Files:**
- Modify: `crates/services/scrape.rs`
- Modify: `crates/services/map.rs`
- Modify: `crates/services/search.rs`
- Test: `tests/services_discovery_services.rs`

**Step 1: Write failing tests**
- map limit/offset behavior preserved
- search includes `search_time_range` mapping
- research shape preserved

**Step 2: Run to fail**
```bash
cargo test --test services_discovery_services -- --nocapture
```

**Step 3: Implement wrappers**
- reuse `scrape_payload` pathway logic but return typed result
- wrap `map_payload`
- wrap `search_results` / `research_payload`

**Step 4: Run to pass**
```bash
cargo test --test services_discovery_services -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/services/scrape.rs crates/services/map.rs crates/services/search.rs tests/services_discovery_services.rs
git commit -m "feat(services): implement scrape/map/search/research services"
```

### Task 2.4: Lifecycle + ingest + screenshot services

**Files:**
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/screenshot.rs`
- Test: `tests/services_lifecycle_services.rs`

**Step 1: Write failing tests**
- status/cancel/list mapping for one lifecycle domain
- ingest result mapping
- screenshot output mapping

**Step 2: Run to fail**
```bash
cargo test --test services_lifecycle_services -- --nocapture
```

**Step 3: Implement wrappers**
- call current jobs/ingest/screenshot functions via `&Config`
- map outputs to service types

**Step 4: Run to pass**
```bash
cargo test --test services_lifecycle_services -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/services/crawl.rs crates/services/embed.rs crates/services/extract.rs crates/services/ingest.rs crates/services/screenshot.rs tests/services_lifecycle_services.rs
git commit -m "feat(services): implement lifecycle ingest and screenshot services"
```

**Phase Gate:**
```bash
cargo check && cargo test
```

---

## Phase 3: CLI Rewire to Services

### Task 3.1: Rewire vector/system CLI handlers

**Files:**
- Modify: `crates/vector/ops/qdrant/commands.rs`
- Modify: `crates/vector/ops/stats.rs`
- Modify: `crates/cli/commands/status.rs`
- Modify: `crates/cli/commands/doctor.rs`
- Test: `tests/cli_system_rewire_regression.rs`

**Step 1: Write failing regression test**
- ensure commands still emit required JSON keys.

**Step 2: Run to fail**
```bash
cargo test --test cli_system_rewire_regression -- --nocapture
```

**Step 3: Rewire minimal internals**
- keep `run_*` signatures.
- replace embedded business logic with services calls.

**Step 4: Run to pass**
```bash
cargo test --test cli_system_rewire_regression -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/vector/ops/qdrant/commands.rs crates/vector/ops/stats.rs crates/cli/commands/status.rs crates/cli/commands/doctor.rs tests/cli_system_rewire_regression.rs
git commit -m "refactor(cli): route system/stats/doctor/status handlers through services"
```

### Task 3.2: Rewire query/search/scrape/job/ingest CLI groups

**Files:**
- Modify: `crates/vector/ops/commands/query.rs`
- Modify: `crates/vector/ops/commands/ask.rs`
- Modify: `crates/vector/ops/commands/evaluate.rs`
- Modify: `crates/vector/ops/commands/suggest.rs`
- Modify: `crates/cli/commands/scrape.rs`
- Modify: `crates/cli/commands/map.rs`
- Modify: `crates/cli/commands/search.rs`
- Modify: `crates/cli/commands/research.rs`
- Modify: `crates/cli/commands/crawl.rs`
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/extract.rs`
- Modify: `crates/cli/commands/github.rs`
- Modify: `crates/cli/commands/reddit.rs`
- Modify: `crates/cli/commands/youtube.rs`
- Modify: `crates/cli/commands/sessions.rs`
- Modify: `crates/cli/commands/screenshot.rs`
- Test: `tests/cli_full_rewire_smoke.rs`

**Step 1: Write failing smoke test matrix**
- verify representative commands execute and include expected keys.

**Step 2: Run to fail**
```bash
cargo test --test cli_full_rewire_smoke -- --nocapture
```

**Step 3: Rewire handlers to services**

**Step 4: Run to pass**
```bash
cargo test --test cli_full_rewire_smoke -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/vector/ops/commands crates/cli/commands tests/cli_full_rewire_smoke.rs
git commit -m "refactor(cli): route query scrape search lifecycle and ingest handlers through services"
```

**Phase Gate:**
```bash
cargo check && cargo test && cargo clippy
```

---

## Phase 4: MCP Rewire (Contract-Preserving)

### Task 4.1: Add MCP <-> service option mappers

**Files:**
- Modify: `crates/mcp/server/common.rs`
- Test: `tests/mcp_option_mappers.rs`

**Step 1: Write failing tests**
- mapping for `SearchTimeRange` to service enum
- pagination clamp/offset behavior

**Step 2: Run to fail**
```bash
cargo test --test mcp_option_mappers -- --nocapture
```

**Step 3: Add mapper helpers**

Example helper:
```rust
pub(super) fn to_service_search_options(
    limit: Option<usize>,
    offset: Option<usize>,
    tr: Option<SearchTimeRange>,
) -> SearchOptions { ... }
```

**Step 4: Run to pass**
```bash
cargo test --test mcp_option_mappers -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/mcp/server/common.rs crates/mcp/server
git commit -m "refactor(mcp): add request-to-service option mappers"
```

### Task 4.2: Rewire MCP handlers to services

**Files:**
- Modify: `crates/mcp/server/handlers_system.rs`
- Modify: `crates/mcp/server/handlers_query.rs`
- Modify: `crates/mcp/server/handlers_crawl_extract.rs`
- Modify: `crates/mcp/server/handlers_embed_ingest.rs`
- Test: `tests/mcp_contract_parity.rs`

**Step 1: Write failing parity tests**
Matrix (must assert field behavior):
- `query`: limit + offset
- `retrieve`: `max_points`
- `search`/`research`: `search_time_range`
- `sources`/`domains`: limit + offset + response_mode
- lifecycle list/status/cancel shape

**Step 2: Run to fail**
```bash
cargo test --test mcp_contract_parity -- --nocapture
```

**Step 3: Rewire handlers**
- replace direct `*_payload` / old helper calls with service calls.
- keep `AxonToolResponse` and `response_mode` behavior.

**Step 4: Run to pass**
```bash
cargo test --test mcp_contract_parity -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/mcp/server/handlers_system.rs crates/mcp/server/handlers_query.rs crates/mcp/server/handlers_crawl_extract.rs crates/mcp/server/handlers_embed_ingest.rs tests/mcp_contract_parity.rs
git commit -m "refactor(mcp): route handlers through services with contract parity"
```

**Phase Gate:**
```bash
cargo check && cargo test && cargo clippy
```

---

## Phase 5: Web Rewrite (Direct Dispatch, No Subprocess)

## Design before coding

### Required structural changes

1. Change start_server signature:
- `crates/web.rs`

From:
```rust
pub async fn start_server(port: u16) -> Result<(), Box<dyn Error>>
```
To:
```rust
pub async fn start_server(cfg: Config) -> Result<(), Box<dyn Error>>
```

2. Update serve command callsite:
- `crates/cli/commands/serve.rs`

From:
```rust
crate::crates::web::start_server(cfg.serve_port).await
```
To:
```rust
crate::crates::web::start_server(cfg.clone()).await
```

3. Store base config in web app state:
- `crates/web.rs`

```rust
struct AppState {
    // existing fields...
    base_cfg: Config,
}
```

4. Replace `args.rs` behavior with typed web override mapping:
- Create: `crates/web/execute/overrides.rs`

```rust
pub(crate) fn cfg_with_ws_overrides(base: &Config, mode: &str, flags: &serde_json::Value) -> Result<Config, String> { ... }
```

This function must preserve existing allowed flags semantics currently in `args.rs`.

5. Remove subprocess dependencies:
- Delete `crates/web/execute/args.rs`
- Delete `crates/web/execute/exe.rs`
- Delete `crates/web/execute/polling.rs`

6. Refactor execute entrypoint:
- `crates/web/execute.rs`
- `handle_command` now receives `&Config` (or cloned effective cfg) and dispatches service calls.

### Task 5.1: Web config plumbing and override mapper

**Files:**
- Modify: `crates/cli/commands/serve.rs`
- Modify: `crates/web.rs`
- Modify: `crates/web/execute.rs`
- Create: `crates/web/execute/overrides.rs`
- Test: `tests/web_ws_override_mapping.rs`

**Step 1: Write failing tests**
- `max_pages`/`render_mode`/`wait`/`responses_mode` mapping checks.
- reject path traversal for output-related flags.

**Step 2: Run to fail**
```bash
cargo test --test web_ws_override_mapping -- --nocapture
```

**Step 3: Implement config plumbing + mapper**

**Step 4: Run to pass**
```bash
cargo test --test web_ws_override_mapping -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/cli/commands/serve.rs crates/web.rs crates/web/execute.rs crates/web/execute/overrides.rs tests/web_ws_override_mapping.rs
git commit -m "refactor(web): plumb base Config and ws override mapping for direct service dispatch"
```

### Task 5.2: Replace sync execution with direct services

**Files:**
- Modify: `crates/web/execute/sync_mode.rs`
- Modify: `crates/web/execute.rs`
- Test: `crates/web/execute/tests/ws_protocol_tests.rs`

**Step 1: Write failing tests**
- sync mode command emits start/output/done without spawning process.

**Step 2: Run to fail**
```bash
cargo test ws_protocol_tests -- --nocapture
```

**Step 3: Implement direct sync dispatch**

**Step 4: Run to pass**
```bash
cargo test ws_protocol_tests -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/web/execute/sync_mode.rs crates/web/execute.rs crates/web/execute/tests/ws_protocol_tests.rs
git commit -m "refactor(web): route sync ws execute modes directly through services"
```

### Task 5.3: Replace async execution with fire-and-forget

**Files:**
- Modify: `crates/web/execute/async_mode.rs`
- Modify: `crates/web/execute/cancel.rs`
- Modify: `crates/web/execute/constants.rs`
- Modify: `crates/web/execute.rs`
- Delete: `crates/web/execute/polling.rs`
- Delete: `crates/web/execute/args.rs`
- Delete: `crates/web/execute/exe.rs`
- Test: `crates/web/execute/tests/ws_event_v2_tests.rs`
- Test: `tests/web_ws_async_fire_and_forget.rs`

**Step 1: Write failing tests**
- async mode emits enqueue/job_id and returns immediately.
- cancel uses service lifecycle cancel, no subprocess.
- no code path calls deleted modules.

**Step 2: Run to fail**
```bash
cargo test ws_event_v2_tests -- --nocapture
cargo test --test web_ws_async_fire_and_forget -- --nocapture
```

**Step 3: Implement fire-and-forget flow**

**Step 4: Run to pass**
```bash
cargo test ws_event_v2_tests -- --nocapture
cargo test --test web_ws_async_fire_and_forget -- --nocapture
cargo check
```

**Step 5: Commit**
```bash
git add crates/web/execute/async_mode.rs crates/web/execute/cancel.rs crates/web/execute/constants.rs crates/web/execute.rs crates/web/execute/tests/ws_event_v2_tests.rs tests/web_ws_async_fire_and_forget.rs
git rm crates/web/execute/polling.rs crates/web/execute/args.rs crates/web/execute/exe.rs
git commit -m "refactor(web): replace subprocess async execution with fire-and-forget service dispatch"
```

**Phase Gate:**
```bash
cargo check && cargo test && cargo clippy
```

---

## Phase 6: Dead Code Cleanup and Re-export Hygiene

### Task 6.1: Remove superseded payload/result helpers only after callsite verification

**Files:**
- Modify: `crates/vector/ops/qdrant/commands.rs`
- Modify: `crates/vector/ops/stats.rs`
- Modify: `crates/vector/ops/commands/query.rs`
- Modify: `crates/vector/ops/commands/ask.rs`
- Modify: `crates/cli/commands/scrape.rs`
- Modify: `crates/cli/commands/map.rs`
- Modify: `crates/cli/commands/search.rs`
- Modify: `crates/cli/commands/research.rs`
- Modify: `crates/cli/commands/doctor.rs`
- Modify: `crates/vector/ops.rs`
- Modify: `crates/vector/ops/qdrant.rs`
- Test: `tests/cleanup_no_legacy_payload_symbols.rs`

**Step 1: Write failing test/check script**

Use `rg`-based assertion in test helper or script:
```bash
rg -n "(sources_payload|domains_payload|stats_payload|query_results|ask_payload|search_results|research_payload|map_payload|scrape_payload|build_doctor_report)" crates
```

**Step 2: Verify current failures**
- command above should find matches before deletion.

**Step 3: Delete migrated legacy helpers + stale re-exports**

**Step 4: Re-run checks and tests**
```bash
rg -n "(sources_payload|domains_payload|stats_payload|query_results|ask_payload|search_results|research_payload|map_payload|scrape_payload|build_doctor_report)" crates
cargo check && cargo test
```

Expected: no remaining symbols except intentionally retained ones (if any, document explicitly).

**Step 5: Commit**
```bash
git add crates/vector/ops/qdrant/commands.rs crates/vector/ops/stats.rs crates/vector/ops/commands/query.rs crates/vector/ops/commands/ask.rs crates/cli/commands/scrape.rs crates/cli/commands/map.rs crates/cli/commands/search.rs crates/cli/commands/research.rs crates/cli/commands/doctor.rs crates/vector/ops.rs crates/vector/ops/qdrant.rs tests/cleanup_no_legacy_payload_symbols.rs
git commit -m "chore: remove superseded payload/result helpers and stale re-exports"
```

---

## Final Verification Matrix

Run all:
```bash
cargo check
cargo test
cargo clippy
cargo fmt --check
./scripts/axon doctor
./scripts/axon sources --json
./scripts/axon domains --json
./scripts/axon query "test" --json
./scripts/axon ask "what is axon" --json
./scripts/axon crawl https://example.com --wait false --json
./scripts/axon crawl status <job_id> --json
```

MCP parity (explicit):
- Query with `limit` + `offset` -> exact windowing preserved.
- Retrieve with `max_points` -> forwarded and respected.
- Search/Research with `search_time_range` -> mapped and respected.
- Domains/Sources with `response_mode` and pagination -> preserved envelope behavior.
- Lifecycle status/cancel/list/cleanup/clear/recover -> unchanged contract keys.

Web parity (explicit):
- Sync mode emits `start -> output -> done`.
- Async mode emits enqueue/job ID and returns immediately (no polling).
- Cancel path uses service cancel and emits expected event shape.
- No subprocess path remains in `crates/web/execute.rs`.

---

## Commit Sequence (Recommended)

1. `feat(services): scaffold services module and events/types base`
2. `feat(services): implement system services`
3. `feat(services): implement query services`
4. `feat(services): implement scrape map and search services`
5. `feat(services): implement lifecycle ingest and screenshot services`
6. `refactor(cli): route handlers through services`
7. `refactor(mcp): route handlers through services with contract parity`
8. `refactor(web): plumb config and replace subprocess execution with direct service dispatch`
9. `chore: remove superseded payload/result helpers and stale re-exports`

---

Plan complete and saved to `docs/plans/2026-03-03-services-layer-plan.md`. Two execution options:

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

Which approach?

This is the way.

---

## Execution Baseline Notes (Phase 0)

- Date: 2026-03-04 (America/New_York)
- Branch: `feat/services-layer-refactor`
- Dispatcher preflight:
  - `git status --short` showed one pre-existing untracked entry: `8001`
  - Plan file present and current at `docs/plans/2026-03-03-services-layer-plan.md`
- Baseline quality gate:
  - `cargo check`: pass
  - `cargo test`: pass (`789 passed; 0 failed; 3 ignored`)
  - `cargo clippy`: pass
  - `cargo fmt --check`: pass
- Baseline functional probes:
  - `./scripts/axon doctor`: pass
  - `./scripts/axon sources --json`: pass (large JSON payload)
  - `./scripts/axon query "baseline-test" --json`: pass

---

## Agent Launch Packet (Paste Into Every Worker Prompt)

Use this exact preamble for every dispatched agent:

```text
You are implementing one task from the Services Layer Refactor plan.

Hard rules:
1) Follow ONLY your assigned file ownership list. Do not edit files outside your assignment.
2) Do not create new mod.rs files. Use modern Rust module layout (foo.rs).
3) Preserve public signatures used by dispatchers/handlers unless the plan task explicitly changes them.
4) Keep MCP schema contract stable (limit/offset/max_points/search_time_range/response_mode behavior).
5) For web async modes, preserve fire-and-forget semantics (enqueue + immediate return, no polling loop).
6) Use TDD: write failing test first, run it, implement minimal fix, run pass, then commit.
7) No broad refactors. No opportunistic cleanup outside assigned files.
8) If blocked by missing context/signature conflict, STOP and report instead of guessing.
9) You are NOT alone in this repository. Other agents are working concurrently.
10) Forbidden git actions: do not run git stash, git reset (--hard or otherwise), git checkout --, git clean, force-push, or any history rewrite.
11) Do not delete or revert changes you did not author.
12) Your role is implementation-only for your assigned task; no repo hygiene operations outside that scope.

Required output in final handoff message:
- Files changed
- Tests added/updated
- Commands run + pass/fail summary
- Any assumptions made
- Exact commit hash
```

---

## Dispatcher Preflight Checklist

Run before dispatching any worker:

1. Confirm branch context.
```bash
git status --short
git branch --show-current
```

2. Confirm plan file is latest.
```bash
ls -l docs/plans/2026-03-03-services-layer-plan.md
```

3. Confirm baseline quality gate is green.
```bash
cargo check && cargo test && cargo clippy && cargo fmt --check
```

4. Confirm no conflicting in-flight changes in target files for first wave.
```bash
git status --short
```

5. Prepare worker task cards with exact file ownership and exact done criteria.
6. Include mandatory repo-safety notice in every worker prompt:
   - "You are not alone in the repo; concurrent edits are expected."
   - "Never stash, reset, checkout --, clean, or rewrite git history."
   - "Never revert/delete changes outside your assignment."

Do not dispatch if baseline gate is red unless failure is documented and explicitly accepted.

---

## Done Definition (Machine-Checkable)

A task is complete only if all are true:

1. Assigned tests exist and pass.
2. `cargo check` passes.
3. No edits outside assigned file ownership (except approved shared test file).
4. No new `mod.rs` files introduced.
5. No schema contract drift for MCP tasks.
6. Commit exists with required scope message.

Validation commands:

```bash
# No forbidden module style additions
rg --files crates | rg "/mod\.rs 

# Current changes overview
git diff --name-only HEAD~1..HEAD

# Project compile gate
cargo check
```

For MCP tasks, also run:

```bash
cargo test --test mcp_option_mappers -- --nocapture
cargo test --test mcp_contract_parity -- --nocapture
```

For web tasks, also run:

```bash
cargo test ws_protocol_tests -- --nocapture
cargo test ws_event_v2_tests -- --nocapture
cargo test --test web_ws_override_mapping -- --nocapture
cargo test --test web_ws_async_fire_and_forget -- --nocapture
```

---

## Merge/Rebase Protocol (Dispatcher)

Use this order per wave:

1. Land smallest/lowest-risk worker first.
2. After each merge, run gate:
```bash
cargo check && cargo test
```
3. Rebase remaining worker branches onto latest integration branch.
4. Resolve conflicts in ownership order; if conflict crosses ownership boundary, dispatcher resolves.
5. After final merge in wave, run full wave gate:
```bash
cargo check && cargo test && cargo clippy
```

Conflict policy:
- If two workers touched the same file unexpectedly, stop next wave dispatch.
- Re-split ownership and re-dispatch only after conflict root cause is documented.

---

## Dispatch Template (Per Task)

```text
Task: <task name>
Ownership (exclusive):
- <file 1>
- <file 2>

Required tests:
- <exact test target command>

Required verification:
- cargo check
- <extra command>

Commit message:
- <exact commit message>

Do not touch:
- <shared files to avoid>
```

This is the way.
