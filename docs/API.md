# API Reference
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 16:51:32 | 02/25/2026 EST

## Table of Contents

1. Scope
2. Transport Summary
3. WebSocket API (`/ws`)
4. HTTP API (`apps/web` routes)
5. Error Model
6. Security Constraints
7. Compatibility Notes
8. Source Map

## Scope

This document covers externally consumed interfaces in this repo:

- Axum WebSocket bridge from `crates/web.rs` (`/ws`)
- Axum download/output routes from `crates/web.rs` and `crates/web/download.rs`
- Next.js API routes under `apps/web/app/api/*`

It does not document internal Rust function signatures.

## Transport Summary

| Surface | Path | Producer | Consumer |
|---|---|---|---|
| WebSocket | `/ws` | `crates/web.rs` + `crates/web/execute/*` | `apps/web/hooks/*` |
| WebSocket | `/ws/shell` | `crates/web.rs` + `crates/web/shell.rs` | terminal UI (`apps/web/app/terminal/*`) |
| HTTP GET | `/output/{*path}` | `crates/web.rs` | browser UI |
| HTTP GET | `/download/{job_id}/...` | `crates/web/download.rs` | browser UI |
| HTTP REST | `/api/*` | Next.js route handlers | browser UI |

## WebSocket API (`/ws`)

### Client -> Server Messages

Defined in `apps/web/lib/ws-protocol.ts` as `WsClientMsg`.

| Type | Shape | Description |
|---|---|---|
| `execute` | `{ type, mode, input, flags }` | Run one allowed axon mode via subprocess |
| `cancel` | `{ type, id, mode?, job_id? }` | Cancel async job (legacy id + v2 context fields) |
| `read_file` | `{ type, path }` | Read a generated file from crawl output context |

`mode` is allowlisted by server-side `ALLOWED_MODES` in `crates/web/execute.rs`.

### Server -> Client Messages

Defined in `apps/web/lib/ws-protocol.ts` as `WsServerMsg`.

| Type | Shape | Description |
|---|---|---|
| `log` | `{ line }` | stderr/log line |
| `command.start` | `{ data: { ctx } }` | Command accepted and context established |
| `command.output.json` | `{ data: { ctx, data } }` | structured command payload |
| `command.output.line` | `{ data: { ctx, line } }` | raw command output line |
| `command.done` | `{ data: { ctx, payload: { exit_code, elapsed_ms? } } }` | command completed |
| `command.error` | `{ data: { ctx, payload: { message, elapsed_ms? } } }` | command/request failed |
| `job.status` | `{ data: { ctx, payload: { status, error?, metrics? } } }` | async job status update |
| `job.progress` | `{ data: { ctx, payload: { phase, percent?, processed?, total? } } }` | async progress update |
| `job.cancel.response` | `{ data: { ctx, payload: { ok, mode?, job_id?, message? } } }` | cancel attempt result |
| `artifact.list` | `{ data: { ctx, artifacts[] } }` | artifact metadata list |
| `artifact.content` | `{ data: { ctx, path, content } }` | artifact content payload |
| `crawl_progress` | `{ job_id, status, pages_crawled, ... }` | crawl compatibility stream (retained) |
| `crawl_files` | `{ files, output_dir, job_id? }` | crawl compatibility manifest (retained) |
| `file_content` | `{ path, content }` | compatibility file content message |
| `stats` | `{ aggregate, containers, container_count }` | docker runtime stats |

`ctx` fields: `exec_id`, `mode`, `input`.

### Mode Execution Rules

- Async modes are server-controlled: `crawl`, `extract`, `embed`, `github`, `reddit`, `youtube`.
- For async modes, server strips client `--wait` and does fire-and-poll behavior.
- `--json` is injected for most modes, except allowlisted exceptions (`search`, `research`).
- Flags are passed through a server allowlist (`ALLOWED_FLAGS`), not blindly forwarded.

## WebSocket API (`/ws/shell`)

Dedicated PTY shell websocket (not multiplexed with `/ws`).

Access constraints:
- loopback-only: non-localhost clients are rejected with `403`

Client -> server messages:
- `{ "type": "input", "data": "ls -la\\n" }`
- `{ "type": "resize", "cols": 120, "rows": 40 }`

Server -> client messages:
- `{ "type": "output", "data": "<pty chunk>" }`

## HTTP API (`apps/web` routes)

### `GET /api/omnibox/files`

Handler: `apps/web/app/api/omnibox/files/route.ts`

Query params:

- none: list available mentionable local docs
- `id=<source:path>`: fetch file by id

Response (list):

```json
{
  "files": [
    {
      "id": "docs:ARCHITECTURE.md",
      "label": "ARCHITECTURE",
      "path": "docs/ARCHITECTURE.md",
      "source": "docs"
    }
  ]
}
```

