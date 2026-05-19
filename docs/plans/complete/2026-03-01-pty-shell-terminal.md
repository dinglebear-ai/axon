# PTY Shell Terminal Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the axon command-mode executor in `/terminal` with a real interactive PTY shell (`$SHELL`) so users can run any program (`ls`, `vim`, `htop`, etc.).

**Architecture:** Add a `/ws/shell` WebSocket endpoint to the Rust axum server that spawns `$SHELL` in a pseudo-terminal (PTY) via `portable-pty` and bridges raw I/O through the WebSocket. The frontend terminal page gets a new `useShellSession` hook that connects to `/ws/shell` directly (separate from the shared `/ws`), forwards raw xterm `onData` events to the PTY, and writes PTY output back to xterm. All existing axon command infrastructure is untouched.

**Tech Stack:** Rust (`portable-pty` crate, axum WS, tokio), TypeScript (xterm.js already wired, new hook, simplified page)

---

## File Map

| File | Action |
|------|--------|
| `Cargo.toml` | Modify — add `portable-pty` dependency |
| `crates/web.rs` | Modify — declare `mod shell`, add `/ws/shell` route |
| `crates/web/shell.rs` | **Create** — PTY spawn + WebSocket bridge |
| `apps/web/hooks/use-shell-session.ts` | **Create** — shell WebSocket hook |
| `apps/web/app/terminal/page.tsx` | Modify — swap session hook, remove command parser |

**No changes needed:** `terminal-emulator.tsx` (already has `onResize` prop wired), `terminal-emulator-wrapper.tsx`, `terminal-toolbar.tsx`, `use-axon-ws.ts`, `ws-protocol.ts`

---

## Task 1: Add `portable-pty` to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add the dependency**

Find the `[dependencies]` block and add after `axum`:

```toml
portable-pty = "0"
```

**Step 2: Verify it resolves**

```bash
cd /home/jmagar/workspace/axon_rust
cargo fetch
```

