# ACP Dynamic Config Options (Model Selection) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Surface ACP agent-advertised config options (models, modes, thought levels) in the Pulse UI so users can select different models per agent instead of seeing hardcoded Claude-only options.

**Architecture:** The ACP protocol's `configOptions` mechanism lets agents advertise available selectors (model, mode, thought_level) during session setup. We capture these from `NewSessionResponse`/`LoadSessionResponse`, emit them as `ServiceEvent`s through the WS bridge, and render them dynamically in the frontend. For the `claude` agent (which uses Claude CLI, not ACP), we keep the existing hardcoded model list as a fallback.

**Tech Stack:** Rust (agent-client-protocol 0.9.5, serde_json, tokio mpsc), TypeScript (Next.js, React, Zod)

---

## Background

### Current State
- Model dropdown in `apps/web/components/omnibox/omnibox-input-bar.tsx:364-367` shows hardcoded `Claude` / `Codex` agent options and `Sonnet` / `Opus` / `Haiku` model options
- `PulseModel = z.enum(['sonnet', 'opus', 'haiku'])` in `apps/web/lib/pulse/types.ts:31`
- The `model` field is sent to `/api/pulse/chat` but **never forwarded** as a WS flag to the Rust backend
- On the Rust side, ACP adapter model is baked into `AXON_ACP_CODEX_ADAPTER_ARGS` env var

### ACP Protocol Support
- `NewSessionResponse.config_options: Option<Vec<SessionConfigOption>>` — agent advertises available config selectors
- Each `SessionConfigOption` has: `id`, `name`, `description`, `category` (mode/model/thought_level), `kind: Select { current_value, options }`
- Client can call `session/set_config_option { sessionId, configId, value }` to change a setting
- Agent responds with full updated config state
- The Rust SDK already has `Client::set_session_config_option()` available

### Key Files

**Rust:**
- `crates/services/types.rs` — ACP event types (`AcpBridgeEvent`, `AcpPromptTurnRequest`, etc.)
- `crates/services/events.rs` — `ServiceEvent` enum and `emit()` helper
- `crates/services/acp.rs` — ACP scaffold, `run_prompt_turn()`, session setup
- `crates/web/execute/sync_mode.rs` — WS mode dispatch, `PulseChatAgent`, flag extraction
- `crates/web/execute/events.rs` — `acp_bridge_event_payload()` serializes events for WS

**Frontend:**
- `apps/web/lib/pulse/types.ts` — `PulseModel`, `PulseAgent`, `PulseChatRequestSchema`
- `apps/web/lib/pulse/chat-stream.ts` — `PulseChatStreamEvent` types, NDJSON parsing
- `apps/web/app/api/pulse/chat/route.ts` — Pulse chat API route, WS flag construction, event handling
- `apps/web/app/settings/settings-data.ts` — `MODEL_OPTIONS` hardcoded array
- `apps/web/hooks/use-pulse-chat.ts` — Chat hook, streams events to UI
- `apps/web/hooks/use-ws-messages.ts` — `pulseModel` state, localStorage persistence
- `apps/web/hooks/use-pulse-workspace.ts` — Workspace behavior hook
- `apps/web/components/omnibox/omnibox-input-bar.tsx` — Agent/model/permission dropdowns

---

### Task 1: Add `AcpConfigOption` types to Rust services layer

**Files:**
- Modify: `crates/services/types.rs`

**Step 1: Add the new types**

Add after `AcpPromptTurnRequest` (line 87):

```rust
/// A single selectable value within an ACP config option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpConfigSelectValue {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

/// An ACP session config option (model selector, mode selector, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpConfigOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub current_value: String,
    pub options: Vec<AcpConfigSelectValue>,
}
```

**Step 2: Add `ConfigOptionsUpdate` variant to `AcpBridgeEvent`**

In the `AcpBridgeEvent` enum (line 126), add a new variant:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpBridgeEvent {
    SessionUpdate(AcpSessionUpdateEvent),
    PermissionRequest(AcpPermissionRequestEvent),
    TurnResult(AcpTurnResultEvent),
    ConfigOptionsUpdate(Vec<AcpConfigOption>),
}
```

**Step 3: Add `model` field to `AcpPromptTurnRequest`**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpPromptTurnRequest {
    pub session_id: Option<String>,
    pub prompt: Vec<String>,
    /// Model config option value to set after session setup (if agent supports it).
    pub model: Option<String>,
}
```

