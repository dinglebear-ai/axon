# Session Documentation — Pulse Chat Probe Timeout Debug

## Session Overview

- Investigated `POST /api/pulse/config` returning `502` after a 60 second wait while `pulse_chat_probe` timed out.
- Narrowed the issue to WebSocket execution correlation rather than the unrelated `.well-known/*` 404s.
- Patched the Rust WebSocket bridge to preserve the client-provided `exec_id` and added focused regression tests.
- Verified the fix with targeted Rust and web test runs.

## Timeline

- Reviewed the reported logs showing `Timeout waiting for axon pulse_chat_probe (60000ms)` from `apps/web/lib/axon-ws-exec.ts`.
- Traced the request path through `apps/web/app/api/pulse/config/route.ts`, the web WS executor, and the Rust WebSocket handler / execute path.
- Confirmed that the frontend sends `exec_id` on `execute` messages and only resolves pending requests when returned frames carry the same `ctx.exec_id`.
- Found that the Rust bridge deserialized only `id`, then generated a fresh execution ID for outbound frames.
- Implemented the correlation fix in `crates/web/ws_handler.rs` and `crates/web/execute.rs`, then added Rust tests for `exec_id` alias handling and legacy cancel behavior.
- Ran focused verification commands in both Rust and `apps/web`.

## Key Findings

- `apps/web/app/api/pulse/config/route.ts:90-101` waits on `runAxonCommandWsStream('pulse_chat_probe', ...)`, so the route blocks until the WS bridge emits completion or error.
- `apps/web/lib/axon-ws-exec.ts:325-372` generates an `execId`, stores a pending request under that ID, and sends `{ type: 'execute', ..., exec_id }`.
- `apps/web/lib/axon-ws-exec.ts:157-203` only routes `command.output.json`, `command.done`, and `command.error` frames when `data.ctx.exec_id` matches the pending request key.
- `crates/web/ws_handler.rs:23-45` previously deserialized only `id`; after the fix it accepts `exec_id` as an alias and stores it in `id`.
- `crates/web/execute.rs:164-186` previously always generated `exec-<uuid>`; after the fix it preserves the inbound client execution ID when present.
- `crates/web/ws_handler.rs:346-368` now contains regression tests proving `execute` accepts `exec_id` while `cancel` continues to accept `id`.

## Technical Decisions

- Ignored the `.well-known/oauth-authorization-server/mcp` and `.well-known/openid-configuration/mcp` 404s because the user explicitly scoped the debug effort away from them.
- Fixed the bug at the WS protocol boundary instead of changing `apps/web/lib/axon-ws-exec.ts`, because the frontend correlation behavior already matched the documented/requested wire shape.
- Preserved backwards compatibility for cancel requests by keeping the existing `id` field and adding `alias = "exec_id"` only for deserialization.
- Left the fallback UUID generation in place in `crates/web/execute.rs:177-182` so server-side callers without a client-provided ID still get a valid `ctx.exec_id`.

## Files Modified

- `crates/web/ws_handler.rs` — accept `exec_id` on inbound execute messages, forward it into command handling, and add regression tests.
- `crates/web/execute.rs` — preserve the inbound client execution ID instead of always generating a new one.
- `/home/jmagar/.copilot/session-state/6f61c970-bda3-418b-84bb-e51f91644d2d/plan.md` — recorded the investigation approach, implementation target, and verification plan for this session.

## Commands Executed

- `git --no-pager status --short` — confirmed a dirty worktree with unrelated changes and avoided touching them.
- `cargo test execute_message_accepts_exec_id_alias && cargo test cancel_message_still_accepts_id_field` — both focused Rust tests passed after the patch.
- `cd apps/web && pnpm vitest run __tests__/api/pulse-config-route.test.ts __tests__/axon-ws-exec.test.ts` — 2 test files passed, 9 tests passed.
- `cargo fmt --all -- crates/web/ws_handler.rs crates/web/execute.rs && cargo fmt --check --all -- crates/web/ws_handler.rs crates/web/execute.rs` — formatting applied and check passed for the touched Rust files.
- `./scripts/axon status --json` — returned status JSON, with a compile-time warning about an unused `header` import in `crates/mcp/server/oauth_google/handlers_protected.rs`.
- `./scripts/axon embed "docs/sessions/2026-03-12-pulse-chat-probe-timeout-debug.md" --json` — returned `{"job_id":"da83a83b-ef21-4e37-bab6-87a0bbb9389d","source":"rust","status":"pending"}`.
- `./scripts/axon embed status "da83a83b-ef21-4e37-bab6-87a0bbb9389d" --json` — reported `status: completed`, `result_json.collection: cortex`, `result_json.source: rust`, `chunks_embedded: 4`, `docs_embedded: 1`.
- `./scripts/axon retrieve "rust" --collection "cortex" --json` — returned `No content found for URL: rust`.
- `./scripts/axon retrieve "docs/sessions/2026-03-12-pulse-chat-probe-timeout-debug.md" --collection "cortex" --json` — returned the saved markdown content with `chunks: 4`.

