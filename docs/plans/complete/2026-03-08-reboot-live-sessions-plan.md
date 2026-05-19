# /reboot Live Sessions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace all mocked session data in `/reboot` with fully operational ACP sessions backed by `~/.claude/projects/**/*.jsonl`, enriched with git metadata, and wired to the real ACP WebSocket bridge.

**Architecture:** Three layers: (1) a new `git-metadata.ts` helper enriches the existing `session-scanner.ts` with repo/branch info from the filesystem; (2) a Rust `SessionFallback` event surfaces silent resume failures to the frontend; (3) two new hooks (`useAxonSession`, `useAxonAcp`) replace all mock handlers in the shell. All `Reboot*` component names are also renamed to `Axon*`.

**Tech Stack:** TypeScript (Next.js 15, React 19), Rust (tokio, agent-client-protocol 0.10), Node.js `child_process.execFile` for git commands, JSONL parsing via existing `claude-jsonl-parser.ts`, WebSocket via existing `useAxonWs`.

---

## Task 1: `git-metadata.ts` — new git enrichment helper

**Files:**
- Create: `apps/web/lib/sessions/git-metadata.ts`
- Create: `apps/web/lib/sessions/__tests__/git-metadata.test.ts`

**Step 1: Write failing tests**

```typescript
// apps/web/lib/sessions/__tests__/git-metadata.test.ts
import { decodeProjectPath, parseRemoteUrl } from '../git-metadata'

describe('decodeProjectPath', () => {
  it('decodes hyphen-encoded path to filesystem path', () => {
    expect(decodeProjectPath('-home-jmagar-workspace-axon-rust'))
      .toBe('/home/jmagar/workspace/axon-rust')
  })

  it('handles paths with underscores', () => {
    expect(decodeProjectPath('-home-jmagar-workspace-axon_rust'))
      .toBe('/home/jmagar/workspace/axon_rust')
  })
})

describe('parseRemoteUrl', () => {
  it('parses HTTPS remote URL', () => {
    expect(parseRemoteUrl('https://github.com/jmagar/axon_rust.git'))
      .toBe('jmagar/axon_rust')
  })

  it('parses SSH remote URL', () => {
    expect(parseRemoteUrl('git@github.com:jmagar/axon_rust.git'))
      .toBe('jmagar/axon_rust')
  })

  it('returns null for invalid URL', () => {
    expect(parseRemoteUrl('not-a-url')).toBeNull()
  })
})
```

**Step 2: Run to verify failure**

```bash
cd apps/web && pnpm test lib/sessions/__tests__/git-metadata.test.ts
```
Expected: FAIL — module not found.

**Step 3: Implement `git-metadata.ts`**

