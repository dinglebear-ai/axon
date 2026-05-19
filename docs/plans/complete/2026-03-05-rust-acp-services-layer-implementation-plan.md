# Rust ACP Services Layer Implementation Plan

Date: 2026-03-05  
Status: Draft (ready for execution)  
Owner: `feat/services-layer-refactor`

## 1) Objective

Replace Pulse chat’s direct `claude` subprocess orchestration in Next.js with a Rust ACP client path in the services layer, while keeping current Pulse streaming UX and permission behavior.

## 2) Why this path

- Current Pulse path is Node-only subprocess orchestration in [route.ts](/home/jmagar/workspace/axon_rust/apps/web/app/api/pulse/chat/route.ts:288).
- Rust already owns the shared execution bridge and service dispatch for most commands in [execute.rs](/home/jmagar/workspace/axon_rust/crates/web/execute.rs:157) and [sync_mode.rs](/home/jmagar/workspace/axon_rust/crates/web/execute/sync_mode.rs:26).
- Existing services layer exists but has no ACP service module yet in [services.rs](/home/jmagar/workspace/axon_rust/crates/services.rs:1).
- ACP Rust SDK is documented and supports both client and agent sides, with stdio subprocess transport and required client callbacks (`request_permission`, `session_notification`) per indexed source and docs.rs trait page.

## 3) Research summary (inputs used)

Protocol/library
- ACP architecture and stdio transport behavior retrieved from:
  - `https://agentclientprotocol.com/get-started/architecture`
  - `https://agentclientprotocol.com/protocol/transports`
  - `https://agentclientprotocol.com/libraries/rust`
- ACP Rust trait details retrieved from indexed docs.rs source id:
  - `.cache/axon-rust/output/scrape-markdown/runs/302e44c7-247b-43e1-9f7a-0bc329217402/0001-docs-rs-agent-client-protocol-latest-agent-client-protocol-trait-client-html.md`
- ACP reference code scraped and indexed:
  - `https://raw.githubusercontent.com/agentclientprotocol/rust-sdk/main/examples/client.rs`
  - `https://raw.githubusercontent.com/agentclientprotocol/rust-sdk/main/examples/agent.rs`

Adapter strategy references
- `https://raw.githubusercontent.com/zed-industries/claude-agent-acp/main/README.md`
- `https://raw.githubusercontent.com/zed-industries/codex-acp/main/README.md`

Codebase entry points analyzed
- Pulse Node orchestration: [route.ts](/home/jmagar/workspace/axon_rust/apps/web/app/api/pulse/chat/route.ts:124), [claude-stream-types.ts](/home/jmagar/workspace/axon_rust/apps/web/app/api/pulse/chat/claude-stream-types.ts:114), [chat-stream.ts](/home/jmagar/workspace/axon_rust/apps/web/lib/pulse/chat-stream.ts:1)
- Rust execution bridge: [web.rs](/home/jmagar/workspace/axon_rust/crates/web.rs:232), [execute.rs](/home/jmagar/workspace/axon_rust/crates/web/execute.rs:157), [constants.rs](/home/jmagar/workspace/axon_rust/crates/web/execute/constants.rs:5)
- Services layer: [services.rs](/home/jmagar/workspace/axon_rust/crates/services.rs:1), [events.rs](/home/jmagar/workspace/axon_rust/crates/services/events.rs:3), [types.rs](/home/jmagar/workspace/axon_rust/crates/services/types.rs:1)

## 4) Scope and non-goals

In scope
- Add Rust ACP client orchestration service(s) for Pulse-style chat turns.
- Keep current web stream contract (`status`, `assistant_delta`, `tool_use`, `thinking_content`, `done`, `error`) compatible.
- Keep permission-policy enforcement behavior equivalent to current Pulse flow.
- Make adapter binary configurable so Claude/Codex/Gemini ACP adapters can be swapped without JS orchestration rewrite.

Out of scope (this plan)
- Rewriting external adapter repos.
- Replacing all web APIs with Rust in one PR.
- New UX for permissions/tool reviews.

## 5) Proposed architecture

### 5.1 New Rust service module

Add `crates/services/acp.rs` with:
- ACP client session lifecycle:
  - spawn adapter subprocess (`stdio`)
  - initialize
  - new/load session
  - prompt turn
  - cancel
- Client callback implementation to handle:
  - `request_permission`
  - `session_notification` streaming updates
- Mapping ACP updates to current Pulse stream event model.

### 5.2 Event bridge