Expected: no errors, lock file updated with `portable-pty` and its deps.

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): add portable-pty for PTY shell support"
```

---

## Task 2: Create `crates/web/shell.rs` — PTY WebSocket handler

**Files:**
- Create: `crates/web/shell.rs`

**Step 1: Write the file**

```rust
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde::Deserialize;
use std::io::{Read, Write};
use tokio::sync::mpsc;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ShellClientMsg {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

pub async fn handle_shell_ws(socket: WebSocket) {
    let (ws_tx, ws_rx) = socket.split();
    if let Err(e) = run_shell(ws_tx, ws_rx).await {
        tracing::warn!("shell session error: {e}");
    }
}

async fn run_shell(
    ws_tx: impl SinkExt<Message, Error = axum::Error> + Send + Unpin + 'static,
    mut ws_rx: impl StreamExt<Item = Result<Message, axum::Error>> + Send + Unpin,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    pair.slave.spawn_command(cmd)?;
    drop(pair.slave); // child keeps slave open; drop our reference

    let master = pair.master;
    let reader = master.try_clone_reader()?;
    let writer = master.take_writer()?;

    let (pty_out_tx, mut pty_out_rx) = mpsc::channel::<String>(256);
    let (pty_in_tx, pty_in_rx) = mpsc::channel::<Vec<u8>>(256);

    // Blocking task: reads PTY output → sends JSON to channel
    let reader_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        let mut reader = reader;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let json = serde_json::json!({"type": "output", "data": s}).to_string();
                    if pty_out_tx.blocking_send(json).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Blocking task: drains input channel → writes to PTY stdin
    let writer_task = tokio::task::spawn_blocking(move || {
        let mut writer = writer;
        let mut rx: mpsc::Receiver<Vec<u8>> = pty_in_rx;
        loop {
            match rx.blocking_recv() {
                None => break,
                Some(bytes) => {
                    let _ = writer.write_all(&bytes);
                }
            }
        }
    });

    // Async task: drains pty_out channel → sends to WS client
    use std::sync::Arc;
    use tokio::sync::Mutex;
    let ws_tx = Arc::new(Mutex::new(ws_tx));
    let ws_tx_clone = ws_tx.clone();
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = pty_out_rx.recv().await {
            let mut tx = ws_tx_clone.lock().await;
            if tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // WS receive loop: dispatches input + resize to PTY
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => match serde_json::from_str::<ShellClientMsg>(&text) {
                Ok(ShellClientMsg::Input { data }) => {
                    let _ = pty_in_tx.send(data.into_bytes()).await;
                }
                Ok(ShellClientMsg::Resize { cols, rows }) => {
                    let _ = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
                Err(_) => {}
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    reader_task.abort();
    writer_task.abort();
    sender_task.abort();
    Ok(())
}
```

**Step 2: Check it compiles (type check only, no tests yet)**

```bash
cargo check 2>&1 | grep -E "error|warning" | head -20
```

Expected: no errors (warnings about unused imports may appear — ignore for now).

**Step 3: Commit**

```bash
git add crates/web/shell.rs
git commit -m "feat(web): PTY shell WebSocket handler in crates/web/shell.rs"
```

---

## Task 3: Wire `/ws/shell` route into `crates/web.rs`

**Files:**
- Modify: `crates/web.rs` (lines 1-5 for `mod shell;`, ~line 51 for route)

**Step 1: Add module declaration**

At the top of `crates/web.rs`, after the existing `mod` declarations:

```rust
mod shell;
```

The existing block is:
```rust
mod docker_stats;
mod download;
mod execute;
mod pack;
```

Becomes:
```rust
mod docker_stats;
mod download;
mod execute;
mod pack;
mod shell;
```

**Step 2: Add the upgrade handler function**

After the existing `ws_upgrade` function (~line 139), add:

```rust
async fn shell_ws_upgrade(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(shell::handle_shell_ws)
}
```

**Step 3: Add the route**

In the `Router::new()` block (~line 51), add after `.route("/ws", get(ws_upgrade))`:

```rust
.route("/ws/shell", get(shell_ws_upgrade))
```

The block becomes:
```rust
let app = Router::new()
    .route("/ws", get(ws_upgrade))
    .route("/ws/shell", get(shell_ws_upgrade))
    .route("/output/{*path}", get(serve_output_file))
    .with_state(state)
    .merge(download_routes);
```

**Step 4: Verify it compiles**

```bash
cargo check 2>&1 | grep "error" | head -20
```

Expected: no errors.

**Step 5: Run existing tests to confirm no regressions**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all passing (same count as before this PR).

**Step 6: Commit**

```bash
git add crates/web.rs
git commit -m "feat(web): add /ws/shell route for PTY shell sessions"
```

---

## Task 4: Create `apps/web/hooks/use-shell-session.ts`

**Files:**
- Create: `apps/web/hooks/use-shell-session.ts`

**Step 1: Write the hook**

```typescript
'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import type { WsStatus } from '@/lib/ws-protocol'

const BASE_BACKOFF = 1000
const MAX_BACKOFF = 30000

interface UseShellSessionOptions {
  /** Called with raw PTY output as it arrives. */
  onOutput: (data: string) => void
}

interface UseShellSessionReturn {
  /** WebSocket connection status. */
  status: WsStatus
  /** Send raw terminal input (keystrokes, escape sequences) to the PTY. */
  sendInput: (data: string) => void
  /** Notify the PTY of terminal dimension changes. */
  resize: (cols: number, rows: number) => void
}

/**
 * Manages a dedicated WebSocket connection to /ws/shell that bridges
 * a server-side PTY. All terminal I/O passes through raw JSON messages —
 * no command parsing, no mode routing.
 */
export function useShellSession({ onOutput }: UseShellSessionOptions): UseShellSessionReturn {
  const [status, setStatus] = useState<WsStatus>('disconnected')
  const wsRef = useRef<WebSocket | null>(null)
  const onOutputRef = useRef(onOutput)
  onOutputRef.current = onOutput
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const attemptsRef = useRef(0)
  const connectRef = useRef<() => void>(() => {})

  const scheduleReconnect = useCallback(() => {
    if (timerRef.current) return
    const delay = Math.min(BASE_BACKOFF * 2 ** attemptsRef.current, MAX_BACKOFF)
    attemptsRef.current++
    timerRef.current = setTimeout(() => {
      timerRef.current = null
      connectRef.current()
    }, delay)
  }, [])

  const connect = useCallback(() => {
    if (
      wsRef.current?.readyState === WebSocket.CONNECTING ||
      wsRef.current?.readyState === WebSocket.OPEN
    )
      return

    // Derive /ws/shell URL from NEXT_PUBLIC_AXON_WS_URL or window.location
    const proto = globalThis.location?.protocol === 'https:' ? 'wss:' : 'ws:'
    const envUrl = process.env.NEXT_PUBLIC_AXON_WS_URL
    const base = envUrl
      ? envUrl.replace(/\/ws$/, '')
      : `${proto}//${globalThis.location?.host}`
    const wsUrl = `${base}/ws/shell`

    try {
      const ws = new WebSocket(wsUrl)
      wsRef.current = ws

      ws.onopen = () => {
        attemptsRef.current = 0
        setStatus('connected')
      }

      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data as string) as { type: string; data?: string }
          if (msg.type === 'output' && msg.data) {
            onOutputRef.current(msg.data)
          }
        } catch {
          /* malformed JSON — ignore */
        }
      }

      ws.onclose = () => {
        setStatus('reconnecting')
        scheduleReconnect()
      }

      ws.onerror = () => {
        /* onclose fires after onerror — handled there */
      }
    } catch {
      scheduleReconnect()
    }
  }, [scheduleReconnect])

  connectRef.current = connect

  useEffect(() => {
    connect()
    return () => {
      wsRef.current?.close()
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [connect])

  const sendInput = useCallback((data: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'input', data }))
    }
  }, [])

  const resize = useCallback((cols: number, rows: number) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'resize', cols, rows }))
    }
  }, [])

  return { status, sendInput, resize }
}
```

**Step 2: Verify TypeScript compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm tsc --noEmit 2>&1 | grep "error" | head -20
```

