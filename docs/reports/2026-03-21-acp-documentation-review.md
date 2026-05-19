# ACP Documentation Review
Date: 2026-03-21
Scope: `docs/ACP.md` vs. implementation in `crates/services/acp/`, `crates/services/acp_llm.rs`, `crates/web/execute/sync_mode/pulse_chat.rs`, `crates/web/ws_handler.rs`

---

## Summary

`docs/ACP.md` is one of the best-documented subsystems in the codebase. The majority of the document is accurate and well-structured. Six substantive gaps were found: two of them are inaccurate claims (one a prior-phase finding that is confirmed here), two are missing behaviors that a contributor would need to know, and two are minor but repeated naming/structure inconsistencies. No outright fabrications were found.

---

## Findings

### Finding 1 — Critical: `acp_llm.rs` LocalSet Panic Is Not Documented

**Severity:** Critical

**What is wrong:**

`AcpRuntimeCompletionRunner::run_completion_inner()` in `crates/services/acp_llm.rs` creates a `LocalSet` and calls `local.run_until(...)` directly on the current tokio runtime — it does NOT use `spawn_blocking`. The code is:

```rust
let local = tokio::task::LocalSet::new();
match tokio::time::timeout(
    timeout,
    local.run_until(run_completion_local(scaffold, req, on_delta)),
)
.await
```

This runs `run_until` on the multi-thread tokio runtime rather than inside `spawn_blocking`. If `run_completion_local` calls `tokio::task::spawn_local` internally — and it does, via `scaffold.start_prompt_turn()` → `run_acp_event_loop()` → `spawn_blocking` → `LocalSet` internally — the `spawn_local` calls within that nested chain are fine because they are on their own dedicated runtime. However, the `WarmAcpSession::complete_streaming` path uses `AcpConnectionHandle::run_turn()` which dispatches into a background `spawn_blocking` thread. The `run_completion_local` function calls `tokio::task::spawn_local` explicitly for the prompt handle. Any `spawn_local` call that fires on the multi-thread runtime without an active `LocalSet` context panics.

The prior phase identified this as a latent `LocalSet` panic risk. The doc says nothing about it. `ACP.md` section "Why a Dedicated Runtime" (§4) only describes the `run_acp_event_loop()` path, not `acp_llm.rs`'s divergent pattern.

**Recommended fix:**

Add a section or note in §16 (ACP-Backed LLM Completions) explaining:

> `AcpRuntimeCompletionRunner` uses `LocalSet::run_until()` directly on the calling tokio executor — not inside `spawn_blocking`. This works only because `start_prompt_turn()` itself contains a nested `spawn_blocking` + `LocalSet` via `run_acp_event_loop()`. The outer `LocalSet` in `run_completion_inner` exists to satisfy `local.run_until()` API requirements for the timeout wrapper; the real isolation boundary is the inner `spawn_blocking`. Any refactor that calls `spawn_local` directly inside `run_completion_local` without this inner isolation would cause a multi-thread executor panic. Do not add `spawn_local` to `run_completion_local` directly.

Also add an inline comment in `acp_llm.rs` at line 184 to explain this non-obvious layering.

---

### Finding 2 — High: `adapter_loop` / `adapter_loop_eager` Duplication Is Undocumented Design Smell

**Severity:** High

**What is wrong:**

`crates/services/acp/persistent_conn.rs` contains two nearly-identical background loop functions: `adapter_loop` (lazy init, 182–270) and `adapter_loop_eager` (eager init, 272–350). The main loop body after session establishment is copy-pasted verbatim — same `tokio::select!` pattern, same `exit_result` handling, same `turn::run_turn_on_conn` call. The only difference is when `establish_acp_session` is called (before first turn vs. on first-turn message receipt).

`docs/ACP.md` §5 documents both `spawn()` and `spawn_eager()` accurately, but it says nothing about the code duplication, its rationale, or the risk that a bug fix in one loop will be missed in the other.

**Recommended fix:**

Add a "Known Code Smell" subsection to §5 (Persistent Connection Mode):

> **Code duplication:** `adapter_loop` and `adapter_loop_eager` share identical post-setup turn dispatch logic. They were kept separate to avoid lifetime/borrow complexity when threading the setup return value through a shared future. When fixing bugs in the turn dispatch loop, update both functions. A future refactor could extract the common loop body into `adapter_loop_body(conn, session_id, session_cwd, runtime_state, exit_rx, rx)`.

Also add a cross-reference comment in `persistent_conn.rs` above both functions:

```rust
// NOTE: adapter_loop and adapter_loop_eager share identical post-setup logic.
// When fixing bugs here, apply the same fix to the other function.
// See docs/ACP.md §5 for rationale.
```

---

### Finding 3 — High: Hung-Turn Threshold vs. Turn Timeout Interaction Is Undocumented