**Step 4: Run `cargo check` to verify**

Run: `cargo check 2>&1 | head -40`
Expected: Compile errors in `acp.rs` and `sync_mode.rs` where `AcpPromptTurnRequest` is constructed without the new `model` field. This is expected — we fix those in subsequent tasks.

**Step 5: Commit**

```bash
git add crates/services/types.rs
git commit -m "feat(acp): add AcpConfigOption types and model field to prompt request"
```

---

### Task 2: Map ACP SDK config options to service types in `acp.rs`

**Files:**
- Modify: `crates/services/acp.rs`

**Step 1: Add mapping function**

Add after `map_permission_request_event()` (around line 208):

```rust
use agent_client_protocol::{SessionConfigOption as SdkConfigOption, SessionConfigKind, SessionConfigSelectOptions, SessionConfigOptionCategory};

/// Convert ACP SDK config options into our service-layer representation.
pub fn map_config_options(options: &[SdkConfigOption]) -> Vec<AcpConfigOption> {
    options
        .iter()
        .filter_map(|opt| {
            let SessionConfigKind::Select(ref select) = opt.kind;
            let values = match &select.options {
                SessionConfigSelectOptions::Ungrouped(opts) => opts
                    .iter()
                    .map(|o| AcpConfigSelectValue {
                        value: o.value.0.clone(),
                        name: o.name.clone(),
                        description: o.description.clone(),
                    })
                    .collect(),
                SessionConfigSelectOptions::Grouped(groups) => groups
                    .iter()
                    .flat_map(|g| &g.options)
                    .map(|o| AcpConfigSelectValue {
                        value: o.value.0.clone(),
                        name: o.name.clone(),
                        description: o.description.clone(),
                    })
                    .collect(),
            };
            let category = opt.category.as_ref().map(|c| match c {
                SessionConfigOptionCategory::Mode => "mode".to_string(),
                SessionConfigOptionCategory::Model => "model".to_string(),
                SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
                SessionConfigOptionCategory::Other(s) => s.clone(),
            });
            Some(AcpConfigOption {
                id: opt.id.0.clone(),
                name: opt.name.clone(),
                description: opt.description.clone(),
                category,
                current_value: select.current_value.0.clone(),
                options: values,
            })
        })
        .collect()
}
```

**Step 2: Add the import for `AcpConfigOption`**

At the top of `acp.rs`, add `AcpConfigOption` to the imports from `types`:

```rust
use crate::crates::services::types::{
    AcpAdapterCommand, AcpBridgeEvent, AcpConfigOption, AcpPermissionRequestEvent,
    AcpPromptTurnRequest, AcpSessionUpdateEvent, AcpSessionUpdateKind, AcpTurnResultEvent,
};
```

Also add the SDK imports at the top:

```rust
use agent_client_protocol::{
    Agent, Client, ClientSideConnection, ContentBlock, InitializeRequest, LoadSessionRequest,
    NewSessionRequest, PromptRequest, ProtocolVersion, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
    SessionNotification, SessionUpdate, StopReason,
    SetSessionConfigOptionRequest,
};
use agent_client_protocol_schema::{
    SessionConfigKind, SessionConfigSelectOptions, SessionConfigOptionCategory,
};
```

Note: `SessionConfigKind`, `SessionConfigSelectOptions`, and `SessionConfigOptionCategory` are in the schema crate which is re-exported. Check if they're accessible from `agent_client_protocol` directly or need `agent_client_protocol_schema`. Run `cargo check` to confirm.

**Step 3: Run `cargo check`**

Run: `cargo check 2>&1 | head -30`
Expected: Still errors for missing `model` field in `AcpPromptTurnRequest` literals. The new mapping function should compile cleanly.

**Step 4: Commit**

```bash
git add crates/services/acp.rs
git commit -m "feat(acp): add map_config_options to convert SDK types to service types"
```

---

### Task 3: Capture config options from session response and support model setting

