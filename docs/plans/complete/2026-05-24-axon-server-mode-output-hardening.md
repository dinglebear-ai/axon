# Axon Server Mode Output Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make server-mode CLI output obey the same human-by-default, JSON-only-with-`--json` contract as local mode.

**Architecture:** Server mode remains a REST adapter, but it must deserialize REST payloads into typed command results and call the same command renderers where possible. Unsupported server-mode commands must fail loudly instead of falling back to JSON.

**Tech Stack:** Rust 2024, clap CLI config, reqwest REST client, serde/serde_json, existing Axon service result types, existing command renderers.

---

## Research And Review Findings Applied

- Server mode previously bypassed command renderers through `src/cli/server_mode/render.rs`, so output parity depended on hand-maintained branches.
- Generic JSON fallback in non-`--json` server mode was the direct cause of `stats` and `ask` printing JSON by default.
- `ask` human rendering had a server-url special case, which made shared rendering depend on routing state instead of render intent.
- `status` and `stats` had wide human rows that exceeded the console cap even after JSON fallback was removed.
- Installed binary drift can hide source fixes; `/home/jmagar/.local/bin/axon` must point at the verified repo build when validating live behavior.

## File Map

- Modify `src/cli/server_mode/render.rs`: explicit server human renderer registry, no JSON fallback, adapter-to-shared-renderer behavior.
- Modify `src/cli/server_mode_tests.rs`: parity guard that every server-routed command has an explicit human renderer.
- Modify `src/cli/commands/ask.rs`: remove server-url-specific output behavior from shared ask renderer.
- Modify `src/cli/commands/status.rs`: split status rows so label/id/progress/error stay under the display cap.
- Modify `src/cli/commands/status_tests.rs`: cap regression tests for status rows.
- Modify `src/vector/ops/stats/display.rs`: wrap payload field names under the display cap.
- Modify `src/vector/ops/stats/display_tests.rs`: cap regression tests for stats field rendering.

### Task 1: Remove JSON Fallback From Server Mode

**Files:**
- Modify: `src/cli/server_mode/render.rs`
- Test: `src/cli/server_mode_tests.rs`

- [x] **Step 1: Add explicit human renderer availability contract**

Add `server_human_renderer_available(command: CommandKind) -> bool` and make the existing server-dispatch test assert every server-routed command is included.

- [x] **Step 2: Replace fallback JSON with an error**

Change the non-JSON `_` branch in `render_server_result` to:

```rust
Err(format!("{} has no server-mode human renderer", cfg.command).into())
```

- [x] **Step 3: Add or wire explicit render branches**

Add branches for `Doctor`, `Sources`, `Domains`, `Map`, `Query`, `Retrieve`, `Evaluate`, `Suggest`, `Search`, and `Research`, reusing existing command renderers where they exist.

### Task 2: Make Ask Rendering Route-Independent

**Files:**
- Modify: `src/cli/commands/ask.rs`
- Modify: `src/cli/server_mode/render.rs`

- [x] **Step 1: Remove `cfg.server_url` output special case**

`print_ask_human` should not branch on server URL. It renders based on output mode only.

- [x] **Step 2: Make server-mode REST ask render as non-streaming human output**

In `render_ask`, clone `Config`, set `ask_stream = false`, then call `print_ask_human`.

### Task 3: Enforce Console Width Caps

**Files:**
- Modify: `src/cli/commands/status.rs`
- Modify: `src/cli/commands/status_tests.rs`
- Modify: `src/vector/ops/stats/display.rs`
- Modify: `src/vector/ops/stats/display_tests.rs`

- [x] **Step 1: Split status rows**

Render status label, id, progress, and error on separate lines. Truncate label and continuation text with the indentation counted inside the 120-character cap.

- [x] **Step 2: Wrap stats payload fields**

Render field names under a `Field Names:` heading and wrap comma-separated fields onto continuation lines capped at 120 characters including indentation.

- [x] **Step 3: Add regression tests**

Assert rendered status and stats lines do not exceed the cap.

### Task 4: Verify Live Runtime

**Files:**
- Operational: `/home/jmagar/.local/bin/axon`
- Operational: Docker container `axon`

- [x] **Step 1: Run static and test verification**

Run:

```bash
cargo fmt --all --check
cargo check --bin axon
cargo test
```

Expected: all pass.

- [x] **Step 2: Rebuild and restart runtime**

Run:

```bash
cargo build --bin axon
ln -sfn /home/jmagar/workspace/axon_rust/target/debug/axon /home/jmagar/.local/bin/axon
docker restart axon
```

Expected: `/home/jmagar/.local/bin/axon` points to this repo's `target/debug/axon`; Docker `axon` is healthy.

- [x] **Step 3: Validate console contract**

Run:

```bash
axon stats
axon stats --json
axon status
axon ask --no-stream --limit 1 'What is Axon? Answer in one sentence.'
axon ask --no-stream --limit 1 --json 'What is Axon? Answer in one sentence.'
```

Expected: non-JSON commands are human-readable; `--json` commands emit JSON.

## Self-Review

- Spec coverage: covers human-by-default, JSON-only-with-`--json`, server/local output parity, status cap, and installed binary drift verification.
- Placeholder scan: no placeholder implementation steps remain.
- Type consistency: server-mode renderers use existing `CommandKind`, `Config`, and service result types.
