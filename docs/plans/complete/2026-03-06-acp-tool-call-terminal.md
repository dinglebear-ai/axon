# ACP Tool Call Terminal Rendering — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Render ACP tool call output inline in Pulse chat using read-only xterm.js terminals, matching Zed's Terminal entity pattern for tool calls.

**Architecture:** When ACP `ToolCall`/`ToolCallUpdate` events arrive, they are forwarded to the frontend as `tool_use` events with enriched fields (tool_call_id, tool_name, tool_status, content). The frontend renders each tool call as a collapsible card with a `ToolCallTerminal` component — a read-only xterm.js instance that receives streamed content via `TerminalHandle.write()`. No PTY or shell server needed; the adapter owns the subprocess.

**Tech Stack:** Rust (ACP event enrichment), React (ToolCallTerminal component), xterm.js (terminal rendering), existing TerminalEmulator component.

---

## Context

### ACP SDK Tool Call Types

```
ToolCall { tool_call_id, title, status, kind, content, locations, raw_input, raw_output }
ToolCallUpdate { tool_call_id, fields: ToolCallUpdateFields }
ToolCallUpdateFields { kind?, status?, title?, content?, locations?, raw_input?, raw_output? }
ToolCallContent = Content(ContentBlock) | Diff(Diff) | Terminal(Terminal)
ToolCallStatus = Running | Completed | ...
```

### Current Wire Format (tool_use event)

```json
{
  "type": "tool_use",
  "session_id": "...",
  "tool_call_id": "tool-1",
  "delta": "",
  "tool_name": "Read file",
  "tool_status": "Running"
}
```

### What's Missing

1. **`content` not forwarded** — `ToolCallUpdateFields.content` (Vec<ToolCallContent>) is extracted for `tool_name`/`tool_status` but the actual content text is dropped.
2. **`raw_input`/`raw_output` not forwarded** — These contain the tool's JSON input and result.
3. **No frontend rendering** — Tool calls appear as plain text badges, not terminal-style output.

### Files Involved

| File | Role |
|------|------|
| `crates/services/acp.rs` | Extracts tool details from ACP SDK types |
| `crates/services/types.rs` | Wire format types (`AcpSessionUpdateEvent`) |
| `apps/web/lib/pulse/chat-stream.ts` | Stream event type definitions |
| `apps/web/hooks/pulse-chat-helpers.ts` | Stream event handler (tool_use branch) |
| `apps/web/lib/pulse/types.ts` | `PulseToolUse`, `PulseMessageBlock` types |
| `apps/web/components/terminal/terminal-emulator.tsx` | Existing xterm.js component |

---

### Task 1: Enrich Tool Call Wire Format with Content

**Files:**
- Modify: `crates/services/types.rs` — Add `tool_content` and `tool_input` to `AcpSessionUpdateEvent`
- Modify: `crates/services/acp.rs` — Extract content text from `ToolCallUpdateFields.content` and `raw_input`
- Test: `tests/services_acp_event_mapping.rs`

**Step 1: Write the failing test**

In `tests/services_acp_event_mapping.rs`, add:

```rust
#[test]
fn map_session_notification_extracts_tool_content_text() {
    let update = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new("tool-content-1"),
        ToolCallUpdateFields::new()
            .title("Bash")
            .content(vec![ToolCallContent::from(ContentBlock::text("hello world"))]),
    ));
    let notification = SessionNotification {
        session_id: SessionId::new("session-content"),
        update,
    };
    let mapped = map_session_notification(&notification);
    assert_eq!(mapped.tool_call_id.as_deref(), Some("tool-content-1"));
    assert_eq!(mapped.tool_content.as_deref(), Some("hello world"));
}

#[test]
fn map_session_notification_extracts_tool_raw_input() {
    let input_json = serde_json::json!({"command": "ls -la"});
    let update = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new("tool-input-1"),
        ToolCallUpdateFields::new()
            .title("Bash")
            .raw_input(input_json.clone()),
    ));
    let notification = SessionNotification {
        session_id: SessionId::new("session-input"),
        update,
    };
    let mapped = map_session_notification(&notification);
    assert_eq!(mapped.tool_input.as_ref(), Some(&input_json));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test map_session_notification_extracts_tool_content -- --nocapture`
Expected: FAIL — `tool_content` and `tool_input` fields don't exist on `AcpSessionUpdateEvent`

