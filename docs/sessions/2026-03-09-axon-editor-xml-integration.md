# `<axon:editor>` XML → PlateJS Editor Integration

**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Commit:** `a4ceffd7`

---

## Session Overview

Wired the full pipeline so ACP agents (Claude, Codex, Gemini) can write content directly into the PlateJS editor in `axon-shell.tsx` using `<axon:editor>` XML tags. The agent wraps markdown in `<axon:editor op="replace|append">…</axon:editor>` and the content appears live in the editor. Additionally, `<axon:editor>` blocks are displayed as tool-call cards in the chat UI (not stripped silently), similar to how Claude displays tool use.

---

## Timeline

1. **Investigation**: Confirmed 5 gaps in the existing pipeline — no WS message type, no hook handler, no shell wiring, no Rust `ServiceEvent` variant, no post-turn parsing.
2. **Task 1 (complete, prior session)**: Added `editor_update` to WebSocket protocol types (`lib/ws-protocol.ts`).
3. **Task 2**: Added `onEditorUpdate` callback to `use-axon-acp.ts` + unwrap logic for the `command.output.json` envelope.
4. **Task 3**: Wired `AxonShell.onEditorUpdate` → `setEditorMarkdown` + auto-opens editor pane.
5. **Task 4**: Added `ServiceEvent::EditorWrite { content, operation }` variant to `crates/services/events.rs`.
6. **Task 5**: Added `parse_editor_blocks()` to `crates/services/acp/persistent_conn.rs`; emits `EditorWrite` events after each turn.
7. **Task 6**: Added `EditorWrite` arm to `dispatch_acp_event` in `pulse_chat.rs` → forwards as `editor_update` WS message.
8. **Task 7**: Injected editor syntax guide as system context preamble into first ACP turn (workaround for ACP lacking a `system` field on `NewSessionRequest`).
9. **Task 8**: `AxonMessageList` parses `<axon:editor>` blocks and renders `EditorWriteCard` tool-call cards (op type + content preview).
10. **Stash cleanup**: `stash@{0}` was a lefthook auto-backup from a prior failed commit; confirmed it predated our changes, dropped it.
11. **Test fix**: `tests/cli_help_contract.rs` had a stale `youtube_help_describes_video_url_or_id_only` test — `youtube` subcommand was replaced by `ingest`. Updated to `ingest_help_describes_target_argument`.
12. **Commit**: All hooks green — `a4ceffd7`.

---

## Key Findings

- **ACP `NewSessionRequest` has no `system` field** — `cwd`, `mcp_servers`, `meta` only. System context must be prepended to the first user message of new sessions (`session_id.is_none()`).
- **`assistant_text` only accessible in `persistent_conn.rs`** — `handle_pulse_chat` does not have direct access. `EditorWrite` events must be emitted from `run_turn_on_conn` after the turn completes.
- **WebSocket envelope**: all ACP events arrive as `command.output.json` with `ctx.mode === 'pulse_chat'`; the inner `data.data` is the ACP event. Frontend unwrap logic in `use-axon-acp.ts:40-46` handles this.
- **`<axon:editor>` blocks stripped from `displayText`** but also rendered as `EditorWriteCard` components inline in the message bubble — user sees both the written content preview and the surrounding text.
- **Biome `noUselessFragments` error**: outer `<>…</>` wrapping an IIFE in `axon-message-list.tsx` was flagged; fixed by calling the IIFE directly as the ternary result.

---

## Technical Decisions

| Decision | Rationale |
|---|---|
| XML tags (`<axon:editor>`) over MCP tool | User explicitly chose this approach; MCP tool adds unnecessary complexity |
| Emit `EditorWrite` after turn (not per-delta) | `assistant_text` accumulates across deltas; only complete blocks should fire |
| System context via first-message prepend | ACP protocol limitation — no session-level system prompt field |
| Display blocks as tool-call cards AND strip from text | Matches Claude's tool-use UX; user sees action taken, not raw XML |
| `op="replace"` default | Safer default — explicit `append` opt-in required |

---

## Files Modified

