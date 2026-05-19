# Remove ACP And Standardize Gemini Headless Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Axon's ACP adapter/session stack and make Gemini headless the only LLM synthesis path for ask, research, evaluate, suggest, debug, and extract fallback.

**Architecture:** Split the neutral completion request/response types away from the ACP-named module first, then remove ACP runtime branches, ACP config, MCP `acp` routing, docs, tests, and dependencies. The final shape is a small headless completion gateway backed by `src/services/llm_backend/headless/gemini.rs`, with `OPENAI_MODEL` retained only as the Gemini model override.

**Tech Stack:** Rust, Tokio subprocesses, Gemini CLI stream JSON, Clap/env config, MCP schema via `rmcp::schemars`, Cargo tests.

---

## Current ACP Surfaces

- `src/services/acp.rs` and `src/services/acp/**`: adapter spawn, SDK bridge, permissions, session runtime, persistent sessions, cache, mapping, preflight.
- `src/services/acp_llm.rs` and `src/services/acp_llm/**`: ACP-named completion gateway; currently dispatches to headless when `ask_backend` is headless.
- `src/services/acp::apply_env_allowlist`: currently used by Gemini headless subprocess spawn; must be moved before ACP deletion.
- `src/services/types/acp.rs`: ACP adapter/session/event wire types.
- `src/mcp/server/handlers_acp.rs`, `src/mcp/schema.rs`, `src/mcp/server.rs`: MCP `acp` action.
- `src/core/config/types/{config.rs,enums.rs}`, `src/core/config/parse/{helpers.rs,tuning.rs,toml_config.rs}`, `src/core/config/parse/build_config/config_literal.rs`: ACP backend/config/env parsing.
- `src/vector/ops/commands/{ask.rs,streaming.rs,evaluate.rs,suggest.rs}`, `src/services/{search.rs,debug.rs,extract.rs}`, `src/jobs/lite/workers/runners/extract.rs`: completion call sites.
- Tests: `tests/services_acp_*.rs` and inline ACP tests under `src/services/acp*`, `src/services/types/acp`, `src/vector/ops/commands/*`.
- Docs/env/scripts: `.env`, `.env.example`, `config.example.toml`, `README.md`, `AGENTS.md`, `CLAUDE.md`, `src/mcp/CLAUDE.md`, `docs/ACP.md`, `docs/ASK.md`, `docs/CONFIG.md`, `docs/commands/{ask,research,debug,suggest,evaluate,mcp}.md`, `docs/mcp/*`, `docs/auth/API-TOKEN.md`, `scripts/bench-ask.sh`.
- Dependencies to check after source removal: `agent-client-protocol`, `tokio-tungstenite`, and `dashmap`.

## File Structure After Removal

- `src/services/llm_backend.rs`: exports neutral completion types and headless completion entry points.
- `src/services/llm_backend/types.rs`: new neutral request/response/usage types replacing `AcpCompletion*`.
- `src/services/llm_backend/headless.rs`, `src/services/llm_backend/headless/gemini.rs`, and `src/services/llm_backend/headless/env.rs`: Gemini-only headless implementation plus neutral subprocess env isolation.
- `src/services/llm_backend/concurrency.rs`: neutral completion semaphore and turn timeout helpers.
- `src/vector/ops/commands/streaming.rs`: neutral helper functions for ask/evaluate streaming and non-streaming completions.
- `src/core/config/types/enums.rs`: remove the `AskBackend` enum entirely.
- `src/core/config/types/config.rs`: remove ACP adapter/prewarm/WS fields and add neutral LLM concurrency/timeout fields if config struct storage is preferred over env-only helpers.
- `src/mcp/schema.rs` and `src/mcp/server.rs`: no `acp` action.
- Docs describe Gemini headless only.

## Implementation Steps

#### Chunk 1: Neutralize Completion Types

### Task 1: Create neutral Gemini completion foundation

**Files:**
- Create: `src/services/llm_backend/types.rs`
- Create: `src/services/llm_backend/headless/env.rs`
- Create: `src/services/llm_backend/concurrency.rs`
- Modify: `src/services/llm_backend.rs`
- Modify: `src/services/llm_backend/headless/{dispatch.rs,gemini.rs,common.rs}`
- Test: `tests/services_acp_llm.rs`
- Test: `tests/services_acp_spawn_env.rs`

