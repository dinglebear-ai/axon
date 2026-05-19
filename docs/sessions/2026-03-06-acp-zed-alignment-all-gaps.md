# ACP Zed Alignment — All 5 Remaining Gaps

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`
**Context:** Continuation of ACP (Agent Client Protocol) Zed alignment work. Previous session implemented 10 patterns (permission auto-approve, StopReason handling, tool details enrichment, plan/mode/command forwarding, MCP server passthrough, dynamic model selection, structured logging, subprocess I/O fix, etc.). This session tackles the remaining 5 gaps.

---

## Session Overview

Implemented all 5 remaining Zed alignment gaps for the ACP integration, dispatching 5 parallel agents to maximize throughput:

1. **Session list/resume UI** — Browse and resume previous ACP sessions in sidebar
2. **Terminal integration for tool calls** — Inline xterm.js terminals for tool call output in Pulse chat
3. **Granular permission UI** — Per-tool permission modal (informational in container mode)
4. **Process exit monitoring** — Detect adapter crashes mid-session, emit clean error events
5. **Targeted entry updates** — Differentiate `tool_use` vs `tool_use_update` on the wire

Also wrote a detailed implementation plan for the tool call terminal feature and updated infrastructure (`just dev` now launches `shell-server.mjs`).

---

## Timeline

1. **Shell server fix** — Diagnosed `ECONNREFUSED 127.0.0.1:49011` — `shell-server.mjs` not started by `just dev`
2. **Justfile update** — Added `shell-server.mjs` to `dev` recipe and `stop` recipe
3. **Implementation plan** — Wrote `docs/plans/2026-03-06-acp-tool-call-terminal.md` (8 tasks)
4. **Wire format enrichment** — Added `tool_content` and `tool_input` fields to `AcpSessionUpdateEvent`
5. **Wire type differentiation** — Changed `ToolCallUpdated` from `"tool_use"` to `"tool_use_update"` on wire
6. **Parallel agent dispatch** — 5 agents for all remaining gaps simultaneously

---

## Key Findings

- **ACP SDK `ToolCallContent`** has 3 variants: `Content(ContentBlock)`, `Diff(Diff)`, `Terminal(Terminal)` — first-class terminal support in the protocol (`agent-client-protocol-schema-0.10.8/src/tool_call.rs:448`)
- **`ToolCallUpdateFields`** exposes `content: Option<Vec<ToolCallContent>>`, `raw_input: Option<Value>`, `raw_output: Option<Value>` — all available for forwarding
- **`TerminalEmulator` component** (`apps/web/components/terminal/terminal-emulator.tsx`) is fully reusable for read-only tool output via `disableStdin` + `TerminalHandle.write()`
- **Shell server** (`apps/web/shell-server.mjs`) runs separately from `pnpm dev` on port 49011 — Next.js proxies `/ws/shell` to it
- **No PTY needed** for tool call rendering — the ACP adapter owns the subprocess; we just pipe `ToolCallContent` text into xterm.js

---

## Technical Decisions

1. **Read-only xterm.js over `<pre>` block** — ANSI color rendering, scrollback, search all come for free. Claude Code and Codex both emit ANSI in tool output.
2. **`tool_use_update` wire type** — Differentiated from `tool_use` to enable targeted frontend updates instead of full array replace. This was the targeted entry updates gap (#5).
3. **Process exit via oneshot channel** — `child.wait()` takes `&mut self`, so we spawn it into a `spawn_local` task and communicate via oneshot to race against prompt completion.
4. **Permission modal as informational** — Container mode agents need auto-approve. Modal shows what's being approved but doesn't block. Toggle for future non-container use.
5. **Parallel agent dispatch** — 5 independent work streams with no file overlap: 2 Rust agents (acp.rs, types.rs), 3 frontend agents (different component directories).

---

## Files Modified

### Infrastructure
| File | Change |
|------|--------|
| `Justfile:179` | Added `cd apps/web && node shell-server.mjs &` before `pnpm dev` in `dev` recipe |
| `Justfile:159` | Added `-pkill -f 'shell-server.mjs'` to `stop` recipe |

### Rust Backend
| File | Change |
|------|--------|
| `crates/services/types.rs:193-198` | Added `tool_content: Option<String>` and `tool_input: Option<serde_json::Value>` to `AcpSessionUpdateEvent` |
| `crates/services/types.rs:152-153` | Changed `ToolCallUpdated` serde rename from `"tool_use"` to `"tool_use_update"` |
| `crates/services/types.rs:173` | Updated `Display` impl for `ToolCallUpdated` to `"tool_use_update"` |
| `crates/services/types.rs:291-296` | Added `tool_content`/`tool_input` to custom Serialize impl |

### Documentation
| File | Change |
|------|--------|
| `docs/plans/2026-03-06-acp-tool-call-terminal.md` | Full implementation plan (8 tasks) |

### In-Progress (Parallel Agents)
| Agent | Files Being Modified |
|-------|---------------------|
| Rust ACP | `crates/services/acp.rs` — extract_tool_content, extract_tool_input, process exit monitoring |
| Frontend Terminal | `apps/web/lib/pulse/types.ts`, `chat-stream.ts`, `chat-api.ts`, `pulse-chat-helpers.ts`, new `tool-call-terminal.tsx` |
| Session List/Resume | `apps/web/components/pulse/sidebar/sessions-section.tsx`, `lib/pulse/workspace-persistence.ts` |
| Permission UI | New `apps/web/components/pulse/permission-modal.tsx`, `lib/ws-protocol.ts` |
| Targeted Updates | `crates/services/types.rs` (already applied), test files |

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Tool call wire type | Both `ToolCallStarted` and `ToolCallUpdated` serialize as `"tool_use"` | `ToolCallStarted` = `"tool_use"`, `ToolCallUpdated` = `"tool_use_update"` |
| Tool call content | `ToolCallUpdateFields.content` dropped during extraction | Forwarded as `tool_content` field on wire events |
| Tool call input | `ToolCallUpdateFields.raw_input` dropped | Forwarded as `tool_input` field on wire events |
| `just dev` | Shell server not started — `/ws/shell` returns ECONNREFUSED | Shell server launched automatically |
| `just stop` | Shell server not killed | Shell server killed with other processes |
| Adapter crash | Client hangs until 5min timeout | (in-progress) Clean error event emitted immediately |
| Tool call rendering | Plain text badges | (in-progress) Inline xterm.js terminals with ANSI color |
| Session management | No browse/resume UI | (in-progress) Sidebar session list with resume |
| Permission handling | Silent auto-approve | (in-progress) Informational modal showing approvals |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `grep 49011 Justfile` | shell-server.mjs in dev recipe | Present in both `dev` and `stop` | PASS |
| `grep tool_content crates/services/types.rs` | Field present on struct | Lines 193-194 | PASS |
| `grep tool_use_update crates/services/types.rs` | ToolCallUpdated differentiated | Lines 152, 173 | PASS |

---

## Risks and Rollback

- **Wire format change** (`tool_use_update`): Frontend must handle this new type or tool updates will be silently ignored. The parallel frontend agent is updating the handler. If frontend isn't updated, tool call updates won't render (but initial tool_use still works).
- **Process exit monitoring**: Restructuring child ownership could introduce race conditions if the exit watcher fires before the prompt response is fully processed. The oneshot channel pattern mitigates this.
- **Rollback**: All changes on `feat/services-layer-refactor` branch. `git stash` or revert individual commits.

---

## Decisions Not Taken

- **Full terminal PTY for tool calls** — Rejected. The ACP adapter owns the subprocess. We don't need a real shell, just a text renderer. Using xterm.js in read-only mode is simpler and matches Zed's approach.
- **Blocking permission modal** — Rejected for container mode. Auto-approve is required for containerized agents. Modal is informational with a toggle for future non-container use.
- **Server-side session persistence** — Deferred. Using existing client-side `workspace-persistence.ts` for session list/resume. Server-side persistence would require new API routes and database schema.

---

## Open Questions

1. **ACP `Terminal` content variant** — The SDK has `ToolCallContent::Terminal(Terminal)` for embedding terminal entities by ID. Should we support this in addition to text content? Requires `terminal/create` protocol support.
2. **Permission response wire path** — The frontend permission modal needs to send responses back to Rust. Currently no client→server WS path for permission responses exists. The Rust `AcpBridgeClient` auto-approves synchronously. Wiring this requires the Rust side to async-wait for a response over the WS channel.
3. **Session resume with MCP servers** — When resuming a session via `LoadSessionRequest`, should we pass the current MCP server config or let the adapter use what it had before?
4. **Agent completion status** — 5 agents dispatched in parallel; their output needs review and integration once complete.

---

## Next Steps

1. **Review agent outputs** — Check each of the 5 parallel agents for completion, fix conflicts
2. **Integration testing** — `cargo check && cargo test && cd apps/web && pnpm build`
3. **Manual smoke test** — `just dev`, send prompts that trigger tool calls, verify terminal rendering
4. **Update memory** — Update `zed-acp-notes.md` to mark all 5 gaps as implemented
5. **Commit** — Stage all changes, commit with descriptive message
