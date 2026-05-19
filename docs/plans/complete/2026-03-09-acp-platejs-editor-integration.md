# ACP→PlateJS Editor Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give ACP agents (Claude, Codex, Gemini) the ability to write content directly into the PlateJS editor in AxonShell via structured `<axon:editor>` tags in their responses.

**Architecture:** Agents wrap editor content in `<axon:editor op="replace|append">` XML tags (consistent with how PlateJS AI integration works with structured output). The Rust ACP layer parses accumulated assistant text after each turn and emits `editor_update` WebSocket events. The frontend receives these events and updates the PlateJS editor. No MCP tool needed — the XML tag mechanism is the complete solution.

**Tech Stack:** Rust (ServiceEvent, pulse_chat drive loop), TypeScript (ws-protocol, use-axon-acp, axon-shell), PlateJS v52, WebSocket protocol

---

## ROOT CAUSE ANALYSIS

Five gaps all required to close. Close them in order — each builds on the previous.

| Gap | Location | Description |
|-----|----------|-------------|
| 1 | `apps/web/lib/ws-protocol.ts` | No `editor_update` WS server message type |
| 2 | `apps/web/hooks/use-axon-acp.ts` | No `onEditorUpdate` callback, no `editor_update` case in switch |
| 3 | `apps/web/components/reboot/axon-shell.tsx` | `setEditorMarkdown` not wired to `useAxonAcp` |
| 4 | `crates/services/events.rs` | `ServiceEvent` has no `EditorWrite` variant |
| 5 | `crates/web/execute/sync_mode/pulse_chat.rs` | `drive_turn_events` never parses agent output for editor markers |

There is also no system prompt instruction for agents — covered in Task 7.

---

## Phase 1 — Frontend Foundation (TypeScript only, no Rust needed yet)

### Task 1: Add `editor_update` to WebSocket protocol

**Files:**
- Modify: `apps/web/lib/ws-protocol.ts`

The `WsServerMsg` union type on line 10 needs a new member. Add it after the `error` variant.

**Step 1: Write the failing test**