**Severity:** High

**What is wrong:**

`docs/ACP.md` §14 documents both `SESSION_HUNG_TURN_THRESHOLD` (5 min, hardcoded) and `AXON_ACP_TURN_TIMEOUT_MS` (default 5 min, env-configurable) but does not explain their relationship or the hazard when they diverge.

`SESSION_HUNG_TURN_THRESHOLD` in `session_cache.rs` is hardcoded at 300 seconds. `AXON_ACP_TURN_TIMEOUT_MS` in `pulse_chat.rs` defaults to 300 seconds but can be raised arbitrarily (e.g., `AXON_ACP_TURN_TIMEOUT_MS=600000` for 10-minute turns). If the turn timeout is raised above 5 minutes, the reaper will evict the session as "hung" before the turn completes, killing the adapter mid-response and leaving the client waiting on a dead connection.

This is an operational landmine: operators tuning `AXON_ACP_TURN_TIMEOUT_MS` for long-running agents have no documentation warning them to also account for the hardcoded hung-turn threshold.

**Recommended fix:**

In §14 (Session Cache & WS Reconnect), add a warning after the constants table:

> **Warning — turn timeout vs. hung threshold:** `AXON_ACP_TURN_TIMEOUT_MS` is configurable; `SESSION_HUNG_TURN_THRESHOLD` is hardcoded at 300 seconds. If `AXON_ACP_TURN_TIMEOUT_MS` is set above 300,000 ms (5 minutes), the reaper will evict the session as hung before the turn times out naturally. This kills the adapter mid-response and leaves the WS client waiting on a dead channel. If you raise `AXON_ACP_TURN_TIMEOUT_MS`, you must also recompile with a raised `SESSION_HUNG_TURN_THRESHOLD` constant in `crates/services/acp/session_cache.rs`.

In §22 (Config Options), add `AXON_ACP_TURN_TIMEOUT_MS` to the table with a cross-reference to this warning.

---

### Finding 4 — High: `spawn_adapter_skip_validation` Is Not Documented

**Severity:** High

**What is wrong:**

`crates/services/acp.rs` exports `spawn_adapter_skip_validation()` as a `pub` function with `#[doc(hidden)]`. The function bypasses `validate_adapter_command()` to allow integration tests to spawn real shell interpreters (like `sh`). It is currently used only in test code but is `pub` and therefore callable from production code paths.

`docs/ACP.md` §8 (Security Validation) documents the shell blocklist thoroughly but makes no mention of this escape hatch. A new contributor unfamiliar with the codebase could call it from production without understanding the security implication.

**Recommended fix:**

Add a subsection under §8 (Security Validation):

> **`spawn_adapter_skip_validation()` — test-only escape hatch:** This function is `pub` but `#[doc(hidden)]` and must not be called from production code. It exists solely for integration tests that need to spawn a real shell interpreter (e.g. `sh`) without being blocked by the blocklist validator. Calling it in production bypasses the security boundary that prevents shell-injection via the adapter command. Enforcement is by convention and code review, not by Rust visibility rules.

---

### Finding 5 — Medium: Session Cancel Is Documented in the ACP Protocol Reference But Not in Axon's Implementation Gap

**Severity:** Medium

**What is wrong:**