**Step 3: Add fields to `AcpSessionUpdateEvent`**

In `crates/services/types.rs`, add to `AcpSessionUpdateEvent`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub tool_content: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub tool_input: Option<serde_json::Value>,
```

**Step 4: Extract content in `acp.rs`**

Add a new extraction function and wire it into `map_session_notification`:

```rust
fn extract_tool_content(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::ToolCall(tc) => {
            tc.content.iter().filter_map(|c| match c {
                ToolCallContent::Content(content) => content.content.iter().filter_map(|block| {
                    match block { ContentBlock::Text(t) => Some(t.text.as_str()), _ => None }
                }).next().map(String::from),
                _ => None,
            }).next()
        }
        SessionUpdate::ToolCallUpdate(tcu) => {
            tcu.fields.content.as_ref().and_then(|contents| {
                contents.iter().filter_map(|c| match c {
                    ToolCallContent::Content(content) => content.content.iter().filter_map(|block| {
                        match block { ContentBlock::Text(t) => Some(t.text.clone()), _ => None }
                    }).next(),
                    _ => None,
                }).next()
            })
        }
        _ => None,
    }
}

fn extract_tool_input(update: &SessionUpdate) -> Option<serde_json::Value> {
    match update {
        SessionUpdate::ToolCall(tc) => tc.raw_input.clone(),
        SessionUpdate::ToolCallUpdate(tcu) => tcu.fields.raw_input.clone(),
        _ => None,
    }
}
```

Wire into `map_session_notification`:

```rust
pub fn map_session_notification(notification: &SessionNotification) -> AcpSessionUpdateEvent {
    let kind = map_session_update_kind(&notification.update);
    let text_delta = extract_text_delta(&notification.update);
    let tool_call_id = extract_tool_call_id(&notification.update);
    let (tool_name, tool_status) = extract_tool_details(&notification.update);
    let tool_content = extract_tool_content(&notification.update);
    let tool_input = extract_tool_input(&notification.update);
    AcpSessionUpdateEvent {
        session_id: notification.session_id.0.to_string(),
        kind,
        text_delta,
        tool_call_id,
        tool_name,
        tool_status,
        tool_content,
        tool_input,
    }
}
```

Update the custom `Serialize` impl for `AcpBridgeEvent::SessionUpdate` to include the new fields:

```rust
if let Some(ref content) = update.tool_content {
    map.serialize_entry("tool_content", content)?;
}
if let Some(ref input) = update.tool_input {
    map.serialize_entry("tool_input", input)?;
}
```

**Step 5: Fix all struct literal sites**

Update every `AcpSessionUpdateEvent { ... }` literal to include `tool_content: None, tool_input: None`. Check:
- `crates/web/execute/tests/ws_event_v2_tests.rs`
- `tests/services_acp_event_mapping.rs`
- `tests/services_acp_smoke.rs`

**Step 6: Run tests to verify they pass**

Run: `cargo test map_session_notification_extracts -- --nocapture`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/services/types.rs crates/services/acp.rs tests/services_acp_event_mapping.rs
git commit -m "feat(acp): forward tool_content and tool_input in ToolCall wire events"
```

---

### Task 2: Frontend Types for Tool Call Content

**Files:**
- Modify: `apps/web/lib/pulse/chat-stream.ts` — Add tool call stream event type
- Modify: `apps/web/lib/pulse/types.ts` — Enrich `PulseToolUse` and `PulseMessageBlock`

**Step 1: Update `PulseToolUse` type**

In `apps/web/lib/pulse/types.ts`:

```typescript
export interface PulseToolUse {
  name: string
  input: Record<string, unknown>
  toolCallId?: string
  status?: string
  content?: string
}
```

**Step 2: Update `PulseMessageBlock` tool_use variant**

```typescript
export type PulseMessageBlock =
  | { type: 'text'; content: string }
  | { type: 'tool_use'; name: string; input: Record<string, unknown>; result?: string; toolCallId?: string; status?: string; content?: string }
  | { type: 'thinking'; content: string }
```

**Step 3: Add tool_use_update stream event type**

In `apps/web/lib/pulse/chat-stream.ts`, add to `PulseChatStreamEventPayload`:

```typescript
| { type: 'tool_use_update'; toolCallId: string; status?: string; content?: string; toolName?: string }
```