Create `apps/web/__tests__/ws-protocol.test.ts` (if it doesn't exist):

```typescript
import { describe, it, expect } from 'vitest'
import type { WsServerMsg } from '@/lib/ws-protocol'

describe('ws-protocol editor_update', () => {
  it('editor_update message type is assignable to WsServerMsg', () => {
    const msg: WsServerMsg = {
      type: 'editor_update',
      content: '# README\n\nHello world',
      operation: 'replace',
    }
    expect(msg.type).toBe('editor_update')
    expect(msg.content).toBe('# README\n\nHello world')
    expect(msg.operation).toBe('replace')
  })

  it('editor_update with append operation', () => {
    const msg: WsServerMsg = {
      type: 'editor_update',
      content: '\n## Additional section',
      operation: 'append',
    }
    expect(msg.operation).toBe('append')
  })
})
```

**Step 2: Run test to verify it fails**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm test -- ws-protocol
```
Expected: FAIL — TypeScript error "Type '{ type: 'editor_update'... }' is not assignable to type 'WsServerMsg'"

**Step 3: Add the type to `ws-protocol.ts`**

In `apps/web/lib/ws-protocol.ts`, locate the `WsServerMsg` union (line 10–44). Add after the `error` variant on line 44, before the closing semicolon:

```typescript
  | { type: 'editor_update'; content: string; operation: 'replace' | 'append' }
```

The final lines of the `WsServerMsg` type should look like:
```typescript
  | { type: 'result'; session_id?: string; result?: string; [key: string]: unknown }
  | { type: 'error'; message?: string; [key: string]: unknown }
  | { type: 'editor_update'; content: string; operation: 'replace' | 'append' }
```

**Step 4: Run test to verify it passes**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm test -- ws-protocol
```
Expected: PASS

**Step 5: Type-check the whole project**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm exec tsc --noEmit
```
Expected: 0 errors

**Step 6: Commit**

```bash
git add apps/web/lib/ws-protocol.ts apps/web/__tests__/ws-protocol.test.ts
git commit -m "feat(web): add editor_update WS message type to protocol"
```

---

### Task 2: Handle `editor_update` in `use-axon-acp.ts`

**Files:**
- Modify: `apps/web/hooks/use-axon-acp.ts`

The hook needs an `onEditorUpdate` callback option and a new `case 'editor_update':` branch in the subscribe switch.

**Step 1: Write the failing test**

The existing test file is `apps/web/__tests__/use-axon-acp.test.ts` (create if absent):

```typescript
import { describe, it, expect, vi } from 'vitest'

// Lightweight mock — test the handler logic in isolation
describe('use-axon-acp editor_update handling', () => {
  it('calls onEditorUpdate when editor_update event arrives', () => {
    const onEditorUpdate = vi.fn()

    // Simulate the switch-case logic directly
    function handleMsg(
      msg: Record<string, unknown>,
      handlers: { onEditorUpdate?: (content: string, op: string) => void },
    ) {
      switch (msg.type) {
        case 'editor_update': {
          const content = (msg.content as string) ?? ''
          const operation = (msg.operation as string) ?? 'replace'
          handlers.onEditorUpdate?.(content, operation)
          break
        }
      }
    }

    handleMsg(
      { type: 'editor_update', content: '# README', operation: 'replace' },
      { onEditorUpdate },
    )

    expect(onEditorUpdate).toHaveBeenCalledWith('# README', 'replace')
    expect(onEditorUpdate).toHaveBeenCalledTimes(1)
  })

  it('defaults operation to replace when missing', () => {
    const onEditorUpdate = vi.fn()

    function handleMsg(
      msg: Record<string, unknown>,
      handlers: { onEditorUpdate?: (content: string, op: string) => void },
    ) {
      switch (msg.type) {
        case 'editor_update': {
          const content = (msg.content as string) ?? ''
          const operation = (msg.operation as string) ?? 'replace'
          handlers.onEditorUpdate?.(content, operation)
          break
        }
      }
    }

    handleMsg({ type: 'editor_update', content: '# Hello' }, { onEditorUpdate })
    expect(onEditorUpdate).toHaveBeenCalledWith('# Hello', 'replace')
  })
})
```

**Step 2: Run test to verify it passes** (logic test — passes even before hook changes)

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm test -- use-axon-acp
```

**Step 3: Add `onEditorUpdate` to hook interface and handler**

In `apps/web/hooks/use-axon-acp.ts`:

**Add to `UseAxonAcpOptions` interface** (after `onTurnComplete?`):
```typescript
  onEditorUpdate?: (content: string, operation: 'replace' | 'append') => void
```

**Destructure the new option** in the function signature (after `onTurnComplete`):
```typescript
  onEditorUpdate,
```

**Add handler in the subscribe `switch`** (after the `case 'error':` block, before the closing `}`):
```typescript
        case 'editor_update': {
          const content = (msg.content as string) ?? ''
          const operation = ((msg.operation as string) ?? 'replace') as 'replace' | 'append'
          onEditorUpdate?.(content, operation)
          break
        }
```

**Add `onEditorUpdate` to the deps array** of the `useEffect` subscribe block (after `onTurnComplete`):
```typescript
  }, [subscribe, onMessagesChange, onSessionIdChange, onSessionFallback, onTurnComplete, onEditorUpdate])
```

**Step 4: Verify type-check**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm exec tsc --noEmit
```

**Step 5: Commit**

```bash
git add apps/web/hooks/use-axon-acp.ts apps/web/__tests__/use-axon-acp.test.ts
git commit -m "feat(web): add onEditorUpdate callback to useAxonAcp hook"
```

---

### Task 3: Wire `AxonShell` to update editor on agent writes

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx`

The `useAxonAcp` call in `AxonShell` needs to pass `onEditorUpdate`. When the agent writes to the editor, also open the editor pane if it's closed.

**Step 1: Identify the exact change location**

In `axon-shell.tsx`, lines 190–197:
```typescript
  const { submitPrompt, isStreaming, connected } = useAxonAcp({
    activeSessionId,
    agent: pulseAgent ?? 'claude',
    onSessionIdChange,
    onSessionFallback: undefined,
    onMessagesChange,
    onTurnComplete,
  })
```

**Step 2: Add `onEditorUpdate` handler and wire it**

First, add the handler (add after `onTurnComplete` definition near line 185):
```typescript
  const onEditorUpdate = useCallback(
    (content: string, operation: 'replace' | 'append') => {
      setEditorMarkdown((prev) => (operation === 'append' ? prev + '\n' + content : content))
      // Ensure the editor pane is visible when the agent writes to it
      persistEditorOpen(true)
    },
    [persistEditorOpen],
  )
```

Then update the `useAxonAcp` call to include `onEditorUpdate`:
```typescript
  const { submitPrompt, isStreaming, connected } = useAxonAcp({
    activeSessionId,
    agent: pulseAgent ?? 'claude',
    onSessionIdChange,
    onSessionFallback: undefined,
    onMessagesChange,
    onTurnComplete,
    onEditorUpdate,
  })
```

**Note:** `persistEditorOpen` is already defined in the file (line ~456). `onEditorUpdate` depends on it, so it must be defined AFTER `persistEditorOpen`. Check that `onEditorUpdate` is placed after line 456 in the file.

**Step 3: Verify type-check**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm exec tsc --noEmit
```

**Step 4: Lint**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm lint
```

**Step 5: Commit**

```bash
git add apps/web/components/reboot/axon-shell.tsx
git commit -m "feat(web): wire onEditorUpdate to setEditorMarkdown in AxonShell"
```

---

## Phase 2 — Rust Backend: Structured Output Parsing

The frontend is now ready to receive `editor_update` events. Now we teach the Rust backend to emit them by parsing agent output for `<axon:editor>` tags.

### Agent Output Format (what agents must produce)

The agent outputs a special XML block anywhere in its response:

```xml
<axon:editor op="replace">
# README

This is a placeholder README file.

## Installation

```bash
npm install
```

## Usage

Run the app with `npm start`.
</axon:editor>
```

Rules:
- `op` attribute is optional, defaults to `replace`
- Valid values: `replace` (overwrites entire editor), `append` (adds to end)
- Content between tags is markdown
- Multiple `<axon:editor>` blocks are processed in order
- The block is stripped from the chat display so users don't see raw XML

---

### Task 4: Add `EditorWrite` variant to `ServiceEvent`

**Files:**
- Modify: `crates/services/events.rs`

**Step 1: Write the failing test**

In `crates/services/events.rs`, find the `#[cfg(test)] mod tests {` block. Add:

```rust
    #[test]
    fn editor_write_variant_is_cloneable() {
        let event = ServiceEvent::EditorWrite {
            content: "# README".to_string(),
            operation: "replace".to_string(),
        };
        let cloned = event.clone();
        assert_eq!(cloned, event);
    }

    #[test]
    fn editor_write_roundtrip_fields() {
        let content = "# Hello\n\nWorld".to_string();
        let operation = "append".to_string();
        match ServiceEvent::EditorWrite {
            content: content.clone(),
            operation: operation.clone(),
        } {
            ServiceEvent::EditorWrite { content: c, operation: o } => {
                assert_eq!(c, content);
                assert_eq!(o, operation);
            }
            _ => panic!("wrong variant"),
        }
    }
```

**Step 2: Run test to verify it fails**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -p axon_cli services::events -- --nocapture 2>&1 | tail -20
```
Expected: FAIL — `EditorWrite` is not a variant of `ServiceEvent`

**Step 3: Add `EditorWrite` variant**

In `crates/services/events.rs`, the enum currently (line 39–43):
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceEvent {
    Log { level: LogLevel, message: String },
    AcpBridge { event: AcpBridgeEvent },
}
```

Change to:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceEvent {
    Log { level: LogLevel, message: String },
    AcpBridge { event: AcpBridgeEvent },
    /// Editor content update to push to the frontend via WebSocket.
    EditorWrite { content: String, operation: String },
}
```

**Step 4: Fix exhaustive matches**

Rust will now report compile errors wherever `ServiceEvent` is matched without handling `EditorWrite`. Find all match sites:

```bash
cd /home/jmagar/workspace/axon_rust
cargo check 2>&1 | grep "non-exhaustive"
```

The main match site is `dispatch_acp_event` in `crates/web/execute/sync_mode/pulse_chat.rs`. Add a handler arm (covered in Task 6). For now, add a stub arm to unblock compilation:

In `dispatch_acp_event`:
```rust
        ServiceEvent::EditorWrite { .. } => {
            // Handled in Task 6 — placeholder to satisfy exhaustive match
        }
```

**Step 5: Run test**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test events -- --nocapture 2>&1 | tail -10
```
Expected: PASS

**Step 6: Commit**

```bash
git add crates/services/events.rs
git commit -m "feat(services): add EditorWrite variant to ServiceEvent"
```

---

### Task 5: Parse agent output for `<axon:editor>` tags in `pulse_chat.rs`

**Files:**
- Modify: `crates/web/execute/sync_mode/pulse_chat.rs`

After `result` event is received, inspect `AcpRuntimeState.assistant_text` for `<axon:editor>` blocks and emit `ServiceEvent::EditorWrite` for each one found.

**Important:** `assistant_text` is already accumulated in `AcpRuntimeState` (in `bridge.rs`, capped at 1 MiB). We just need to read it after the turn completes.

**Step 1: Write the test**

Add unit tests in `crates/web/execute/sync_mode/pulse_chat.rs` (or create `crates/web/execute/sync_mode/editor_parse.rs` for the pure function):

```rust
/// Extract (content, operation) pairs from text containing <axon:editor> blocks.
/// Returns Vec<(content, operation)>.
pub(super) fn parse_editor_blocks(text: &str) -> Vec<(String, String)> {
    // Implementation in Step 3
    todo!()
}

#[cfg(test)]
mod editor_parse_tests {
    use super::parse_editor_blocks;

    #[test]
    fn no_blocks_returns_empty() {
        assert!(parse_editor_blocks("no blocks here").is_empty());
    }

    #[test]
    fn single_replace_block() {
        let text = r#"Here is your README:

<axon:editor op="replace">
# README

Hello world
</axon:editor>

Done!"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "replace");
        assert!(blocks[0].0.contains("# README"));
        assert!(blocks[0].0.contains("Hello world"));
    }

    #[test]
    fn default_operation_is_replace() {
        let text = "<axon:editor>content here</axon:editor>";
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "replace");
    }

    #[test]
    fn append_operation() {
        let text = r#"<axon:editor op="append">## New Section</axon:editor>"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks[0].1, "append");
    }

    #[test]
    fn multiple_blocks_in_order() {
        let text = r#"
<axon:editor op="replace"># First</axon:editor>
<axon:editor op="append">## Second</axon:editor>
"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].1, "replace");
        assert_eq!(blocks[1].1, "append");
    }

    #[test]
    fn strips_leading_trailing_whitespace_from_content() {
        let text = "<axon:editor op=\"replace\">\n  # Title  \n</axon:editor>";
        let blocks = parse_editor_blocks(text);
        // Content should be trimmed
        assert!(!blocks[0].0.starts_with('\n'));
    }

    #[test]
    fn invalid_op_defaults_to_replace() {
        let text = r#"<axon:editor op="invalid">content</axon:editor>"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks[0].1, "replace");
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test editor_parse -- --nocapture 2>&1 | tail -20
```
Expected: FAIL — `todo!()` panics

**Step 3: Implement `parse_editor_blocks`**

Add this function to `pulse_chat.rs` (or `editor_parse.rs` — but keep it in `pulse_chat.rs` to avoid creating a new file unless needed for line count):

```rust
/// Parse `<axon:editor op="...">content</axon:editor>` blocks from text.
///
/// Returns a vec of `(content, operation)` pairs in document order.
/// `operation` is `"replace"` or `"append"` — invalid values default to `"replace"`.
/// Content is trimmed of leading/trailing whitespace.
pub(super) fn parse_editor_blocks(text: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut search_from = 0;

    while let Some(open_start) = text[search_from..].find("<axon:editor") {
        let abs_open = search_from + open_start;
        // Find the `>` that closes the opening tag
        let Some(tag_end_rel) = text[abs_open..].find('>') else { break };
        let abs_tag_end = abs_open + tag_end_rel + 1; // position after `>`

        // Extract operation attribute from the opening tag
        let opening_tag = &text[abs_open..abs_tag_end];
        let operation = if let Some(op_start) = opening_tag.find(r#"op=""#) {
            let after_quote = op_start + 4; // skip `op="`
            if let Some(op_end) = opening_tag[after_quote..].find('"') {
                let op_val = &opening_tag[after_quote..after_quote + op_end];
                match op_val {
                    "append" => "append",
                    _ => "replace",
                }
            } else {
                "replace"
            }
        } else {
            "replace"
        };

        // Find the closing tag
        let Some(close_start_rel) = text[abs_tag_end..].find("</axon:editor>") else { break };
        let abs_close = abs_tag_end + close_start_rel;
        let abs_close_end = abs_close + "</axon:editor>".len();

        let content = text[abs_tag_end..abs_close].trim().to_string();
        if !content.is_empty() {
            results.push((content, operation.to_string()));
        }

        search_from = abs_close_end;
    }

    results
}
```

**Step 4: Run tests to verify they pass**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test editor_parse -- --nocapture 2>&1 | tail -20
```
Expected: All PASS

