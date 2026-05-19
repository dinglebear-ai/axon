# /reboot Live Sessions Design
Date: 2026-03-08

Replace all mocked session data in `/reboot` with fully operational ACP sessions
backed by `~/.claude/projects/**/*.jsonl`, enriched with git metadata, and wired
to the real ACP WebSocket bridge.

---

## Scope

Six sections of work, all building toward a single goal: the `/reboot` page shows
real Claude CLI sessions, restores full chat history on select, and sends real
prompts through the ACP WebSocket — no mocks remaining.

---

## Section 1 — Backend: Git metadata enrichment

### New file: `apps/web/lib/sessions/git-metadata.ts`

Standalone helper with a single export: `enrichWithGit(projectPath: string)`.

**Responsibilities:**
1. Decode `~/.claude/projects/` folder name back to a real filesystem path
   (reverse the hyphen-encoding already performed by `session-scanner.ts`)
2. Walk up the directory tree from that path looking for `.git/`
3. Run two git commands via `child_process.execFile`:
   - `git rev-parse --abbrev-ref HEAD` → branch name
   - `git remote get-url origin` → remote URL
4. Parse remote URL — handle both HTTPS (`https://github.com/owner/repo.git`)
   and SSH (`git@github.com:owner/repo.git`) formats; extract `owner/repo`
5. Cache results in a module-level `Map<string, GitMeta>` keyed by project folder.
   Same project scanned N times pays the git cost once per process lifetime.
6. Graceful degradation: not a git repo, git not installed, command times out,
   any error → return `{}` (no `repo`, no `branch`), never throw.

**Types added to `SessionSummary`:**
```typescript
repo?:   string   // e.g. "jmagar/axon_rust"
branch?: string   // e.g. "feat/live-sessions" (max 40 chars, trail-off)
```

**Integration point:** `session-scanner.ts` calls `enrichWithGit()` per project
folder during `scanSessions()`, merging the result into each `SessionFile`.

**Lossy cleanup (Zed pattern):** If a decoded project path no longer exists on
disk, skip enrichment and omit `repo`/`branch` rather than erroring.

---

## Section 2 — Backend (Rust): `SessionFallback` event

When `load_session()` fails and falls back to `new_session()`, the frontend
currently receives no signal. We add an explicit event.

### `crates/services/types.rs`

New variant on `AcpBridgeEvent`:
```rust
SessionFallback {
    old_session_id: String,
    new_session_id: String,
},
```

Serializes to:
```json
{ "type": "session_fallback", "old_session_id": "...", "new_session_id": "..." }
```

### `crates/services/acp.rs`

Emit the event at both fallback sites (≈line 930, ≈line 1304) immediately after
`new_session()` succeeds — before any `SessionUpdate` events arrive from the agent:

```rust
emit(&tx, ServiceEvent::AcpBridge {
    event: AcpBridgeEvent::SessionFallback {
        old_session_id: requested_session_id.0.to_string(),
        new_session_id: response.session_id.0.to_string(),
    },
});
```

No changes required to `crates/web.rs` or the execute layer — `AcpBridgeEvent`
variants are forwarded generically via `acp_bridge_event_payload()`.

### TypeScript stream pipeline

`session_fallback` must be threaded through:
- `apps/web/app/api/pulse/chat/stream-parser.ts` — update `parserState.sessionId`
  from `event.new_session_id` when `event.type === 'session_fallback'`
- `apps/web/hooks/pulse-chat-helpers.ts` — call `setChatSessionId(newSessionId)`
  immediately on `session_fallback` (fixes the interrupted-stream gap)
- Optional toast: "Couldn't resume previous session — started fresh"

---

## Section 3 — `/reboot` sidebar: real session list

### Session card mapping

| Card field | Source |
|---|---|
| title | `preview` truncated to 60 chars (first user message from JSONL) |
| repo | `repo` from git enrichment; fallback: `project` folder name |
| branch | `branch` from git enrichment; omit metadata row if absent |
| agent | `"Claude"` (hardcoded — source is `~/.claude/projects`) |
| lastMessageAt | `mtimeMs` formatted as relative time (e.g. "2h ago") |
| hasUnread | **removed** — no equivalent concept in real sessions |

### Sidebar wiring

- Replace `SESSION_ITEMS` import with `useRecentSessions()` call at the
  `RebootShell` level; pass `sessions` down to `RebootSidebar` as a prop
- `reboot-mock-data.ts`: gut `SESSION_ITEMS`, `INITIAL_MESSAGES`, and
  `EDITOR_FILES` mocks; keep the file but empty it (or delete if nothing remains)
- Search filters across: `preview`, `project`, `repo`, `branch`
- Sort: `mtimeMs DESC` by default (newest first)
- Empty-query optimization (Zed pattern): skip fuzzy scoring when search is blank,
  return all in timestamp order

### `"+"` button

Sets `activeSessionId` to `null`, clears chat pane, shows empty "Agent is ready"
state. Session creation is lazy — no ACP call until first prompt is submitted.

---

## Section 4 — Chat history restoration

### New hook: `useAxonSession(sessionId: string | null)`