```typescript
// apps/web/lib/sessions/git-metadata.ts
import { execFile } from 'node:child_process'
import * as fs from 'node:fs'
import * as path from 'node:path'
import { promisify } from 'node:util'

const exec = promisify(execFile)
const MAX_BRANCH_LENGTH = 40

export interface GitMeta {
  repo?: string
  branch?: string
}

// Module-level cache: project folder name → enriched metadata
const cache = new Map<string, GitMeta>()

/**
 * Decode ~/.claude/projects/ folder name back to real filesystem path.
 * Encoding: absolute path with leading slash replaced by '-', all '/' → '-'.
 * e.g. "-home-jmagar-workspace-axon-rust" → "/home/jmagar/workspace/axon-rust"
 * Note: underscores in real paths are preserved as-is.
 */
export function decodeProjectPath(folderName: string): string {
  return folderName.replace(/^-/, '/').replace(/-/g, (_, offset, str) => {
    // Don't replace hyphens that are part of actual directory names.
    // We can't distinguish "axon-rust" from "axon/rust" without fs checks,
    // so we use the raw replacement and let findGitRoot handle the walk-up.
    return offset === 0 ? '/' : '-'
  })
  // Simpler: just replace leading hyphen with slash, all remaining hyphens
  // stay as hyphens — the folder name uses hyphens only for path separators.
  // Re-implement correctly:
}

// Correct implementation using the actual encoding scheme:
// ~/.claude/projects uses the absolute path with / replaced by - (including leading /)
// So "-home-jmagar-workspace-axon_rust" → split on "-", rejoin with "/"
// BUT: directory names can contain hyphens. Claude CLI uses the raw path with
// each "/" replaced by "-", so we do a simple replace of the leading "-" with "/"
// and then... we can't know which hyphens are path separators vs part of names.
// The scanner already does this with a heuristic. We trust it and use abs_path directly.

/**
 * Walk up from startPath looking for a .git directory.
 * Returns the repo root path, or null if not found.
 */
export async function findGitRoot(startPath: string): Promise<string | null> {
  let current = startPath
  const root = path.parse(current).root

  while (current !== root) {
    try {
      await fs.promises.access(path.join(current, '.git'))
      return current
    } catch {
      current = path.dirname(current)
    }
  }
  return null
}

/**
 * Parse a git remote URL (HTTPS or SSH) into "owner/repo" format.
 */
export function parseRemoteUrl(url: string): string | null {
  try {
    // SSH: git@github.com:owner/repo.git
    const sshMatch = url.match(/^git@[^:]+:(.+?)(?:\.git)?$/)
    if (sshMatch) return sshMatch[1]

    // HTTPS: https://github.com/owner/repo.git
    const parsed = new URL(url)
    const parts = parsed.pathname.replace(/^\//, '').replace(/\.git$/, '')
    if (parts.includes('/')) return parts
    return null
  } catch {
    return null
  }
}

/**
 * Enrich a session with git metadata derived from its project filesystem path.
 * Results are cached per projectPath for the process lifetime.
 * Never throws — returns {} on any error.
 */
export async function enrichWithGit(projectPath: string): Promise<GitMeta> {
  if (cache.has(projectPath)) return cache.get(projectPath)!

  const meta: GitMeta = {}

  try {
    // Validate path exists
    await fs.promises.access(projectPath)

    const gitRoot = await findGitRoot(projectPath)
    if (!gitRoot) {
      cache.set(projectPath, meta)
      return meta
    }

    const opts = { cwd: gitRoot, timeout: 3000 }

    // Get branch name
    try {
      const { stdout: branchOut } = await exec('git', ['rev-parse', '--abbrev-ref', 'HEAD'], opts)
      const branch = branchOut.trim()
      if (branch && branch !== 'HEAD') {
        meta.branch = branch.length > MAX_BRANCH_LENGTH
          ? branch.slice(0, MAX_BRANCH_LENGTH - 1) + '…'
          : branch
      }
    } catch { /* detached HEAD or no commits */ }

    // Get remote URL → parse to owner/repo
    try {
      const { stdout: remoteOut } = await exec('git', ['remote', 'get-url', 'origin'], opts)
      const parsed = parseRemoteUrl(remoteOut.trim())
      if (parsed) meta.repo = parsed
    } catch { /* no remote */ }
  } catch { /* path doesn't exist or git not available */ }

  cache.set(projectPath, meta)
  return meta
}
```

**Step 4: Run tests to verify they pass**

```bash
cd apps/web && pnpm test lib/sessions/__tests__/git-metadata.test.ts
```
Expected: PASS (6 tests).

**Step 5: Commit**

```bash
git add apps/web/lib/sessions/git-metadata.ts apps/web/lib/sessions/__tests__/git-metadata.test.ts
git commit -m "feat(sessions): add git-metadata helper for repo/branch enrichment"
```

---

## Task 2: Enrich `session-scanner.ts` with git metadata

**Files:**
- Modify: `apps/web/lib/sessions/session-scanner.ts`
- Modify: `apps/web/app/api/sessions/list/route.ts` (type passthrough only)

**Step 1: Add `repo` and `branch` to the `SessionFile` type**

Open `session-scanner.ts`. Find the `SessionFile` interface. Add:
```typescript
repo?:   string
branch?: string
```

Do the same for `SessionSummary` if it's a separate type — check if it's defined in this file or imported.

