# Fastlearn Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a new `fastlearn` command/action that performs bounded-concurrency web research with live progress, best-effort auto-embedding, and streaming synthesis, without changing existing `research` behavior.

**Architecture:** Keep `research` untouched and add a new isolated pipeline in `crates/cli/commands/fastlearn.rs`. Route that pipeline through both CLI (`axon fastlearn`) and MCP (`action: "fastlearn"`). Use Tavily search -> concurrent fetch/extract (8 workers) -> per-item best-effort embed -> streamed synthesis -> structured output with timings/counters/failures.

**Tech Stack:** Rust (Tokio, futures `buffer_unordered`), `spider_agent` (search/fetch/extract), existing Axon vector embed APIs (`embed_text_with_metadata`), existing SSE/LLM patterns.

---

## Task 1: Add Command Surface for `fastlearn`

**Files:**
- Modify: `crates/core/config/types.rs`
- Modify: `crates/core/config/cli.rs`
- Modify: `crates/core/config/parse.rs`
- Modify: `crates/cli/commands.rs`
- Modify: `lib.rs`
- Test: in-file tests in `crates/core/config/types.rs`

**Step 1: Write failing test in `types.rs`**

```rust
#[test]
fn test_command_kind_fastlearn_as_str() {
    assert_eq!(CommandKind::Fastlearn.as_str(), "fastlearn");
}
```

**Step 2: Run test to verify fail**

Run: `cargo test test_command_kind_fastlearn_as_str -- --nocapture`
Expected: FAIL (variant missing)

**Step 3: Add enum + routing plumbing**

- Add `Fastlearn` to `CommandKind` and `as_str()`.
- Add `Fastlearn(TextArg)` to `CliCommand`.
- Parse `CliCommand::Fastlearn(args)` -> `(CommandKind::Fastlearn, args.value)`.
- Export command module placeholder in `crates/cli/commands.rs`:

```rust
pub mod fastlearn;
pub use fastlearn::run_fastlearn;
```

- Wire in `lib.rs` imports and `run_once` match arm:

```rust
CommandKind::Fastlearn => run_fastlearn(cfg).await?,
```

**Step 4: Run targeted tests**

Run: `cargo test test_command_kind_fastlearn_as_str -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/core/config/types.rs crates/core/config/cli.rs crates/core/config/parse.rs crates/cli/commands.rs lib.rs
git commit -m "feat(cli): add fastlearn command routing"
```

---

## Task 2: Create `fastlearn` Data Model + Skeleton Handler

**Files:**
- Create: `crates/cli/commands/fastlearn.rs`
- Modify: `crates/cli/commands.rs` (if not done in Task 1)
- Test: `crates/cli/commands/fastlearn.rs` (module tests)

**Step 1: Write failing compile-time shape test**

```rust
#[test]
fn fastlearn_result_serializes_required_fields() {
    let payload = serde_json::to_value(FastlearnPayload::minimal_for_test()).unwrap();
    assert!(payload.get("search_results").is_some());
    assert!(payload.get("embed_stats").is_some());
    assert!(payload.get("timing_ms").is_some());
}
```

**Step 2: Run test to verify fail**

Run: `cargo test fastlearn_result_serializes_required_fields -- --nocapture`
Expected: FAIL (types not defined)

**Step 3: Add base structs and public entry points**

Define:
- `FastlearnTimingMs`
- `FastlearnCounters`
- `FastlearnEmbedStats`
- `FastlearnFailure`
- `FastlearnPayload`

Add function stubs:

```rust
pub async fn fastlearn_payload(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
) -> Result<serde_json::Value, Box<dyn Error>>

pub async fn run_fastlearn(cfg: &Config) -> Result<(), Box<dyn Error>>
```

**Step 4: Run test + compile**

Run: `cargo test fastlearn_result_serializes_required_fields -- --nocapture`
Expected: PASS

Run: `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/fastlearn.rs crates/cli/commands.rs
git commit -m "feat(fastlearn): add payload types and handler skeleton"
```

---

## Task 3: Implement Search Stage + Query Resolution + Timing

**Files:**
- Modify: `crates/cli/commands/fastlearn.rs`
- Reference: `crates/cli/commands/research.rs`, `crates/cli/commands/search.rs`
- Test: `crates/cli/commands/fastlearn.rs`

**Step 1: Write failing test for config/query guards**

```rust
#[tokio::test]
async fn fastlearn_rejects_missing_query() {
    let mut cfg = test_config("");
    cfg.command = CommandKind::Fastlearn;
    cfg.query = None;
    cfg.positional.clear();
    let err = run_fastlearn(&cfg).await.unwrap_err();
    assert!(err.to_string().contains("query"));
}
```

**Step 2: Run test to verify fail**

Run: `cargo test fastlearn_rejects_missing_query -- --nocapture`
Expected: FAIL

**Step 3: Implement search stage**

- Validate `TAVILY_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`.
- Resolve query from `--query` or positional.
- Build Tavily `SearchOptions` with limit+offset and optional `TimeRange`.
- Record `search_ms` with `Instant`.