**Step 4: Commit**

```bash
git add apps/web/lib/pulse/types.ts apps/web/lib/pulse/chat-stream.ts
git commit -m "feat(web): add tool call content types for terminal rendering"
```

---

### Task 3: Handle Tool Call Updates in Stream Event Handler

**Files:**
- Modify: `apps/web/hooks/pulse-chat-helpers.ts` — Handle `tool_use_update` events, update tool blocks with content/status

**Step 1: Update `makeStreamEventHandler` to handle tool_use_update**

After the existing `tool_use` handler block, add:

```typescript
if (event.type === 'tool_use_update' && event.toolCallId) {
  ensureDraftAdded()
  // Find and update the matching tool block
  const blockIdx = acc.partialBlocks.findLastIndex(
    (b) => b.type === 'tool_use' && b.toolCallId === event.toolCallId,
  )
  if (blockIdx >= 0) {
    const block = acc.partialBlocks[blockIdx]
    if (block.type === 'tool_use') {
      acc.partialBlocks[blockIdx] = {
        ...block,
        status: event.status ?? block.status,
        content: event.content
          ? (block.content ?? '') + event.content
          : block.content,
      }
    }
  }
  // Also update the tool in partialTools
  const toolIdx = acc.partialTools.findLastIndex(
    (t) => t.toolCallId === event.toolCallId,
  )
  if (toolIdx >= 0) {
    const tool = acc.partialTools[toolIdx]
    acc.partialTools[toolIdx] = {
      ...tool,
      status: event.status ?? tool.status,
      content: event.content
        ? (tool.content ?? '') + event.content
        : tool.content,
    }
  }
  setLiveToolUses([...acc.partialTools])
  updateChatMessage(assistantDraft.id!, (m) => ({
    ...m,
    toolUses: [...acc.partialTools],
    blocks: [...acc.partialBlocks],
  }))
  return
}
```

**Step 2: Update existing `tool_use` handler to include toolCallId**

In the existing `tool_use` event handler, pass through `toolCallId`:

```typescript
if (event.type === 'tool_use' && event.tool) {
  flush()
  ensureDraftAdded()
  const enrichedTool = { ...event.tool, toolCallId: event.tool.toolCallId }
  acc.partialTools.push(enrichedTool)
  acc.partialBlocks.push({
    type: 'tool_use',
    name: event.tool.name,
    input: event.tool.input,
    toolCallId: event.tool.toolCallId,
  })
  // ... rest unchanged
}
```

**Step 3: Commit**

```bash
git add apps/web/hooks/pulse-chat-helpers.ts
git commit -m "feat(web): handle tool_use_update events with content streaming"
```

---

### Task 4: Create ToolCallTerminal Component

**Files:**
- Create: `apps/web/components/pulse/tool-call-terminal.tsx`

**Step 1: Create the component**