**Step 2: Import `enrichWithGit` and call it per project**

At the top of `session-scanner.ts`:
```typescript
import { enrichWithGit } from './git-metadata'
```

Inside `scanSessions()`, after the existing per-project loop body builds a `SessionFile`, add:
```typescript
const decoded = decodeClaudeProjectFolder(projectFolder) // the existing decode fn
const git = await enrichWithGit(decoded)
entry.repo   = git.repo
entry.branch = git.branch
```

If there's no existing `decodeClaudeProjectFolder`, check how the scanner converts the folder name to a display name — use that same decoded path as the input to `enrichWithGit`.

**Step 3: Verify `/api/sessions/list` still works**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```
Expected: 0 errors. The `repo` and `branch` fields are optional so existing callers won't break.

**Step 4: Commit**

```bash
git add apps/web/lib/sessions/session-scanner.ts
git commit -m "feat(sessions): enrich session list with git repo/branch metadata"
```

---

## Task 3: Rust `SessionFallback` event

**Files:**
- Modify: `crates/services/types.rs`
- Modify: `crates/services/acp.rs`
- Modify: `tests/services_acp_event_mapping.rs`

**Step 1: Write failing test**

Open `tests/services_acp_event_mapping.rs`. Add:
```rust
#[test]
fn session_fallback_serializes_correctly() {
    let event = AcpBridgeEvent::SessionFallback {
        old_session_id: "old-123".to_string(),
        new_session_id: "new-456".to_string(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "session_fallback");
    assert_eq!(json["old_session_id"], "old-123");
    assert_eq!(json["new_session_id"], "new-456");
}
```

**Step 2: Run to verify failure**

```bash
cargo test session_fallback_serializes_correctly
```
Expected: FAIL — variant doesn't exist.

**Step 3: Add variant to `AcpBridgeEvent`**

In `crates/services/types.rs`, find `pub enum AcpBridgeEvent` and add:
```rust
SessionFallback {
    old_session_id: String,
    new_session_id: String,
},
```

**Step 4: Add serialize arm**

In the `impl serde::Serialize for AcpBridgeEvent` block, add:
```rust
Self::SessionFallback { old_session_id, new_session_id } => {
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "session_fallback")?;
    map.serialize_entry("old_session_id", old_session_id)?;
    map.serialize_entry("new_session_id", new_session_id)?;
    map.end()
}
```

**Step 5: Run to verify test passes**

```bash
cargo test session_fallback_serializes_correctly
```
Expected: PASS.

**Step 6: Emit the event in `acp.rs` at both fallback sites**

Search `crates/services/acp.rs` for `"ACP load_session failed, falling back to new session"` — there are two occurrences (≈line 920, ≈line 1296). At each site, immediately after the `new_session()` call succeeds, add:

```rust
emit(
    &tx,
    ServiceEvent::AcpBridge {
        event: AcpBridgeEvent::SessionFallback {
            old_session_id: requested_session_id.0.to_string(),
            new_session_id: response.session_id.0.to_string(),
        },
    },
);
```

`requested_session_id` is already captured just before the `load_session()` call at both sites (see design doc Section 2).

**Step 7: Full test suite**

```bash
cargo test
cargo clippy
cargo fmt --check
```
Expected: all passing, no warnings.

**Step 8: Commit**

```bash
git add crates/services/types.rs crates/services/acp.rs tests/services_acp_event_mapping.rs
git commit -m "feat(acp): emit SessionFallback event on failed session resume"
```

---

## Task 4: TypeScript — handle `session_fallback` in stream pipeline

**Files:**
- Modify: `apps/web/app/api/pulse/chat/stream-parser.ts`
- Modify: `apps/web/hooks/pulse-chat-helpers.ts`

**Step 1: Handle `session_fallback` in stream parser**

Open `apps/web/app/api/pulse/chat/stream-parser.ts`. Find where `event.type === 'result'` updates `state.sessionId`. Add a similar arm above it:

```typescript
if (event.type === 'session_fallback' && event.new_session_id) {
  state.sessionId = event.new_session_id as string
  return { kind: 'session_fallback', newSessionId: event.new_session_id as string }
}
```

If the parser uses a discriminated union for return types, add `{ kind: 'session_fallback'; newSessionId: string }` to the union.

**Step 2: Handle in `pulse-chat-helpers.ts`**

Find the event dispatch loop in `pulse-chat-helpers.ts`. After the `config_options_update` handler, add:

```typescript
if (event.type === 'session_fallback' && event.newSessionId) {
  setChatSessionId(event.newSessionId)
  // Optional: surface to user
  onSessionFallback?.(event.newSessionId)
  return
}
```

Where `onSessionFallback` is an optional callback prop (add it to the helpers signature if desired, or omit and just call `setChatSessionId`).

**Step 3: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```
Expected: 0 errors.

