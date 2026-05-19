# ACP LLM Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all listed OpenAI-compatible HTTP chat calls with ACP-backed prompt turns while preserving command behavior, output contracts, and service-layer boundaries.

**Architecture:** Introduce a shared ACP LLM gateway in `crates/services` that accepts role-based prompt blocks and returns structured turn results, token deltas, and optional usage metadata. Migrate command call sites to this gateway in phases: vector ask/evaluate/suggest, extract fallback, debug analysis, and research synthesis. Keep Tavily search in place for research discovery, but switch synthesis to ACP.

**Tech Stack:** Rust, Tokio, agent-client-protocol SDK, existing ACP runtime (`crates/services/acp`), Axon service layer, existing CLI/service test harness.

---

## File Structure And Ownership

- `crates/services/acp_llm.rs` (create): Single ACP-backed LLM gateway for one-shot prompt completion and streamed delta callbacks.
- `crates/services/acp_llm/` (create as needed): Focused helpers for adapter resolution, event collection, prompt schema, and response parsing.
- `crates/services.rs` (modify): Export the new `acp_llm` module.
- `crates/vector/ops/commands/streaming.rs` (modify): Replace direct OpenAI HTTP request building with ACP gateway calls.
- `crates/vector/ops/commands/suggest.rs` (modify): Replace direct LLM request function with ACP gateway call.
- `crates/core/content/engine.rs` (modify): Remove OpenAI endpoint string construction from extract fallback config.
- `crates/core/content/deterministic.rs` (modify): Replace fallback HTTP POST with ACP gateway call and retain deterministic result flattening.
- `crates/services/debug.rs` (modify): Replace direct OpenAI request with ACP gateway call.
- `crates/services/search.rs` (modify): Keep Tavily search via `spider_agent`, replace synthesis LLM call with ACP.
- `crates/cli/commands/research.rs` (modify): Update prereq validation messaging from OPENAI-specific requirements to ACP adapter requirements.
- `crates/core/config/types/config.rs` (modify): Update field docs to separate legacy OpenAI fields from ACP-backed command path.
- `crates/core/config/parse/build_config.rs` (modify if needed): Parse any new ACP LLM env knobs (agent selection, model override, timeout) if introduced.
- `tests/services_acp_llm.rs` (create): Unit/integration tests for gateway behavior with deterministic event simulation.
- `tests/services_query_services.rs` (modify): Add regression coverage for ask/evaluate/suggest payload contracts under ACP path.
- `tests/services_discovery_services.rs` (modify): Add research synthesis path tests with ACP-mocked responses.
- `docs/sessions/YYYY-MM-DD-HH-MM-acp-llm-migration.md` (create): Session log with root-cause/decision trail.

### Task 1: Add Shared ACP LLM Gateway

**Files:**
- Create: `crates/services/acp_llm.rs`
- Modify: `crates/services.rs`
- Test: `tests/services_acp_llm.rs`

- [ ] **Step 1: Write failing test for non-streaming ACP completion result extraction**

```rust
#[tokio::test]
async fn complete_text_returns_turn_result_content() {
    let fake = FakeAcpRunner::turn_result("hello from acp");
    let result = complete_text_with_runner(&fake, sample_request("prompt")).await;
    assert_eq!(result.unwrap().text, "hello from acp");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test services_acp_llm::complete_text_returns_turn_result_content -- --nocapture`
Expected: FAIL with missing `acp_llm` module/function symbols.

- [ ] **Step 3: Implement minimal gateway API and runner abstraction**

```rust
pub struct AcpLlmRequest {
    pub system: Option<String>,
    pub user: String,
    pub model: Option<String>,
    pub stream: bool,
}

pub struct AcpLlmResponse {
    pub text: String,
    pub usage: Option<AcpUsageSnapshot>,
}

pub async fn complete_text(cfg: &Config, req: AcpLlmRequest) -> Result<AcpLlmResponse, Box<dyn Error>>;
pub async fn complete_streaming(
    cfg: &Config,
    req: AcpLlmRequest,
    on_delta: impl FnMut(&str) -> Result<(), Box<dyn Error>>,
) -> Result<AcpLlmResponse, Box<dyn Error>>;
```

- [ ] **Step 4: Run focused tests**

Run: `cargo test services_acp_llm -- --nocapture`
Expected: PASS for initial extraction and event parsing tests.