```tsx
'use client'

import { ChevronDown, ChevronRight, Loader2, CheckCircle2, XCircle } from 'lucide-react'
import dynamic from 'next/dynamic'
import { useCallback, useEffect, useRef, useState } from 'react'
import type { TerminalHandle } from '@/components/terminal/terminal-emulator'

const TerminalEmulator = dynamic(
  () => import('@/components/terminal/terminal-emulator').then((m) => m.TerminalEmulator),
  { ssr: false },
)

interface ToolCallTerminalProps {
  toolName: string
  toolCallId: string
  input?: Record<string, unknown>
  content?: string
  status?: string
}

function StatusIcon({ status }: { status?: string }) {
  if (!status || status === 'Running') {
    return <Loader2 className="size-3.5 animate-spin text-[var(--axon-primary)]" />
  }
  if (status === 'Completed') {
    return <CheckCircle2 className="size-3.5 text-emerald-400" />
  }
  return <XCircle className="size-3.5 text-red-400" />
}

export function ToolCallTerminal({
  toolName,
  toolCallId,
  input,
  content,
  status,
}: ToolCallTerminalProps) {
  const [expanded, setExpanded] = useState(true)
  const termRef = useRef<TerminalHandle>(null)
  const writtenLenRef = useRef(0)

  // Write new content deltas to the terminal
  useEffect(() => {
    if (!content || !termRef.current) return
    const newContent = content.slice(writtenLenRef.current)
    if (newContent) {
      termRef.current.write(newContent)
      writtenLenRef.current = content.length
    }
  }, [content])

  // Collapse automatically when completed
  useEffect(() => {
    if (status === 'Completed' && content && content.length > 500) {
      setExpanded(false)
    }
  }, [status, content])

  const noopOnData = useCallback(() => {}, [])

  const inputSummary = input
    ? Object.entries(input)
        .map(([k, v]) => `${k}: ${typeof v === 'string' ? v.slice(0, 80) : JSON.stringify(v).slice(0, 80)}`)
        .join(', ')
        .slice(0, 120)
    : ''

  return (
    <div
      className="my-1.5 overflow-hidden rounded-md border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.6)]"
      data-tool-call-id={toolCallId}
    >
      {/* Header */}
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[length:var(--text-sm)] transition-colors hover:bg-[var(--surface-float)]"
      >
        {expanded ? (
          <ChevronDown className="size-3.5 text-[var(--text-dim)]" />
        ) : (
          <ChevronRight className="size-3.5 text-[var(--text-dim)]" />
        )}
        <StatusIcon status={status} />
        <span className="font-medium text-[var(--text-secondary)]">{toolName}</span>
        {inputSummary && (
          <span className="truncate text-[var(--text-dim)]">{inputSummary}</span>
        )}
      </button>

      {/* Terminal body */}
      {expanded && content && (
        <div className="h-48 border-t border-[var(--border-subtle)]">
          <TerminalEmulator
            ref={termRef}
            onData={noopOnData}
            className="h-full w-full"
          />
        </div>
      )}
    </div>
  )
}
```

**Step 2: Commit**

```bash
git add apps/web/components/pulse/tool-call-terminal.tsx
git commit -m "feat(web): add ToolCallTerminal component for inline tool output"
```

---

### Task 5: Render ToolCallTerminal in Chat Message List

**Files:**
- Modify: Chat message rendering component (wherever `PulseMessageBlock` of type `tool_use` is rendered)
- Find: `apps/web/components/pulse/` — the message bubble or block renderer

**Step 1: Find the message block renderer**

Search for where `tool_use` blocks are rendered in the chat UI. Look for the component that maps over `blocks` or `toolUses` from `ChatMessage`.

**Step 2: Replace plain tool_use rendering with ToolCallTerminal**

Where tool_use blocks currently render as text badges, replace with:

```tsx
import { ToolCallTerminal } from '@/components/pulse/tool-call-terminal'

// Inside the block renderer:
case 'tool_use':
  return (
    <ToolCallTerminal
      key={`tool-${block.toolCallId ?? idx}`}
      toolName={block.name}
      toolCallId={block.toolCallId ?? `tool-${idx}`}
      input={block.input}
      content={block.content}
      status={block.status}
    />
  )
```

**Step 3: Commit**

```bash
git add apps/web/components/pulse/
git commit -m "feat(web): render tool calls as inline terminals in Pulse chat"
```

---

### Task 6: Wire Rust tool_use_update Events to Frontend Stream

**Files:**
- Modify: `crates/web/execute/sync_mode.rs` — Map `tool_use` ACP bridge events with content to `tool_use_update` stream events
- Modify: `apps/web/lib/pulse/chat-api.ts` — Handle `tool_use_update` in the stream parser

**Step 1: Check how ACP events flow to the NDJSON stream**

The `pulse_chat` mode uses WS (not NDJSON). Events flow:
1. `acp.rs` emits `ServiceEvent::AcpBridge` with `AcpBridgeEvent::SessionUpdate`
2. `sync_mode.rs:dispatch_acp_event` serializes and sends JSON over WS
3. Frontend receives via `use-ws-messages.ts`

The frontend Pulse chat currently uses the `/api/pulse/chat` NDJSON route (not WS) for the Claude CLI subprocess path. But for ACP mode, it uses WS.

Check both paths and ensure `tool_use_update` events reach the handler:
- WS path: already works — `dispatch_acp_event` forwards all ACP bridge events as JSON
- NDJSON path: needs the stream parser to recognize `tool_use_update`

**Step 2: Update `chat-api.ts` stream event handler**

In `apps/web/lib/pulse/chat-api.ts`, ensure the `ChatStreamEvent` type union includes `tool_use_update`:

