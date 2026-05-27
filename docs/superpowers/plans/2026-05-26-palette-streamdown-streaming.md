# Palette Streamdown Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render Axon palette markdown with Streamdown and stream `/v1/ask` answer tokens into the desktop palette as they arrive.

**Architecture:** Add completed markdown rendering first, then expose a server-sent events route for `ask`, then bridge that SSE stream through Tauri window events into React state. The palette keeps the existing non-streaming JSON path as fallback, so streaming can fail closed without breaking command execution.

**Tech Stack:** Rust, Axum, Tauri v2, React 19, TypeScript, Streamdown, Aurora design tokens.

---

## File Structure

- Modify `apps/palette-tauri/package.json`: add `streamdown`.
- Modify `apps/palette-tauri/src/App.tsx`: render markdown output with Streamdown, track streaming run state, listen for Tauri stream events, and use streaming for `ask`.
- Modify `apps/palette-tauri/src/styles.css`: add Aurora-styled markdown output rules.
- Modify `apps/palette-tauri/src/lib/format.ts`: expose output display kind helpers.
- Modify `apps/palette-tauri/src/lib/axonClient.ts`: add request builder helpers for streaming without duplicating action body logic.
- Modify `apps/palette-tauri/src-tauri/src/lib.rs`: add `axon_http_stream_request` command that reads SSE and emits palette stream events.
- Modify `src/web/server/handlers/ask.rs`: add `v1_ask_stream` SSE handler.
- Modify `src/web/server/routing.rs`: mount `/v1/ask/stream` beside `/v1/ask`.
- Create `src/web/server/handlers/ask_stream.rs`: keep streaming route code focused and testable.
- Modify `src/vector/ops/commands/ask/output.rs` or adjacent ask streaming module: expose a service-facing token callback API instead of stdout-only streaming.
- Add/update tests near changed code:
  - `apps/palette-tauri/src/lib/format.test.ts` if the app already has test setup; otherwise use TypeScript typecheck as frontend verification.
  - `src/web/server/handlers/ask_stream_tests.rs`
  - `apps/palette-tauri/src-tauri/src/lib.rs` unit tests for SSE parsing helper functions.

## Task 1: Render Completed Markdown With Streamdown

**Files:**
- Modify: `apps/palette-tauri/package.json`
- Modify: `apps/palette-tauri/src/App.tsx`
- Modify: `apps/palette-tauri/src/styles.css`
- Modify: `apps/palette-tauri/src/lib/format.ts`

- [ ] **Step 1: Add dependency**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/palette-tauri
pnpm add streamdown
```

Expected: `package.json` and `pnpm-lock.yaml` include `streamdown`.

- [ ] **Step 2: Add output-kind helper**

In `apps/palette-tauri/src/lib/format.ts`, add:

```ts
export type OutputKind = "markdown" | "code";

export function outputKindFor(subcommand: string): OutputKind {
  switch (subcommand) {
    case "ask":
    case "scrape":
    case "summarize":
    case "research":
    case "suggest":
      return "markdown";
    default:
      return "code";
  }
}
```

- [ ] **Step 3: Render markdown output in the palette**

In `apps/palette-tauri/src/App.tsx`, change the format import:

```ts
import { formatPayload, outputKindFor } from "@/lib/format";
```

Add the Streamdown import:

```ts
import { Streamdown } from "streamdown";
```

Near `const outputText = "text" in run ? run.text : "";`, add:

```ts
const outputKind = active ? outputKindFor(active.subcommand) : "code";
```

Replace the output body block:

```tsx
{"text" in run && (
  outputKind === "markdown" ? (
    <div className="output-body output-markdown">
      <Streamdown>{run.text}</Streamdown>
    </div>
  ) : (
    <pre className="output-body output-code">
      <code>{run.text}</code>
    </pre>
  )
)}
```

- [ ] **Step 4: Style Streamdown output with Aurora tokens**

In `apps/palette-tauri/src/styles.css`, replace the current `.output-body` block with:

```css
.output-body {
  flex: 1;
  min-height: 0;
  margin: 8px 0 0;
  overflow: auto;
  padding: 14px;
  color: var(--aurora-text-primary);
  background: var(--aurora-control-surface);
  border: 1px solid var(--aurora-border-default);
  border-radius: 8px;
}

