# crates/web — WebSocket Execution Bridge
Last Modified: 2026-02-27

## Role

`crates/web` is the axum WebSocket bridge consumed by `apps/web` (Next.js). It has no static UI of its own.

- The active frontend is `apps/web` — all UI decisions live there.
- `crates/web` handles: WebSocket connection lifecycle, CLI subprocess execution, Docker stats broadcasting, output file serving, and crawl artifact download endpoints.

## Source of Truth

For branding, theme, layout, and frontend UX decisions: `apps/web`.

## Directory Intent

```
crates/web.rs                          # Axum server wiring and routes
crates/web/
├── docker_stats.rs                    # Container stats broadcasting (bollard, 500ms poll)
├── download.rs                        # HTTP endpoints for crawl artifact downloads (files + zip)
├── pack.rs                            # Output packaging helpers
├── tailscale_auth.rs                  # Shared token auth + check_auth() entry point
├── logs/                              # Log streaming support
├── snapshots/                         # insta snapshot files (committed; update with `cargo insta review`)
├── execute.rs                         # Subprocess launch + WS output pump (entry point)
├── execute/
│   ├── args.rs                        # CLI arg allow-list parsing
│   ├── constants.rs                   # ALLOWED_MODES, ALLOWED_FLAGS
│   ├── context.rs                     # Execute request context builder
│   ├── events.rs                      # WS event type definitions (ExecuteEvent)
│   ├── exe.rs                         # Binary path resolution
│   ├── files.rs                       # Output file serving for completed jobs
│   ├── mcp_config.rs                  # MCP server config loader (hot-reloaded by ACP)
│   ├── overrides.rs                   # Per-request config overrides
│   ├── session_guard.rs               # Concurrent session semaphore guard
│   ├── ws_send.rs                     # WS message send helpers
│   ├── sync_mode.rs                   # Sync-mode execution entry point
│   ├── sync_mode/
│   │   └── pulse_chat/               # Pulse Chat sync-mode bridge
│   │       ├── connection.rs          # ACP adapter connection management
│   │       └── events.rs             # Pulse Chat WS event mapping
│   ├── async_mode.rs                  # Async-mode execution entry point
│   └── tests/                         # Execute integration tests
└── ws_handler/
    ├── acp_session.rs                 # WS connection → ACP session lifecycle
    ├── dispatch.rs                    # WS message routing
    ├── params.rs                      # WS connection parameters
    ├── prewarm.rs                     # ACP adapter pre-warming on WS connect
    ├── pulse_chat.rs                  # Pulse Chat WS handler entry point
    ├── pulse_chat/                    # Pulse Chat submodules
    ├── rate_limiter.rs                # Per-connection rate limiting
    ├── service_calls.rs               # Service function dispatch from WS handlers
    ├── subprocess.rs                  # Subprocess execution bridge
    ├── types.rs                       # WsConnState and shared handler types
    └── cancel.rs                      # WS cancel request handler
```

## WebSocket Protocol

Single multiplexed WS connection. Messages keyed by `"type"`:

| Direction | type | Payload |
|-----------|------|---------|
| Client → Server | `execute` | `{ "mode": "...", "input": "..." }` |
| Client → Server | `cancel` | `{ "job_id": "..." }` |
| Server → Client | `output` | stdout JSON data |
| Server → Client | `log` | stderr progress/spinner text |
| Server → Client | `done` | job completed |
| Server → Client | `error` | job failed with message |
| Server → Client | `stats` | Docker container stats (polled every 500ms via bollard) |

ANSI codes are stripped from log output via `console::strip_ansi_codes()`.

## Security Model

`execute.rs` enforces strict whitelists before spawning any subprocess:
- **`ALLOWED_MODES`**: list of valid CLI subcommands (e.g. `scrape`, `crawl`, `ask`) — rejects anything not in this list
- **`ALLOWED_FLAGS`**: set of permitted CLI flags — rejects unknown flags

Unknown modes or flags return an error WS event without spawning a process. Do not bypass these whitelists when adding new execute routes.

### Auth Stack (`web.rs` + `tailscale_auth.rs`)

| Layer | Env var | Default | Notes |
|-------|---------|---------|-------|
| **API token** | `AXON_WEB_API_TOKEN` | — | Bearer / x-api-key / `?token=` query param. |

### `DownloadAuthState`

Download routes use a lighter state struct (`DownloadAuthState`) that carries `job_dirs` and `api_token` — same shared token gate as `AppState` without WS/stats overhead.

## Docker Stats Caveat