Expected: no errors for this new file.

**Step 3: Commit**

```bash
git add apps/web/hooks/use-shell-session.ts
git commit -m "feat(web): useShellSession hook — dedicated /ws/shell WebSocket"
```

---

## Task 5: Rewrite `apps/web/app/terminal/page.tsx`

**Files:**
- Modify: `apps/web/app/terminal/page.tsx`

**Step 1: Replace the file contents**

The new page is significantly simpler — no input buffering, no command parsing, no history, no WELCOME_BANNER. Raw xterm `onData` → PTY input; PTY output → xterm `write`.

```tsx
'use client'

import dynamic from 'next/dynamic'
import { useCallback, useEffect, useRef, useState } from 'react'
import type { TerminalHandle } from '@/components/terminal/terminal-emulator'
import { TerminalEmulatorWrapper } from '@/components/terminal/terminal-emulator-wrapper'
import { TerminalToolbar } from '@/components/terminal/terminal-toolbar'
import { useShellSession } from '@/hooks/use-shell-session'

const NeuralCanvas = dynamic(() => import('@/components/neural-canvas'), { ssr: false })

export default function TerminalPage() {
  const terminalRef = useRef<TerminalHandle | null>(null)
  const [searchVisible, setSearchVisible] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')

  // Shell session — dedicated /ws/shell WebSocket, no mode routing
  const { status, sendInput, resize } = useShellSession({
    onOutput: (data) => terminalRef.current?.write(data),
  })

  // Forward raw xterm keystrokes/sequences directly to the PTY
  const handleData = useCallback(
    (data: string) => {
      sendInput(data)
    },
    [sendInput],
  )

  // Notify PTY when xterm dimensions change (FitAddon fires this after fit())
  const handleResize = useCallback(
    (cols: number, rows: number) => {
      resize(cols, rows)
    },
    [resize],
  )

  useEffect(() => {
    document.title = 'Terminal — Axon'
    const timer = setTimeout(() => terminalRef.current?.focus(), 200)
    return () => clearTimeout(timer)
  }, [])

  const handleClear = useCallback(() => terminalRef.current?.clear(), [])

  const handleCopy = useCallback(() => {
    const text = terminalRef.current?.getSelectedText() ?? ''
    if (text) {
      navigator.clipboard.writeText(text).catch(() => {
        /* ignore clipboard errors */
      })
    }
  }, [])

  const handleSearchChange = useCallback((val: string) => {
    setSearchQuery(val)
    if (val) terminalRef.current?.search(val)
  }, [])

  const handleToggleSearch = useCallback(() => setSearchVisible((prev) => !prev), [])

  return (
    <div className="relative flex h-screen flex-col overflow-hidden">
      {/* Background */}
      <div className="fixed inset-0 z-0">
        <NeuralCanvas />
      </div>

      {/* Toolbar */}
      <header className="relative z-30 flex-shrink-0">
        <TerminalToolbar
          status={status}
          isRunning={false}
          onClear={handleClear}
          onCopy={handleCopy}
          onCancelCurrent={() => {}}
          searchVisible={searchVisible}
          onToggleSearch={handleToggleSearch}
        />
      </header>

      {/* Terminal area */}
      <main className="relative z-10 flex flex-1 flex-col overflow-hidden p-2">
        <div
          className="relative flex-1 overflow-hidden rounded-xl border"
          style={{
            background: 'rgba(3,7,18,0.95)',
            borderColor: 'var(--axon-border, rgba(255,135,175,0.12))',
          }}
        >
          {/* Search overlay */}
          {searchVisible && (
            <div
              className="absolute right-3 top-2 z-20 flex items-center gap-1 rounded-md border px-2 py-1"
              style={{
                background: 'rgba(9,18,37,0.95)',
                borderColor: 'rgba(175,215,255,0.2)',
              }}
            >
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => handleSearchChange(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') {
                    setSearchVisible(false)
                    setSearchQuery('')
                    terminalRef.current?.focus()
                  }
                  if (e.key === 'Enter') {
                    terminalRef.current?.search(searchQuery)
                  }
                }}
                placeholder="Search..."
                className="w-40 bg-transparent font-mono text-xs outline-none"
                style={{ color: 'var(--text-primary)' }}
                aria-label="Terminal search"
              />
              <button
                type="button"
                onClick={() => {
                  setSearchVisible(false)
                  setSearchQuery('')
                  terminalRef.current?.focus()
                }}
                className="ml-1 text-xs"
                style={{ color: 'var(--text-muted)' }}
                aria-label="Close search"
              >
                ✕
              </button>
            </div>
          )}

          {/* xterm.js terminal — onResize notifies PTY of dimension changes */}
          <TerminalEmulatorWrapper
            ref={terminalRef}
            onData={handleData}
            onResize={handleResize}
            className="h-full w-full"
          />
        </div>
      </main>
    </div>
  )
}
```