.output-code {
  font-family: var(--aurora-font-mono);
  font-size: 12px;
  line-height: 1.55;
  white-space: pre-wrap;
  overflow-wrap: break-word;
  word-break: normal;
}

.output-markdown {
  font-size: var(--aurora-type-body-sm);
  line-height: 1.58;
}

.output-markdown :where(h1, h2, h3) {
  margin: 14px 0 8px;
  color: var(--aurora-text-primary);
  font-family: var(--aurora-font-display);
  font-weight: var(--aurora-weight-heading);
  line-height: 1.22;
}

.output-markdown :where(h1) {
  font-size: 18px;
}

.output-markdown :where(h2) {
  font-size: 15px;
}

.output-markdown :where(h3) {
  font-size: 13px;
}

.output-markdown :where(p, ul, ol, blockquote, pre, table) {
  margin: 0 0 10px;
}

.output-markdown :where(a) {
  color: var(--aurora-accent-strong);
  text-decoration: none;
}

.output-markdown :where(a:hover) {
  text-decoration: underline;
}

.output-markdown :where(code) {
  padding: 1px 4px;
  color: var(--aurora-code-function);
  background: color-mix(in srgb, var(--aurora-panel-strong) 82%, transparent);
  border: 1px solid var(--aurora-border-default);
  border-radius: 5px;
  font-family: var(--aurora-font-mono);
  font-size: 0.92em;
}

.output-markdown :where(pre) {
  overflow: auto;
  padding: 12px;
  background: var(--aurora-page-bg);
  border: 1px solid var(--aurora-border-default);
  border-radius: 8px;
}

.output-markdown :where(pre code) {
  padding: 0;
  background: transparent;
  border: 0;
}

.output-markdown :where(blockquote) {
  padding-left: 10px;
  color: var(--aurora-text-muted);
  border-left: 3px solid var(--aurora-accent-primary);
}
```

- [ ] **Step 5: Verify frontend**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/palette-tauri
pnpm typecheck
pnpm vite:build
```