- [ ] **Step 1: Create the neutral type module**

Create `src/services/llm_backend/types.rs` with the contents currently in `src/services/acp_llm/types.rs`, renamed as follows:

```rust
use std::error::Error as StdError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub model: Option<String>,
    pub stream: bool,
}

impl CompletionRequest {
    #[must_use]
    pub fn new(user_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: None,
            user_prompt: user_prompt.into(),
            model: None,
            stream: false,
        }
    }

    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSnapshot {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: Option<UsageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTurnResult {
    pub text: String,
    pub usage: Option<UsageSnapshot>,
}

#[must_use]
pub fn extract_completion_result(turn_result: CompletionTurnResult) -> CompletionResponse {
    CompletionResponse {
        text: turn_result.text,
        usage: turn_result.usage,
    }
}

pub fn normalize_stream_flag(mut req: CompletionRequest, stream: bool) -> CompletionRequest {
    req.stream = stream;
    req
}
```

Do not include `impl From<agent_client_protocol::Usage>` in the new neutral type module; the target architecture removes the ACP SDK dependency and headless Gemini currently reports no structured token usage.

- [ ] **Step 2: Move subprocess env isolation before deleting ACP**

Create `src/services/llm_backend/headless/env.rs` by moving only the environment isolation behavior currently exposed as `crate::services::acp::apply_env_allowlist`. The new function should clear the child env and re-add only required safe variables.

Minimum test contract:

```rust
// In the replacement for tests/services_acp_spawn_env.rs:
// denied: OPENAI_API_KEY, OPENAI_BASE_URL, AXON_MCP_HTTP_TOKEN,
// TAVILY_API_KEY, GITHUB_TOKEN, BEADS_DOLT_PASSWORD, CLAUDECODE.
// allowed when present: HOME source resolution inputs needed for Gemini auth,
// GOOGLE_* Gemini auth variables if current behavior requires them.
```

Update Gemini to import the neutral function:

```rust
use crate::services::llm_backend::headless::env::apply_env_allowlist;
```

- [ ] **Step 3: Add neutral completion limiter and timeout helpers**

Create `src/services/llm_backend/concurrency.rs` with:

```rust
pub const DEFAULT_LLM_COMPLETION_CONCURRENCY: usize = 4;
pub const DEFAULT_LLM_COMPLETION_TIMEOUT_SECS: u64 = 300;

pub async fn acquire_completion_permit() -> Result<tokio::sync::OwnedSemaphorePermit, Box<dyn std::error::Error + Send + Sync>> {
    // Read AXON_LLM_COMPLETION_CONCURRENCY once, clamp >0, default 4.
}

pub fn completion_timeout() -> std::time::Duration {
    // Read AXON_LLM_COMPLETION_TIMEOUT_SECS, default 300s.
}
```

Add tests proving:

```text
AXON_LLM_COMPLETION_CONCURRENCY unset -> 4
AXON_LLM_COMPLETION_CONCURRENCY=0 -> 4
AXON_LLM_COMPLETION_CONCURRENCY=2 limits fake concurrent completions to 2
AXON_LLM_COMPLETION_TIMEOUT_SECS unset -> 300
AXON_LLM_COMPLETION_TIMEOUT_SECS=0 -> 300
```

- [ ] **Step 4: Re-export neutral types**

In `src/services/llm_backend.rs`, export the new module:

```rust
pub mod concurrency;
pub mod headless;
pub mod types;

pub use types::{
    CompletionRequest, CompletionResponse, CompletionTurnResult,
    UsageSnapshot, extract_completion_result, normalize_stream_flag,
};
```

- [ ] **Step 5: Do not add ACP compatibility aliases**

Do not create `AcpCompletion*` compatibility aliases in `src/services/acp_llm.rs`. They hide stale imports. The implementation should move the types, update all call sites to neutral names, and delete `acp_llm` in the next task.