**Step 4: Verify test + compile**

Run:
- `cargo test fastlearn_rejects_missing_query -- --nocapture`
- `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/fastlearn.rs
git commit -m "feat(fastlearn): implement query guards and search stage timing"
```

---

## Task 4: Implement Concurrent Fetch+Extract Stage (8 workers)

**Files:**
- Modify: `crates/cli/commands/fastlearn.rs`
- Reference: `spider_agent` usage patterns in `crates/cli/commands/research.rs`
- Test: `crates/cli/commands/fastlearn.rs`

**Step 1: Write failing test for concurrency default**

```rust
#[test]
fn fastlearn_default_concurrency_is_eight() {
    assert_eq!(fastlearn_default_concurrency(), 8);
}
```

**Step 2: Run test to verify fail**

Run: `cargo test fastlearn_default_concurrency_is_eight -- --nocapture`
Expected: FAIL

**Step 3: Implement bounded concurrent pipeline**

Use stream + buffer:

```rust
let results = stream::iter(urls.into_iter().enumerate().map(|(idx, url)| {
    let agent = agent.clone();
    async move { process_one_url(agent, idx, url, extraction_prompt.clone()).await }
}))
.buffer_unordered(8)
.collect::<Vec<_>>()
.await;
```

Per URL:
- `agent.fetch(url)`
- `agent.extract(html, prompt)`
- capture success/failure struct
- keep failures non-fatal

**Step 4: Validate**

Run:
- `cargo test fastlearn_default_concurrency_is_eight -- --nocapture`
- `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/fastlearn.rs
git commit -m "feat(fastlearn): add bounded concurrent fetch and extract stage"
```

---

## Task 5: Add Best-Effort Auto-Embedding per Successful Extraction

**Files:**
- Modify: `crates/cli/commands/fastlearn.rs`
- Reference: `crates/vector/ops/tei.rs:290` (`embed_text_with_metadata`)
- Test: `crates/cli/commands/fastlearn.rs`

**Step 1: Write failing embed-stats behavior test**

```rust
#[test]
fn fastlearn_embed_stats_track_failures_without_aborting() {
    let mut stats = FastlearnEmbedStats::default();
    stats.record_failure();
    assert_eq!(stats.failed, 1);
}
```

**Step 2: Run test to verify fail**

Run: `cargo test fastlearn_embed_stats_track_failures_without_aborting -- --nocapture`
Expected: FAIL

**Step 3: Implement embedding integration**

For each successful extraction:
- Build deterministic text payload (`title`, `url`, extracted JSON pretty text).
- Call:

```rust
embed_text_with_metadata(cfg, &content, &url, "fastlearn", Some(&title)).await
```

- On success: increment `embed_stats.succeeded`.
- On error: increment `embed_stats.failed`, append `FastlearnFailure { stage: "embed", ... }`, continue.

**Step 4: Validate**

Run:
- `cargo test fastlearn_embed_stats_track_failures_without_aborting -- --nocapture`
- `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/fastlearn.rs
git commit -m "feat(fastlearn): add best-effort auto-embedding with failure accounting"
```

---

## Task 6: Implement Streaming Synthesis + Fallback

**Files:**
- Modify: `crates/cli/commands/fastlearn.rs`
- Reference: `crates/vector/ops/commands/streaming.rs`
- Test: `crates/cli/commands/fastlearn.rs`

**Step 1: Write failing synthesis fallback unit test**

```rust
#[test]
fn fastlearn_marks_synthesis_fallback_when_stream_unavailable() {
    let state = FastlearnSynthesisState::fallback_for_test();
    assert!(state.used_fallback);
}
```

**Step 2: Run test to verify fail**

Run: `cargo test fastlearn_marks_synthesis_fallback_when_stream_unavailable -- --nocapture`
Expected: FAIL

**Step 3: Implement synthesis stage**

- Build synthesis prompt from successful extractions.
- Attempt SSE streaming (`stream: true`) and print tokens in non-JSON mode.
- If streaming fails, warn once and run non-streaming completion.
- Populate `usage` and `synthesis_ms`.

**Step 4: Validate**

Run:
- `cargo test fastlearn_marks_synthesis_fallback_when_stream_unavailable -- --nocapture`
- `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/fastlearn.rs
git commit -m "feat(fastlearn): stream synthesis output with non-streaming fallback"
```

---

## Task 7: Add Verbose Live Progress + Final Reporting

**Files:**
- Modify: `crates/cli/commands/fastlearn.rs`
- Test: `crates/cli/commands/fastlearn.rs`

**Step 1: Write failing output-shape test for payload**

```rust
#[test]
fn fastlearn_payload_contains_counters_failures_and_timings() {
    let value = serde_json::json!(FastlearnPayload::minimal_for_test());
    assert!(value.get("counters").is_some());
    assert!(value.get("failures").is_some());
    assert!(value.get("timing_ms").is_some());
}
```

**Step 2: Run test to verify fail**

Run: `cargo test fastlearn_payload_contains_counters_failures_and_timings -- --nocapture`
Expected: FAIL

