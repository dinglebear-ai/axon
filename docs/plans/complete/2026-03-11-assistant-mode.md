# Assistant Mode Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an "Assistant" mode to the sidebar rail dropdown, alongside Sessions and Files, giving users a dedicated chat mode that runs with a fixed CWD (`AXON_DATA_DIR/axon/assistant`) and shows only assistant-context chat history.

**Architecture:** A new `RailMode` ('assistant') is added to the config and sidebar. The frontend sends an `assistant_mode: true` flag with each pulse_chat WS message; the Rust backend intercepts this flag, resolves the assistant CWD from `AXON_DATA_DIR`, creates the directory, and passes it to the ACP adapter subprocess. Session history for assistant mode is scanned from the corresponding `~/.claude/projects/` subdirectory via a dedicated API route.

**Tech Stack:** TypeScript (Next.js 16 App Router, React 19), Rust (axum, tokio), Zod v4

---

## File Map

### New files
| File | Responsibility |
|------|---------------|
| `apps/web/app/api/assistant/sessions/route.ts` | List assistant-mode sessions from the resolved Claude project dir |
| `apps/web/hooks/use-assistant-sessions.ts` | React hook: fetch/reload assistant session list |

### Modified files
| File | Change |
|------|--------|
| `apps/web/components/reboot/axon-ui-config.ts` | Add `'assistant'` to `RailMode` + `RAIL_MODES` |
| `apps/web/components/reboot/axon-sidebar.tsx` | Render assistant session list in `RailContent`; update search placeholder + subtitle |
| `apps/web/components/reboot/axon-shell.tsx` | Wire assistant sessions hook; pass `assistant_mode` flag via ACP; persist rail mode 'assistant' |
| `apps/web/hooks/use-axon-acp.ts` | Accept optional `assistantMode?: boolean`; include in WS flags |
| `crates/web/execute/constants.rs` | Add `"assistant_mode"` to `ALLOWED_FLAGS` |
| `crates/web/execute/sync_mode/types.rs` | Add `assistant_mode: bool` to `DirectParams` |
| `crates/web/execute/sync_mode/params.rs` | Extract `assistant_mode` from flags in `extract_params` |
| `crates/web/execute/sync_mode/dispatch.rs` | Pass `assistant_mode` from `DirectParams` to `handle_pulse_chat` |
| `crates/web/execute/sync_mode/pulse_chat.rs` | Use assistant CWD when `assistant_mode=true`; scope ACP conn key to include mode |

---

## Chunk 1: Rust Backend — `assistant_mode` flag + CWD override

### Task 1: Add `assistant_mode` to the ALLOWED_FLAGS list

**Files:**
- Modify: `crates/web/execute/constants.rs:34-70`

- [ ] **Step 1: Read the file**

  Verify current ALLOWED_FLAGS ends at `("session_id", "--session-id")`.

- [ ] **Step 2: Add the flag entry**

  ```rust
  // In ALLOWED_FLAGS, after ("session_id", "--session-id"):
  ("assistant_mode", "--assistant-mode"),
  ```

- [ ] **Step 3: Verify it compiles**

  Run: `cargo check 2>&1 | head -20`
  Expected: no errors

- [ ] **Step 4: Commit**

  ```bash
  git add crates/web/execute/constants.rs
  git commit -m "feat(web): add assistant_mode to ALLOWED_FLAGS"
  ```

---

### Task 2: Add `assistant_mode` to `DirectParams` and extract from flags

**Files:**
- Modify: `crates/web/execute/sync_mode/types.rs:62-72`
- Modify: `crates/web/execute/sync_mode/params.rs:55-88`
- Test: `crates/web/execute/sync_mode/params.rs` (inline tests)