- [ ] **Step 6: Run a narrow compile check**

Run:

```bash
cargo check
```

Expected: failures are only stale imports of `AcpCompletion*`, `AcpUsageSnapshot`, `apply_env_allowlist`, or `CompletionRunner`. Fix them by using neutral `llm_backend` types and the neutral headless env module.

- [ ] **Step 7: Commit**

```bash
git add src/services/llm_backend.rs src/services/llm_backend src/services/llm_backend/headless tests
git commit -m "refactor: add neutral Gemini completion foundation"
```

#### Chunk 2: Remove ACP Runtime From Completion Flow

### Task 2: Replace `acp_llm` with a bounded Gemini-only gateway

**Files:**
- Modify: `src/services/llm_backend.rs`
- Modify: `src/services/llm_backend/headless/dispatch.rs`
- Modify: `src/services/llm_backend/headless/gemini.rs`
- Delete: `src/services/llm_backend/headless/claude.rs`
- Delete: `src/services/llm_backend/headless/codex.rs`
- Modify: `src/vector/ops/commands/streaming.rs`
- Modify: `src/vector/ops/commands/ask.rs`
- Modify: `src/vector/ops/commands/ask/output.rs`
- Modify: `src/vector/ops/commands/evaluate.rs`
- Modify: `src/vector/ops/commands/evaluate/streaming.rs`
- Modify: `src/vector/ops/commands/suggest.rs`
- Modify: `src/services/{search.rs,debug.rs,extract.rs}`
- Modify: `src/jobs/lite/workers/runners/extract.rs`
- Delete after replacement: `src/services/acp_llm.rs`, `src/services/acp_llm/{pool.rs,runner.rs,warm.rs,ws_runner.rs,types.rs}`
- Test: `tests/services_acp_llm.rs` renamed to `tests/services_llm_backend.rs`
- Test: add fake Gemini command tests for concurrency, timeout, malformed stream JSON, tool event rejection, nonzero exit with redacted stderr, and missing auth/home behavior.

- [ ] **Step 1: Add headless completion functions**

In `src/services/llm_backend.rs`, add:

```rust
use std::error::Error as StdError;

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    let _permit = concurrency::acquire_completion_permit().await?;
    headless::dispatch::complete_text(req).await
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    let _permit = concurrency::acquire_completion_permit().await?;
    headless::dispatch::complete_streaming(req, on_delta).await
}
```

- [ ] **Step 2: Make Gemini unconditional**

Delete `HeadlessAgent`, `AXON_ASK_AGENT` routing, `headless/claude.rs`, and `headless/codex.rs`. `llm_backend::complete_text` and `llm_backend::complete_streaming` should call Gemini directly. Keep only:

```rust
headless::gemini::complete_streaming(req, on_delta).await
```

- [ ] **Step 3: Rename request/response imports in headless modules**

Change imports like:

```rust
use crate::services::acp_llm::{AcpCompletionRequest, AcpCompletionResponse};
```

to:

```rust
use crate::services::llm_backend::{CompletionRequest, CompletionResponse};
```

Then rename function signatures from `AcpCompletionRequest` / `AcpCompletionResponse` to `CompletionRequest` / `CompletionResponse` and return `Box<dyn Error + Send + Sync>`.

- [ ] **Step 4: Remove warm-session parameters and ACP blocking runtimes from streaming helpers**

In `src/vector/ops/commands/streaming.rs`, replace signatures that accept `Option<WarmAcpSession>` with no warm parameter:

```rust
pub(crate) async fn run_streaming_completion_ttft(
    req: CompletionRequest,
    print_tokens: bool,
    tagged: Option<(UnboundedSender<TaggedToken>, &'static str)>,
) -> Result<(String, Option<std::time::Instant>), Box<dyn Error + Send + Sync>> {
    run_streaming_completion_inner(req, print_tokens, tagged, true).await
}
```

Remove the ACP-era `spawn_blocking` current-thread runtime unless a compile error proves the Gemini future is `!Send`. The normal path should await directly:

```rust
let stream_result = llm_backend::complete_streaming(req, |delta| {
    process_one_delta(&mut state, delta, print_tokens, tagged.as_ref(), capture_ttft)
}).await;
```