- [ ] **Step 5: Commit**

```bash
git add crates/services/acp_llm.rs crates/services.rs tests/services_acp_llm.rs
git commit -m "feat(acp): add shared ACP-backed LLM gateway"
```

### Task 2: Migrate `ask`/`evaluate` Streaming + Non-Streaming Paths

**Files:**
- Modify: `crates/vector/ops/commands/streaming.rs`
- Modify: `crates/vector/ops/commands/ask/output.rs`
- Modify: `crates/vector/ops/commands/evaluate/streaming.rs`
- Test: `crates/vector/ops/commands/streaming.rs` (existing test module + new ACP unit tests)

- [ ] **Step 1: Write failing tests for ACP-backed ask/baseline/judge wrappers**

```rust
#[tokio::test]
async fn ask_llm_non_streaming_uses_acp_gateway() {
    let cfg = Config::test_default();
    let out = ask_llm_non_streaming(&cfg, "q", "ctx").await.unwrap();
    assert!(out.contains("expected"));
}
```

- [ ] **Step 2: Run test to verify failure on removed OpenAI request builder usage**

Run: `cargo test streaming::ask_llm_non_streaming_uses_acp_gateway -- --nocapture`
Expected: FAIL because test hook/gateway path not wired.

- [ ] **Step 3: Replace `build_openai_chat_request` usage with ACP gateway calls**

```rust
let req = AcpLlmRequest::chat(system_prompt, user_prompt)
    .with_model(cfg.openai_model.clone())
    .with_stream(true);
let answer = acp_llm::complete_streaming(cfg, req, |delta| {
    if print_tokens { print!("{delta}"); }
    Ok(())
}).await?;
```

- [ ] **Step 4: Preserve tagged-stream behavior for parallel evaluate mode**

```rust
acp_llm::complete_streaming(cfg, req, |delta| {
    let _ = tx.send(TaggedToken { stream, delta: delta.to_string() });
    Ok(())
}).await
```

- [ ] **Step 5: Run ask/evaluate tests and compile checks**

Run: `cargo test streaming:: -- --nocapture`
Expected: PASS for SSE-independent logic plus new ACP path tests.

Run: `cargo test evaluate:: -- --nocapture`
Expected: PASS with no behavior regression in fallback/parallel rendering logic.

- [ ] **Step 6: Commit**

```bash
git add crates/vector/ops/commands/streaming.rs crates/vector/ops/commands/ask/output.rs crates/vector/ops/commands/evaluate/streaming.rs
git commit -m "refactor(vector): route ask and evaluate LLM calls through ACP"
```

### Task 3: Migrate `suggest` Command LLM Call

**Files:**
- Modify: `crates/vector/ops/commands/suggest.rs`
- Test: `crates/vector/ops/commands/suggest.rs`

- [ ] **Step 1: Add failing test for LLM request function using ACP gateway output**

```rust
#[tokio::test]
async fn request_suggestions_reads_text_from_acp_response() {
    let content = request_suggestions_from_llm(&cfg, "prompt").await.unwrap();
    assert!(content.contains("suggestions"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test suggest::request_suggestions_reads_text_from_acp_response -- --nocapture`
Expected: FAIL with missing ACP-backed implementation.

- [ ] **Step 3: Swap direct HTTP JSON parsing for ACP gateway call**

```rust
let req = AcpLlmRequest::chat(system_prompt, user_prompt.to_string())
    .with_model(cfg.openai_model.clone());
let resp = acp_llm::complete_text(cfg, req).await?;
Ok(resp.text)
```

- [ ] **Step 4: Re-run suggest unit tests**

Run: `cargo test suggest:: -- --nocapture`
Expected: PASS for parsing/filtering regressions and ACP request wrapper test.

- [ ] **Step 5: Commit**

```bash
git add crates/vector/ops/commands/suggest.rs
git commit -m "refactor(suggest): use ACP gateway for suggestion generation"
```

### Task 4: Migrate Extract Fallback LLM Path

**Files:**
- Modify: `crates/core/content/engine.rs`
- Modify: `crates/core/content/deterministic.rs`
- Test: `crates/core/content/deterministic.rs` (new tests)

- [ ] **Step 1: Write failing test for fallback extraction using ACP text payload**