`docker_stats.rs` polls bollard for container stats every 500ms and broadcasts to all WS clients. This requires `/var/run/docker.sock` to be mounted. When running inside `axon-workers`, the socket is **not** mounted — stats will be silently unavailable. HTTP/WS endpoints remain functional.

## Ports

| Binding | Purpose |
|---------|---------|
| `0.0.0.0:49000` | HTTP + WebSocket server (`axon serve`) |

Port 49000 is the backend server port. The Next.js frontend (`apps/web`) at port 49010 proxies to it.

## Adding a New HTTP Endpoint

1. Add the route in `crates/web.rs` (`Router::new().route(...)`)
2. Implement the handler as a free function in the appropriate `crates/web/` module
3. Add the handler to `ALLOWED_MODES` or `ALLOWED_FLAGS` if it involves subprocess execution
4. Write an integration test in `crates/web/execute/tests/` or a snapshot test if response shape is fixed

## Agent Guidance

When asked to review or polish the frontend visual system, audit and update `apps/web` first.

## Testing

```bash
cargo test web            # WS bridge + execute pipeline tests
cargo test download       # artifact download endpoint tests
cargo test -- --nocapture # show subprocess output during tests
```

Snapshot tests use `insta`. After intentional response shape changes, update snapshots:

```bash
cargo test web -- --nocapture   # run to generate new snapshots
cargo insta review              # approve/reject each diff interactively
```

Snapshot files live in `crates/web/snapshots/` and are committed to git.

## ACP Architecture: One-Shot vs Persistent Connection

Two complete code paths exist for ACP (Adapter Control Protocol) prompt execution, with different lifecycle and timeout semantics.

### One-Shot Mode (`crates/services/acp/runtime.rs`)

Spawns a fresh adapter subprocess per prompt turn. Each call to `run_acp_turn()` goes through: spawn adapter, initialize connection, set up session, apply config/model, execute prompt, tear down. After a successful turn, the runtime awaits the adapter's exit with a **10-second timeout** (`tokio::time::timeout(Duration::from_secs(10), exit_rx)`) to let it flush its session file before the child handle is dropped (which triggers `kill_on_drop` SIGKILL).

**Trade-off:** Clean state each turn (no leaked context), but higher latency due to process spawn overhead.

### Persistent-Connection Mode (`crates/services/acp/persistent_conn.rs`)

Keeps a single adapter process alive for the entire WebSocket connection lifetime. Turns are dispatched via an `mpsc` channel to the long-lived process. The adapter is set up lazily on the first turn. Timeout: **3600 seconds (1 hour)** for the overall connection (configurable via `adapter_timeout_secs` on the adapter config, defaulting to 3600s when unset).

**Trade-off:** Lower latency on subsequent turns (reuses `ClientSideConnection`, session state preserved), but adapter process must be managed across the full WS connection lifetime.

### Shared Finalization

Both paths call `finalize_successful_turn()` from `bridge.rs` for consistent turn completion behavior: logging, `EditorWrite` emission, and `TurnResult` event dispatch. This ensures the WS client sees identical event shapes regardless of which code path executed the turn.

## ACP Session Cache Parameters

Hardcoded constants in `crates/services/acp/session_cache.rs` govern session caching for WebSocket reconnect replay:

| Parameter | Value | Location | Description |
|-----------|-------|----------|-------------|
| `SESSION_TTL` | 30 minutes | `session_cache.rs:19` | Time before an idle cached session is reaped |
| `MAX_REPLAY_BUFFER` | 4096 messages | `session_cache.rs:23` | Message-count cap on replay buffer per session |
| `MAX_REPLAY_BUFFER_BYTES` | 4 MiB | `session_cache.rs:28` | Byte-based cap on replay buffer per session |
| Reaper interval | 60 seconds | `session_cache.rs:233` | How often the background task checks for expired sessions |
| `AXON_ACP_MAX_CONCURRENT_SESSIONS` | 8 (default) | `web.rs:36` | Semaphore-based limit on concurrent ACP sessions |

The replay buffer enforces both limits: a message is only appended if the buffer has fewer than `MAX_REPLAY_BUFFER` entries **and** the cumulative byte size stays within `MAX_REPLAY_BUFFER_BYTES`.

The reaper is started lazily via `std::sync::Once` on the first session insertion (`ensure_reaper()`). It runs `reap_expired()` every 60 seconds, evicting any session whose last activity exceeds `SESSION_TTL`.

All parameters except `AXON_ACP_MAX_CONCURRENT_SESSIONS` are hardcoded constants, not configurable via environment variables.