Extend service events in [events.rs](/home/jmagar/workspace/axon_rust/crates/services/events.rs:3) with ACP-specific payload variants (no nested JSON strings), then translate to WS v2 event messages in web execute/ws layers.

### 5.3 Web integration path

Phase 1 integration (lowest risk):
- Keep `/api/pulse/chat` route as transport shell.
- Replace Node `spawn('claude', ...)` path with call into Rust worker WS mode (new mode, e.g. `pulse_chat`), and stream back unchanged Pulse NDJSON protocol.

Phase 2 integration (optional immediate follow-up):
- Move remaining request assembly/context retrieval from Node to Rust service APIs if desired.

## 6) Implementation phases (TDD required each phase)

### Phase A: ACP foundation in Rust

Files
- `Cargo.toml` (add `agent-client-protocol` crate)
- `crates/services.rs` (export `acp` module)
- `crates/services/acp.rs` (new)
- `crates/services/types.rs` (ACP request/response types)
- `crates/services/events.rs` (ACP event variants)

Tests
- `tests/services_acp_smoke.rs`
- `tests/services_acp_event_mapping.rs`

Acceptance
- Service can spawn a configurable ACP adapter command and complete initialize/session/prompt against a mock ACP agent fixture.

### Phase B: Web execute bridge support

Files
- `crates/web/execute/constants.rs` (allow new mode)
- `crates/web/execute/sync_mode.rs` or new dedicated mode file
- `crates/web/execute/events.rs` (map ACP service events to WS messages)
- `apps/web/lib/ws-protocol.ts` (if needed for new mode typing only)

Tests
- `crates/web/execute/tests/*` new coverage for mode routing and output framing.

Acceptance
- WS `execute` request in new mode returns streamed events and terminal `done/error` frame.

### Phase C: Pulse route cutover

Files
- `apps/web/app/api/pulse/chat/route.ts`
- `apps/web/app/api/pulse/chat/claude-stream-types.ts` (retire Claude-CLI-specific arg building)
- `apps/web/lib/pulse/*` (only where contract glue is needed)

Tests
- existing pulse stream parser/route tests updated to pass with Rust-backed stream.

Acceptance
- No direct `spawn('claude', ...)` in Pulse route.
- Existing frontend behavior unchanged for streaming and doc operations.

### Phase D: Adapter configuration + provider switching

Files
- `.env.example`
- `crates/core/config/*` (new ACP adapter command/env wiring)
- `docker-compose.yaml` (only if runtime env wiring needed)

Required env model (example)
- `AXON_ACP_ADAPTER_CMD` (default adapter command)
- `AXON_ACP_ADAPTER_ARGS`
- provider-specific keys remain in `.env`

Acceptance
- Switching between claude/codex/gemini ACP adapter binaries is env-only.

## 7) Effort and change estimate

Estimated code touched
- Rust: ~10-16 files (new module + execute integration + config + tests)
- Web TS: ~3-6 files (Pulse route and minimal glue)
- Infra/docs: ~2-4 files

Estimated implementation effort
- Phase A: 1.5-2.5 days
- Phase B: 0.5-1 day
- Phase C: 0.5-1.5 days
- Phase D + hardening: 0.5-1 day
- Total: 3-6 engineering days depending on test complexity and adapter edge cases.

## 8) Primary risks

- ACP SDK futures are often `!Send` in examples; thread model must match runtime constraints.
- Permission callback parity: current Pulse permission model is local policy-first; ACP introduces protocol permission requests that must map cleanly.
- Event ordering/flush behavior differences vs current Claude stream parser.
- Session resume semantics (`sessionId`) must remain compatible with existing Pulse UX.

## 9) Verification matrix

- Unit
  - ACP event -> Pulse stream mapping
  - Permission decision mapping
  - session cancel behavior
- Integration
  - mock ACP agent fixture turn flow
  - WS execute mode streaming + done/error
- Regression
  - current Pulse API tests green
  - `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`
  - `pnpm test` for affected web tests

## 10) Rollout strategy

- Behind feature flag/env (`AXON_PULSE_BACKEND=rust-acp|node-claude`)
- Shadow mode option: execute Rust ACP path and compare terminal outputs/logs without user-visible switch for first pass.
- Remove legacy Node claude subprocess path only after parity tests and manual acceptance.

## 11) Exit criteria

- Pulse route no longer spawns `claude` directly.
- Rust ACP service handles prompt turns with streamed updates and permission flow.
- Existing Pulse frontend behavior and APIs remain stable.
- Adapter switch for Claude/Codex/Gemini is configuration-only.