```typescript
export type ChatStreamEvent =
  | { type: 'status'; phase: string }
  | { type: 'assistant_delta'; delta: string }
  | { type: 'thinking_content'; content: string }
  | { type: 'tool_use'; tool: PulseToolUse }
  | { type: 'tool_use_update'; toolCallId: string; status?: string; content?: string; toolName?: string }
  | { type: 'config_options_update'; configOptions: AcpConfigOption[] }
  | { type: 'heartbeat'; elapsed_ms: number }
  | { type: 'done'; response: PulseChatResponse }
  | { type: 'error'; error: string; code?: string }
```

**Step 3: Commit**

```bash
git add apps/web/lib/pulse/chat-api.ts crates/web/execute/sync_mode.rs
git commit -m "feat: wire tool_use_update events through WS and NDJSON stream paths"
```

---

### Task 7: Process Exit Monitoring

**Files:**
- Modify: `crates/services/acp.rs` — Monitor child process exit in `run_prompt_turn` and `run_session_probe`

**Step 1: Add process exit monitoring to `run_prompt_turn`**

After spawning the adapter and starting the IO task, add a process exit watcher that emits an error event if the adapter crashes before the prompt completes:

```rust
// In run_prompt_turn, after spawning stderr reader and io_task:
let exit_tx = tx.clone();
let child_id = child.id();
tokio::task::spawn_local(async move {
    let exit_status = child.wait().await;
    match exit_status {
        Ok(status) if !status.success() => {
            emit(
                &exit_tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: format!(
                        "ACP adapter process exited unexpectedly: pid={} status={}",
                        child_id.unwrap_or(0),
                        status
                    ),
                },
            );
        }
        Err(err) => {
            emit(
                &exit_tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: format!("ACP adapter process wait failed: {err}"),
                },
            );
        }
        Ok(_) => {} // Clean exit handled by the prompt response path
    }
});
```

**Important:** Remove the `child.kill().await` and `child.wait().await` at the end of `run_prompt_turn` since the exit watcher now owns the child. Instead, the child is consumed by the `spawn_local` task. This requires restructuring: separate the `Child` handle so the exit watcher gets ownership of the wait, and the cleanup path can still kill if needed.

Better approach: use `tokio::select!` between the prompt future and the child exit:

```rust
// Replace the linear prompt flow with a select between prompt completion and child death
tokio::select! {
    prompt_result = conn.prompt(PromptRequest::new(session_id.clone(), prompt_blocks)) => {
        let prompt_response = prompt_result.map_err(|err| err.to_string())?;
        // ... existing stop_reason handling and TurnResult emit ...
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    exit_status = child.wait() => {
        let status_str = match &exit_status {
            Ok(s) => format!("{s}"),
            Err(e) => format!("error: {e}"),
        };
        return Err(format!("ACP adapter crashed mid-session: {status_str}"));
    }
}
```

**Note:** This requires careful handling because `child.wait()` takes `&mut self` — the child needs to be wrapped in a structure that allows concurrent access, or the wait future needs to be prepared before the prompt call. Use `child.wait()` in a `spawn_local` and communicate via a oneshot channel.

**Step 2: Apply same pattern to `run_session_probe`**

Same exit monitoring pattern.

**Step 3: Commit**

```bash
git add crates/services/acp.rs
git commit -m "feat(acp): monitor adapter process exit for crash detection"
```

---

### Task 8: Integration Smoke Test

**Step 1: Verify cargo builds clean**

```bash
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

**Step 2: Verify web builds clean**

```bash
cd apps/web && pnpm build
```

**Step 3: Manual smoke test**

1. `just dev` — start full stack
2. Open Pulse chat, send a prompt that triggers tool calls
3. Verify tool calls render as collapsible terminal cards with:
   - Tool name in header
   - Spinning status while running
   - Content streamed into xterm.js
   - Checkmark on completion
   - Collapsible/expandable

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: ACP tool call terminal rendering — complete integration"
```

---

## Summary

| Task | Description | Effort |
|------|-------------|--------|
| 1 | Enrich tool call wire format with content/input | Medium |
| 2 | Frontend types for tool call content | Small |
| 3 | Handle tool_use_update in stream event handler | Medium |
| 4 | Create ToolCallTerminal component | Medium |
| 5 | Render ToolCallTerminal in chat message list | Small |
| 6 | Wire events through WS and NDJSON paths | Small |
| 7 | Process exit monitoring | Medium |
| 8 | Integration smoke test | Small |