**Files:**
- Modify: `crates/services/acp.rs` — the `run_prompt_turn()` function

**Step 1: Capture config options from `new_session` response**

In `run_prompt_turn()`, the current code (around line 367-395) discards the session response's `config_options`. Change the `new_session` branch to capture them:

```rust
let (session_id, initial_config_options) = match session_setup {
    AcpSessionSetupRequest::New(new_session) => {
        emit(
            &tx,
            ServiceEvent::Log {
                level: "info".to_string(),
                message: "ACP runtime: creating new session".to_string(),
            },
        );
        let response = conn.new_session(new_session)
            .await
            .map_err(|err| err.to_string())?;
        (response.session_id, response.config_options)
    }
    AcpSessionSetupRequest::Load(load_session) => {
        emit(
            &tx,
            ServiceEvent::Log {
                level: "info".to_string(),
                message: "ACP runtime: loading existing session".to_string(),
            },
        );
        let session_id = load_session.session_id.clone();
        let response = conn.load_session(load_session)
            .await
            .map_err(|err| err.to_string())?;
        (session_id, response.config_options)
    }
};
```

**Step 2: Emit config options as a service event**

Right after capturing the session_id and config_options, emit them:

```rust
if let Some(ref config_options) = initial_config_options {
    let mapped = map_config_options(config_options);
    if !mapped.is_empty() {
        emit(
            &tx,
            ServiceEvent::AcpBridge {
                event: AcpBridgeEvent::ConfigOptionsUpdate(mapped),
            },
        );
    }
}
```

**Step 3: Set model config option if requested**

After emitting config options and before sending the prompt, check if the request includes a model preference and the agent supports it:

```rust
// If the caller requested a specific model and the agent advertises a model config option,
// call set_session_config_option before the prompt.
if let Some(ref requested_model) = req.model {
    if let Some(ref config_options) = initial_config_options {
        let has_model_option = config_options.iter().any(|opt| {
            opt.category.as_ref().is_some_and(|c| matches!(c, SessionConfigOptionCategory::Model))
        });
        if has_model_option {
            let model_config_id = config_options
                .iter()
                .find(|opt| opt.category.as_ref().is_some_and(|c| matches!(c, SessionConfigOptionCategory::Model)))
                .map(|opt| opt.id.0.clone());
            if let Some(config_id) = model_config_id {
                emit(
                    &tx,
                    ServiceEvent::Log {
                        level: "info".to_string(),
                        message: format!("ACP runtime: setting model to {requested_model}"),
                    },
                );
                let set_response = conn
                    .set_session_config_option(SetSessionConfigOptionRequest::new(
                        session_id.clone(),
                        config_id,
                        requested_model.clone(),
                    ))
                    .await
                    .map_err(|err| format!("failed to set ACP model config: {err}"))?;
                // Emit updated config options
                let updated = map_config_options(&set_response.config_options);
                if !updated.is_empty() {
                    emit(
                        &tx,
                        ServiceEvent::AcpBridge {
                            event: AcpBridgeEvent::ConfigOptionsUpdate(updated),
                        },
                    );
                }
            }
        }
    }
}
```

**Step 4: Handle `ConfigOptionUpdate` in session notifications**

In `map_session_notification()`, the `ConfigOptionUpdate` variant from streaming notifications should also emit config updates. However the `SessionUpdate::ConfigOptionUpdate` variant from the ACP SDK contains the updated config options. For now, the kind mapping in `map_session_update_kind()` already maps it to `AcpSessionUpdateKind::ConfigOptionUpdate`. A follow-up can extract the full config state from the notification if needed.

**Step 5: Fix `AcpPromptTurnRequest` construction sites**

Fix `sync_mode.rs` line 658-661:

```rust
let req = AcpPromptTurnRequest {
    session_id,
    prompt: vec![input],
    model: None, // TODO: Task 4 wires this from WS flags
};
```

**Step 6: Run `cargo check`**

Run: `cargo check 2>&1 | head -30`
Expected: Clean compile (or minor fixups for import paths).

**Step 7: Commit**

```bash
git add crates/services/acp.rs crates/web/execute/sync_mode.rs
git commit -m "feat(acp): capture config options from session response, support model setting"
```

---