Expected: both commands pass. Manually run `ask`, `scrape`, and `status`: markdown actions render rich output; `status` remains code/preformatted.

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/package.json apps/palette-tauri/pnpm-lock.yaml apps/palette-tauri/src/App.tsx apps/palette-tauri/src/styles.css apps/palette-tauri/src/lib/format.ts
git commit -m "feat(palette): render markdown output with streamdown"
```

## Task 2: Extract Ask Streaming Service API

**Files:**
- Modify: `src/vector/ops/commands/ask/output.rs`
- Modify or create focused helper near existing ask streaming code if needed.

- [ ] **Step 1: Write failing service-level test**

Add a test near the existing ask streaming tests that proves token callbacks are collected without stdout. If there is no clean unit seam, introduce a small pure helper test first:

```rust
#[test]
fn stream_event_accumulator_appends_deltas() {
    let mut answer = String::new();
    append_ask_delta(&mut answer, "Hello");
    append_ask_delta(&mut answer, " world");
    assert_eq!(answer, "Hello world");
}
```

Expected helper signature:

```rust
pub(crate) fn append_ask_delta(answer: &mut String, delta: &str) {
    answer.push_str(delta);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd /home/jmagar/workspace/axon_rust
cargo test append_ask_delta
```

Expected: FAIL because `append_ask_delta` does not exist.

- [ ] **Step 3: Add callback-shaped API**

In the ask output/streaming module, add a service-facing wrapper:

```rust
pub(crate) async fn ask_llm_answer_with_deltas<F>(
    cfg: &Config,
    query: &str,
    context: &str,
    mut on_delta: F,
) -> Result<AskLlmCompletion, Box<dyn Error>>
where
    F: FnMut(&str) + Send,
{
    let client = http_client()?;
    let llm_started = Instant::now();
    let mut answer = String::new();

    let result = ask_llm_streaming_ttft_with_callback(
        cfg,
        client,
        query,
        context,
        |delta| {
            append_ask_delta(&mut answer, delta);
            on_delta(delta);
        },
    )
    .await;

    match result {
        Ok(ttft_at) => Ok(AskLlmCompletion::Streamed {
            answer,
            ttft_at,
            llm_total_ms: llm_started.elapsed().as_millis(),
        }),
        Err(err) => {
            log_warn(&format!(
                "ask: streaming callback failed, falling back to non-streaming: {err}"
            ));
            let answer = ask_llm_non_streaming(cfg, client, query, context).await?;
            Ok(AskLlmCompletion::Fallback {
                answer,
                llm_total_ms: llm_started.elapsed().as_millis(),
            })
        }
    }
}
```

If `ask_llm_streaming_ttft_with_callback` does not exist, extract it from the existing stdout streaming function. Keep the existing CLI function as a wrapper that passes a callback which writes to stdout only when requested.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test append_ask_delta
cargo test ask_streaming
```

Expected: new helper test passes and existing streaming tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/vector/ops/commands/ask
git commit -m "refactor(ask): expose token delta callback"
```

## Task 3: Add `/v1/ask/stream` SSE Route

**Files:**
- Create: `src/web/server/handlers/ask_stream.rs`
- Modify: `src/web/server/handlers.rs` or module declaration file that exports handlers.
- Modify: `src/web/server/routing.rs`
- Test: `src/web/server/handlers/ask_stream_tests.rs`

- [ ] **Step 1: Write failing route test**

Create `src/web/server/handlers/ask_stream_tests.rs`:

```rust
use axum::http::StatusCode;

#[tokio::test]
async fn ask_stream_rejects_empty_query() {
    let response = super::v1_ask_stream_test_response(serde_json::json!({
        "query": ""
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

If a test harness helper does not exist, create a local test helper in `ask_stream.rs` under `#[cfg(test)]` that calls the handler with a default config.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test ask_stream_rejects_empty_query
```

Expected: FAIL because `ask_stream` route/test helper does not exist.

- [ ] **Step 3: Implement SSE event types**

In `src/web/server/handlers/ask_stream.rs`:

```rust
use axum::{
    Extension, Json,
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
};
use futures_util::Stream;
use serde::Serialize;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::core::config::Config;
use crate::services::client_contract::RestAskRequest as AskRequestBody;
use super::super::error::HttpError;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AskStreamEvent {
    Meta { phase: &'static str },
    Delta { text: String },
    Done { answer: String },
    Error { message: String },
}

fn sse_json(event_name: &'static str, value: &AskStreamEvent) -> Event {
    Event::default()
        .event(event_name)
        .json_data(value)
        .unwrap_or_else(|_| Event::default().event("error").data("{\"type\":\"error\",\"message\":\"encode failed\"}"))
}
```

- [ ] **Step 4: Implement handler skeleton**

Add:

```rust
pub async fn v1_ask_stream(
    Extension(cfg): Extension<Arc<Config>>,
    Json(req): Json<AskRequestBody>,
) -> impl IntoResponse {
    use super::super::types::ASK_QUERY_MAX_CHARS;

    if req.query.trim().is_empty() {
        return HttpError::bad_request("query is required").into_response();
    }
    if req.query.chars().count() > ASK_QUERY_MAX_CHARS {
        return HttpError::payload_too_large(format!("query exceeds {ASK_QUERY_MAX_CHARS} chars"))
            .into_response();
    }

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let mut req_cfg = (*cfg).clone();
    super::ask::apply_ask_overrides(&mut req_cfg, &req);
    req_cfg.ask_stream = true;

    tokio::spawn(async move {
        let _ = tx.send(Ok(sse_json("meta", &AskStreamEvent::Meta { phase: "retrieving" }))).await;
        let result = crate::services::query::ask_stream(&req_cfg, &req.query, |delta| {
            let tx = tx.clone();
            let delta = delta.to_string();
            tokio::spawn(async move {
                let _ = tx.send(Ok(sse_json("delta", &AskStreamEvent::Delta { text: delta }))).await;
            });
        }).await;

        match result {
            Ok(answer) => {
                let _ = tx.send(Ok(sse_json("done", &AskStreamEvent::Done { answer }))).await;
            }
            Err(err) => {
                let _ = tx.send(Ok(sse_json("error", &AskStreamEvent::Error { message: err.to_string() }))).await;
            }
        }
    });

    Sse::new(ReceiverStream::new(rx)).into_response()
}
```

Adjust the `query::ask_stream` call to the actual function created in Task 2. Avoid holding non-`Send` errors across `tokio::spawn`; collapse errors to strings inside the task.

- [ ] **Step 5: Mount route**

In `src/web/server/routing.rs`, mount beside `/v1/ask` by merging it into the write routes:

```rust
.route("/v1/ask/stream", post(handlers::ask_stream::v1_ask_stream))
```

Use the same write scope as `/v1/ask`.

- [ ] **Step 6: Run server tests**

Run:

```bash
cargo test ask_stream
cargo test v1_ask_auth_layer
```

Expected: stream route validates input; existing `/v1/ask` auth tests still pass. If auth tests need route inventory updates, add `("POST", "/v1/ask/stream")` to the existing protected route test cases.

- [ ] **Step 7: Commit**

```bash
git add src/web/server/handlers/ask_stream.rs src/web/server/handlers/ask_stream_tests.rs src/web/server/routing.rs src/vector/ops/commands/ask src/services
git commit -m "feat(web): add streaming ask endpoint"
```

## Task 4: Add Tauri SSE Bridge

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing SSE parser tests**

In `apps/palette-tauri/src-tauri/src/lib.rs`, add tests for a pure helper:

```rust
#[cfg(test)]
mod stream_tests {
    use super::parse_sse_data_line;

    #[test]
    fn parses_sse_data_line() {
        assert_eq!(
            parse_sse_data_line("data: {\"type\":\"delta\",\"text\":\"hi\"}"),
            Some("{\"type\":\"delta\",\"text\":\"hi\"}".to_string())
        );
    }

    #[test]
    fn ignores_non_data_sse_line() {
        assert_eq!(parse_sse_data_line("event: delta"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/palette-tauri/src-tauri
cargo test parse_sse_data_line
```

Expected: FAIL because helper does not exist.

- [ ] **Step 3: Add request and event types**

In `apps/palette-tauri/src-tauri/src/lib.rs`, add:

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaletteStreamRequest {
    base_url: String,
    token: Option<String>,
    path: String,
    body: serde_json::Value,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PaletteStreamEvent {
    Started { path: String },
    Delta { text: String },
    Done { answer: Option<String> },
    Error { message: String },
}

fn parse_sse_data_line(line: &str) -> Option<String> {
    line.strip_prefix("data:").map(|value| value.trim().to_string())
}
```

- [ ] **Step 4: Add streaming command**

Add:

```rust
#[tauri::command]
async fn axon_http_stream_request(
    window: tauri::Window,
    request: PaletteStreamRequest,
) -> Result<(), String> {
    use futures_util::StreamExt;

    let url = format!("{}{}", request.base_url.trim_end_matches('/'), request.path);
    window
        .emit("palette://stream", PaletteStreamEvent::Started { path: request.path.clone() })
        .map_err(|err| err.to_string())?;

    let client = reqwest::Client::new();
    let mut builder = client
        .post(url)
        .header("accept", "text/event-stream")
        .json(&request.body);
    if let Some(token) = request.token.as_deref().filter(|token| !token.trim().is_empty()) {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("stream request failed with HTTP {status}: {text}"));
    }

    let mut pending = String::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = pending.find('\n') {
            let line = pending[..pos].trim_end_matches('\r').to_string();
            pending.drain(..=pos);
            handle_palette_sse_line(&window, &line)?;
        }
    }
    if !pending.trim().is_empty() {
        handle_palette_sse_line(&window, pending.trim_end_matches('\r'))?;
    }
    Ok(())
}
```

Add `handle_palette_sse_line`:

```rust
fn handle_palette_sse_line(window: &tauri::Window, line: &str) -> Result<(), String> {
    let Some(data) = parse_sse_data_line(line) else {
        return Ok(());
    };
    let value: serde_json::Value = serde_json::from_str(&data).map_err(|err| err.to_string())?;
    match value.get("type").and_then(|kind| kind.as_str()) {
        Some("delta") => {
            let text = value.get("text").and_then(|text| text.as_str()).unwrap_or_default();
            window.emit("palette://stream", PaletteStreamEvent::Delta { text: text.to_string() })
        }
        Some("done") => {
            let answer = value.get("answer").and_then(|answer| answer.as_str()).map(str::to_string);
            window.emit("palette://stream", PaletteStreamEvent::Done { answer })
        }
        Some("error") => {
            let message = value
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("stream error")
                .to_string();
            window.emit("palette://stream", PaletteStreamEvent::Error { message })
        }
        _ => Ok(()),
    }
    .map_err(|err| err.to_string())
}
```

Register `axon_http_stream_request` in the Tauri invoke handler beside `axon_http_request`.

- [ ] **Step 5: Run Tauri tests**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/palette-tauri/src-tauri
cargo test parse_sse_data_line
cargo check
```

Expected: tests and check pass.

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/lib.rs
git commit -m "feat(palette): bridge streaming HTTP into tauri events"
```

## Task 5: Use Streaming for Palette Ask

**Files:**
- Modify: `apps/palette-tauri/src/lib/axonClient.ts`
- Modify: `apps/palette-tauri/src/App.tsx`

- [ ] **Step 1: Extract request builder**

In `apps/palette-tauri/src/lib/axonClient.ts`, add:

```ts
export interface PaletteHttpRequest {
  baseUrl: string;
  token: string | null;
  method: "GET" | "POST";
  path: string;
  body: Record<string, unknown> | null;
}

export function buildActionRequest(
  client: Client,
  action: PaletteAction,
  arg: string,
  config: PaletteConfig,
): PaletteHttpRequest {
  const body = bodyFor(action, arg, config);
  return {
    baseUrl: client.baseUrl,
    token: tokenFromHeaders(client.headers),
    method: body.method,
    path: body.path,
    body: body.body,
  };
}
```

Refactor the existing `executeAction` switch into a shared `bodyFor` helper so non-streaming and streaming use the same request body.

- [ ] **Step 2: Add stream run state**

In `apps/palette-tauri/src/App.tsx`, extend `RunState`:

```ts
| { kind: "streaming"; title: string; subtitle: string; text: string }
```

Update output helpers:

```ts
function outputTitle(run: RunState): string {
  if (run.kind === "idle") return "Ready";
  if (run.kind === "streaming") return run.title;
  return run.title;
}
```

Update badges/spinners so `streaming` is treated like running:

```ts
if (run.kind === "streaming") return "info";
```

- [ ] **Step 3: Listen for Tauri stream events**

In `App.tsx`, add:

```ts
type PaletteStreamEvent =
  | { type: "started"; path: string }
  | { type: "delta"; text: string }
  | { type: "done"; answer?: string | null }
  | { type: "error"; message: string };
```

Add effect:

```ts
useEffect(() => {
  let disposed = false;
  const unlisten = appWindow.listen<PaletteStreamEvent>("palette://stream", (event) => {
    if (disposed) return;
    const payload = event.payload;
    if (payload.type === "delta") {
      setRun((current) =>
        current.kind === "streaming"
          ? { ...current, text: current.text + payload.text }
          : current,
      );
    } else if (payload.type === "done") {
      setRun((current) =>
        current.kind === "streaming"
          ? {
              kind: "success",
              title: "Ask question completed",
              subtitle: current.subtitle,
              text: payload.answer ?? current.text,
              result: { ok: true, status: 200, path: "/v1/ask/stream", method: "POST", payload: { answer: payload.answer ?? current.text } },
            }
          : current,
      );
    } else if (payload.type === "error") {
      setRun({
        kind: "error",
        title: "Ask question failed",
        subtitle: "/v1/ask/stream",
        text: payload.message,
        result: { ok: false, status: 0, path: "/v1/ask/stream", method: "POST", payload: { error: payload.message } },
      });
    }
  });
  return () => {
    disposed = true;
    void unlisten.then((fn) => fn());
  };
}, []);
```

- [ ] **Step 4: Add streaming submit branch**

In `submit`, before non-streaming `executeAction`, add:

```ts
if (action.subcommand === "ask") {
  const request = buildActionRequest(client, action, argument, config);
  setRun({
    kind: "streaming",
    title: "Streaming Ask question",
    subtitle: `${request.method} /v1/ask/stream`,
    text: "",
  });
  try {
    await invoke("axon_http_stream_request", {
      request: {
        ...request,
        path: "/v1/ask/stream",
        body: request.body,
      },
    });
    return;
  } catch (err) {
    setRun({
      kind: "running",
      title: `Running ${action.label}`,
      subtitle: commandLine,
    });
  }
}
```

Then let the existing non-streaming path run as fallback.

- [ ] **Step 5: Verify frontend typecheck**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/palette-tauri
pnpm typecheck
pnpm vite:build
```

Expected: both pass.

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/src/App.tsx apps/palette-tauri/src/lib/axonClient.ts
git commit -m "feat(palette): stream ask responses into command output"
```

## Task 6: End-to-End Verification on Dookie and Steamy

**Files:**
- No source changes unless defects are found.

- [ ] **Step 1: Start Axon server locally**

Run from repo root:

```bash
./scripts/axon serve
```

Expected: server listens on the configured HTTP port and `/readyz` passes.

- [ ] **Step 2: Smoke test streaming endpoint with curl**

Run:

```bash
TOKEN="$(grep '^AXON_MCP_HTTP_TOKEN=' ~/.axon/.env | cut -d= -f2-)"
curl -N \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"query":"what is axon?","collection":"axon"}' \
  https://axon.tootie.tv/v1/ask/stream
```

Expected: `event: meta`, one or more `event: delta`, then `event: done`.

- [ ] **Step 3: Build palette**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/palette-tauri
pnpm build
```

Expected: Tauri release build succeeds.

- [ ] **Step 4: Copy newest exe to Steamy desktop**

Use the established agent-os/SSH copy path from the previous palette work. Copy the newly built Windows executable directly to the Steamy desktop instead of relying on shortcut updates.

Expected: `C:\Users\jmaga\Desktop\Axon Palette.exe` exists and launches without a terminal window.

- [ ] **Step 5: Manual UI checks**

On Steamy:

1. Launch `Axon Palette.exe`.
2. Run `ask what is axon?`.
3. Confirm text appears incrementally, not only at the end.
4. Confirm Streamdown renders headings/lists/code cleanly.
5. Run `status`.
6. Confirm status still renders as preformatted code.
7. Close/reopen palette.
8. Confirm compact idle size still starts centered and only as tall as the input.

- [ ] **Step 6: Check logs for regressions**

Run on dookie:

```bash
journalctl --since "10 minutes ago" --no-pager -u axon 2>/dev/null | tail -n 100
journalctl --since "10 minutes ago" --no-pager -k | rg -i 'zsh\\[[0-9]+\\]: segfault|oom|panic' || true
```

Expected: no new server panics and no new zsh segfaults during the test window.

- [ ] **Step 7: Commit or amend verification fixes**

If defects were fixed:

```bash
git add <changed-files>
git commit -m "fix(palette): harden streaming ask verification issues"
```

If no defects:

```bash
git status --short
```

Expected: only intentional work remains.

## Self-Review

**Spec coverage:** The plan covers Streamdown completed markdown rendering, true token streaming, server route exposure, Tauri bridge, React state handling, fallback behavior, and Steamy deployment verification.

**Placeholder scan:** No implementation step uses TBD/TODO/later language. Each task includes concrete files, code shape, commands, and expected results.

**Type consistency:** `RunState`, `PaletteStreamEvent`, `PaletteHttpRequest`, `PaletteStreamRequest`, and `AskStreamEvent` names are consistent across tasks.