```typescript
// Returns:
{
  messages: MessageItem[]
  loading: boolean
  error: string | null
  reload: () => void
}
```

**On `sessionId` change:**
1. Set `loading = true`, clear previous messages
2. If `sessionId === null` → return `[]` immediately (new session)
3. `GET /api/sessions/${sessionId}` → `ParsedMessage[]`
4. Convert `ParsedMessage[]` → `MessageItem[]`:
   - Generate stable `id` from index
   - Set `timestamp` from session `mtimeMs` (no per-message timestamps in JSONL)
   - `chainOfThought` and `files` empty (only live turns produce these)
5. Set `messages`, clear `loading`
6. On error: set `error`, render retry button in chat pane

**Replay semantics (Zed pattern):** Re-fetch from JSONL on every session select.
No caching — ensures the view always reflects what Claude CLI has written to disk,
including turns added by the agent since the last sidebar scan.

**Empty session:** Zero messages → render existing empty state unchanged.

---

## Section 5 — Real prompt submission via ACP WebSocket

### New hook: `useAxonAcp(activeSessionId, onSessionIdChange)`

Owns the WebSocket connection and all ACP event handling for `/reboot`.

**Connection:** Reuses `useAxonWs` (same endpoint as `/pulse`, `/ws?token=...`).
One persistent connection per page load, shared across session switches.

**Submit flow:**
```
user submits prompt
  → optimistically append user MessageItem to local message list
  → send WS: { type: "execute", mode: "pulse_chat", input,
               flags: { session_id: activeSessionId ?? undefined, agent: "claude" } }
  → stream incoming events:
      assistant_delta    → append/grow streaming assistant MessageItem
      thinking_content   → populate chainOfThought on current MessageItem
      tool_use           → append tool call entry to current MessageItem
      session_fallback   → call onSessionIdChange(newId) + show toast
      result             → finalize assistant message, mark turn complete,
                           reload sidebar (sessions list reflects new mtimeMs)
      permission_request → surface permission UI in RebootPromptComposer
      error              → show inline error, remove optimistic user message
```

**Streaming render:** `assistant_delta` events grow the current assistant
`MessageItem` in place — no re-mounts, state updates only.

**New session ID capture:** When `activeSessionId` is `null` (new session), the
`result` event carries the newly assigned session ID. Call `onSessionIdChange(id)`
to set `activeSessionId` in shell state and trigger a sidebar reload.

**After each completed turn:** Call `useRecentSessions().reload()` to pick up the
updated `mtimeMs` and preview from the JSONL file Claude CLI wrote to disk.

---

## Section 6 — New session flow

```
click "+"
  → activeSessionId = null
  → clear current message list
  → show empty "Agent is ready" state
  → clear railQuery
```

First submitted prompt omits `session_id` from WS flags → ACP calls `new_session()`
→ `result` event returns new UUID → `onSessionIdChange(uuid)` → sidebar reloads →
new card appears at top sorted by `mtimeMs DESC`.

Session title in sidebar comes from the first user message preview extracted by
`session-scanner.ts` on the next scan (triggered by sidebar reload).

---

## Architecture: two new hooks

`reboot-shell.tsx` stays clean by delegating to two focused hooks:

```
RebootShell
├── useRecentSessions()           — session list from /api/sessions/list
├── useAxonSession(activeId)      — JSONL history fetch for selected session
└── useAxonAcp(activeId, onIdChange)  — WebSocket + prompt submission + event stream
```

---

## Files touched

| File | Change |
|---|---|
| `apps/web/lib/sessions/git-metadata.ts` | **new** — git enrichment helper |
| `apps/web/lib/sessions/session-scanner.ts` | call `enrichWithGit()`, add `repo`/`branch` to types |
| `apps/web/app/api/sessions/list/route.ts` | types updated (repo, branch passthrough) |
| `crates/services/types.rs` | add `SessionFallback` variant + serialize |
| `crates/services/acp.rs` | emit `SessionFallback` at two fallback sites |
| `apps/web/app/api/pulse/chat/stream-parser.ts` | handle `session_fallback` event |
| `apps/web/hooks/pulse-chat-helpers.ts` | `setChatSessionId` on `session_fallback` |
| `apps/web/hooks/use-axon-session.ts` | **new** — JSONL fetch + ParsedMessage→MessageItem |
| `apps/web/hooks/use-axon-acp.ts` | **new** — WebSocket + ACP event stream |
| `apps/web/components/reboot/reboot-shell.tsx` | wire all three hooks, remove mock handlers |
| `apps/web/components/reboot/reboot-sidebar.tsx` | accept real sessions prop, remove mock import |
| `apps/web/components/reboot/reboot-mock-data.ts` | gut SESSION_ITEMS, INITIAL_MESSAGES mocks |
| `apps/web/components/reboot/reboot-prompt-composer.tsx` | wire submit to useAxonAcp |

---

## Non-goals

- Codex session support (`~/.codex/sessions`) — separate effort
- Backend session persistence (Postgres) — localStorage in `/pulse` is out of scope here
- Renaming `reboot-*` components to `axon-*` — naming cleanup is a separate pass
- Editor pane wiring — mock files remain for now; editor is a separate feature
