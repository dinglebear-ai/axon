# WebSocket Protocol Contract

Last Modified: 2026-03-09

This document is the **single source of truth** for the WebSocket protocol between
`apps/web` (Next.js, TypeScript) and `crates/web` (axum, Rust).

Both sides must be kept in sync. When changing anything here:

1. Update the Rust side: `crates/web/execute/constants.rs` (`ALLOWED_MODES`, `ALLOWED_FLAGS`)
2. Update the TypeScript side: `apps/web/lib/ws-protocol.ts` (`MODES` constant)

---

## Connection

Single multiplexed WebSocket connection proxied through Next.js:

```
Client (browser) → /ws → Next.js (next.config.ts) → AXON_BACKEND_URL/ws → axum server (port 49000)
```

---

## Message Shapes

### Client → Server

| `type` | Fields | Description |
|--------|--------|-------------|
| `execute` | `mode: string`, `input: string`, `flags: Record<string, string \| boolean>` | Run a CLI subcommand |
| `cancel` | `id: string`, `mode?: string`, `job_id?: string` | Cancel a running job |
| `read_file` | `path: string` | Read a file from the output directory |
| `permission_response` | `tool_call_id: string`, `option_id: string`, `session_id: string` | Reply to an ACP permission request; routed by `(session_id, tool_call_id)` compound key (SEC-7) |

### Server → Client

| `type` | Fields | Description |
|--------|--------|-------------|
| `log` | `line: string` | Stderr progress/spinner text (ANSI codes stripped) |
| `command.start` | `data: { ctx }` | Execution started |
| `command.output.json` | `data: { ctx, data: unknown }` | Structured JSON output from stdout |
| `command.output.line` | `data: { ctx, line: string }` | Non-JSON line from stdout |
| `command.done` | `data: { ctx, payload: { exit_code, elapsed_ms? } }` | Command finished |
| `command.error` | `data: { ctx, payload: { message, elapsed_ms? } }` | Command failed |
| `job.status` | `data: { ctx, payload: { status, error?, metrics? } }` | Async job status update |
| `job.progress` | `data: { ctx, payload: { phase, percent?, processed?, total? } }` | Async job progress |
| `job.cancel.response` | `data: { ctx, payload: { ok, mode?, job_id?, message? } }` | Cancel acknowledgement |
| `artifact.list` | `data: { ctx, artifacts: ArtifactEntry[] }` | List of output artifacts |
| `artifact.content` | `data: { ctx, path, content }` | Content of a requested artifact |
| `crawl_files` | `files: CrawlFile[]`, `output_dir: string`, `job_id?: string` | Crawl output file list |
| `crawl_progress` | `job_id`, `status`, `pages_crawled`, `pages_discovered`, `md_created`, `thin_md`, `phase` | Live crawl stats |
| `file_content` | `path: string`, `content: string` | Response to `read_file` |
| `stats` | `aggregate: AggregateStats`, `containers: Record<string, ContainerStats>`, `container_count: number` | Docker container stats (polled every 500ms) |
| `editor_update` | `content: string`, `operation: 'replace' \| 'append'` | ACP-driven PlateJS editor patch; `replace` overwrites editor content, `append` concatenates |
| `session_fallback` | `old_session_id: string`, `new_session_id: string` | Emitted by ACP bridge when a session resume fails and a new session is created in its place |

#### Context Object (`ctx`)

All `command.*`, `job.*`, and `artifact.*` messages carry a context block:

```typescript
interface WsV2CommandContext {
  exec_id: string  // unique identifier for this execution
  mode: string     // the CLI subcommand (must be in ALLOWED_MODES)
  input: string    // the input URL/query/text
}
```

---

## Allowed Modes

The following modes are accepted by the Rust backend (`ALLOWED_MODES` in `crates/web/execute/constants.rs`).
The TypeScript `MODES` array in `apps/web/lib/ws-protocol.ts` must contain a matching entry for each.

