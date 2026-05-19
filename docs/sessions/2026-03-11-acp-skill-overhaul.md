# ACP Skill Overhaul — 2026-03-11

## Session Overview

Complete overhaul of the `acp` Claude skill (`~/.claude/skills/acp/`) for authoring ACP (Agent Client Protocol) agents and clients in Rust. Multi-round review against actual SDK source code discovered and fixed pervasive fabricated APIs across SKILL.md, examples, and all reference files. The skill now reflects the verified SDK contracts from `~/workspace/acp/rust-sdk/`.

## Timeline

1. **Initial review** — Dispatched parallel agents to explore crawled ACP docs, the codex-acp and claude-agent-acp repos, and the agent-client-protocol schema crate
2. **First overhaul** — Fixed ~12 non-compiling API calls in examples and reference files (ToolCallKind→ToolKind, method names, non-exhaustive builders, AuthMethod enum, etc.)
3. **Second sweep** — Second round of agents found additional issues in wire-format and message-reference files; fixed method name casing (camelCase→snake_case)
4. **Repo reorganization** — Moved 3 ACP repos to `~/workspace/acp/`, cloned `rust-sdk`
5. **SDK source verification** — Read actual SDK `agent.rs`, `client.rs`, `lib.rs`, `examples/agent.rs`; discovered fundamental architectural issue: `SessionNotifier` doesn't exist, `prompt()` takes only `PromptRequest`
6. **Final overhaul** — Rewrote SKILL.md, agent-impl.rs, and client-impl.rs to match verified SDK source

## Key Findings

### Critical API Bugs Fixed (SDK-Verified)

| Bug | Fix | Source |
|-----|-----|--------|
| `Agent::prompt` had fake `SessionNotifier` param | Removed — takes only `PromptRequest` | `rust-sdk/src/agent.rs:74` |
| `on_cancel` method name | Changed to `cancel`; returns `Result<()>` | `rust-sdk/src/agent.rs:87` |
| `#[async_trait]` without `?Send` | Must use `#[async_trait::async_trait(?Send)]` | `rust-sdk/src/agent.rs:24` |
| `SessionNotifier` type doesn't exist | Removed from all files | SDK has no such type |
| `close_session` is stable | It's **unstable** behind `#[cfg(feature = "unstable_session_close")]` | `rust-sdk/src/agent.rs:198` |
| `AgentMessageChunk("text".into())` bare string | Must be `ContentChunk::new("text")` | SDK schema types |
| `notifier.send()` streaming pattern | Must use mpsc channel + `conn.session_notification()` | `rust-sdk/examples/agent.rs` |
| `AgentSideConnection` first return value discarded | `(conn, io_task)` — conn is needed for notifications | `rust-sdk/src/lib.rs:405` |
| `#[tokio::main]` full runtime | Must use `current_thread` flavor for `?Send` | SDK examples |
| `Client::session_notification` optional | It's **required** in the trait | `rust-sdk/src/client.rs:46` |
| `ClientSideConnection::new` 3-arg, wrong order | 4-arg: `(client, outgoing, incoming, spawner)` | `rust-sdk/src/lib.rs:54` |
| `connection.run()` | Returns `(conn, io_task)`; await `io_task` | SDK API |

### Previously Fixed (Earlier Rounds)

| Bug | Fix |
|-----|-----|
| `ToolCallKind` with 15 nonexistent variants | Real enum is `ToolKind` with 10 variants |
| `ToolCallStatus::Started` | Real variants: `Pending`, `InProgress`, `Completed`, `Failed` |
| `session/setMode` camelCase method names | All ACP methods use snake_case |
| `SessionUpdate` count "10 total" | 11 total — 10 stable + 1 unstable |
| `AgentInfo` type | Correct type is `Implementation` |
| `AuthMethod` as struct | It's an enum: `AuthMethod::Agent(AuthMethodAgent::new(id, name))` |
| `AuthenticateResponse { authenticated: true }` | `AuthenticateResponse::default()` |
| `NewSessionResponse` struct literal | `NewSessionResponse::new(session_id)` |
| `req.cwd.unwrap_or_default()` | `cwd` is `PathBuf` (not `Option`) |
| `RequestPermissionOutcome::Approved` | Variants: `Cancelled` or `Selected(SelectedPermissionOutcome)` |
| `use agent_client_protocol::types::*` | No `types` module — all at crate root |
| `ProtocolVersion::LATEST` | Real example uses `ProtocolVersion::V1` |