### Task 4: Wire model flag through WS bridge and serialize config option events

**Files:**
- Modify: `crates/web/execute/sync_mode.rs`
- Modify: `crates/web/execute/events.rs`

**Step 1: Extract `model` from WS flags in `sync_mode.rs`**

In the `DirectParams` struct (around line 62), add a `model` field:

```rust
pub(super) struct DirectParams {
    mode: ServiceMode,
    input: String,
    cfg: Arc<Config>,
    limit: usize,
    offset: usize,
    max_points: Option<usize>,
    agent: PulseChatAgent,
    session_id: Option<String>,
    model: Option<String>,
}
```

In `extract_params()` (the function that builds `DirectParams`), extract the model:

```rust
let model = flags
    .get("model")
    .and_then(serde_json::Value::as_str)
    .map(ToString::to_string);
```

Add it to the `Some(DirectParams { ... })` return.

**Step 2: Pass model to `AcpPromptTurnRequest` in the `PulseChat` dispatch arm**

In the `ServiceMode::PulseChat` match arm (line 654+):

```rust
let req = AcpPromptTurnRequest {
    session_id,
    prompt: vec![input],
    model,
};
```

**Step 3: Add `ConfigOptionsUpdate` serialization in `events.rs`**

In `acp_bridge_event_payload()` (line 128), add the new variant:

```rust
AcpBridgeEvent::ConfigOptionsUpdate(options) => {
    let serialized_options: Vec<serde_json::Value> = options
        .iter()
        .map(|opt| {
            serde_json::json!({
                "id": opt.id,
                "name": opt.name,
                "description": opt.description,
                "category": opt.category,
                "currentValue": opt.current_value,
                "options": opt.options.iter().map(|v| serde_json::json!({
                    "value": v.value,
                    "name": v.name,
                    "description": v.description,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "type": "config_options_update",
        "configOptions": serialized_options,
    })
}
```

**Step 4: Run `cargo check && cargo test`**

Run: `cargo check && cargo test web -- --nocapture 2>&1 | tail -20`
Expected: Clean compile. Existing tests pass. The `extract_params` tests may need the new `model` field added to assertions.

**Step 5: Fix failing tests**

In `sync_mode.rs` tests, add `model: None` to any `DirectParams` assertions and `assert_eq!(params.model, None)` where appropriate. Add a new test:

```rust
#[test]
fn extract_params_reads_model_for_pulse_chat() {
    let base = Config::default();
    let context = ExecCommandContext {
        exec_id: "test".to_string(),
        mode: "pulse_chat".to_string(),
        input: "hello".to_string(),
        cfg: Arc::new(base),
    };
    let flags = serde_json::json!({"agent": "codex", "model": "o3"});
    let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
    assert_eq!(params.agent, PulseChatAgent::Codex);
    assert_eq!(params.model.as_deref(), Some("o3"));
}
```

**Step 6: Commit**

```bash
git add crates/web/execute/sync_mode.rs crates/web/execute/events.rs
git commit -m "feat(acp): wire model flag through WS bridge, serialize config option events"
```

---

### Task 5: Forward model flag from `/api/pulse/chat` route to WS

**Files:**
- Modify: `apps/web/app/api/pulse/chat/route.ts`

**Step 1: Add model to WS flags**

At line 447-451 in route.ts, the `wsFlags` construction currently sends only `session_id` and `agent`. Add `model`:

```typescript
const wsFlags: Record<string, string | boolean> = {}
if (req.sessionId) {
  wsFlags.session_id = req.sessionId
}
wsFlags.agent = req.agent
wsFlags.model = req.model
```

**Step 2: Handle `config_options_update` events in `handlePulsePayload`**

In the `switch (type)` block (around line 377), add a new case:

```typescript
case 'config_options_update': {
  const configOptions = data.configOptions
  if (Array.isArray(configOptions)) {
    emit({ type: 'config_options_update', configOptions } as PulseChatStreamEventPayload)
  }
  return
}
```

**Step 3: Run `pnpm build` to check types**

Run: `cd apps/web && pnpm build 2>&1 | tail -20`
Expected: Type error because `config_options_update` is not in `PulseChatStreamEventPayload`. Fix in Task 6.

**Step 4: Commit**