| Mode | Category | Gets `--json` | Async (AMQP-backed) |
|------|----------|:---:|:---:|
| `scrape` | content | yes | no |
| `crawl` | content | yes | yes |
| `map` | content | yes | no |
| `extract` | content | yes | yes |
| `search` | rag | yes | no |
| `research` | rag | yes | no |
| `embed` | ops | yes | yes |
| `debug` | service | yes | no |
| `doctor` | service | yes | no |
| `query` | rag | yes | no |
| `retrieve` | rag | yes | no |
| `ask` | rag | yes | no |
| `evaluate` | rag | yes | no |
| `suggest` | rag | yes | no |
| `sources` | ops | yes | no |
| `domains` | ops | yes | no |
| `stats` | ops | yes | no |
| `status` | ops | yes | no |
| `dedupe` | ops | yes | no |
| `github` | ingest | yes | yes |
| `reddit` | ingest | yes | yes |
| `youtube` | ingest | yes | yes |
| `sessions` | ingest | yes | no |
| `screenshot` | content | yes | no |
| `pulse_chat` | acp | no | no |
| `mcp_refresh` | internal | yes | no |
| `pulse_chat_probe` | acp/internal | no | no |

> **Rule:** `NO_JSON_MODES` is currently empty — all modes receive `--json` by default (including `search` and `research`, which were previously excluded but are now routed through structured JSON dispatch). The `evaluate` events path disables `--json` via a separate guard in `args.rs`.

---

## Allowed Flags

Flags accepted by the Rust backend (`ALLOWED_FLAGS` in `crates/web/execute/constants.rs`).
Unknown flags are silently ignored — they are never forwarded to the subprocess.

| JSON key | CLI flag |
|----------|----------|
| `max_pages` | `--max-pages` |
| `max_depth` | `--max-depth` |
| `limit` | `--limit` |
| `collection` | `--collection` |
| `format` | `--format` |
| `render_mode` | `--render-mode` |
| `include_subdomains` | `--include-subdomains` |
| `discover_sitemaps` | `--discover-sitemaps` |
| `sitemap_since_days` | `--sitemap-since-days` |
| `embed` | `--embed` |
| `diagnostics` | `--diagnostics` |
| `yes` | `--yes` |
| `wait` | `--wait` |
| `research_depth` | `--research-depth` |
| `search_time_range` | `--search-time-range` |
| `sort` | `--sort` |
| `time` | `--time` |
| `max_posts` | `--max-posts` |
| `min_score` | `--min-score` |
| `scrape_links` | `--scrape-links` |
| `include_source` | `--include-source` |
| `claude` | `--claude` |
| `codex` | `--codex` |
| `gemini` | `--gemini` |
| `project` | `--project` |
| `output_dir` | `--output-dir` |
| `delay_ms` | `--delay-ms` |
| `request_timeout_ms` | `--request-timeout-ms` |
| `performance_profile` | `--performance-profile` |
| `batch_concurrency` | `--batch-concurrency` |
| `depth` | `--depth` |
| `responses_mode` | `--responses-mode` |
| `agent` | `--agent` |
| `model` | `--model` |
| `session_mode` | `--session-mode` |
| `mcp_servers` | `--mcp-servers` |
| `blocked_mcp_tools` | `--blocked-mcp-tools` |
| `session_id` | `--session-id` |
| `assistant_mode` | `--assistant-mode` |
| `enable_fs` | `--enable-fs` |
| `enable_terminal` | `--enable-terminal` |
| `permission_timeout_secs` | `--permission-timeout-secs` |
| `adapter_timeout_secs` | `--adapter-timeout-secs` |
| `offset` | `--offset` |
| `max_points` | `--max-points` |

> **Security note:** `output_dir` values are validated for path traversal (`..` components)
> before being forwarded to the subprocess. Values containing parent-directory components
> are rejected and a warning is logged.

---

## Security Model

- **Mode allowlist**: any mode not in `ALLOWED_MODES` returns a `command.error` without spawning a process.
- **Flag allowlist**: unknown flags are dropped silently; only `ALLOWED_FLAGS` entries reach the CLI.
- **`output_dir` path guard**: `..` components in `output_dir` values are rejected server-side.
- **Auth**: all `/api/*` and WebSocket routes require a valid `AXON_WEB_API_TOKEN` (enforced by `proxy.ts` / middleware).