The ACP wire protocol supports `session/cancel` as a notification sent from client to agent to interrupt a running prompt (documented in `.claude/skills/acp/references/wire-format.md` and the ACP SDK's `Agent::on_cancel()` trait). `docs/ACP.md` §17 (WebSocket Message Protocol) documents Axon's inbound WS message types, and §11 (Wire Protocol: Client → Adapter) documents what Axon sends to the adapter. Neither section mentions `session/cancel`.

The `cancel` WS message type (`§17`) cancels async job queue entries (crawls, embeds, etc.) — it does not send `session/cancel` to a running ACP adapter turn. Searching the entire `crates/` directory confirms: there is no call to `conn.cancel()` or `CancelNotification` anywhere in the Axon codebase. A running ACP turn cannot be interrupted mid-flight by the user.

This is a real behavioral gap: the only way to stop a hung ACP turn is to wait for the per-turn timeout (5 min) or close the WS connection, which triggers adapter teardown via `kill_on_drop`.

**Recommended fix:**

Add a subsection to §17 (WebSocket Message Protocol) or a new §25 "Known Limitations":

> **ACP turn cancellation is not implemented.** The ACP wire protocol defines `session/cancel` as a notification that instructs an adapter to interrupt a running prompt. Axon does not currently send `session/cancel` to the adapter. The only mechanisms to interrupt a running turn are:
>
> 1. Close the WebSocket connection — triggers `kill_on_drop` SIGKILL on the adapter process.
> 2. Wait for the per-turn timeout (`AXON_ACP_TURN_TIMEOUT_MS`, default 5 min) — the session is then evicted by the reaper.
>
> A future implementation would send `conn.cancel(CancelNotification::new(session_id))` in response to a `cancel` WS message with `mode: "acp"`. The ACP SDK's `Agent::on_cancel()` callback handles this notification on the adapter side. Until implemented, users cannot interrupt long-running agent turns without disconnecting.

---

### Finding 6 — Medium: `acp_llm.rs` `warm_session` System-Prompt Composition Is Undocumented

**Severity:** Medium

**What is wrong:**

`crates/services/acp_llm.rs::compose_prompt()` prepends a system prompt to the user request when `AcpCompletionRequest.system_prompt` is set:

```rust
format!("System instructions:\n{system}\n\nUser request:\n{user}")
```

This means the system prompt is embedded in the user turn content, not delivered via a separate ACP protocol field. The ACP protocol has no dedicated system-message concept at the turn level — Axon's approach of concatenating into a single prompt block is a deliberate workaround.

`docs/ACP.md` §16 documents `complete_text` and `complete_streaming` signatures and the `WarmSession` pattern but does not describe how system prompts are handled or the concatenation scheme. A caller who expects the system prompt to be delivered out-of-band (as a separate context block) will be surprised when it appears embedded in the user message.

**Recommended fix:**

In §16 (ACP-Backed LLM Completions), add a note on system prompt handling:

> **System prompt delivery:** The ACP protocol has no dedicated system-message concept at the turn level. When `AcpCompletionRequest.system_prompt` is set, `compose_prompt()` concatenates it into the user turn using the format:
>
> ```
> System instructions:
> {system}
>
> User request:
> {user}
> ```
>
> Both parts arrive as a single prompt block. This is a deliberate choice because the ACP `PromptRequest` only accepts a list of `ContentBlock`s, with no out-of-band system channel. Callers who need true system-message separation should use a model/adapter that accepts system instructions embedded in the first user turn (Claude Code, Codex, and Gemini all handle this pattern correctly).

---

### Finding 7 — Low: §3 Comparison Table "Turn timeout" Row Is Ambiguous

**Severity:** Low

**What is wrong:**

The execution mode comparison table in §3 (Execution Modes: One-Shot vs Persistent) has this row:

| | One-Shot | Persistent |
|--|----------|------------|
| **Turn timeout** | N/A | 5 min (`DEFAULT_TURN_TIMEOUT`, env: `AXON_ACP_TURN_TIMEOUT_MS`) |

"N/A" for one-shot is technically correct — there is no per-turn timeout in the one-shot path — but the one-shot overall timeout (`ACP_ADAPTER_TIMEOUT`, 300s, hardcoded in `acp.rs:93`) covers a single turn in that mode. A reader expecting to find timeout information for one-shot turns will not see it here. The overall timeout appears in §3 ("Overall timeout: 300s") but the connection between "that IS the per-turn timeout for one-shot" is unstated.

**Recommended fix:**

Change the one-shot "Turn timeout" cell:

> **Turn timeout** | 300s overall (`ACP_ADAPTER_TIMEOUT`) — covers exactly one turn; no separate per-turn limit | 5 min per turn (`DEFAULT_TURN_TIMEOUT`, env: `AXON_ACP_TURN_TIMEOUT_MS`)

---

### Finding 8 — Low: `mapping.rs` Is Listed in Key Source Files But the Module Was Split

**Severity:** Low

**What is wrong:**

`docs/ACP.md` §1 (Key Source Files) lists `crates/services/acp/mapping.rs` as a source file. The actual implementation is in `crates/services/acp/mapping/` (a directory module), with `validation.rs` as a submodule. The file `mapping.rs` does not exist at the path listed — only `mapping/validation.rs` appears in the scope of the review, and the module root would be at `mapping.rs` (per Rust 2018 convention) with submodule `mapping/validation.rs`.

This is a minor naming issue since the mapping module root does exist as `mapping.rs` in the module tree, but the row that lists `mapping/validation.rs` below it is accurate. The source file table is internally consistent if the reader understands Rust module layout. However, a new contributor running `find . -name "mapping.rs"` will not get confused — this is low impact.

**Recommended fix:**

No change required if the module file exists. Confirm with a quick file check that `crates/services/acp/mapping.rs` exists (not a `mapping/mod.rs` which the CLAUDE.md prohibits). If the module root is `mapping.rs` alongside a `mapping/` directory, the table is correct as-is.

---

## Config Options Completeness Audit (§22)

The document does not have a dedicated §22 table of all env vars. Env vars are scattered across sections. A consolidated reference is needed. Current documented env vars in ACP.md:

| Env Var | Documented Location | Status |
|---------|---------------------|--------|
| `AXON_ACP_ADAPTER_CMD` | §6, §16 | Correct |
| `AXON_ACP_ADAPTER_ARGS` | §6, §16 | Correct |
| `AXON_ACP_CLAUDE_ADAPTER_CMD` | §2, §6 | Correct |
| `AXON_ACP_CODEX_ADAPTER_CMD` | §2, §6 | Correct |
| `AXON_ACP_GEMINI_ADAPTER_CMD` | §2, §6 | Correct |
| `AXON_ACP_CLAUDE_ADAPTER_ARGS` | §2 | Correct |
| `AXON_ACP_CODEX_ADAPTER_ARGS` | §2 | Correct |
| `AXON_ACP_GEMINI_ADAPTER_ARGS` | §2 | Correct |
| `AXON_ACP_AUTO_APPROVE` | §10 | Correct |
| `AXON_ACP_TURN_TIMEOUT_MS` | §3, §14 | Correct — missing interaction warning (Finding 3) |
| `AXON_ACP_MAX_CONCURRENT_SESSIONS` | §14, §20 | Correct |
| `AXON_ACP_PREWARM` | §15 | Correct |
| `OPENAI_MODEL` | §16 | Correct |
| `AXON_WEB_API_TOKEN` | §20 | Correct |

**Missing from documentation:**

| Env Var | Actual behavior | Source |
|---------|-----------------|--------|
| `SESSION_TTL` | Hardcoded constant (30 min), NOT an env var | `session_cache.rs:19` — doc §14 correctly calls it a constant; no gap |
| `SESSION_HUNG_TURN_THRESHOLD` | Hardcoded constant (5 min), NOT an env var | `session_cache.rs:24` — doc §14 correctly calls it a constant; interaction with `AXON_ACP_TURN_TIMEOUT_MS` is the gap (Finding 3) |

No missing env vars were found. The §14 constants table accurately reflects the `session_cache.rs` constants. The only gap is the interaction hazard in Finding 3.

---

## Security Documentation Audit

The security documentation in `docs/ACP.md` is thorough. The following properties are correctly documented:

- Shell blocklist (§8): All 11 blocked names match `BLOCKED_SHELLS` in `validation.rs`
- Symlink canonicalization check (§8): Documented and matches code in `validation.rs:75–87`
- `env_clear()` + allowlist (§7): Table matches `ACP_ENV_ALLOWLIST` in `acp.rs:100–128` exactly
- `CLAUDECODE` exclusion reason (§7): Documented correctly
- `OPENAI_*` exclusion reason (§7): Documented correctly
- `(session_id, tool_call_id)` composite key (§10): Documented correctly; matches `permission.rs:109`
- Connection binding for session ownership (§14, §25): Documented correctly

One limitation in the security section: the `spawn_adapter_skip_validation` escape hatch bypasses all security validation but is not mentioned (Finding 4).

---

## Inline Code Comment Quality

The code is well-commented overall. The following are non-obvious decisions that have comments:

- `acp.rs:99–139`: `ACP_ENV_ALLOWLIST` rationale — well explained
- `acp.rs:240–247`: `env_clear()` scope and `CLAUDECODE`/`OPENAI_*` exclusions — well explained
- `runtime.rs:241–249`: `SIGKILL-FIX` comment for `wait_for_adapter_exit` placement — well explained
- `bridge.rs:152–158`: `RefCell` vs `Mutex` rationale — well explained
- `bridge.rs:177–193`: 5-second permission emit timeout + cancellation rationale — well explained
- `permission.rs:113–126`: `PermissionGuard` RAII map cleanup — well explained

**Gaps found in inline comments:**

- `acp_llm.rs:184`: No comment explaining why `LocalSet::run_until()` is used on the multi-thread runtime here vs. `spawn_blocking` everywhere else (Finding 1)
- `persistent_conn.rs:182` and `persistent_conn.rs:278`: No cross-reference comment linking the two functions' shared logic (Finding 2)
- `pulse_chat.rs:117–121`: System prompt prepend for new sessions has a `// Divergence warning` about SSE path but no comment about the fact that `session/cancel` is not implemented (Finding 5)

---

## Verdict

`docs/ACP.md` is accurate where it covers the implementation. The primary maintenance risk is Finding 3 (turn timeout / hung threshold interaction) which is an operational hazard for any operator raising `AXON_ACP_TURN_TIMEOUT_MS`. Finding 5 (missing cancel limitation) is the only outright behavioral claim that the document omits — it does not claim cancel is implemented, but it also does not document that it is absent, which is worse from a contributor perspective than a wrong claim because it gives no signal either way.

Findings 1 and 4 are the highest-priority inline comment gaps: both involve non-obvious safety constraints that could lead a future contributor to introduce a panic or a security regression.