```bash
git add apps/web/app/api/pulse/chat/route.ts
git commit -m "feat(pulse): forward model flag and config_options_update events to stream"
```

---

### Task 6: Add config option types to frontend

**Files:**
- Modify: `apps/web/lib/pulse/types.ts`
- Modify: `apps/web/lib/pulse/chat-stream.ts`

**Step 1: Add ACP config option types in `types.ts`**

After `PulseAgent` (line 34):

```typescript
export const AcpConfigSelectValue = z.object({
  value: z.string(),
  name: z.string(),
  description: z.string().optional(),
})
export type AcpConfigSelectValue = z.infer<typeof AcpConfigSelectValue>

export const AcpConfigOption = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().optional(),
  category: z.string().optional(),
  currentValue: z.string(),
  options: z.array(AcpConfigSelectValue),
})
export type AcpConfigOption = z.infer<typeof AcpConfigOption>
```

**Step 2: Widen `PulseModel` to accept freeform strings**

Currently `PulseModel = z.enum(['sonnet', 'opus', 'haiku'])`. This needs to accept arbitrary model IDs from ACP agents. Change to:

```typescript
export const PulseModel = z.string().default('sonnet')
export type PulseModel = z.infer<typeof PulseModel>
```

This is a breaking change — all code that does `['sonnet', 'opus', 'haiku'].includes(m)` validation needs to be relaxed. The hardcoded list becomes a **fallback** for when the agent doesn't advertise config options.

**Step 3: Add `config_options_update` to `PulseChatStreamEventPayload`**

In `chat-stream.ts`, add to the union:

```typescript
| { type: 'config_options_update'; configOptions: AcpConfigOption[] }
```

And add the import:

```typescript
import type { AcpConfigOption, PulseChatResponse, PulseToolUse } from '@/lib/pulse/types'
```

**Step 4: Fix `PulseModel` validation in `use-ws-messages.ts`**

At line 222, change:

```typescript
if (m && ['sonnet', 'opus', 'haiku'].includes(m)) setPulseModel(m)
```

to:

```typescript
if (m && typeof m === 'string' && m.length > 0) setPulseModel(m)
```

**Step 5: Fix `workspace-persistence.ts` model validation**

At line 77, change:

```typescript
parsed.agent === 'codex' || parsed.agent === 'claude' ? parsed.agent : 'claude'
```

Keep this as-is (agent validation is fine). But the model validation (if any) should accept freeform strings.

**Step 6: Run `pnpm build`**

Run: `cd apps/web && pnpm build 2>&1 | tail -30`
Expected: Type errors where `PulseModel` was used as a literal union. Fix all call sites.

**Step 7: Commit**

```bash
git add apps/web/lib/pulse/types.ts apps/web/lib/pulse/chat-stream.ts apps/web/hooks/use-ws-messages.ts apps/web/lib/pulse/workspace-persistence.ts
git commit -m "feat(pulse): add AcpConfigOption types, widen PulseModel to accept freeform strings"
```

---

### Task 7: Add `acpConfigOptions` state and handle config_options_update in chat hook

**Files:**
- Modify: `apps/web/hooks/use-pulse-chat.ts`

**Step 1: Add state for ACP config options**

In `usePulseChat()`, add state:

```typescript
const [acpConfigOptions, setAcpConfigOptions] = useState<AcpConfigOption[]>([])
```

Import `AcpConfigOption` from types.

**Step 2: Handle `config_options_update` events in `onEvent`**

In the `onEvent` callback inside `handlePrompt` (around line 313), add:

```typescript
if (event.type === 'config_options_update' && event.configOptions) {
  setAcpConfigOptions(event.configOptions)
  return
}
```

**Step 3: Return `acpConfigOptions` from the hook**

Add to the return object:

```typescript
return {
  // ...existing
  acpConfigOptions,
  setAcpConfigOptions,
}
```

**Step 4: Run `pnpm build`**

Run: `cd apps/web && pnpm build 2>&1 | tail -20`
Expected: Clean or minor type fixups.

**Step 5: Commit**

```bash
git add apps/web/hooks/use-pulse-chat.ts
git commit -m "feat(pulse): track ACP config options in chat hook"
```