Delete all `WarmAcpSession` branches and comments.

- [ ] **Step 5: Remove warm-session acquisition from ask/research/evaluate/suggest/debug**

For each call site, delete `AskBackend::Acp` branches and calls to `acp_llm::warm_session(cfg, None)` or `acp_llm::warm_session(cfg, tx.clone())`. Pass only neutral completion requests to `llm_backend::complete_streaming` or `llm_backend::complete_text`, matching the existing call's streaming behavior.

For `src/vector/ops/commands/ask.rs`, the validation should reduce to:

```rust
pub(super) fn validate_ask_llm_config(_cfg: &Config) -> anyhow::Result<()> {
    llm_backend::headless::gemini::validate_command()
        .map_err(|e| anyhow::anyhow!("{e}"))
}
```

- [ ] **Step 6: Add Gemini command and auth safety**

Add validation around `AXON_HEADLESS_GEMINI_CMD`:

```text
Reject shell command strings containing whitespace shell syntax, pipes, redirects, or `sh -c` style forms.
Resolve the executable path before spawn.
In service mode, require an absolute path or reject world-writable parent directories in the resolved path.
```

Harden `prepare_gemini_home()`:

```text
Canonicalize source home.
Reject symlinked auth files using symlink_metadata.
Require regular files.
Reject group/world-writable auth files on Unix.
Fail clearly when no supported auth file exists.
```

Add tests for path hijack, shell-command rejection, missing auth, symlink auth file, loose auth permissions, malformed stream JSON, tool event rejection, nonzero exit with redacted stderr, timeout, and concurrency limiting.

- [ ] **Step 7: Rename the service completion tests**

Rename `tests/services_acp_llm.rs` to `tests/services_llm_backend.rs` and replace names:

```rust
use axon::services::llm_backend::{
    CompletionRequest, CompletionResponse, CompletionTurnResult,
    UsageSnapshot, complete_text,
};
```

Delete the test that asserts `AXON_ACP_ADAPTER_CMD` is required.

- [ ] **Step 8: Delete ACP completion files**

Delete:

```text
src/services/acp_llm.rs
src/services/acp_llm/pool.rs
src/services/acp_llm/runner.rs
src/services/acp_llm/types.rs
src/services/acp_llm/warm.rs
src/services/acp_llm/ws_runner.rs
```

- [ ] **Step 9: Run focused tests**

Run:

```bash
cargo test services_llm_backend
cargo test gemini_headless
cargo test llm_completion_concurrency
cargo test llm_completion_timeout
cargo test ask_llm
cargo test research
```

Expected: All pass. If any test still imports `acp_llm`, update it to `llm_backend` or delete it if it only covered ACP behavior.

- [ ] **Step 10: Commit**

```bash
git add src tests
git commit -m "refactor: route completions through bounded Gemini headless backend"
```

#### Chunk 3: Remove ACP Config And Backend Selector

### Task 3: Delete ACP env/config fields and backend mode