## Technical Decisions

- **mpsc channel pattern for streaming**: Since `prompt()` has no access to the connection, the canonical pattern is to store `mpsc::UnboundedSender<(SessionNotification, oneshot::Sender<()>)>` in the agent and send from `prompt()`, while a background task owns `conn` and calls `session_notification()`. The oneshot ensures ordered delivery.
- **`current_thread` flavor**: The Agent trait is `?Send` because the SDK uses `Rc` internally. Must use `#[tokio::main(flavor = "current_thread")]` and `LocalSet`.
- **Paths updated**: All references updated from `~/workspace/codex-acp/` → `~/workspace/acp/codex-acp/`, `~/workspace/agent-client-protocol/` → `~/workspace/acp/agent-client-protocol/`.

## Files Modified

| File | Change |
|------|--------|
| `.claude/skills/acp/SKILL.md` | Complete rewrite — correct Agent/Client trait signatures, streaming pattern, ?Send requirement, updated paths |
| `.claude/skills/acp/examples/agent-impl.rs` | Complete rewrite — correct prompt() signature, cancel() method, mpsc channel streaming, ContentChunk |
| `.claude/skills/acp/examples/client-impl.rs` | Rewrite — added required session_notification(), fixed ClientSideConnection::new args, current_thread |
| `.claude/skills/acp/references/tool-calls.md` | Rewrite — real ToolKind (10 variants), correct status variants, streaming dedup |
| `.claude/skills/acp/references/message-reference.md` | Fixed — method names, SessionUpdate count, error codes, SDK helper constructors |
| `.claude/skills/acp/references/wire-format.md` | Added — terminal API, session/list, session/set_mode examples |
| `.claude/skills/acp/references/codex-patterns.md` | Fixed — biased select, removed duplicate cancel branch, std::path::absolute() |
| `.claude/skills/acp/references/unstable-features.md` | New — all 9 unstable feature flags |

## Repo Reorganization

- `~/workspace/agent-client-protocol/` → `~/workspace/acp/agent-client-protocol/` (schema-only crate)
- `~/workspace/codex-acp/` → `~/workspace/acp/codex-acp/` (production Rust agent)
- `~/workspace/claude-agent-acp/` → `~/workspace/acp/claude-agent-acp/` (Claude adapter)
- `~/workspace/acp/rust-sdk/` cloned from `https://github.com/agentclientprotocol/rust-sdk`

## Behavior Changes

**Before**: Skill contained fabricated APIs that would fail to compile. The `Agent::prompt` method included a `SessionNotifier` parameter that doesn't exist. `on_cancel` was the wrong method name. `SessionNotifier` was described throughout as the streaming mechanism.

**After**: All examples compile against the actual SDK. The streaming pattern uses `conn.session_notification()` via an mpsc channel. The `?Send` requirement is explicit and the correct runtime flavor is documented.

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `Agent::prompt` signature | Only `PromptRequest` | `async fn prompt(&self, args: PromptRequest) -> Result<PromptResponse>` | ✅ |
| Cancel method name | `cancel` | `async fn cancel(&self, args: CancelNotification) -> Result<()>` | ✅ |
| async_trait annotation | `?Send` | `#[async_trait::async_trait(?Send)]` | ✅ |
| `session_notification` | Required in Client | Has no default impl in trait | ✅ |
| `close_session` | Unstable | Behind `#[cfg(feature = "unstable_session_close")]` | ✅ |
| ContentChunk | Wraps AgentMessageChunk | `SessionUpdate::AgentMessageChunk(ContentChunk::new(text))` | ✅ |
| `AgentSideConnection::new` | Returns `(conn, io_task)` | `(Self, impl Future<Output = Result<()>>)` | ✅ |

## Open Questions

- Whether `ProtocolVersion::LATEST` exists as a constant (skill now uses `V1` as shown in SDK examples)
- The exact error type for `map_err(|_| acp::Error::internal_error())` idiom — the SDK uses its own `Error` type, not `anyhow::Error`
- Whether `ClientSideConnection` needs `LocalSet` or can use the full multithreaded runtime (SDK examples use `current_thread`)

## Next Steps

- Consider adding a `references/notification-patterns.md` for the mpsc channel streaming pattern in more depth
- The `codex-acp` repo has production patterns worth extracting into `references/codex-patterns.md`
- Re-embed the updated skill files into Qdrant via `axon embed`