---

### Task 8: Surface ACP config options in the omnibox model dropdown

**Files:**
- Modify: `apps/web/hooks/use-pulse-workspace.ts` — thread `acpConfigOptions` through
- Modify: `apps/web/components/omnibox/omnibox-input-bar.tsx` — dynamic model dropdown
- Modify: `apps/web/app/settings/settings-data.ts` — export `CLAUDE_MODEL_OPTIONS` constant

**Step 1: Thread `acpConfigOptions` through workspace hook**

In `use-pulse-workspace.ts`, destructure `acpConfigOptions` from the `chat` object and include it in the return.

**Step 2: Define model option lists**

In `settings-data.ts`, rename `MODEL_OPTIONS` to `CLAUDE_MODEL_OPTIONS` (keeping the same data). Export a helper:

```typescript
export const CLAUDE_MODEL_IDS = ['sonnet', 'opus', 'haiku'] as const

export const CLAUDE_MODEL_OPTIONS: { id: string; label: string; sub: string; badge?: string }[] = [
  { id: 'sonnet', label: 'Claude Sonnet 4.6', sub: 'Balanced intelligence and speed', badge: 'Default' },
  { id: 'opus', label: 'Claude Opus 4.6', sub: 'Most capable — best for complex tasks' },
  { id: 'haiku', label: 'Claude Haiku 4.5', sub: 'Fastest response — most efficient' },
]
```

**Step 3: Make the omnibox model dropdown agent-aware**

In `omnibox-input-bar.tsx`, compute the model options dynamically:

```tsx
const modelOptions = useMemo(() => {
  if (pulseAgent === 'claude') {
    return CLAUDE_MODEL_OPTIONS.map((o) => ({ value: o.id, label: o.label }))
  }
  // For ACP agents, use config options with category "model"
  const modelConfig = acpConfigOptions?.find((o) => o.category === 'model')
  if (modelConfig) {
    return modelConfig.options.map((o) => ({ value: o.value, label: o.name }))
  }
  // Fallback: show generic "Default" option
  return [{ value: 'default', label: 'Default' }]
}, [pulseAgent, acpConfigOptions])
```

Replace the hardcoded `<select>` options with:

```tsx
<select
  id="omnibox-pulse-model-selector"
  name="omnibox_pulse_model_selector"
  value={pulseModel}
  onChange={(e) => setPulseModel(e.target.value)}
  className="..."
  aria-label="Model selector"
>
  {modelOptions.map((opt) => (
    <option key={opt.value} value={opt.value}>
      {opt.label}
    </option>
  ))}
</select>
```

**Step 4: Reset model when agent changes**

When the user switches agents, the model should reset to the first available option for the new agent. In the `setPulseAgent` handler (or via a `useEffect`), detect agent change and set the model:

```typescript
useEffect(() => {
  if (pulseAgent === 'claude' && !CLAUDE_MODEL_IDS.includes(pulseModel as any)) {
    setPulseModel('sonnet')
  }
}, [pulseAgent])
```

This prevents stale codex model IDs from persisting when switching back to Claude.

**Step 5: Pass `acpConfigOptions` prop through component chain**

Thread the prop from `omnibox-component-impl.tsx` → `omnibox-input-bar.tsx`. Add it to the props interface of `OmniboxInputBar`.

**Step 6: Run `pnpm build && pnpm lint`**

Run: `cd apps/web && pnpm build && pnpm lint 2>&1 | tail -20`

**Step 7: Commit**

```bash
git add apps/web/hooks/use-pulse-workspace.ts apps/web/components/omnibox/ apps/web/app/settings/settings-data.ts
git commit -m "feat(pulse): dynamic model dropdown from ACP config options"
```

---

### Task 9: Update settings page and keyboard shortcuts for dynamic models

**Files:**
- Modify: `apps/web/app/settings/settings-sections.tsx` — use dynamic model list
- Modify: `apps/web/hooks/use-pulse-workspace.ts` — keyboard shortcuts

**Step 1: Update settings model selector**

In `settings-sections.tsx`, the model selector at line 56 uses `CLAUDE_MODEL_OPTIONS`. Make it agent-aware similar to the omnibox — show ACP config options when available, fallback to Claude options.