**Step 5: Emit `EditorWrite` events after turn completes**

In `drive_turn_events`, after receiving the `result` event (the `result = &mut result_rx` arm), the `AcpRuntimeState` isn't directly accessible here. But `handle_pulse_chat` owns the `conn_handle` which owns the `AcpRuntimeState` via the `AcpBridgeClient`.

The cleanest approach: after `drive_turn_events` returns, read the accumulated `assistant_text` in `handle_pulse_chat` and emit editor events. But `assistant_text` is private inside `AcpRuntimeState`.

Look at the current state: `AcpRuntimeState.assistant_text` is `pub(super)` accessible from within `crates/services/acp`. We need to expose a method to read it from `handle_pulse_chat`.

**Add a public read method to `AcpRuntimeState`** in `crates/services/acp/bridge.rs`:

```rust
impl AcpRuntimeState {
    /// Return accumulated assistant text for this session (capped at 1 MiB).
    pub fn take_assistant_text(&self) -> String {
        self.assistant_text.borrow().clone()
    }
}
```

Then expose `runtime_state` or the text through `AcpConnectionHandle`. Check `crates/services/acp/runtime.rs` for how `AcpConnectionHandle` is structured and what it exposes.

**Step 6: Wire the emission in `handle_pulse_chat`**

After the `drive_turn_events` call in `handle_pulse_chat`:

```rust
    let result = drive_turn_events(result_rx, event_rx, tx.clone(), ws_ctx.clone()).await;

    // After turn completes: parse accumulated text for editor writes.
    // Access assistant_text via the connection handle's runtime state.
    // (Implementation depends on how AcpConnectionHandle exposes runtime_state)
    // TODO: Task 6 implements dispatch_acp_event handler; this reads + emits.
    if let Ok(text) = &result {
        let _ = text; // suppress unused warning until Task 6 is done
        // emit_editor_writes_from_text(&conn_handle, &tx, &ws_ctx).await;
    }

    result
```

**NOTE:** Complete the wiring in Task 6 after `dispatch_acp_event` is updated.

**Step 7: Run all tests**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -- --nocapture 2>&1 | tail -20
```
Expected: all passing

**Step 8: Commit**

```bash
git add crates/web/execute/sync_mode/pulse_chat.rs crates/services/acp/bridge.rs
git commit -m "feat(acp): parse <axon:editor> blocks from agent output after turn"
```

---

### Task 6: `dispatch_acp_event` — forward `EditorWrite` to WebSocket

**Files:**
- Modify: `crates/web/execute/sync_mode/pulse_chat.rs`
- Modify: `crates/services/acp/runtime.rs` (to expose runtime_state read access)

**Step 1: Replace the stub `EditorWrite` arm in `dispatch_acp_event`**

In `dispatch_acp_event`, replace the placeholder from Task 4:

```rust
        ServiceEvent::EditorWrite { content, operation } => {
            send_json_owned(
                tx.clone(),
                ws_ctx.clone(),
                serde_json::json!({
                    "type": "editor_update",
                    "content": content,
                    "operation": operation,
                }),
            )
            .await;
        }