Response (single file):

```json
{
  "file": {
    "id": "docs:ARCHITECTURE.md",
    "label": "ARCHITECTURE",
    "path": "docs/ARCHITECTURE.md",
    "source": "docs",
    "content": "..."
  }
}
```

Errors:

- `404` not found/invalid id

### `POST /api/pulse/chat`

Handler: `apps/web/app/api/pulse/chat/route.ts`

Request schema from `PulseChatRequestSchema` (`apps/web/lib/pulse/types.ts`):

- `prompt` string
- `sessionId?` string (resume prior Claude session)
- `documentMarkdown` string (default `""`)
- `selectedCollections` string[] (default `["cortex"]`)
- `threadSources` string[] (default `[]`)
- `scrapedContext?` object
- `conversationHistory` array of `{ role: "user"|"assistant", content: string }`
- `permissionLevel`: `plan | accept-edits | bypass-permissions` (default `bypass-permissions`)
- `model`: `sonnet | opus | haiku` (default `sonnet`)
- `effort`: `low | medium | high` (default `medium`)
- additional CLI control fields (for example: `maxTurns`, `maxBudgetUsd`, `disableSlashCommands`, `allowedTools`, `disallowedTools`, `addDir`, `betas`, `toolsRestrict`)

Response:
- streaming NDJSON (`content-type: application/x-ndjson`)
- event types: `status`, `delta`, `heartbeat`, `done`, `error`

Errors:

- `400` invalid request schema
- `500` runtime failure

### `GET /api/pulse/doc`

Handler: `apps/web/app/api/pulse/doc/route.ts`

Query params:

- none: list pulse docs
- `filename=<name>.md`: load one pulse doc

Errors:

- `404` filename not found
- `500` loader failure

### `POST /api/pulse/save`

Handler: `apps/web/app/api/pulse/save/route.ts`

Request schema:

- `title` string
- `markdown` string
- `tags?` string[]
- `collections?` string[]
- `embed?` boolean (default `true`)

Response:

```json
{ "path": "...", "filename": "...", "saved": true }
```

Behavior:

- Saves note to pulse storage.
- If `embed=true` and `TEI_URL` + `QDRANT_URL` are set, chunks/embeds note and upserts to Qdrant.
- Embed failure does not fail save.

Errors:

- `400` invalid request schema
- `500` save failure

### `POST /api/ai/copilot`

Handler: `apps/web/app/api/ai/copilot/route.ts`

Request:

- `{ prompt, system?, model? }` validated by `CopilotRequestSchema`
- header `x-copilot-stream: 1` enables NDJSON stream mode

Response modes:
- default JSON: raw `generateText(...)` result object
- NDJSON stream: `start` then `done` event (payload includes `completion`)

Errors:

- `400` invalid schema
- `401` missing `AI_GATEWAY_API_KEY`
- `408` abort/timeout
- `500` runtime failure

## Error Model

WebSocket:

- Command/protocol errors are emitted as `type: "error"` messages.
- Invalid mode requests are rejected by server before subprocess spawn.

HTTP:

- `400` client payload invalid
- `401` missing/invalid auth for protected routes
- `404` resource not found
- `408` request timeout/abort path
- `500` internal runtime error

## Security Constraints

- WebSocket command surface is constrained by explicit mode and flag allowlists.
- File APIs enforce path safety and source-root containment.
- Output/download routes reject traversal and serve from validated roots only.
- URL fetching uses SSRF controls documented in `docs/SECURITY.md`.

## Compatibility Notes

- Active UI runtime is `apps/web`.
- `axon serve` (`crates/web.rs` + `crates/web/*`) is the core websocket/output/download bridge runtime for the web app.
- Only the old standalone static serve page UX is deprecated.
- `/ws` v2 event names are canonical (`command.*`, `job.*`, `artifact.*`).
- `crawl_progress`, `crawl_files`, and `file_content` remain as compatibility channels.
- Keep `apps/web/lib/ws-protocol.ts` and Rust websocket payloads in sync.

## Source Map

- `crates/web.rs`
- `crates/web/shell.rs`
- `crates/web/execute.rs`
- `crates/web/execute/async_mode.rs`
- `crates/web/execute/files.rs`
- `crates/web/download.rs`
- `apps/web/lib/ws-protocol.ts`
- `apps/web/app/api/omnibox/files/route.ts`
- `apps/web/app/api/pulse/chat/route.ts`
- `apps/web/app/api/pulse/doc/route.ts`
- `apps/web/app/api/pulse/save/route.ts`
- `apps/web/app/api/ai/copilot/route.ts`