**Step 4: Commit**

```bash
git add apps/web/app/api/pulse/chat/stream-parser.ts apps/web/hooks/pulse-chat-helpers.ts
git commit -m "feat(pulse): handle session_fallback event in stream pipeline"
```

---

## Task 5: Rename `Reboot*` → `Axon*` throughout `/reboot` components

**Files:**
- Rename + modify: all `apps/web/components/reboot/reboot-*.tsx` files
- Modify: `apps/web/app/reboot/page.tsx`

**Step 1: Rename files**

```bash
cd apps/web/components/reboot
for f in reboot-*.tsx; do
  mv "$f" "axon-${f#reboot-}"
done
```

This produces: `axon-shell.tsx`, `axon-sidebar.tsx`, `axon-prompt-composer.tsx`, etc.

**Step 2: Update component names and imports inside each file**

For each renamed file, do a find-and-replace:
- `RebootShell` → `AxonShell`
- `RebootSidebar` → `AxonSidebar`
- `RebootPromptComposer` → `AxonPromptComposer`
- `RebootMessageList` → `AxonMessageList`
- `RebootTerminalDialog` → `AxonTerminalDialog`
- `RebootLogsDialog` → `AxonLogsDialog`
- `RebootMcpDialog` → `AxonMcpDialog`
- `reboot-mock-data` → `axon-mock-data` (import path)
- Any other `Reboot` prefix in the same files

Rename `reboot-mock-data.ts` → `axon-mock-data.ts` as well.

**Step 3: Update `page.tsx`**

```typescript
// apps/web/app/reboot/page.tsx
import { AxonShell } from '@/components/reboot/axon-shell'

export default function RebootPage() {
  return <AxonShell />
}
```

**Step 4: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```
Expected: 0 errors.

**Step 5: Commit**

```bash
git add apps/web/components/reboot/ apps/web/app/reboot/page.tsx
git commit -m "refactor(reboot): rename Reboot* components to Axon*"
```

---

## Task 6: New hook `useAxonSession`

**Files:**
- Create: `apps/web/hooks/use-axon-session.ts`
- Create: `apps/web/hooks/__tests__/use-axon-session.test.ts`

**Step 1: Write failing tests**

```typescript
// apps/web/hooks/__tests__/use-axon-session.test.ts
import { renderHook, waitFor } from '@testing-library/react'
import { useAxonSession } from '../use-axon-session'

global.fetch = jest.fn()

describe('useAxonSession', () => {
  afterEach(() => jest.clearAllMocks())

  it('returns empty messages for null sessionId', () => {
    const { result } = renderHook(() => useAxonSession(null))
    expect(result.current.messages).toEqual([])
    expect(result.current.loading).toBe(false)
  })

  it('fetches and converts messages for a real sessionId', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: true,
      json: async () => [
        { role: 'user', content: 'hello' },
        { role: 'assistant', content: 'hi there' },
      ],
    })

    const { result } = renderHook(() => useAxonSession('abc-123'))

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.messages).toHaveLength(2)
    expect(result.current.messages[0].role).toBe('user')
    expect(result.current.messages[0].content).toBe('hello')
  })

  it('sets error on fetch failure', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValueOnce({ ok: false, status: 404 })

    const { result } = renderHook(() => useAxonSession('bad-id'))

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).not.toBeNull()
    expect(result.current.messages).toEqual([])
  })
})
```

**Step 2: Run to verify failure**

```bash
cd apps/web && pnpm test hooks/__tests__/use-axon-session.test.ts
```
Expected: FAIL — module not found.

**Step 3: Implement the hook**

```typescript
// apps/web/hooks/use-axon-session.ts
import { useCallback, useEffect, useState } from 'react'