```

**Step 2: Complete the wiring in `handle_pulse_chat`**

After `drive_turn_events` returns, emit editor writes from accumulated text. This requires reading `assistant_text` from the connection handle.

First, check what `AcpConnectionHandle` exposes in `crates/services/acp/runtime.rs`. Look for how `runtime_state` is stored and accessed. If `AcpConnectionHandle` doesn't expose `assistant_text`, add a helper:

In `crates/services/acp/runtime.rs` (or wherever `AcpConnectionHandle` is defined), add:

```rust
impl AcpConnectionHandle {
    /// Return the accumulated assistant text from the most recent session.
    pub fn take_assistant_text(&self) -> Option<String> {
        // Access via bridge client's runtime_state
        // (exact implementation depends on how bridge_client is stored)
        self.bridge_client.runtime_state.assistant_text.borrow().clone().into()
    }
}
```

Then in `handle_pulse_chat`, after `drive_turn_events`:

```rust
    let run_result = drive_turn_events(result_rx, event_rx, tx.clone(), ws_ctx.clone()).await;

    // Parse accumulated assistant text for <axon:editor> blocks.
    let assistant_text = conn_handle.take_assistant_text().unwrap_or_default();
    for (content, operation) in parse_editor_blocks(&assistant_text) {
        let event = crate::crates::services::events::ServiceEvent::EditorWrite { content, operation };
        dispatch_acp_event(event, &tx, &ws_ctx).await;
    }

    run_result
```

**Step 3: Write integration test**

Add to `crates/web/execute/sync_mode/pulse_chat.rs` tests:

```rust
    #[test]
    fn parse_editor_blocks_emits_correct_event_data() {
        // Verify that blocks from agent text produce the right JSON shape
        let blocks = parse_editor_blocks(
            r#"I'll create that now:
<axon:editor op="replace">
# README

Hello world.
</axon:editor>

Done!"#
        );
        assert_eq!(blocks.len(), 1);
        let (content, op) = &blocks[0];
        assert_eq!(op, "replace");
        assert!(content.contains("# README"));
        assert!(content.contains("Hello world."));
        // Verify the block is NOT present at unexpected positions
        assert!(!content.contains("I'll create that now:"));
        assert!(!content.contains("Done!"));
    }