- [ ] **Step 1: Write the failing test first (TDD red phase)**

  Add to the `#[cfg(test)]` block in `params.rs`:

  ```rust
  #[test]
  fn extract_params_reads_assistant_mode_flag() {
      let base = Config::default();
      let context = ExecCommandContext {
          exec_id: "test".to_string(),
          mode: "pulse_chat".to_string(),
          input: "hello".to_string(),
          flags: serde_json::Value::Null,
          cfg: Arc::new(base),
      };
      let flags = serde_json::json!({"assistant_mode": true});
      let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
      assert!(params.assistant_mode);
  }

  #[test]
  fn extract_params_assistant_mode_defaults_false() {
      let base = Config::default();
      let context = ExecCommandContext {
          exec_id: "test".to_string(),
          mode: "pulse_chat".to_string(),
          input: "hello".to_string(),
          flags: serde_json::Value::Null,
          cfg: Arc::new(base),
      };
      let flags = serde_json::json!({});
      let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
      assert!(!params.assistant_mode);
  }
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  Run: `cargo test extract_params_reads_assistant_mode 2>&1 | tail -20`
  Expected: compile error — `assistant_mode` field doesn't exist yet

- [ ] **Step 3: Add `assistant_mode` to `DirectParams`**

  In `types.rs`, add to the `DirectParams` struct after `model`:

  ```rust
  pub(super) assistant_mode: bool,
  ```

- [ ] **Step 4: Extract `assistant_mode` in `extract_params`**

  In `params.rs`, inside `extract_params`, after the `model` extraction:

  ```rust
  let assistant_mode = flags
      .get("assistant_mode")
      .and_then(serde_json::Value::as_bool)
      .unwrap_or(false);
  ```

  And add to the `DirectParams { .. }` literal:

  ```rust
  assistant_mode,
  ```

- [ ] **Step 5: Fix the destructuring in `dispatch.rs`**

  In `dispatch_service`, the `let DirectParams { ... } = params;` destructuring must add:

  ```rust
  assistant_mode,
  ```

- [ ] **Step 6: Run tests to confirm they pass**

  Run: `cargo test extract_params 2>&1 | tail -30`
  Expected: all `extract_params_*` tests pass

- [ ] **Step 7: Commit**

  ```bash
  git add crates/web/execute/sync_mode/types.rs \
          crates/web/execute/sync_mode/params.rs \
          crates/web/execute/sync_mode/dispatch.rs
  git commit -m "feat(web): add assistant_mode to DirectParams and extract from flags"
  ```

---

### Task 3: Use assistant CWD in `pulse_chat.rs`

**Files:**
- Modify: `crates/web/execute/sync_mode/pulse_chat.rs`
- Modify: `crates/web/execute/sync_mode/dispatch.rs` (pass assistant_mode to handle_pulse_chat)

- [ ] **Step 1: Update `handle_pulse_chat` signature**

  Add `assistant_mode: bool` parameter after `agent`:

  ```rust
  pub(super) async fn handle_pulse_chat(
      cfg: Arc<Config>,
      input: String,
      session_id: Option<String>,
      model: Option<String>,
      agent: PulseChatAgent,
      assistant_mode: bool,          // NEW
      tx: mpsc::Sender<String>,
      ws_ctx: CommandContext,
      permission_responders: acp_svc::PermissionResponderMap,
      acp_connection: AcpConn,
  ) -> Result<(), String> {
  ```

- [ ] **Step 2: Pass `assistant_mode` to `get_or_create_acp_connection`**

  Update the call site in `handle_pulse_chat`:

  ```rust
  let conn_handle =
      get_or_create_acp_connection(
          &acp_connection,
          &req,
          agent,
          assistant_mode,        // NEW
          &cfg,
          &permission_responders,
      )
      .await?;
  ```

- [ ] **Step 3: Update `get_or_create_acp_connection` to accept and use `assistant_mode`**

  Add `assistant_mode: bool` to the function signature.

  Update the `agent_key` to include the mode so assistant sessions get their own adapter:

  ```rust
  let agent_key = if assistant_mode {
      format!("{agent:?}:assistant")
  } else {
      format!("{agent:?}")
  };
  ```

  Replace the existing `let cwd = env::current_dir()...` call:

  ```rust
  let cwd = if assistant_mode {
      // Resolve AXON_DATA_DIR env var; default to ~/.local/share/axon
      let base = std::env::var("AXON_DATA_DIR")
          .unwrap_or_else(|_| {
              let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
              format!("{home}/.local/share/axon")
          });
      let assistant_path = std::path::PathBuf::from(base).join("axon").join("assistant");
      tokio::fs::create_dir_all(&assistant_path)
          .await
          .map_err(|e| format!("failed to create assistant dir: {e}"))?;
      assistant_path
  } else {
      env::current_dir().map_err(|e| e.to_string())?
  };
  ```

  > **Note:** `AXON_DATA_DIR` already contains `axon/` prefix in the compose setup
  > (e.g. `AXON_DATA_DIR=/home/jmagar/appdata` → `axon/assistant` is added here).
  > The full path becomes `$AXON_DATA_DIR/axon/assistant`.

- [ ] **Step 4: Pass `assistant_mode` from dispatch.rs**

  In `dispatch.rs`, inside the `ServiceMode::PulseChat` arm, add `assistant_mode` to the `handle_pulse_chat` call:

  ```rust
  ServiceMode::PulseChat => {
      handle_pulse_chat(
          cfg,
          input,
          session_id,
          model,
          agent,
          assistant_mode,    // NEW
          tx,
          ws_ctx,
          permission_responders,
          acp_connection,
      )
      .await?;
  }
  ```

- [ ] **Step 5: Compile check**

  Run: `cargo check 2>&1 | head -30`
  Expected: clean

- [ ] **Step 6: Run all web tests**

  Run: `cargo test web 2>&1 | tail -30`
  Expected: all passing

- [ ] **Step 7: Commit**

  ```bash
  git add crates/web/execute/sync_mode/pulse_chat.rs \
          crates/web/execute/sync_mode/dispatch.rs
  git commit -m "feat(web): use assistant CWD when assistant_mode=true"
  ```

---

## Chunk 2: Backend — Assistant Sessions API Route

### Task 4: Create `/api/assistant/sessions` endpoint

**Files:**
- Create: `apps/web/app/api/assistant/sessions/route.ts`

The scanner for assistant sessions works by:
1. Reading `AXON_DATA_DIR` (server env var, not exposed to browser)
2. Computing the Claude project dir name via path encoding
3. Scanning JSONL files in that specific project dir (reusing `scanSessions` logic with a project filter)

- [ ] **Step 1: Write the route**

  Create `apps/web/app/api/assistant/sessions/route.ts`:

  ```typescript
  import fs from 'node:fs/promises'
  import os from 'node:os'
  import path from 'node:path'
  import { NextResponse } from 'next/server'
  import { scanAssistantSessions } from '@/lib/sessions/assistant-scanner'

  export async function GET() {
    const sessions = await scanAssistantSessions().catch(() => [])
    return NextResponse.json(sessions)
  }
  ```

- [ ] **Step 2: Create `apps/web/lib/sessions/assistant-scanner.ts`**

  ```typescript
  import fs from 'node:fs/promises'
  import os from 'node:os'
  import path from 'node:path'
  import type { SessionFile } from './session-scanner'
  import { cleanProjectName, mapWithConcurrency, sessionId } from './session-utils'

  /**
   * Encode an absolute path to the Claude projects directory name format.
   * Replaces each '/' with '-', producing the directory name Claude CLI uses.
   * e.g. '/home/jmagar/appdata/axon/assistant' → '-home-jmagar-appdata-axon-assistant'
   */
  function encodePathToProjectName(absPath: string): string {
    return absPath.replace(/\//g, '-')
  }

  /**
   * Resolve the assistant CWD from AXON_DATA_DIR env var.
   * Falls back to ~/.local/share/axon/axon/assistant when AXON_DATA_DIR is unset.
   */
  function resolveAssistantCwd(): string {
    const dataDir = process.env.AXON_DATA_DIR
    if (dataDir) {
      return path.join(dataDir, 'axon', 'assistant')
    }
    return path.join(os.homedir(), '.local', 'share', 'axon', 'axon', 'assistant')
  }

  async function extractPreviewLine(filePath: string): Promise<string | undefined> {
    try {
      const fd = await fs.open(filePath, 'r')
      try {
        const buf = Buffer.allocUnsafe(4096)
        const { bytesRead } = await fd.read(buf, 0, 4096, 0)
        const chunk = buf.subarray(0, bytesRead).toString('utf8')
        for (const line of chunk.split('\n').slice(0, 20)) {
          const trimmed = line.trim()
          if (!trimmed) continue
          let val: Record<string, unknown>
          try {
            val = JSON.parse(trimmed) as Record<string, unknown>
          } catch {
            continue
          }
          if (val.type !== 'user') continue
          const msg = val.message as Record<string, unknown> | undefined
          const content = msg?.content
          let text = ''
          if (typeof content === 'string') {
            text = content
          } else if (Array.isArray(content)) {
            for (const block of content) {
              const bt = (block as Record<string, unknown>).text
              if (typeof bt === 'string') text += `${bt}\n`
            }
          }
          text = text.trim().replace(/\n+/g, ' ')
          if (!text) continue
          return text.length > 80 ? `${text.slice(0, 80)}…` : text
        }
        return undefined
      } finally {
        await fd.close()
      }
    } catch {
      return undefined
    }
  }

  /**
   * Scan sessions from the assistant CWD project directory in ~/.claude/projects/.
   * Returns up to 50 sessions sorted by mtime desc. Never throws.
   */
  export async function scanAssistantSessions(limit = 50): Promise<SessionFile[]> {
    const assistantCwd = resolveAssistantCwd()
    const projectName = encodePathToProjectName(assistantCwd)
    const projectRoot = path.join(os.homedir(), '.claude', 'projects')
    const projectPath = path.join(projectRoot, projectName)

    try {
      await fs.access(projectPath)
    } catch {
      return []
    }

    let fileNames: string[]
    try {
      fileNames = await fs.readdir(projectPath)
    } catch {
      return []
    }

    const jsonlFiles = fileNames.filter((f) => f.endsWith('.jsonl'))
    const results = await mapWithConcurrency(
      jsonlFiles,
      async (fileName) => {
        const absolutePath = path.join(projectPath, fileName)
        try {
          const [stat, preview] = await Promise.all([
            fs.stat(absolutePath),
            extractPreviewLine(absolutePath),
          ])
          if (!stat.isFile()) return null
          return {
            id: sessionId(absolutePath),
            absolutePath,
            project: 'assistant',
            filename: fileName.slice(0, -'.jsonl'.length),
            mtimeMs: stat.mtimeMs,
            sizeBytes: stat.size,
            preview,
            repo: undefined,
            branch: undefined,
            agent: 'claude' as const,
          } satisfies SessionFile
        } catch {
          return null
        }
      },
      8,
    )

    return results
      .filter((r): r is SessionFile => r !== null)
      .sort((a, b) => b.mtimeMs - a.mtimeMs)
      .slice(0, limit)
  }
  ```

- [ ] **Step 3: Verify TypeScript types compile**

  Run: `cd apps/web && pnpm tsc --noEmit 2>&1 | head -30`
  Expected: no errors in new files

- [ ] **Step 4: Commit**

  ```bash
  git add apps/web/app/api/assistant/sessions/route.ts \
          apps/web/lib/sessions/assistant-scanner.ts
  git commit -m "feat(web): add assistant sessions API route and scanner"
  ```

---

## Chunk 3: Frontend — Rail Mode + Sidebar

### Task 5: Add 'assistant' to RailMode config

**Files:**
- Modify: `apps/web/components/reboot/axon-ui-config.ts`

- [ ] **Step 1: Update the type and RAIL_MODES array**

  ```typescript
  import { Bot, FolderOpen, MessageSquareText } from 'lucide-react'

  export type RailMode = 'sessions' | 'files' | 'assistant'

  export const RAIL_MODES: ReadonlyArray<RailModeItem> = [
    { id: 'sessions', label: 'Sessions', icon: MessageSquareText },
    { id: 'files', label: 'Files', icon: FolderOpen },
    { id: 'assistant', label: 'Assistant', icon: Bot },
  ]
  ```

  > Use `Bot` from lucide-react (already in the project's deps) — represents a general-purpose AI assistant, distinct from the coding-context sessions icon.

- [ ] **Step 2: Verify TypeScript**

  Run: `cd apps/web && pnpm tsc --noEmit 2>&1 | head -30`
  Expected: errors on axon-shell.tsx (good — it means the type propagated)

- [ ] **Step 3: Commit**

  ```bash
  git add apps/web/components/reboot/axon-ui-config.ts
  git commit -m "feat(web): add assistant rail mode to config"
  ```

---

### Task 6: Update the sidebar to render assistant session list

**Files:**
- Modify: `apps/web/components/reboot/axon-sidebar.tsx`

- [ ] **Step 1: Update `RailContent` props to accept assistant sessions**

  Add `assistantSessions` and `activeAssistantSessionId` / `onSelectAssistantSession` to the `RailContent` function props:

  ```typescript
  function RailContent({
    mode,
    sessions,
    activeSessionId,
    onSelectSession,
    assistantSessions,
    activeAssistantSessionId,
    onSelectAssistantSession,
    fileEntries,
    fileLoading,
    selectedFilePath,
    onSelectFile,
    query,
  }: {
    mode: RailMode
    sessions: SessionSummary[]
    activeSessionId: string | null
    onSelectSession: (sessionId: string) => void
    assistantSessions: SessionSummary[]
    activeAssistantSessionId: string | null
    onSelectAssistantSession: (sessionId: string) => void
    fileEntries: FileEntry[]
    fileLoading: boolean
    selectedFilePath: string | null
    onSelectFile: (entry: FileEntry) => void
    query: string
  }) {
  ```

- [ ] **Step 2: Add the `'assistant'` rendering branch**

  After the `if (mode === 'files')` block and before `return null`, add:

  ```typescript
  if (mode === 'assistant') {
    const filteredSessions = assistantSessions.filter((session) => {
      if (!normalizedQuery) return true
      return session.preview?.toLowerCase().includes(normalizedQuery) ?? false
    })

    return (
      <ul className="mt-1 space-y-0.5">
        {filteredSessions.length === 0 ? (
          <li className="px-3 py-4 text-xs text-[var(--text-dim)]">
            No assistant chats yet. Start a conversation below.
          </li>
        ) : null}
        {filteredSessions.map((session) => {
          const isActive = session.id === activeAssistantSessionId
          const title = session.preview?.slice(0, 60) ?? 'Untitled'
          return (
            <li key={session.id}>
              <button
                type="button"
                onClick={() => onSelectAssistantSession(session.id)}
                aria-current={isActive ? 'true' : undefined}
                className={`w-full border-l-2 px-0 py-2 text-left transition-colors ${railItemClass(isActive)}`}
              >
                <div className="px-3">
                  <div className="flex items-start justify-between gap-2">
                    <span className="text-[13px] font-medium">{title}</span>
                    <span className="text-[11px] text-[var(--text-dim)]">
                      {formatRelativeTime(session.mtimeMs)}
                    </span>
                  </div>
                </div>
              </button>
            </li>
          )
        })}
      </ul>
    )
  }
  ```

- [ ] **Step 3: Update `AxonSidebar` props**

  Add to the `AxonSidebar` component props interface:

  ```typescript
  assistantSessions: SessionSummary[]
  activeAssistantSessionId: string | null
  onSelectAssistantSession: (sessionId: string) => void
  ```

  And pass them through to `RailContent`:

  ```typescript
  <RailContent
    mode={railMode}
    sessions={sessions}
    activeSessionId={activeSessionId}
    onSelectSession={onSelectSession}
    assistantSessions={assistantSessions}
    activeAssistantSessionId={activeAssistantSessionId}
    onSelectAssistantSession={onSelectAssistantSession}
    fileEntries={fileEntries}
    fileLoading={fileLoading}
    selectedFilePath={selectedFilePath}
    onSelectFile={onSelectFile}
    query={railQuery}
  />
  ```

- [ ] **Step 4: Update search placeholder**

  Change the placeholder logic:

  ```typescript
  placeholder={
    railMode === 'sessions'
      ? 'Search sessions...'
      : railMode === 'assistant'
        ? 'Search assistant chats...'
        : 'Search files...'
  }
  ```

- [ ] **Step 5: Update subtitle**

  Change:

  ```typescript
  const subtitle =
    railMode === 'sessions'
      ? activeSessionRepo
      : railMode === 'assistant'
        ? 'assistant'
        : 'workspace root'
  ```

- [ ] **Step 6: Verify TypeScript**

  Run: `cd apps/web && pnpm tsc --noEmit 2>&1 | head -40`
  Expected: errors in axon-shell.tsx only (consumer of updated props)

- [ ] **Step 7: Commit**

  ```bash
  git add apps/web/components/reboot/axon-sidebar.tsx
  git commit -m "feat(web): render assistant session list in sidebar"
  ```

---

## Chunk 4: Frontend — Hook + Shell Wiring

### Task 7: Create `use-assistant-sessions` hook

**Files:**
- Create: `apps/web/hooks/use-assistant-sessions.ts`

- [ ] **Step 1: Write the hook**

  ```typescript
  'use client'

  import { useCallback, useEffect, useState } from 'react'
  import { apiFetch } from '@/lib/api-fetch'
  import type { SessionSummary } from '@/hooks/use-recent-sessions'

  export function useAssistantSessions() {
    const [sessions, setSessions] = useState<SessionSummary[]>([])

    const reload = useCallback(async () => {
      try {
        const res = await apiFetch('/api/assistant/sessions')
        if (!res.ok) {
          setSessions([])
          return
        }
        const data = (await res.json()) as SessionSummary[]
        setSessions(Array.isArray(data) ? data : [])
      } catch {
        setSessions([])
      }
    }, [])

    useEffect(() => {
      void reload()
    }, [reload])

    return { sessions, reload }
  }
  ```

- [ ] **Step 2: Verify it compiles**

  Run: `cd apps/web && pnpm tsc --noEmit 2>&1 | grep use-assistant`
  Expected: no errors for this file

- [ ] **Step 3: Commit**

  ```bash
  git add apps/web/hooks/use-assistant-sessions.ts
  git commit -m "feat(web): add useAssistantSessions hook"
  ```

---

### Task 8: Update `use-axon-acp` to forward `assistantMode` flag

**Files:**
- Modify: `apps/web/hooks/use-axon-acp.ts`

- [ ] **Step 1: Add `assistantMode` option**

  In `UseAxonAcpOptions`, add:

  ```typescript
  /** When true, sends assistant_mode:true to the backend so it uses the assistant CWD. */
  assistantMode?: boolean
  ```

  Update the function signature:

  ```typescript
  export function useAxonAcp({
    activeSessionId,
    agent = 'claude',
    assistantMode = false,
    onSessionIdChange,
    // ...
  }: UseAxonAcpOptions) {
  ```

- [ ] **Step 2: Forward the flag in `submitPrompt`**

  In the `send({ ... })` call, add to `flags`:

  ```typescript
  flags: {
    ...(activeSessionId ? { session_id: activeSessionId } : {}),
    agent,
    ...(assistantMode ? { assistant_mode: true } : {}),
  },
  ```

- [ ] **Step 3: Update deps array for `submitPrompt`**

  Add `assistantMode` to the `useCallback` deps array:

  ```typescript
  [connected, isStreaming, activeSessionId, agent, assistantMode, send, onMessagesChange],
  ```

- [ ] **Step 4: Verify**

  Run: `cd apps/web && pnpm tsc --noEmit 2>&1 | grep use-axon-acp`
  Expected: no errors

- [ ] **Step 5: Commit**

  ```bash
  git add apps/web/hooks/use-axon-acp.ts
  git commit -m "feat(web): forward assistantMode flag in useAxonAcp"
  ```

---

### Task 9: Wire everything in `axon-shell.tsx`

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx`

This is the largest change — the shell needs to:
1. Accept `'assistant'` in `readStoredRailMode`
2. Fetch assistant sessions via `useAssistantSessions`
3. Maintain a separate `activeAssistantSessionId` state
4. Pass `assistantMode` to `useAxonAcp`
5. Pass all new props to `AxonSidebar`
6. Handle "New session" in assistant mode (same logic — clear active id)

- [ ] **Step 1: Update `readStoredRailMode` guard**

  Change:

  ```typescript
  function readStoredRailMode(key: string, fallback: RailMode): RailMode {
    try {
      const v = window.localStorage.getItem(key)
      if (v === 'sessions' || v === 'files' || v === 'assistant') return v
      return fallback
    } catch {
      return fallback
    }
  }
  ```

- [ ] **Step 2: Import and call `useAssistantSessions`**

  ```typescript
  import { useAssistantSessions } from '@/hooks/use-assistant-sessions'
  ```

  In `AxonShell`:

  ```typescript
  const { sessions: assistantSessions, reload: reloadAssistantSessions } = useAssistantSessions()
  const [activeAssistantSessionId, setActiveAssistantSessionId] = useState<string | null>(null)
  ```

- [ ] **Step 3: Pass `assistantMode` to `useAxonAcp`**

  Update the `useAxonAcp` call to include `assistantMode`:

  ```typescript
  const { submitPrompt, isStreaming, connected } = useAxonAcp({
    activeSessionId: railMode === 'assistant' ? activeAssistantSessionId : activeSessionId,
    agent: pulseAgent ?? 'claude',
    assistantMode: railMode === 'assistant',
    onSessionIdChange,
    onSessionFallback: undefined,
    onMessagesChange,
    onTurnComplete,
    onEditorUpdate,
  })
  ```

  > **Note:** When in assistant mode, the `activeSessionId` passed to `useAxonAcp` should be the `activeAssistantSessionId`. This ensures session continuity in assistant mode works the same way as in sessions mode.

- [ ] **Step 4: Update `onTurnComplete` to reload assistant sessions when appropriate**

  ```typescript
  const onTurnComplete = useCallback(() => {
    reloadSessions()
    reloadSession()
    if (railMode === 'assistant') {
      reloadAssistantSessions()
    }
  }, [reloadSessions, reloadSession, reloadAssistantSessions, railMode])
  ```

- [ ] **Step 5: Add `handleSelectAssistantSession`**

  ```typescript
  const handleSelectAssistantSession = useCallback(
    (sessionId: string) => {
      setActiveAssistantSessionId(sessionId)
      setActiveSessionId(null) // clear coding session context
      setSessionKey((k) => k + 1)
    },
    [],
  )
  ```

- [ ] **Step 6: Update `handleNewSession` to clear both session IDs**

  ```typescript
  const handleNewSession = useCallback(() => {
    setActiveSessionId(null)
    setActiveAssistantSessionId(null)
    setLiveMessages([])
    setSessionKey((k) => k + 1)
  }, [])
  ```

- [ ] **Step 7: Update `sidebarProps`**

  Add the new assistant props:

  ```typescript
  const sidebarProps = {
    sessions: rawSessions,
    railMode,
    onRailModeChange: setRailModeTracked,
    railQuery,
    onRailQueryChange: setRailQuery,
    activeSessionId,
    activeSessionRepo: activeSession?.project ?? '',
    assistantSessions,
    activeAssistantSessionId,
    onSelectAssistantSession: handleSelectAssistantSession,
    fileEntries: workspace.fileEntries,
    fileLoading: workspace.fileLoading,
    selectedFilePath: workspace.selectedFilePath,
    onNewSession: handleNewSession,
  } as const
  ```

- [ ] **Step 8: Update the collapsed sidebar icon loop**

  The icon strip on the collapsed sidebar already iterates `RAIL_MODES` — it will automatically show the Bot icon once the config change is in place. No change needed here.

- [ ] **Step 9: Verify the full TypeScript build**

  Run: `cd apps/web && pnpm tsc --noEmit 2>&1 | head -40`
  Expected: no errors

- [ ] **Step 10: Run lint**

  Run: `cd apps/web && pnpm lint 2>&1 | head -30`
  Expected: clean

- [ ] **Step 11: Commit**

  ```bash
  git add apps/web/components/reboot/axon-shell.tsx
  git commit -m "feat(web): wire assistant mode in AxonShell"
  ```

---

## Chunk 5: Integration Check + Final Rust Tests

### Task 10: Full Rust test suite + clippy

**Files:** (no edits)

- [ ] **Step 1: Run `just verify`**

  Run: `just verify 2>&1 | tail -40`
  Expected: fmt-check ✓, clippy ✓, check ✓, test ✓

  If clippy warns about unused variables or `too_many_arguments` on `handle_pulse_chat`, fix:
  - Add `#[allow(clippy::too_many_arguments)]` if argument count exceeds clippy's default limit (7)
    or split into a helper struct.

- [ ] **Step 2: If `handle_pulse_chat` exceeds 120 lines, extract CWD resolution into a helper**

  Check: `wc -l crates/web/execute/sync_mode/pulse_chat.rs`

  If over 500 lines (file limit), consider splitting. If any function is over 120 lines, extract:

  ```rust
  /// Resolve the working directory for the ACP adapter.
  /// Returns the assistant dir (creating it if needed) or the process CWD.
  async fn resolve_adapter_cwd(assistant_mode: bool) -> Result<std::path::PathBuf, String> {
      if assistant_mode {
          let base = std::env::var("AXON_DATA_DIR").unwrap_or_else(|_| {
              let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
              format!("{home}/.local/share/axon")
          });
          let path = std::path::PathBuf::from(base).join("axon").join("assistant");
          tokio::fs::create_dir_all(&path)
              .await
              .map_err(|e| format!("failed to create assistant dir: {e}"))?;
          Ok(path)
      } else {
          std::env::current_dir().map_err(|e| e.to_string())
      }
  }
  ```

  And call it from `get_or_create_acp_connection`:

  ```rust
  let cwd = resolve_adapter_cwd(assistant_mode).await?;
  ```

- [ ] **Step 3: Add a unit test for `resolve_adapter_cwd` (no-assistant branch)**

  ```rust
  #[tokio::test]
  async fn resolve_adapter_cwd_non_assistant_returns_current_dir() {
      let result = resolve_adapter_cwd(false).await;
      assert!(result.is_ok());
      assert_eq!(result.unwrap(), std::env::current_dir().unwrap());
  }
  ```

- [ ] **Step 4: Re-run `just verify`**

  Run: `just verify 2>&1 | tail -20`
  Expected: all passing

- [ ] **Step 5: Commit**

  ```bash
  git add crates/web/execute/sync_mode/pulse_chat.rs
  git commit -m "refactor(web): extract resolve_adapter_cwd helper + add test"
  ```

---

### Task 11: Manual smoke test

This requires the dev stack running. Verify:

- [ ] **Start infrastructure**

  ```bash
  docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-chrome
  cargo run --bin axon -- embed worker &
  cd apps/web && pnpm dev
  ```

- [ ] **Open the UI at http://localhost:49010**

  1. Click the mode dropdown in the sidebar — confirm "Assistant" appears with a Bot icon
  2. Switch to "Assistant" mode
  3. Type a general question (e.g., "What is the capital of France?")
  4. Confirm the response is received in the chat panel
  5. Switch back to "Sessions" — confirm previous coding sessions still show
  6. Switch back to "Assistant" — confirm the assistant chat appears in the list
  7. Verify the file at `$AXON_DATA_DIR/axon/assistant/` was created

- [ ] **Verify Claude session path**

  ```bash
  ls ~/.claude/projects/ | grep assistant
  # Should show something like: -home-jmagar-appdata-axon-assistant
  ```

- [ ] **Commit (if any fixes needed from smoke test)**

  ```bash
  git add -p
  git commit -m "fix(web): address issues found in smoke test"
  ```

---

## Summary

After completing all tasks, the following will be true:

1. **UI**: The sidebar dropdown shows Sessions / Files / Assistant. Switching to Assistant shows the assistant chat history list.
2. **Chat isolation**: All chats in Assistant mode run with CWD = `$AXON_DATA_DIR/axon/assistant`, keeping coding sessions and general assistant sessions completely separate.
3. **Session persistence**: Assistant sessions are listed in the sidebar after each turn completes; clicking a past session resumes it.
4. **Agent compatibility**: Claude, Codex, and Gemini all work in assistant mode (the `agent` flag still functions; each agent gets its own adapter in assistant mode due to the `agent:assistant` key).
5. **No regressions**: All existing Sessions and Files mode behavior is unchanged.

### Key invariants to verify after implementation
- `RAIL_MODES` array has exactly 3 entries
- `readStoredRailMode` accepts `'sessions' | 'files' | 'assistant'`
- `ALLOWED_FLAGS` in `constants.rs` includes `"assistant_mode"`
- `DirectParams.assistant_mode` defaults to `false` (no breaking change)
- `handle_pulse_chat` signature update is reflected in all call sites
- The assistant directory is only created on first turn (lazy init, not at startup)