**Step 2: Update keyboard shortcuts**

In `use-pulse-workspace.ts` line 305, the `Alt+1/2/3` shortcuts map to hardcoded `['sonnet', 'opus', 'haiku']`. Make them context-aware:

```typescript
if (pulseAgent === 'claude') {
  const modelByIndex: string[] = ['sonnet', 'opus', 'haiku']
  setPulseModel(modelByIndex[Number(key) - 1] ?? 'sonnet')
} else {
  // For ACP agents, map to the first 3 model options from config
  const modelConfig = acpConfigOptions?.find((o) => o.category === 'model')
  if (modelConfig && modelConfig.options.length > 0) {
    const idx = Number(key) - 1
    const opt = modelConfig.options[idx]
    if (opt) setPulseModel(opt.value)
  }
}
```

**Step 3: Commit**

```bash
git add apps/web/app/settings/settings-sections.tsx apps/web/hooks/use-pulse-workspace.ts
git commit -m "feat(pulse): agent-aware model selector in settings and keyboard shortcuts"
```

---

### Task 10: Update tests

**Files:**
- Modify: `apps/web/__tests__/workspace-persistence.test.ts`
- Modify: `apps/web/__tests__/pulse-types.test.ts`
- Modify: `apps/web/__tests__/pulse-chat-route-streaming.test.ts`

**Step 1: Fix workspace-persistence tests**

Tests that assert `agent: 'claude'` should still pass. Tests that validate model values need to accept freeform strings now instead of checking enum membership.

**Step 2: Fix pulse-types tests**

If `PulseModel` was tested as an enum, update to test it as `z.string()`.

**Step 3: Fix pulse-chat-route-streaming tests**

The test at line 314 (`passes agent flag to pulse_chat WS mode`) should also verify model is forwarded:

```typescript
it('passes model flag to pulse_chat WS mode', async () => {
  // ... setup
  await post(makeRequest({ prompt: 'hello', agent: 'codex', model: 'o3' }))
  // ... verify
  expect(wsOptions.flags?.model).toBe('o3')
})
```

**Step 4: Run full test suite**

Run: `cd apps/web && pnpm test 2>&1 | tail -30`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add apps/web/__tests__/
git commit -m "test(pulse): update tests for dynamic model selection"
```

---

### Task 11: Rebuild and verify end-to-end

**Step 1: Rebuild Rust backend**

Run: `docker compose build axon-workers 2>&1 | tail -10`

**Step 2: Restart services**

Run: `docker compose up -d axon-workers axon-web`

**Step 3: Test with codex agent**

Use the test script pattern from `/tmp/test-acp.js` to verify:
1. Send `pulse_chat` with `agent: "codex"` — verify `config_options_update` event arrives with available models
2. Send `pulse_chat` with `agent: "codex"` and `model: "o3"` — verify the model is set before the prompt

**Step 4: Test with claude agent**

Verify the Claude path still works with hardcoded sonnet/opus/haiku — no regressions.

**Step 5: Verify frontend**

1. Open Pulse UI
2. Select "Codex" agent
3. Verify model dropdown populates with codex-advertised models (after first chat message establishes a session)
4. Select "Claude" agent
5. Verify model dropdown shows Sonnet/Opus/Haiku

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(acp): end-to-end dynamic model selection via ACP config options"
```

---

## Notes

### Known Limitation: Config Options Arrive After First Message
ACP config options come from the `session/new` response, which happens during the first prompt turn. This means the model dropdown for ACP agents will initially show "Default" until the first message is sent. After that, the agent's available models populate the dropdown. This is acceptable for v1 — a follow-up could do a "probe" session setup without a prompt to discover options eagerly.

### Claude CLI Path Unchanged
The `claude` agent uses Claude CLI (not ACP), so it never sends `config_options_update`. The hardcoded Sonnet/Opus/Haiku options remain the fallback for `pulseAgent === 'claude'`.

### `PulseModel` Type Widening
Changing `PulseModel` from `z.enum(...)` to `z.string()` is the most impactful change — it touches persistence, validation, and type safety throughout the frontend. Every file that references `PulseModel` as a union type needs review. The test fixups in Task 10 should catch all breakages.