```

**Step 4: Run all Rust tests**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -- --nocapture 2>&1 | tail -30
```
Expected: all passing

**Step 5: Clippy clean**

```bash
cd /home/jmagar/workspace/axon_rust
cargo clippy 2>&1 | grep -E "^error" | head -20
```
Expected: 0 errors

**Step 6: Check monolith policy**

```bash
cd /home/jmagar/workspace/axon_rust
python3 scripts/enforce_monoliths.py 2>&1 | tail -20
```
Expected: no violations (if `pulse_chat.rs` grows past 500 lines, split `parse_editor_blocks` into `pulse_chat/editor_parse.rs`)

**Step 7: Commit**

```bash
git add crates/web/execute/sync_mode/pulse_chat.rs crates/services/acp/runtime.rs
git commit -m "feat(acp): emit editor_update WS events from <axon:editor> blocks"
```

---

## Phase 3 — Agent Instructions (System Prompt)

### Task 7: Add `<axon:editor>` syntax to agent system context

**Files:**
- Investigate: `crates/services/acp/session.rs` — find `NewSessionRequest` setup
- Investigate: `crates/services/acp/runtime.rs` — find where system prompt is set
- Modify: whichever file configures the `system_prompt` field on `NewSessionRequest`

**Step 1: Find where `NewSessionRequest` is constructed**

```bash
cd /home/jmagar/workspace/axon_rust
grep -rn "NewSessionRequest" crates/services/acp/ --include="*.rs"
```

**Step 2: Check what fields `NewSessionRequest` has**

```bash
cd /home/jmagar/workspace/axon_rust
grep -rn "pub struct NewSessionRequest" ~/.cargo/registry/ --include="*.rs" 2>/dev/null | head -5
# If not found:
cargo doc --open -p agent_client_protocol 2>/dev/null
```

**Step 3: Add editor instruction to system prompt**

If `NewSessionRequest` has a `system_prompt` field, add the editor instruction:

```rust
const EDITOR_TOOL_INSTRUCTION: &str = r#"
## Editor Integration

You have access to the Axon Pulse Editor. To write content directly into the editor, wrap your markdown in `<axon:editor>` tags:

```
<axon:editor op="replace">
# Your content here

Write any markdown content.
</axon:editor>
```

- `op="replace"` (default): replaces entire editor content
- `op="append"`: appends to existing content

Use this whenever the user asks you to write, create, or update a document in the editor.
"#;
```

Append this to the existing system prompt, or set it as the system prompt if none exists.

**Step 4: Verify the instruction is passed to the agent**

Manual smoke test: start the dev server, open AxonShell, type "create a README in the editor". Verify:
1. Agent responds with `<axon:editor>` tags in its response
2. Editor pane updates with the content
3. Editor pane auto-opens if it was closed

**Step 5: Commit**

```bash
git add crates/services/acp/
git commit -m "feat(acp): add editor write instruction to agent system context"
```

---

## Phase 4 — MCP Tool (Explicit Agent Editor API)

This phase adds a proper `editor:write` MCP tool so agents can call it explicitly via tool use rather than structured output. This is the "clean" path for future extensibility.

### Task 8: Add `editor:write` to MCP schema

**Files:**
- Modify: `crates/mcp/schema.rs`
- Modify: `docs/MCP-TOOL-SCHEMA.md`

**Step 1: Check current MCP schema structure**

```bash
cd /home/jmagar/workspace/axon_rust
grep -n "\"editor\"" crates/mcp/schema.rs | head -10
grep -n "\"ingest\"" crates/mcp/schema.rs | head -5  # use as reference for adding new action
```

**Step 2: Write the failing test**

Find the MCP schema validation tests in `crates/mcp/schema.rs` or `crates/mcp/` tests. Add:

```rust
    #[test]
    fn editor_write_action_is_valid() {
        let params = serde_json::json!({
            "action": "editor",
            "subaction": "write",
            "content": "# README\n\nHello world",
            "operation": "replace"
        });
        // validate_params should accept editor:write without error
        assert!(validate_axon_params(&params).is_ok());
    }
```

**Step 3: Add the `editor` action to the schema**

In `crates/mcp/schema.rs`, add the `editor` action with `write` subaction. Follow the pattern of existing actions (e.g., `ingest`).