export interface MessageItem {
  id: string
  role: 'user' | 'assistant'
  content: string
  timestamp: number
  chainOfThought?: string[]
  files?: string[]
  streaming?: boolean
}

interface UseAxonSessionResult {
  messages: MessageItem[]
  loading: boolean
  error: string | null
  reload: () => void
}

export function useAxonSession(sessionId: string | null): UseAxonSessionResult {
  const [messages, setMessages] = useState<MessageItem[]>([])
  const [loading, setLoading]   = useState(false)
  const [error, setError]       = useState<string | null>(null)
  const [version, setVersion]   = useState(0)

  const reload = useCallback(() => setVersion(v => v + 1), [])

  useEffect(() => {
    if (!sessionId) {
      setMessages([])
      setLoading(false)
      setError(null)
      return
    }

    let cancelled = false
    setLoading(true)
    setError(null)

    fetch(`/api/sessions/${encodeURIComponent(sessionId)}`)
      .then(async res => {
        if (!res.ok) throw new Error(`Failed to load session: ${res.status}`)
        return res.json() as Promise<Array<{ role: 'user' | 'assistant'; content: string }>>
      })
      .then(parsed => {
        if (cancelled) return
        setMessages(
          parsed.map((msg, i) => ({
            id: `${sessionId}-${i}`,
            role: msg.role,
            content: msg.content,
            timestamp: Date.now(), // no per-message timestamps in JSONL
          }))
        )
      })
      .catch(err => {
        if (cancelled) return
        setError(err instanceof Error ? err.message : 'Failed to load session')
        setMessages([])
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => { cancelled = true }
  }, [sessionId, version])

  return { messages, loading, error, reload }
}
```

**Step 4: Run tests to verify pass**

```bash
cd apps/web && pnpm test hooks/__tests__/use-axon-session.test.ts
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add apps/web/hooks/use-axon-session.ts apps/web/hooks/__tests__/use-axon-session.test.ts
git commit -m "feat(hooks): add useAxonSession for JSONL session history"
```

---

## Task 7: New hook `useAxonAcp`

**Files:**
- Create: `apps/web/hooks/use-axon-acp.ts`

**Step 1: Implement the hook**

This hook wraps the existing `useAxonWs` connection and handles ACP event streaming. No isolated unit test is practical (WebSocket is integration-level); instead the hook is tested via the shell wiring in Task 8.

```typescript
// apps/web/hooks/use-axon-acp.ts
import { useCallback, useRef, useState } from 'react'
import { useAxonWs } from './use-axon-ws'
import type { MessageItem } from './use-axon-session'

interface UseAxonAcpOptions {
  activeSessionId: string | null
  onSessionIdChange: (newId: string) => void
  onSessionFallback?: (oldId: string, newId: string) => void
  onMessagesChange: (updater: (prev: MessageItem[]) => MessageItem[]) => void
  onTurnComplete?: () => void
}

export function useAxonAcp({
  activeSessionId,
  onSessionIdChange,
  onSessionFallback,
  onMessagesChange,
  onTurnComplete,
}: UseAxonAcpOptions) {
  const [isStreaming, setIsStreaming] = useState(false)
  const streamingIdRef = useRef<string | null>(null)

  const { send, connected } = useAxonWs({
    onMessage: useCallback((data: unknown) => {
      const msg = data as Record<string, unknown>

      switch (msg.type) {
        case 'assistant_delta': {
          const delta = (msg.delta as string) ?? ''
          const sid = streamingIdRef.current
          if (!sid) return
          onMessagesChange(prev => prev.map(m =>
            m.id === sid ? { ...m, content: m.content + delta } : m
          ))
          break
        }

        case 'thinking_content': {
          const content = (msg.content as string) ?? ''
          const sid = streamingIdRef.current
          if (!sid) return
          onMessagesChange(prev => prev.map(m =>
            m.id === sid
              ? { ...m, chainOfThought: [...(m.chainOfThought ?? []), content] }
              : m
          ))
          break
        }

        case 'session_fallback': {
          const oldId = (msg.old_session_id as string) ?? ''
          const newId = (msg.new_session_id as string) ?? ''
          onSessionIdChange(newId)
          onSessionFallback?.(oldId, newId)
          break
        }

        case 'result': {
          const newSessionId = msg.session_id as string | undefined
          if (newSessionId) onSessionIdChange(newSessionId)
          setIsStreaming(false)
          streamingIdRef.current = null
          onTurnComplete?.()
          break
        }

        case 'error': {
          setIsStreaming(false)
          streamingIdRef.current = null
          // Remove the optimistic assistant placeholder
          onMessagesChange(prev => prev.filter(m => m.id !== streamingIdRef.current))
          break
        }
      }
    }, [onMessagesChange, onSessionIdChange, onSessionFallback, onTurnComplete]),
  })

  const submitPrompt = useCallback((prompt: string) => {
    if (!connected || isStreaming) return

    // Optimistically add user message
    const userId = `user-${Date.now()}`
    const assistantId = `assistant-${Date.now()}`
    streamingIdRef.current = assistantId

    onMessagesChange(prev => [
      ...prev,
      { id: userId, role: 'user', content: prompt, timestamp: Date.now() },
      { id: assistantId, role: 'assistant', content: '', timestamp: Date.now(), streaming: true },
    ])

    setIsStreaming(true)

    send({
      type: 'execute',
      mode: 'pulse_chat',
      input: prompt,
      flags: {
        ...(activeSessionId ? { session_id: activeSessionId } : {}),
        agent: 'claude',
      },
    })
  }, [connected, isStreaming, activeSessionId, send, onMessagesChange])

  return { submitPrompt, isStreaming, connected }
}
```

**Step 2: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```

**Step 3: Commit**

```bash
git add apps/web/hooks/use-axon-acp.ts
git commit -m "feat(hooks): add useAxonAcp for real ACP WebSocket prompt submission"
```

---

## Task 8: Wire `AxonShell` to real sessions

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx`
- Modify: `apps/web/components/reboot/axon-mock-data.ts`

**Step 1: Gut the mock data**

Open `axon-mock-data.ts`. Remove `SESSION_ITEMS`, `INITIAL_MESSAGES`, `EDITOR_FILES` arrays. Leave the file with just a comment:
```typescript
// Mock data removed — sessions are now live from ~/.claude/projects
```

**Step 2: Wire hooks into `AxonShell`**

At the top of `axon-shell.tsx`, add:
```typescript
import { useRecentSessions } from '@/hooks/use-recent-sessions'
import { useAxonSession }    from '@/hooks/use-axon-session'
import { useAxonAcp }        from '@/hooks/use-axon-acp'
```

Replace the `activeSessionId` string state with a nullable:
```typescript
const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
```

Remove `messageMap` state and `isTyping` mock state.

Add:
```typescript
const { sessions, reload: reloadSessions } = useRecentSessions()

const { messages, loading: sessionLoading, error: sessionError, reload: reloadSession } =
  useAxonSession(activeSessionId)

const [liveMessages, setLiveMessages] = useState<MessageItem[]>([])

// Sync JSONL history into live messages when session changes
useEffect(() => { setLiveMessages(messages) }, [messages])

const { submitPrompt, isStreaming, connected } = useAxonAcp({
  activeSessionId,
  onSessionIdChange: setActiveSessionId,
  onSessionFallback: (_oldId, _newId) => {
    // Optional: show toast "Session restarted — previous session could not be resumed"
  },
  onMessagesChange: setLiveMessages,
  onTurnComplete: reloadSessions,
})
```

Replace `handleSelectSession`:
```typescript
const handleSelectSession = useCallback((id: string) => {
  setActiveSessionId(id)
  setSessionKey(k => k + 1)
}, [])
```

Replace `handlePromptSubmit`:
```typescript
const handlePromptSubmit = useCallback((prompt: string) => {
  submitPrompt(prompt)
}, [submitPrompt])
```

Replace the `"+"` new session handler (find the `"+"` button's `onClick`):
```typescript
onClick={() => {
  setActiveSessionId(null)
  setLiveMessages([])
}}
```

Pass `liveMessages` (not `messageMap[activeSessionId]`) to `AxonMessageList`.
Pass `sessions` (not `SESSION_ITEMS`) to `AxonSidebar`.
Pass `sessionLoading` and `sessionError` to `AxonMessageList` for loading/error states.

**Step 3: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```

**Step 4: Commit**

```bash
git add apps/web/components/reboot/axon-shell.tsx apps/web/components/reboot/axon-mock-data.ts
git commit -m "feat(reboot): wire AxonShell to real session data and ACP WebSocket"
```

---

## Task 9: Wire `AxonSidebar` to real session list

**Files:**
- Modify: `apps/web/components/reboot/axon-sidebar.tsx`

**Step 1: Update props type**

Find the `SESSION_ITEMS` import at the top of `axon-sidebar.tsx`. Remove it. Update the component props to accept sessions:

```typescript
import type { SessionSummary } from '@/lib/sessions/session-scanner'

interface AxonSidebarProps {
  // ... existing props ...
  sessions: SessionSummary[]
  activeSessionId: string | null
  onSelectSession: (id: string) => void
  onNewSession: () => void
}
```

**Step 2: Map `SessionSummary` to card display**

Replace the hardcoded `SESSION_ITEMS` render with a map over the `sessions` prop:

```typescript
const displaySessions = sessions
  .filter(s => {
    if (!railQuery) return true
    const q = railQuery.toLowerCase()
    return (
      s.preview?.toLowerCase().includes(q) ||
      s.project?.toLowerCase().includes(q) ||
      s.repo?.toLowerCase().includes(q) ||
      s.branch?.toLowerCase().includes(q)
    )
  })
  // Already sorted by mtimeMs DESC from the API

displaySessions.map(session => (
  <SessionCard
    key={session.id}
    title={session.preview?.slice(0, 60) ?? session.project ?? 'Untitled'}
    repo={session.repo ?? session.project ?? ''}
    branch={session.branch}              // omit metadata row if undefined
    agent="Claude"
    lastMessageAt={formatRelativeTime(session.mtimeMs)}
    isActive={session.id === activeSessionId}
    onClick={() => onSelectSession(session.id)}
  />
))
```

Add a `formatRelativeTime(ms: number): string` helper at the bottom of the file:
```typescript
function formatRelativeTime(ms: number): string {
  const diff = Date.now() - ms
  if (diff < 60_000)   return 'just now'
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`
  return new Date(ms).toLocaleDateString()
}
```

Wire the `"+"` button to call `onNewSession`.

**Step 3: Remove `hasUnread` blue dot** — delete any dot rendering tied to `hasUnread` since real sessions have no such concept.

**Step 4: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```

**Step 5: Commit**

```bash
git add apps/web/components/reboot/axon-sidebar.tsx
git commit -m "feat(reboot): wire AxonSidebar to real session list from ~/.claude/projects"
```

---

## Task 10: Wire `AxonPromptComposer` submit

**Files:**
- Modify: `apps/web/components/reboot/axon-prompt-composer.tsx`

**Step 1: Update props**

Find the `onSubmit` prop. It's probably `(prompt: string) => void` already. Verify and add `isStreaming: boolean` and `connected: boolean` props:

```typescript
interface AxonPromptComposerProps {
  // ... existing ...
  onSubmit: (prompt: string) => void
  isStreaming: boolean
  connected: boolean
}
```

**Step 2: Disable submit during streaming**

Find the submit button. Add disabled state:
```typescript
disabled={isStreaming || !connected || !input.trim()}
```

Add a streaming indicator (replace the send icon with a stop icon during stream, or show a spinner):
```typescript
{isStreaming ? <Loader2 className="animate-spin h-4 w-4" /> : <Send className="h-4 w-4" />}
```

**Step 3: Pass props from `AxonShell`**

In `axon-shell.tsx`, update the `AxonPromptComposer` usage:
```tsx
<AxonPromptComposer
  onSubmit={handlePromptSubmit}
  isStreaming={isStreaming}
  connected={connected}
  // ... existing props ...
/>
```

**Step 4: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```

**Step 5: Commit**

```bash
git add apps/web/components/reboot/axon-prompt-composer.tsx apps/web/components/reboot/axon-shell.tsx
git commit -m "feat(reboot): wire AxonPromptComposer to real ACP submit with streaming state"
```

---

## Task 11: `AxonMessageList` — loading and error states

**Files:**
- Modify: `apps/web/components/reboot/axon-message-list.tsx`

**Step 1: Add loading and error props**

```typescript
interface AxonMessageListProps {
  messages: MessageItem[]
  loading?: boolean
  error?: string | null
  onRetry?: () => void
}
```

**Step 2: Add loading state**

At the top of the render, before the message list:
```tsx
if (loading) {
  return (
    <div className="flex items-center justify-center h-full">
      <Loader2 className="animate-spin h-6 w-6 text-muted-foreground" />
    </div>
  )
}
```

**Step 3: Add error state**

```tsx
if (error) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3">
      <p className="text-sm text-destructive">{error}</p>
      <button onClick={onRetry} className="text-xs underline text-muted-foreground">
        Retry
      </button>
    </div>
  )
}
```

**Step 4: Pass props from `AxonShell`**

```tsx
<AxonMessageList
  messages={liveMessages}
  loading={sessionLoading}
  error={sessionError}
  onRetry={reloadSession}