**Step 2: Verify TypeScript compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm tsc --noEmit 2>&1 | grep "error" | head -20
```

Expected: no errors.

**Step 3: Check for unused imports from the old page (should be gone)**

The old imports `useAxonWs`, `useTerminalSession`, `TerminalHistory` are removed. Verify no TS unused-import warnings.

**Step 4: Commit**

```bash
git add apps/web/app/terminal/page.tsx
git commit -m "feat(web): terminal page — real PTY shell via useShellSession"
```

---

## Task 6: Full build + test gate

**Step 1: Rust tests (no regressions)**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test --lib 2>&1 | tail -10
```

Expected: same pass count as before, no new failures.

**Step 2: Clippy**

```bash
cargo clippy 2>&1 | grep "error\[" | head -20
```

Expected: no errors. Warnings about `use std::sync::Arc` inside `async fn` are acceptable (it's intentional).

**Step 3: Fmt check**

```bash
cargo fmt --check
```

Expected: clean. If not: run `cargo fmt` and amend the shell.rs commit.

**Step 4: Monolith check**

```bash
./scripts/enforce_monoliths.py 2>&1 | grep -E "FAIL|ERROR" | head -10
```

Expected: no failures. `crates/web/shell.rs` is ~120 lines, within the 500-line limit. `run_shell` function is ~70 lines, within the 120-line hard limit.

**Step 5: TS type check (full)**

```bash
cd apps/web && pnpm tsc --noEmit 2>&1 | grep "error TS" | head -20
```

Expected: no errors.

**Step 6: Final commit if any fmt fixes needed**

```bash
git add -p
git commit -m "style: cargo fmt on shell.rs"
```

---

## Task 7: Manual smoke test

**Prerequisites:** Docker stack running (`docker compose up -d`)

**Step 1: Rebuild the workers container (picks up new Rust binary)**

```bash
docker compose build axon-workers && docker compose up -d axon-workers
```

**Step 2: Open browser at `https://axon.tootie.tv/terminal`**

Expected:
- Toolbar shows `TERMINAL • CONNECTED`
- Shell prompt appears (bash/zsh prompt from the container)

**Step 3: Type `ls` and press Enter**

Expected: directory listing output, NOT `axon: unknown mode: ls`

**Step 4: Type `echo $SHELL` and press Enter**

Expected: `/bin/bash` (or whatever `$SHELL` is in the container)

**Step 5: Test resize — resize browser window**

Expected: shell reflows correctly (text wraps to new column count, prompt adjusts)

**Step 6: Test Ctrl+C**

Type a long-running command like `sleep 100`, press Ctrl+C.
Expected: shell cancels it and shows `^C` followed by a new prompt.

**Step 7: Test `vim` (requires full PTY)**

```bash
vim /tmp/test.txt
```

Expected: vim opens with full screen rendering (curses UI). Press `:q!` to exit.

---

## Gotchas

**`$SHELL` in Docker container** — The `axon-workers` container uses `debian:bookworm-slim` base; `/bin/bash` is present. `$SHELL` defaults to `/bin/bash` if the env var is unset.

**PTY reader blocks on EOF** — When the shell process exits, `reader.read()` returns `Ok(0)`. The reader task exits cleanly, which closes `pty_out_tx`, which causes the sender task to exit, which closes the WS. This is correct behavior — the frontend reconnects automatically.

**`portable-pty` slave drop** — `drop(pair.slave)` must happen after `spawn_command()`. The child process inherits the slave's file descriptors; dropping our handle doesn't affect the child.

**`run_shell` function complexity** — The function is ~70 lines, under the 80-line warn threshold. If clippy or monolith checks flag it, extract the PTY setup into `fn spawn_pty() -> Result<(Box<dyn MasterPty>, ...)>`.

**`NEXT_PUBLIC_AXON_WS_URL`** — If set to `ws://host:49000/ws`, the shell hook strips `/ws` suffix and appends `/ws/shell` → `ws://host:49000/ws/shell`. This is correct.

**No auth on `/ws/shell`** — Intentional for homelab use. The endpoint is only reachable through the Tailscale/SWAG reverse proxy, same as all other endpoints.