The action schema:
```json
{
  "action": "editor",
  "subaction": "write",  // required
  "content": "string",   // required: markdown content to write
  "operation": "replace | append"  // optional, default "replace"
}
```

**Step 4: Add MCP handler for `editor:write`**

In `crates/mcp/server.rs` (or wherever action routing lives), add:

```rust
        ("editor", Some("write")) => {
            handle_editor_write(params, session_id).await
        }
```

The handler:
```rust
async fn handle_editor_write(
    params: &AxonParams,
    _session_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let content = params.content.as_deref().unwrap_or("").to_string();
    let operation = params.operation.as_deref().unwrap_or("replace").to_string();

    // Store in the turn-scoped pending store (Task 9)
    crate::crates::web::execute::sync_mode::editor_registry::set_pending(content.clone(), operation.clone());

    Ok(serde_json::json!({
        "success": true,
        "message": format!("Content ({} chars, op={}) queued for editor", content.len(), operation)
    }))
}
```

**Step 5: Run tests**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test mcp -- --nocapture 2>&1 | tail -20
```

**Step 6: Commit**

```bash
git add crates/mcp/schema.rs crates/mcp/server.rs docs/MCP-TOOL-SCHEMA.md docs/MCP.md
git commit -m "feat(mcp): add editor:write action to MCP schema and handler"
```

---

### Task 9: Turn-scoped editor write registry

**Files:**
- Create: `crates/web/execute/sync_mode/editor_registry.rs`
- Modify: `crates/web/execute/sync_mode/pulse_chat.rs`

This provides the bridge between the MCP `editor:write` handler and the `pulse_chat` WS session. Since Axon is single-user homelab software, a simple global pending store is sufficient.

**Step 1: Write the failing test**

Create `crates/web/execute/sync_mode/editor_registry.rs`:

```rust
//! Turn-scoped pending store for MCP `editor:write` → WebSocket routing.
//!
//! Single-user homelab assumption: one active `pulse_chat` turn at a time.
//! The MCP handler writes here; `handle_pulse_chat` drains here after the turn.

use std::sync::OnceLock;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
struct PendingEditorWrite {
    writes: Vec<(String, String)>, // (content, operation)
}

static PENDING: OnceLock<Mutex<PendingEditorWrite>> = OnceLock::new();

fn pending() -> &'static Mutex<PendingEditorWrite> {
    PENDING.get_or_init(|| Mutex::new(PendingEditorWrite::default()))
}

/// Store a pending editor write (called by MCP editor:write handler).
pub async fn set_pending(content: String, operation: String) {
    pending().lock().await.writes.push((content, operation));
}