**Files:**
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/config_impls.rs`
- Modify: `src/core/config/types/enums.rs`
- Modify: `src/core/config/parse/helpers.rs`
- Modify: `src/core/config/parse/tuning.rs`
- Modify: `src/core/config/parse/toml_config.rs`
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Modify: `src/core/config/parse/build_config/tests/**`
- Modify: `src/main.rs`
- Modify: `src/cli/commands/{serve.rs,mcp.rs}`

- [ ] **Step 1: Remove ACP fields from `Config`**

Delete these fields from `src/core/config/types/config.rs`:

```rust
pub acp_adapter_cmd: Option<String>,
pub acp_adapter_args: Option<String>,
pub acp_prewarm: bool,
pub acp_ws_url: Option<String>,
pub acp_ws_token: Option<String>,
pub ask_backend: AskBackend,
pub ask_agent: String,
```

Keep `openai_model` and update its comment to:

```rust
/// Gemini model override for headless LLM synthesis.
/// Retained as `OPENAI_MODEL` for backward compatibility.
pub openai_model: String,
```

Add or retain neutral settings only if they are stored on `Config` rather than env-only helpers:

```rust
pub llm_completion_concurrency: usize,
pub llm_completion_timeout_secs: u64,
```

- [ ] **Step 2: Remove `AskBackend`**

In `src/core/config/types/enums.rs`, delete the `AskBackend` enum and its impls. Remove imports of `AskBackend` from `src/core/config/types/config.rs`, `src/core/config/types.rs`, and parser modules.

- [ ] **Step 3: Remove ask backend TOML parsing**

In `src/core/config/parse/toml_config.rs`, delete `TomlAskSection.backend`. In `src/core/config/parse/tuning.rs`, delete `ask_backend()` and the assignment to `cfg.ask_backend`. Also delete parsing for `AXON_ASK_AGENT`; Gemini is unconditional.

- [ ] **Step 4: Remove ACP env resolution helpers**

In `src/core/config/parse/helpers.rs`, delete:

```rust
resolve_ask_adapter_cmd()
resolve_ask_adapter_args()
```

Delete their tests.

- [ ] **Step 5: Remove config literal ACP assignments**

In `src/core/config/parse/build_config/config_literal.rs`, delete:

```rust
cfg.acp_adapter_cmd = resolve_ask_adapter_cmd();
cfg.acp_adapter_args = resolve_ask_adapter_args();
cfg.acp_prewarm = env_bool("AXON_ACP_PREWARM", true);
cfg.acp_ws_url = env::var("AXON_ACP_WS_URL")
    .ok()
    .filter(|value| !value.trim().is_empty());
cfg.acp_ws_token = env::var("AXON_ACP_WS_TOKEN")
    .ok()
    .filter(|value| !value.trim().is_empty());
```

Do not delete parsing for the new neutral envs if they were added to `Config`:

```text
AXON_LLM_COMPLETION_CONCURRENCY
AXON_LLM_COMPLETION_TIMEOUT_SECS
```

- [ ] **Step 6: Remove warm-pool startup**

In `src/cli/commands/serve.rs` and `src/cli/commands/mcp.rs`, remove:

```rust
use crate::services::acp_llm;
acp_llm::init_warm_pool(cfg);
```

- [ ] **Step 7: Remove ACP blocking-thread comments**

In `src/main.rs`, remove comments and function names that refer to `acp_blocking_thread_limit`. If the function only exists for ACP, delete it and use the default Tokio blocking thread behavior.

- [ ] **Step 8: Remove warm-session timing/API fields or rename them**

Update `src/vector/ops/commands/ask/timing.rs` and service result types so output no longer exposes ACP-era `warm_session_ready_ms` or `llm_warm_path`. Replace with neutral fields if diagnostics still need them:

```text
llm_queue_wait_ms
llm_spawn_ms
llm_ttft_ms
llm_total_ms
llm_backend = "gemini_headless"
```

- [ ] **Step 9: Compile config**

Run:

```bash
cargo check
```

Expected: failures only from remaining `AskBackend`, `AXON_ASK_AGENT`, `acp_adapter_*`, `acp_ws_*`, `acp_prewarm`, `warm_session_ready_ms`, or `llm_warm_path` references. Remove those references or replace with neutral LLM timing fields.

- [ ] **Step 10: Commit**

```bash
git add src/core src/cli src/main.rs
git commit -m "refactor: remove ACP completion configuration"
```

#### Chunk 4: Remove ACP MCP Action And Service Types

### Task 4: Delete MCP `acp` action and ACP service event types

**Files:**
- Delete: `src/mcp/server/handlers_acp.rs`
- Modify: `src/mcp/server.rs`
- Modify: `src/mcp/schema.rs`
- Modify: `src/services.rs`
- Delete: `src/services/acp.rs`
- Delete: `src/services/acp/**`
- Delete: `src/services/types/acp.rs`
- Delete: `src/services/types/acp/acp_tests.rs`
- Modify: `src/services/types.rs`
- Modify: `src/services/events.rs`
- Test: delete `tests/services_acp_{bridge_event_serialize,event_mapping,lifecycle,security,smoke,spawn_env}.rs`
- Test: add MCP `acp` negative exposure tests before deleting the old ACP tests.

- [ ] **Step 1: Remove MCP action variant**

In `src/mcp/schema.rs`, delete:

```rust
Acp(AcpRequest),
```

Delete `AcpRequest` and `AcpSubaction`.

- [ ] **Step 2: Remove MCP router match arm**

In `src/mcp/server.rs`, delete:

```rust
#[path = "server/handlers_acp.rs"]
mod handlers_acp;
```

and:

```rust
AxonRequest::Acp(req) => self.handle_acp(req).await?,
```

- [ ] **Step 3: Delete ACP service exports**

In `src/services.rs`, delete:

```rust
pub mod acp;
```

In `src/services/types.rs`, delete:

```rust
pub mod acp;
pub use acp::*;
```

- [ ] **Step 4: Remove ACP service events**

In `src/services/events.rs`, remove `ServiceEvent::AcpBridge` and any `AcpBridgeEvent` import. Delete match arms that serialize or forward ACP bridge events.

- [ ] **Step 5: Delete ACP implementation files**

Delete:

```text
src/services/acp.rs
src/services/acp/adapters.rs
src/services/acp/bridge.rs
src/services/acp/bridge/state.rs
src/services/acp/bridge/terminal.rs
src/services/acp/config.rs
src/services/acp/mapping.rs
src/services/acp/mapping/mcp_filters.rs
src/services/acp/mapping/session_setup.rs
src/services/acp/mapping/validation.rs
src/services/acp/permission.rs
src/services/acp/persistent_conn.rs
src/services/acp/persistent_conn/editor.rs
src/services/acp/persistent_conn/session_options.rs
src/services/acp/persistent_conn/turn.rs
src/services/acp/preflight.rs
src/services/acp/runtime.rs
src/services/acp/session.rs
src/services/acp/session_cache.rs
src/services/acp/session_cache/cache.rs
src/services/acp/session_cache/entry.rs
src/services/types/acp.rs
src/services/types/acp/acp_tests.rs
```

- [ ] **Step 6: Delete ACP integration tests**

Delete:

```text
tests/services_acp_bridge_event_serialize.rs
tests/services_acp_event_mapping.rs
tests/services_acp_lifecycle.rs
tests/services_acp_security.rs
tests/services_acp_smoke.rs
tests/services_acp_spawn_env.rs
```

- [ ] **Step 7: Add MCP negative tests**

Add tests proving all of the following:

```text
serde_json::from_value::<AxonRequest>(json!({"action":"acp","subaction":"list_sessions"})) fails.
Generated MCP schema contains no AcpRequest and no acp action variant.
docs/mcp/TOOLS.md and docs/MCP-TOOL-SCHEMA.md contain no active acp action section after docs are updated.
A tool call with action=acp returns invalid action / invalid params rather than list_sessions data.
```

- [ ] **Step 8: Compile**

Run:

```bash
cargo check
```

Expected: remaining errors point to stale ACP imports. Remove those imports or replace them with neutral `llm_backend` imports.

- [ ] **Step 9: Commit**

```bash
git add -A src tests
git commit -m "refactor: remove ACP service and MCP action"
```

#### Chunk 5: Remove ACP Dependency Footprint

### Task 5: Drop unused dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

- [ ] **Step 1: Remove known ACP-only dependencies**

From `Cargo.toml`, remove:

```toml
agent-client-protocol = { version = "0.10.4", features = ["unstable"] }
```

- [ ] **Step 2: Check remaining dependency usage**

Run:

```bash
rg -n "tokio_tungstenite|dashmap|agent_client_protocol|agent-client-protocol" src tests Cargo.toml
```

Expected:
- No `agent_client_protocol` references.
- If `tokio_tungstenite` has no references, remove it from `Cargo.toml`.
- If `dashmap` has no references, remove it from `Cargo.toml`.

- [ ] **Step 3: Regenerate lockfile**

Run:

```bash
cargo check
```

Expected: `Cargo.lock` updates and compile succeeds or exposes stale imports.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: drop ACP dependencies"
```

#### Chunk 6: Update Environment, Docs, And Scripts

### Task 6: Rewrite operator docs around Gemini headless only

**Files:**
- Modify: `.env.example`
- Modify: `config.example.toml`
- Delete: `docs/ACP.md`
- Modify: `docs/ASK.md`
- Modify: `docs/CONFIG.md`
- Modify: `docs/OPERATIONS.md`
- Modify: `docs/DEPLOYMENT.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/TESTING.md`
- Modify: `docs/MCP.md`
- Modify: `docs/mcp/{DEV.md,PATTERNS.md,TOOLS.md,ENV.md}`
- Modify: `docs/commands/{ask.md,research.md,debug.md,suggest.md,evaluate.md,mcp.md}`
- Modify: `docs/auth/API-TOKEN.md`
- Modify: `plugins/README.md`
- Modify: `scripts/bench-ask.sh` only after benchmark evidence is captured.
- Modify locally, not committed: `.env` key cleanup if the file exists.

- [ ] **Step 1: Remove ACP env keys**

In `.env.example`, delete:

```env
AXON_ASK_BACKEND=headless
AXON_ACP_CLAUDE_ADAPTER_CMD=
AXON_ACP_CLAUDE_ADAPTER_ARGS=
AXON_ACP_CODEX_ADAPTER_CMD=
AXON_ACP_CODEX_ADAPTER_ARGS=
AXON_ACP_GEMINI_ADAPTER_CMD=
AXON_ACP_GEMINI_ADAPTER_ARGS=
AXON_ACP_AUTO_APPROVE=
AXON_ACP_PREWARM=true
AXON_ACP_COMPLETION_CONCURRENCY=
AXON_ACP_MAX_SESSIONS=100
AXON_ACP_WS_URL=
AXON_ACP_WS_TOKEN=
AXON_ASK_AGENT=gemini
```

Keep:

```env
AXON_HEADLESS_GEMINI_CMD=gemini
AXON_HEADLESS_GEMINI_HOME=
AXON_LLM_COMPLETION_CONCURRENCY=4
AXON_LLM_COMPLETION_TIMEOUT_SECS=300
OPENAI_MODEL=
```

- [ ] **Step 2: Remove ask backend from TOML example**

In `config.example.toml`, delete the `[ask] backend` docs and replace with:

```toml
# LLM synthesis uses Gemini headless. Override the Gemini model with OPENAI_MODEL
# in the env layer rather than config.toml.
```

- [ ] **Step 3: Delete ACP reference doc**

Delete `docs/ACP.md`. Remove links to it from README-style docs.

- [ ] **Step 4: Rewrite command docs**

For `docs/commands/ask.md`, `research.md`, `debug.md`, `suggest.md`, and `evaluate.md`, replace ACP requirement text with:

```markdown
LLM synthesis uses the Gemini CLI headless path. `OPENAI_MODEL` optionally overrides the default Gemini model. `AXON_HEADLESS_GEMINI_CMD` can point to an alternate Gemini CLI binary.
```

- [ ] **Step 5: Remove MCP `acp` docs**

In `docs/MCP.md`, `docs/mcp/TOOLS.md`, and `docs/commands/mcp.md`, remove `acp` from action lists and delete the `acp` section.

- [ ] **Step 6: Preserve benchmark comparison until evidence exists**

Do not remove ACP/warm comparison cells from `scripts/bench-ask.sh` until the migration has captured latency evidence. First add or preserve a mode that can compare current ACP/headless baseline to Gemini-only behavior:

```bash
scripts/bench-ask.sh --backend headless --agent gemini --mode cold --runs 30
scripts/bench-ask.sh --backend acp --agent gemini --mode warm --runs 30
```

After ACP code is deleted and the migration is accepted, simplify the script in a follow-up commit.

- [ ] **Step 7: Search for stale ACP docs**

Run:

```bash
rg -n "ACP|acp|AXON_ACP|AXON_ASK_BACKEND|AXON_ASK_AGENT|agent-client-protocol" docs .env.example config.example.toml scripts plugins README.md AGENTS.md CLAUDE.md src/mcp/CLAUDE.md
```

Expected: only historical archive/session docs or deliberate migration notes remain. Any active docs must describe Gemini headless only.

- [ ] **Step 8: Clean live runtime env keys without exposing secrets**

If repo `.env` exists, remove only ACP/ask-agent keys from the real runtime file while preserving all unrelated values and secrets:

```bash
cut -d= -f1 .env | rg '^(AXON_ACP_|AXON_ASK_BACKEND|AXON_ASK_AGENT)$'
```

Expected after cleanup: no matching keys. Do not print values.

- [ ] **Step 9: Commit active docs**

```bash
git add -A .env.example config.example.toml docs plugins scripts README.md AGENTS.md CLAUDE.md src/mcp/CLAUDE.md
git commit -m "docs: remove ACP configuration and docs"
```

#### Chunk 7: Verification And Cleanup

### Task 7: Run final verification gates

**Files:**
- No planned source edits unless verification exposes stale references.

- [ ] **Step 1: Static stale-reference scan**

Run:

```bash
rg -n "ACP|acp|Acp|AXON_ACP|AXON_ASK_BACKEND|AXON_ASK_AGENT|agent_client_protocol|agent-client-protocol|warm_session_ready_ms|llm_warm_path" src tests Cargo.toml .env.example config.example.toml docs scripts plugins README.md AGENTS.md CLAUDE.md src/mcp/CLAUDE.md
```

Expected: no active source references. Historical docs under `docs/archive/**` and `docs/sessions/**` may remain if intentionally preserved.

- [ ] **Step 2: Formatting**

Run:

```bash
cargo fmt --check
```

Expected: pass. If it fails, run `cargo fmt`, inspect the diff, and commit formatting with the relevant task commit if not already committed.

- [ ] **Step 3: Compile**

Run:

```bash
cargo check
```

Expected: pass.

- [ ] **Step 4: Focused behavior tests**

Run:

```bash
cargo test gemini_headless
cargo test llm_completion_concurrency
cargo test llm_completion_timeout
cargo test ask
cargo test research
cargo test debug
cargo test evaluate
cargo test suggest
cargo test extract
```

Expected: pass. If `cargo test` rejects multiple filters, run each command separately.

- [ ] **Step 5: Full test suite**

Run:

```bash
cargo test
```

Expected: pass.

- [ ] **Step 6: Optional live smoke when Gemini auth is available**

Run:

```bash
timeout 360s ./scripts/axon ask "What sources are indexed for Axon?" --json
timeout 360s ./scripts/axon research "Axon Gemini headless synthesis smoke test" --json
```

Expected: commands no longer mention ACP adapter config and either produce valid output or fail only for external service/auth prerequisites such as Tavily, Qdrant, TEI, or Gemini auth.

- [ ] **Step 7: Benchmark latency regression before accepting ACP deletion**

Run before deleting benchmark comparison support:

```bash
scripts/bench-ask.sh --backend headless --agent gemini --mode cold --runs 30
scripts/bench-ask.sh --backend acp --agent gemini --mode warm --runs 30
```

Expected: p50/p95 and TTFT differences are recorded in the PR/session notes. If Gemini-only headless regresses beyond the agreed budget, pause and decide whether a neutral warm/preflight mechanism is required before deleting ACP.

- [ ] **Step 8: Final commit if verification caused fixes**

```bash
git add -A
git commit -m "test: verify Gemini headless after ACP removal"
```

## Self-Review

**Spec coverage:** The plan covers all ACP runtime code, ACP completion branches, config/env parsing, MCP action exposure, docs/scripts, dependencies, tests, and the engineering-review safety gaps. It makes Gemini headless unconditional for ask/research/evaluate/suggest/debug/extract fallback.

**Placeholder scan:** No task contains deferred implementation language. Deletions, replacements, commands, and expected outcomes are explicit.

**Type consistency:** Neutral type names are `CompletionRequest`, `CompletionResponse`, `CompletionTurnResult`, and `UsageSnapshot` throughout. `WarmAcpSession` is removed rather than renamed because the target architecture has no warm ACP session; concurrency and timeout controls are neutral `llm_backend` concepts.
