# `axon serve` — WebSocket Execution Bridge
Last Modified: 2026-03-04

Version: 1.2.0
Last Updated: 03/03/2026

## Overview

`axon serve` starts the axum WebSocket bridge used by `apps/web`. It has no static UI of its own — the frontend is the Next.js app in `apps/web`.

Current canonical WebSocket contract documentation lives in [`docs/API.md`](API.md).

## Usage

```bash
axon serve              # default port 49000
axon serve --port 8080  # custom port
```

The server exposes HTTP endpoints and WebSockets at `/ws` and `/ws/shell`. Connect the Next.js frontend (`apps/web`) to this backend via `AXON_BACKEND_URL`.

Bind host is controlled by `AXON_SERVE_HOST`:

- default: `127.0.0.1`
- container/proxy deployments: set `AXON_SERVE_HOST=0.0.0.0`

## Architecture

```
apps/web ──────▶ axum (single port, single binary)
                 │
                 ├── GET /output/{*path}          → serve generated output files
                 ├── GET /download/{job_id}/pack.md   → crawl artifact download
                 ├── GET /download/{job_id}/pack.xml
                 ├── GET /download/{job_id}/archive.zip
                 ├── GET /download/{job_id}/file/{*path}
                 │
                 ├── WS /ws                       → command bridge + docker stats
                     │
                     ├── client→server: {"type":"execute","mode":"scrape","input":"https://...","flags":{}}
                     │   server spawns: tokio::process::Command("axon scrape --json --wait true ...")
                     │   server→client: {"type":"command.output.line","data":{"ctx":...,"line":"..."}}
                     │   server→client: {"type":"command.done","data":{"ctx":...,"payload":{"exit_code":0,"elapsed_ms":1823}}}
                     │
                     ├── client→server: {"type":"cancel","id":"<job_uuid>","mode":"crawl"}
                     │   server→client: {"type":"job.cancel.response","data":{"ctx":...,"payload":{"ok":true,...}}}
                     │
                     └── server→client (broadcast): {"type":"stats","containers":{...},"aggregate":{...}}
                         └── bollard polls Docker socket every 500ms

                 └── WS /ws/shell                 → PTY bridge for terminal UI
                     └── origin-checked + token-gated (non-allowed origins rejected with 403)
```

## WebSocket Authentication

### `/ws` — command bridge

The `/ws` path is a raw Next.js rewrite (not a proxy through Next.js API routes), so Next.js middleware does **not** run on WS upgrade requests. Authentication is enforced at the Rust layer in `crates/web.rs`.

**Gate activation** — the gate is active when `AXON_WEB_API_TOKEN` is set. If unset, the gate is disabled (open — trusted-network deployments only).

**Token validation:** the `?token=` query param is compared against `AXON_WEB_API_TOKEN` using a direct string equality check. This is the same secret used by the Next.js proxy for `/api/*` routes — one token for the whole frontend.

MCP OAuth clients (`atk_` tokens) do **not** have access to `/ws`. MCP clients use the MCP tool API (`/mcp`) instead.

**Token flow for the browser:**

```
AXON_WEB_API_TOKEN (server env)
         ↕ must match
NEXT_PUBLIC_AXON_API_TOKEN (client env, embedded in browser bundle)
         ↓
use-axon-ws.ts appends ?token=<value> to the WS URL
         ↓
crates/web.rs ws_upgrade() checks ?token= against AXON_WEB_API_TOKEN
```

**Environment variables:**

| Variable | Purpose |
|----------|---------|
| `AXON_WEB_API_TOKEN` | WS gate token (server-side). Also used by `proxy.ts` for `/api/*` auth. |
| `NEXT_PUBLIC_AXON_API_TOKEN` | Client-side copy — must equal `AXON_WEB_API_TOKEN`. Sent as `?token=` on WS and `x-api-key` on `/api/*`. |

**Rejected connections** return HTTP 401 before the WebSocket upgrade completes, with the source IP logged at `warn`.

### `/ws/shell` — PTY bridge

Origin checking is enforced in `shell_ws_upgrade()` via `effective_shell_allowed_origins()`, which combines `AXON_SHELL_ALLOWED_ORIGINS` with `AXON_WEB_ALLOWED_ORIGINS`. Non-allowed origins receive HTTP 403 before upgrade.

Token auth uses the same `check_auth()` path as `/ws` — checked against `AXON_WEB_API_TOKEN`. Rejected connections receive HTTP 403 (not 401). Inbound message size is capped at 64 KiB at the upgrade level.

Auth is handled by the shell server (`shell-server.mjs`) separately from the Rust WS gate. See `apps/web/CLAUDE.md → Shell Server`.

---

## Key Design Decisions