| File | Purpose |
|---|---|
| `apps/web/hooks/use-axon-acp.ts` | Added `onEditorUpdate` callback + `editor_update` WS event handler |
| `apps/web/components/reboot/axon-shell.tsx` | Wired `onEditorUpdate` → `setEditorMarkdown` + auto-open pane |
| `apps/web/components/reboot/axon-message-list.tsx` | Added `parseEditorBlocks()`, `EditorWriteCard`, renders tool-call cards |
| `crates/services/events.rs` | Added `ServiceEvent::EditorWrite { content, operation }` variant |
| `crates/services/acp/persistent_conn.rs` (new) | `parse_editor_blocks()` + 6 tests; emits `EditorWrite` post-turn |
| `crates/web/execute/sync_mode/pulse_chat.rs` (new) | `dispatch_acp_event` EditorWrite arm; first-turn system preamble |
| `tests/cli_help_contract.rs` | Updated stale `youtube` test to `ingest` |

---

## Commands Executed

```bash
# Confirm stash had no axon-message-list.tsx
git stash show "stash@{0}" --name-only | grep axon-message-list
# (no output — file not in stash, stash was safe to drop)

git stash drop stash@{0}
# Dropped stash@{0} (97881246dfe40e89f755cd1a59111302f32426ba)

# Test the failing test
cargo test youtube_help_describes_video_url_or_id_only --test cli_help_contract
# FAILED — youtube subcommand no longer exists

cargo test --test cli_help_contract
# 3 passed after fix

git commit -m "feat(acp): wire <axon:editor> XML blocks to PlateJS editor"
# [refactor/acp-performance-modern-rust a4ceffd7] — all hooks green
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|---|---|---|
| Agent writing to editor | Not possible — no pipeline | Agent wraps markdown in `<axon:editor op="replace">` and editor updates in real time |
| `<axon:editor>` in chat UI | Would show raw XML in message bubble | Shows `EditorWriteCard` (op + 3-line preview) + stripped from message text |
| Editor pane open state | Manual only | Auto-opens when agent writes to editor |
| New ACP session | No editor context | First message prepended with syntax guide explaining `<axon:editor>` tags |
| `youtube` CLI help test | Testing removed subcommand (failing) | Tests `ingest --help` TARGET argument (passing) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test --test cli_help_contract` | 3 passed | 3 passed | ✅ |
| `git commit` (lefthook pre-commit) | All hooks green | All hooks green (954 unit tests pass) | ✅ |
| `git stash list` (post-drop) | stash@{0} gone | Old stashes from other branches only | ✅ |

---

## Source IDs + Collections Touched

None — this session was code implementation only, no embed/crawl/RAG operations.

---

## Risks and Rollback

- **System context preamble**: prepended to first message only (`session_id.is_none()`). Adds ~200 chars to first turn. No risk to subsequent turns.
- **`parse_editor_blocks()`** is purely additive — if no `<axon:editor>` tags present, returns empty vec, zero side effects.
- **Rollback**: `git revert a4ceffd7` undoes all 7 files cleanly. No DB migrations, no schema changes.

---

## Decisions Not Taken

- **MCP tool approach**: User explicitly rejected — XML tags are sufficient and simpler.
- **Per-delta parsing**: Would fire incomplete blocks mid-stream. Post-turn emission on full `assistant_text` is correct.
- **`system` field on `NewSessionRequest`**: Does not exist in the ACP protocol; cannot inject system prompt any other way than first-message prepend.
- **Always show editor pane**: Instead auto-opens only on `editor_update` — avoids surprise panel pop on sessions that never use the editor.

---

## Open Questions

- Whether Claude/Codex/Gemini reliably follows the `<axon:editor>` syntax guide in the first-message preamble — needs runtime testing with each agent.
- Whether `op="append"` should insert a newline separator (`\n\n`) between existing content and appended content (currently `\n`).

---

## Next Steps

- Runtime test with Claude agent: ask it to "write a hello world markdown document to the editor" and verify `EditorWriteCard` appears + editor updates.
- Consider adding a test in `apps/web/__tests__/` covering `parseEditorBlocks` (pure function — easy to unit test).
- If system context preamble proves insufficient for Codex/Gemini, explore per-turn injection or a dedicated `axon:editor` MCP tool as fallback.