/// Drain all pending writes (called by `handle_pulse_chat` after turn completes).
pub async fn drain_pending() -> Vec<(String, String)> {
    let mut store = pending().lock().await;
    std::mem::take(&mut store.writes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_and_drain_pending() {
        set_pending("# README".to_string(), "replace".to_string()).await;
        let writes = drain_pending().await;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, "# README");
        assert_eq!(writes[0].1, "replace");
    }

    #[tokio::test]
    async fn drain_clears_state() {
        set_pending("content".to_string(), "append".to_string()).await;
        drain_pending().await;
        let second_drain = drain_pending().await;
        assert!(second_drain.is_empty());
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test editor_registry -- --nocapture 2>&1 | tail -20
```
Expected: FAIL — module not found

**Step 3: Declare module in sync_mode**

In `crates/web/execute/sync_mode.rs` (or wherever sync_mode submodules are declared), add:
```rust
pub(crate) mod editor_registry;
```

**Step 4: Run test again**

```bash
cargo test editor_registry -- --nocapture 2>&1 | tail -20
```
Expected: PASS

**Step 5: Drain pending MCP writes in `handle_pulse_chat`**

In `handle_pulse_chat`, merge the two sources of editor writes (structured output + MCP tool calls):

```rust
    let run_result = drive_turn_events(result_rx, event_rx, tx.clone(), ws_ctx.clone()).await;

    // Source 1: <axon:editor> tags parsed from assembled assistant text
    let assistant_text = conn_handle.take_assistant_text().unwrap_or_default();
    let mut all_writes: Vec<(String, String)> = parse_editor_blocks(&assistant_text);

    // Source 2: MCP editor:write tool calls made during this turn
    let mut mcp_writes = editor_registry::drain_pending().await;
    all_writes.append(&mut mcp_writes);

    // Emit all writes as editor_update WS events
    for (content, operation) in all_writes {
        let event = crate::crates::services::events::ServiceEvent::EditorWrite { content, operation };
        dispatch_acp_event(event, &tx, &ws_ctx).await;
    }

    run_result
```

**Step 6: Run all tests**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -- --nocapture 2>&1 | tail -30
```
Expected: all passing

**Step 7: Check monolith policy**

```bash
python3 scripts/enforce_monoliths.py 2>&1 | tail -10
```

**Step 8: Commit**

```bash
git add crates/web/execute/sync_mode/editor_registry.rs crates/web/execute/sync_mode/pulse_chat.rs
git commit -m "feat(acp): add editor registry + drain MCP editor:write in pulse_chat"
```

---

## Phase 5 — Integration Verification

### Task 10: End-to-end smoke test

**Step 1: Build the project**

```bash
cd /home/jmagar/workspace/axon_rust
cargo build --bin axon 2>&1 | tail -10
```
Expected: clean build

**Step 2: Run full test suite**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test 2>&1 | tail -20
```
Expected: all passing

**Step 3: Run frontend tests**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm test 2>&1 | tail -20
```
Expected: all passing

**Step 4: Run `just verify` (pre-PR gate)**

```bash
cd /home/jmagar/workspace/axon_rust
just verify 2>&1 | tail -30
```
Expected: fmt-check + clippy + check + test all pass

**Step 5: Manual integration smoke test**

1. Start infra: `docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-chrome`
2. Start the serve mode: `cargo run --bin axon -- serve`
3. Start the frontend: `cd apps/web && pnpm dev`
4. Open `http://localhost:49010/reboot`
5. In the ACP chat, type: **"Create a fake README for a Rust CLI project and put it in the editor"**
6. Expected behavior:
   - Agent streams its response in chat
   - Response includes `<axon:editor op="replace">` block
   - After turn completes, editor pane auto-opens
   - Editor contains the README content (rendered as rich text by PlateJS)
   - Chat does NOT show raw `<axon:editor>` XML (strip from display — see Bonus Task)

**Step 6: Final commit + push**

```bash
git add .
git commit -m "chore: mark ACP editor integration complete"
```

---

## Bonus Task: Strip `<axon:editor>` from chat display

**Optional but improves UX.** Users shouldn't see raw XML in the chat messages.

**File:** `apps/web/components/reboot/axon-message-list.tsx` (or wherever message content is rendered)

**Approach:** When rendering an `assistant` message, strip `<axon:editor...>...</axon:editor>` blocks from the display content before rendering. Replace with a small inline badge like `✏ Sent to editor`.

```typescript
function stripEditorBlocks(content: string): { cleaned: string; editorWrites: number } {
  const editorTagRe = /<axon:editor(?:[^>]*)>([\s\S]*?)<\/axon:editor>/g
  let editorWrites = 0
  const cleaned = content.replace(editorTagRe, () => {
    editorWrites++
    return ''
  }).trim()
  return { cleaned, editorWrites }
}
```

Then in the message renderer:
```typescript
const { cleaned, editorWrites } = stripEditorBlocks(message.content)
// Render `cleaned` as the message text
// If editorWrites > 0, show: <span>✏ Sent to editor</span>
```

---

## Testing Checklist

Before claiming this feature complete, verify ALL of the following:

- [ ] `pnpm test` passes in `apps/web`
- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy` reports 0 errors
- [ ] `cargo fmt --check` is clean
- [ ] `just verify` passes
- [ ] Monolith policy clean (all `.rs` files ≤ 500 lines)
- [ ] Manual: agent can create a document in the editor via `<axon:editor>` tags
- [ ] Manual: editor opens automatically when agent writes to it
- [ ] Manual: append operation adds content rather than replacing
- [ ] Manual: chat does not show raw XML (bonus task)
- [ ] Manual: Claude, Codex, AND Gemini can all write to the editor

---

## Key File Reference

| What | Where |
|------|-------|
| WS message types | `apps/web/lib/ws-protocol.ts` |
| ACP hook | `apps/web/hooks/use-axon-acp.ts` |
| Shell layout + editor wiring | `apps/web/components/reboot/axon-shell.tsx` |
| ServiceEvent enum | `crates/services/events.rs` |
| ACP runtime state (assistant_text) | `crates/services/acp/bridge.rs` |
| Turn event loop (where to emit) | `crates/web/execute/sync_mode/pulse_chat.rs` |
| Editor write registry | `crates/web/execute/sync_mode/editor_registry.rs` (new) |
| MCP schema | `crates/mcp/schema.rs` |
| MCP handler routing | `crates/mcp/server.rs` |
| PlateJS editor component | `apps/web/components/pulse/pulse-editor-pane.tsx` |