```rust
#[tokio::test]
async fn extract_items_fallback_parses_results_from_acp_json_text() {
    let response = extract_items_fallback_with_runner(&fake_runner, ...).await.unwrap();
    assert_eq!(response.items.len(), 2);
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test deterministic::extract_items_fallback_parses_results_from_acp_json_text -- --nocapture`
Expected: FAIL until ACP path is wired.

- [ ] **Step 3: Remove endpoint assembly + HTTP POST dependency from fallback config path**

```rust
pub struct FallbackConfig {
    pub model: String,
    pub prompt_text: String,
    pub has_fallback: bool,
}
```

- [ ] **Step 4: Implement ACP-based fallback call and preserve token/cost fields with graceful defaults**

```rust
let resp = acp_llm::complete_text(cfg, req).await?;
let parsed = serde_json::from_str::<Value>(&resp.text).unwrap_or_default();
let usage = resp.usage.unwrap_or_default();
```

- [ ] **Step 5: Run extract-focused tests**

Run: `cargo test deterministic:: -- --nocapture`
Expected: PASS, including existing flattening behavior and new ACP parse path.

- [ ] **Step 6: Commit**

```bash
git add crates/core/content/engine.rs crates/core/content/deterministic.rs
git commit -m "refactor(extract): migrate fallback LLM extraction to ACP"
```

### Task 5: Migrate `debug` Service LLM Call

**Files:**
- Modify: `crates/services/debug.rs`
- Modify: `crates/cli/commands/debug.rs` (only if output contract needs small tweaks)
- Test: `tests/services_discovery_services.rs` or new `tests/services_debug.rs`

- [ ] **Step 1: Add failing service test for debug report analysis field sourced from ACP**

```rust
#[tokio::test]
async fn debug_report_embeds_acp_analysis_text() {
    let result = debug_report(&cfg, "ctx").await.unwrap();
    assert_eq!(result.payload["llm_debug"]["analysis"], "expected analysis");
}
```

- [ ] **Step 2: Run test to confirm failure**

Run: `cargo test debug_report_embeds_acp_analysis_text -- --nocapture`
Expected: FAIL with old OpenAI request path assumptions.

- [ ] **Step 3: Replace direct POST flow with ACP gateway and keep payload shape stable**

```rust
let resp = acp_llm::complete_text(cfg, req).await?;
let analysis = resp.text;
```

- [ ] **Step 4: Run debug service/CLI tests**

Run: `cargo test debug:: -- --nocapture`
Expected: PASS and unchanged JSON/human output contracts.

- [ ] **Step 5: Commit**

```bash
git add crates/services/debug.rs crates/cli/commands/debug.rs tests/services_debug.rs
git commit -m "refactor(debug): route troubleshooting analysis through ACP"
```

### Task 6: Migrate `research` Synthesis From spider_agent OpenAI Client To ACP

**Files:**
- Modify: `crates/services/search.rs`
- Modify: `crates/cli/commands/research.rs`
- Test: `crates/services/search.rs` tests + `tests/services_discovery_services.rs`

- [ ] **Step 1: Add failing unit test for `synthesize` using ACP response text fallback/JSON summary extraction**

```rust
#[tokio::test]
async fn synthesize_extracts_summary_field_from_acp_json() {
    let (summary, usage) = synthesize_with_runner("q", &extractions, &fake).await;
    assert_eq!(summary.unwrap(), "final summary");
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test search::synthesize_extracts_summary_field_from_acp_json -- --nocapture`
Expected: FAIL until ACP-backed synthesis path exists.

- [ ] **Step 3: Decouple research from `Agent::with_openai_compatible`**

```rust
let search_agent = Agent::builder().with_search_tavily(&cfg.tavily_api_key).build()?;
let (summary, usage) = synthesize(query, &extractions, cfg).await;
```

- [ ] **Step 4: Implement ACP-backed `synthesize` and usage mapping**

```rust
let resp = acp_llm::complete_text(cfg, req).await?;
let summary = parse_summary_or_raw(&resp.text);
let usage = resp.usage.unwrap_or_default().into_token_usage();
```

- [ ] **Step 5: Update research prereq guardrails**

Run-time requirement should become: Tavily key + ACP adapter command (and optional model override), not OPENAI base URL.

- [ ] **Step 6: Run research/search tests**