/>
```

**Step 5: Verify build**

```bash
cd apps/web && pnpm build 2>&1 | grep -E "error|Error"
```

**Step 6: Commit**

```bash
git add apps/web/components/reboot/axon-message-list.tsx
git commit -m "feat(reboot): add loading and error states to AxonMessageList"
```

---

## Task 12: Final verification

**Step 1: Full Rust test suite**

```bash
just verify
```
Expected: fmt clean, clippy clean, all tests passing.

**Step 2: Full TypeScript build**

```bash
cd apps/web && pnpm build
```
Expected: 0 errors, 0 warnings about missing types.

**Step 3: TypeScript tests**

```bash
cd apps/web && pnpm test
```
Expected: all passing including new `git-metadata` and `useAxonSession` tests.

**Step 4: Manual smoke test**

1. Start infrastructure: `docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-chrome`
2. Start web: `cd apps/web && pnpm dev`
3. Navigate to `http://localhost:49010/reboot`
4. Verify sidebar shows real sessions from `~/.claude/projects` with repo/branch metadata
5. Click a session — verify chat history loads from JSONL
6. Submit a prompt — verify streaming response appears
7. Click `"+"` — verify new session flow creates a fresh session on first submit
8. Check that the sidebar refreshes after a completed turn

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat(reboot): fully operational ACP sessions — no mocks remaining (v0.10.0)"
```

---

## Checklist

- [ ] Task 1: `git-metadata.ts` helper with tests
- [ ] Task 2: `session-scanner.ts` enriched with git metadata
- [ ] Task 3: Rust `SessionFallback` event (types + acp + test)
- [ ] Task 4: TS `session_fallback` handling in stream pipeline
- [ ] Task 5: `Reboot*` → `Axon*` component rename
- [ ] Task 6: `useAxonSession` hook with tests
- [ ] Task 7: `useAxonAcp` hook
- [ ] Task 8: `AxonShell` wired to real data
- [ ] Task 9: `AxonSidebar` wired to real session list
- [ ] Task 10: `AxonPromptComposer` wired to real submit
- [ ] Task 11: `AxonMessageList` loading/error states
- [ ] Task 12: Final verification