**Step 3: Implement runtime progress and final output**

- Print stage transitions and rolling counters by default in non-JSON mode.
- Include fields in payload:
  - `counters`
  - `embed_stats`
  - `timing_ms` (`search`, `extraction`, `embed`, `synthesis`, `total`)
  - `failures[]`
- Return non-zero only when no meaningful output (e.g. no extractions and no summary).

**Step 4: Validate**

Run:
- `cargo test fastlearn_payload_contains_counters_failures_and_timings -- --nocapture`
- `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/fastlearn.rs
git commit -m "feat(fastlearn): add verbose progress, structured counters, and failure reporting"
```

---

## Task 8: Add MCP Action `fastlearn`

**Files:**
- Modify: `crates/mcp/schema.rs`
- Modify: `crates/mcp/server.rs`
- Test: `crates/mcp/schema.rs` (new tests) and/or `crates/mcp/server.rs` tests if present

**Step 1: Write failing schema parse test**

```rust
#[test]
fn parse_axon_request_supports_fastlearn_action() {
    let raw = serde_json::json!({"action":"fastlearn","query":"rust","limit":5});
    let parsed: AxonRequest = serde_json::from_value(raw).unwrap();
    match parsed {
        AxonRequest::Fastlearn(_) => {}
        _ => panic!("expected fastlearn variant"),
    }
}
```

**Step 2: Run test to verify fail**

Run: `cargo test parse_axon_request_supports_fastlearn_action -- --nocapture`
Expected: FAIL

**Step 3: Implement MCP wiring**

- Add `Fastlearn(FastlearnRequest)` to `AxonRequest`.
- Add request struct:

```rust
pub struct FastlearnRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}
```

- In server:
  - import `fastlearn_payload`
  - add match arm `AxonRequest::Fastlearn(req)`
  - implement `handle_fastlearn` similar to `handle_research`
  - update help action list and tool description to include `fastlearn`

**Step 4: Validate**

Run:
- `cargo test parse_axon_request_supports_fastlearn_action -- --nocapture`
- `cargo check -q`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/mcp/schema.rs crates/mcp/server.rs crates/cli/commands/fastlearn.rs
git commit -m "feat(mcp): add fastlearn action routing and schema"
```

---

## Task 9: Documentation + Command Reference

**Files:**
- Create: `docs/commands/fastlearn.md`
- Modify: `README.md`
- Modify: `docs/MCP.md` (if action table exists)
- Modify: `docs/MCP-TOOL-SCHEMA.md` (if manually maintained in repo)

**Step 1: Write failing docs check step (manual gate)**

Run grep to confirm no `fastlearn` docs yet:

```bash
rg -n "fastlearn" README.md docs/commands docs/MCP.md docs/MCP-TOOL-SCHEMA.md
```

Expected: missing/partial references

**Step 2: Add docs**

- `docs/commands/fastlearn.md`: synopsis, required env vars, concurrency default 8, best-effort embed semantics, timing fields, progress output behavior.
- Update README command table with `fastlearn` row.
- Update MCP docs to include new action + request payload.

**Step 3: Validate docs references**

Run:

```bash
rg -n "fastlearn" README.md docs/commands/fastlearn.md docs/MCP.md docs/MCP-TOOL-SCHEMA.md
```

Expected: all required docs mention `fastlearn`.

**Step 4: Commit**

```bash
git add README.md docs/commands/fastlearn.md docs/MCP.md docs/MCP-TOOL-SCHEMA.md
git commit -m "docs: add fastlearn command and MCP action documentation"
```

---

## Task 10: Final Verification Gate

**Files:**
- No new files; verification only.

**Step 1: Run targeted tests**

```bash
cargo test fastlearn -- --nocapture
cargo test parse_axon_request_supports_fastlearn_action -- --nocapture
cargo test test_command_kind_fastlearn_as_str -- --nocapture
```

Expected: PASS

**Step 2: Run full quality gates**

```bash
cargo fmt --check
cargo clippy
cargo check
cargo test
```

Expected: PASS

**Step 3: Manual smoke commands**

```bash
./scripts/axon fastlearn "tokio task cancellation best practices" --limit 5
./scripts/axon fastlearn "rust async traits" --limit 5 --json
```

Expected:
- live progress during extraction
- streamed synthesis output or one explicit fallback warning
- embed stats and timing block present

**Step 4: MCP smoke**

Use MCP tool payload:

```json
{"action":"fastlearn","query":"tokio cancellation","limit":5,"response_mode":"inline"}
```

Expected: successful structured response with `timing_ms`, `embed_stats`, `failures`.

**Step 5: Final commit (if any verification fixes were needed)**

```bash
git add <fixed-files>
git commit -m "fix: address fastlearn verification regressions"
```

---

## Notes for Executor
- Do not change existing `research` command internals.
- Keep `fastlearn` isolated; share only reusable helpers where clearly DRY.
- Prefer adding unit tests near new logic in `fastlearn.rs` to keep blast radius minimal.
- Ensure no secrets appear in logs/output.