Run: `cargo test search:: -- --nocapture`
Expected: PASS including existing mapper tests and new synthesis ACP tests.

Run: `cargo test run_research_ -- --nocapture`
Expected: PASS for prereq validation tests with updated ACP-centric messages.

- [ ] **Step 7: Commit**

```bash
git add crates/services/search.rs crates/cli/commands/research.rs tests/services_discovery_services.rs
git commit -m "refactor(research): use ACP for synthesis and keep Tavily for search"
```

### Task 7: Configuration, Docs, and Compatibility Guardrails

**Files:**
- Modify: `crates/core/config/types/config.rs`
- Modify: `README.md`
- Modify: `CLAUDE.md` (project section if command requirements changed)
- Modify: `.env.example`

- [ ] **Step 1: Add failing test for config/help text consistency if ACP env vars are required**

```rust
#[test]
fn config_docs_reference_acp_for_llm_commands() {
    let text = std::fs::read_to_string(".env.example").unwrap();
    assert!(text.contains("AXON_ACP_ADAPTER_CMD"));
}
```

- [ ] **Step 2: Run test to verify gap**

Run: `cargo test config_docs_reference_acp_for_llm_commands -- --nocapture`
Expected: FAIL if env/docs are still OpenAI-only.

- [ ] **Step 3: Update docs and env templates for ACP-backed LLM commands**

Include explicit migration note:
- OpenAI HTTP calls removed for `ask/evaluate/suggest/extract fallback/debug/research synthesis`.
- `openai_*` config fields retained temporarily only for model naming compatibility and transition.

- [ ] **Step 4: Re-run doc/config tests**

Run: `cargo test config_docs_reference_acp_for_llm_commands -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/config/types/config.rs README.md CLAUDE.md .env.example
git commit -m "docs(config): document ACP as default LLM execution path"
```

### Task 8: Full Verification, Regression Sweep, and Session Log

**Files:**
- Modify: `docs/sessions/YYYY-MM-DD-HH-MM-acp-llm-migration.md`

- [ ] **Step 1: Run targeted verification matrix**

Run: `cargo test services_acp_llm services_query_services services_discovery_services -- --nocapture`
Expected: PASS.

Run: `cargo test streaming:: suggest:: deterministic:: debug:: search:: -- --nocapture`
Expected: PASS.

- [ ] **Step 2: Run full repository gate**

Run: `just verify`
Expected: PASS (`fmt-check`, `clippy`, `check`, `test`).

- [ ] **Step 3: Manual smoke checks for migrated commands**

Run: `./scripts/axon ask "what is indexed?" --json`
Expected: JSON payload with answer/sources, no OpenAI HTTP call path.

Run: `./scripts/axon evaluate "same question" --json`
Expected: JSON payload with rag/baseline/analysis populated.

Run: `./scripts/axon suggest "rust async" --json`
Expected: suggestion URLs returned.

Run: `./scripts/axon debug "qdrant unreachable" --json`
Expected: doctor report + ACP analysis text.

Run: `./scripts/axon research "tokio task cancellation" --json`
Expected: Tavily results + ACP summary.

- [ ] **Step 4: Record migration decisions and residual risks in session log**

```md
- Why ACP gateway was centralized
- How usage/token accounting behaves when adapter omits usage events
- Remaining fallback behavior and known limitations
```

- [ ] **Step 5: Final commit**

```bash
git add docs/sessions
git commit -m "chore: verify ACP LLM migration and log outcomes"
```

## Risks And Mitigations

- ACP event-stream parsing mismatch can drop final text.
Mitigation: Gate on `AcpBridgeEvent::TurnResult` and add explicit timeout/error tests.

- Token usage may be unavailable from some ACP adapters.
Mitigation: Keep `usage` optional and preserve existing behavior when usage is missing (0/default).

- Research synthesis regression if JSON parsing is strict.
Mitigation: Preserve current fallback that treats non-JSON response content as raw summary.

- Concurrency regressions in evaluate parallel mode.
Mitigation: Keep tagged delta transport identical; only swap provider backend.

## Out Of Scope

- Migrating Tavily web search itself off `spider_agent`.
- Changing websocket `pulse_chat` behavior or ACP runtime internals beyond what is required for reusable gateway calls.
- Removing `openai_*` config fields entirely in this pass.