## Behavior Changes (Before/After)

- Before: `POST /api/pulse/config` could wait the full 60 seconds and return `502` because the browser-side pending WS request never observed a matching `ctx.exec_id`.
- After: the Rust WS bridge echoes the caller's `exec_id`, so `pulse_chat_probe` responses can resolve the correct pending request and the config route can complete normally.
- Before: execute requests used a split protocol where the client sent `exec_id` but the server emitted a different `ctx.exec_id`.
- After: execute requests preserve correlation end-to-end while legacy cancel requests still use `id`.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test execute_message_accepts_exec_id_alias` | Rust WS alias test passes | `ok` | PASS |
| `cargo test cancel_message_still_accepts_id_field` | Legacy cancel field test passes | `ok` | PASS |
| `cd apps/web && pnpm vitest run __tests__/api/pulse-config-route.test.ts __tests__/axon-ws-exec.test.ts` | Focused web tests pass | `Test Files 2 passed (2); Tests 9 passed (9)` | PASS |
| `cargo fmt --check --all -- crates/web/ws_handler.rs crates/web/execute.rs` | Formatting check passes for touched files | passed after formatting | PASS |
| `./scripts/axon embed "docs/sessions/2026-03-12-pulse-chat-probe-timeout-debug.md" --json` | Embed job is queued successfully | `job_id=da83a83b-ef21-4e37-bab6-87a0bbb9389d`, `status=pending` | PASS |
| `./scripts/axon embed status "da83a83b-ef21-4e37-bab6-87a0bbb9389d" --json` | Embed job completes and exposes collection/source metadata | `status=completed`, `collection=cortex`, `source=rust`, `chunks_embedded=4` | PASS |
| `./scripts/axon retrieve "rust" --collection "cortex" --json` | Retrieve works with reported source value | `No content found for URL: rust` | FAIL |
| `./scripts/axon retrieve "docs/sessions/2026-03-12-pulse-chat-probe-timeout-debug.md" --collection "cortex" --json` | Best-effort verification against observed embedded path | returned saved markdown with `chunks: 4` | PASS |

## Source IDs + Collections Touched

- Embed job ID: `da83a83b-ef21-4e37-bab6-87a0bbb9389d`.
- Observed collection from embed status: `cortex`.
- Observed source value from embed command / status: initial embed returned `source: rust`; status returned `result_json.source: rust`.
- Retrieve outcome using observed source value: `./scripts/axon retrieve "rust" --collection "cortex" --json` returned `No content found for URL: rust`.
- Best-effort retrieve outcome using the embedded file path: `./scripts/axon retrieve "docs/sessions/2026-03-12-pulse-chat-probe-timeout-debug.md" --collection "cortex" --json` returned the stored markdown content, so Axon indexing verification is a partial success with a source-ID mismatch.

## Risks and Rollback

- Risk: any consumer that relied on the server always generating a new execution ID would now observe the client-provided one when present.
- Mitigation: fallback UUID generation remains in place for callers that do not send a client execution ID.
- Rollback: revert the changes in `crates/web/ws_handler.rs` and `crates/web/execute.rs` to restore the prior ID-generation behavior.

## Decisions Not Taken

- Did not change the frontend timeout logic in `apps/web/lib/axon-ws-exec.ts` because the pending-request matcher was internally consistent and the bug was in backend correlation.
- Did not investigate the `.well-known/*` 404s further because the user explicitly said they were not the concern for this debugging pass.
- Did not touch unrelated dirty files already present in the repository.

## Open Questions

- The live `POST /api/pulse/config` route was not exercised against a running browser/backend after the patch in this session; verification here is based on focused automated tests and code-path tracing.
- The local-file embed workflow reported `source: rust`, but retrieve only succeeded with the embedded path `docs/sessions/2026-03-12-pulse-chat-probe-timeout-debug.md`; the exact source-ID contract for local file embeds remains unclear from observed output.
- Neo4j memory capture remains dependent on tool availability in the current environment.

## Next Steps

- Re-run the live Pulse settings/config flow in the app to confirm the timeout is gone in the integrated environment.
- If any other WS routes show similar hanging behavior, audit them for correlation-ID preservation across the Rust bridge.
- Clarify the local-file embed source-ID contract so `embed status` output and `retrieve` input agree consistently.