1. **Subprocess execution** — Commands run via `tokio::process::Command` spawning the same binary with `--json --wait true`. This means zero refactoring of existing commands, and a crashing command doesn't take down the server.

2. **`std::env::current_exe()`** — The server spawns itself with different args. Single binary, no external dependencies.

3. **Single command WebSocket, separate shell WebSocket** — `/ws` handles command execution and stats; `/ws/shell` is a dedicated PTY channel.

4. **Flag whitelisting** — Only known flag names (`--max-pages`, `--limit`, `--collection`, etc.) are passed through to subprocess args. User input is never used as raw CLI args (command injection prevention).

5. **Bollard graceful degradation** — If the Docker socket is unavailable, stats broadcasting is silently disabled. The server still works for command execution.

## Modules

| File | Purpose | Lines |
|------|---------|-------|
| `crates/web.rs` | Axum server, routes, WS handlers, shared state | ~300 |
| `crates/web/execute.rs` | Subprocess orchestration + mode/flag validation | ~150 |
| `crates/web/execute/{args,async_mode,polling,files,events,ws_send}.rs` | Arg building, async job polling, artifact/file streaming, v2 WS events | split modules |
| `crates/web/docker_stats.rs` | Bollard Docker stats poller, rate calculations, broadcast | ~281 |
| `crates/web/shell.rs` | `/ws/shell` PTY websocket bridge | ~300 |
| `crates/cli/commands/serve.rs` | `run_serve()` entry point | ~6 |

## WebSocket Protocol

All messages are JSON with a `type` field:

### Client → Server

```json
{"type": "execute", "mode": "scrape", "input": "https://example.com", "flags": {"limit": 10}}
{"type": "cancel", "id": "uuid-of-crawl-job", "mode": "crawl"}
{"type": "read_file", "path": "crawl_artifact.md"}
```

### Server → Client

```json
{"type": "command.start", "data": {"ctx": {"exec_id": "exec-...", "mode": "scrape", "input": "https://example.com"}}}
{"type": "command.output.json", "data": {"ctx": {"exec_id": "exec-..."}, "data": {"url": "https://example.com"}}}
{"type": "command.output.line", "data": {"ctx": {"exec_id": "exec-..."}, "line": "..."}}
{"type": "job.status", "data": {"ctx": {"exec_id": "exec-..."}, "payload": {"status": "running", "metrics": {"phase": "crawl"}}}}
{"type": "job.progress", "data": {"ctx": {"exec_id": "exec-..."}, "payload": {"phase": "crawl", "percent": 52.3}}}
{"type": "artifact.list", "data": {"ctx": {"exec_id": "exec-..."}, "artifacts": [{"kind": "markdown", "path": "pack.md"}]}}
{"type": "artifact.content", "data": {"ctx": {"exec_id": "exec-..."}, "path": "pack.md", "content": "# ..."}}
{"type": "job.cancel.response", "data": {"ctx": {"exec_id": "exec-..."}, "payload": {"ok": true, "mode": "crawl", "job_id": "..."}}}
{"type": "command.done", "data": {"ctx": {"exec_id": "exec-..."}, "payload": {"exit_code": 0, "elapsed_ms": 1823}}}
{"type": "command.error", "data": {"ctx": {"exec_id": "exec-..."}, "payload": {"message": "spawn failed", "elapsed_ms": 400}}}
{"type": "stats", "container_count": 6, "containers": {...}, "aggregate": {...}}
```

Compatibility messages still emitted for frontend migration paths:

- `crawl_progress`
- `crawl_files`
- `file_content`

## Allowed Modes

Only these command modes can be executed from the UI (whitelist in `execute/constants.rs`):

`scrape`, `crawl`, `map`, `extract`, `search`, `research`, `embed`, `debug`, `doctor`, `query`, `retrieve`, `ask`, `evaluate`, `suggest`, `sources`, `domains`, `stats`, `status`, `dedupe`, `github`, `reddit`, `youtube`, `sessions`, `screenshot`, `pulse_chat`, `mcp_refresh` (internal), `pulse_chat_probe` (internal)

## Allowed Flags

Only these flags can be passed from the UI (whitelist in `execute/constants.rs`):

| JSON Key | CLI Flag |
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
| `wait` | `--wait` *(ignored for async modes)* |
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

## Docker Stats

The stats poller connects to the Docker socket via `bollard::Docker::connect_with_local_defaults()` and:

1. Lists containers matching `axon-*` prefix with status `running`
2. For each container, fetches one-shot stats
3. Computes: CPU% (docker stats formula), memory (usage - cache), network I/O rates, block I/O rates
4. Broadcasts the aggregated JSON to all connected WebSocket clients every 500ms
5. The frontend maps per-container CPU to neuron cluster EPSP injection, and network I/O to extra action potential firing
