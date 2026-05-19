# Session: Multi-Agent Code Review & Infrastructure Fixes
**Date**: 2026-03-02

## 1. Session Overview
Conducted a comprehensive multi-agent code review of the `@crates/web/` and `@crates/mcp/` components. Implemented critical security fixes for the shell PTY, optimized Docker telemetry via streaming, and overhauled the Model Context Protocol (MCP) server to use native RAG execution and database-level pagination.

## 2. Timeline
- **Phase 1**: Orchestrated review of `@crates/web/` (PTY shell, Docker stats, argument injection).
- **Phase 2**: Applied security and performance fixes to `@crates/web/`.
- **Phase 3**: Orchestrated review of `@crates/mcp/` (Pagination, subprocess overhead, artifacts).
- **Phase 4**: Applied architectural and performance fixes to `@crates/mcp/` and `crates/jobs/`.
- **Phase 5**: Resolved complex `Send` trait bound violations in async RAG pipelines.
- **Phase 6**: Verified entire workspace via `cargo check`.

## 3. Key Findings
- **Security Vulnerability**: `crates/web/shell.rs` exposed a full shell over WebSockets without IP restrictions or keepalives.
- **Argument Injection**: `crates/web/execute/args.rs` allowed positional inputs to be treated as flags if they started with `-`.
- **Performance Bottleneck**: All MCP job listing handlers used in-memory pagination (`skip().take()`), loading massive arrays from Postgres only to discard them.
- **Performance Bottleneck**: `crates/web/docker_stats.rs` was polling the Docker API every 500ms for all containers.
- **Architectural Debt**: `handle_ask` and `handle_doctor` in the MCP server spawned subprocesses instead of using core logic.

## 4. Technical Decisions
- **Loopback Enforcement**: Restricted the WebSocket shell to `127.0.0.1` using Axum `ConnectInfo` to prevent remote exploitation.
- **Native Logic Integration**: Added `ask_payload` to `crates/vector/ops/commands/ask.rs` to allow the MCP server to execute RAG without shell-out overhead.
- **SQL Pagination**: Standardized all `list_jobs` functions across the `crates/jobs` subsystem to accept `offset` and use `LIMIT OFFSET` at the SQL level.
- **Streaming Metrics**: Replaced the Docker polling loop with per-container `tokio` tasks using Docker's `stream: true` API.

## 5. Files Modified
- `crates/web.rs`: Implemented shell endpoint IP filtering.
- `crates/web/shell.rs`: Added WS keepalives and PTY cleanup.
- `crates/web/execute/args.rs`: Added input sanitization.
- `crates/web/docker_stats.rs`: Refactored to background streaming.
- `crates/mcp/server/common.rs`: Switched to compact JSON artifacts.
- `crates/mcp/server/handlers_*.rs`: Updated to use DB-level pagination.
- `crates/mcp/server/handlers_query.rs`: Migrated `handle_ask` to native logic.
- `crates/mcp/server/handlers_system.rs`: Migrated `handle_doctor` to native logic.
- `crates/vector/ops/commands/ask.rs`: Added `ask_payload`.
- `crates/vector/ops/commands/ask/output.rs`: Fixed `Send` trait bound violations.
- `crates/jobs/crawl/runtime/db.rs`: Added `offset` to list.
- `crates/jobs/extract.rs`: Added `offset` to list.
- `crates/jobs/embed.rs`: Added `offset` to list.
- `crates/jobs/ingest/ops.rs`: Added `offset` to list.
- `crates/jobs/refresh/mod.rs`: Added `offset` to list.
- `crates/cli/commands/status.rs`: Updated for pagination signature.

## 6. Commands Executed
- `cargo check`: Verified compilation across all subsystems.
- `grep -r`: Mapped the pagination and doctor logic across the codebase.

## 7. Behavior Changes
- **Security**: The shell endpoint now returns `403 Forbidden` if accessed from a non-local IP.
- **Performance**: Reduced Docker API traffic by moving to event-driven streaming.
- **Performance**: Large job list requests are now significantly faster and more memory-efficient due to SQL pagination.
- **Reliability**: MCP `ask` and `doctor` commands no longer fail if the `axon` binary is missing from the PATH.

## 8. Verification Evidence
| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo check` | Success | Finished dev profile | PASS |

## 9. Source IDs + Collections Touched
- This session log is about to be embedded into the `axon_rust` collection.

## 10. Risks and Rollback
- **Risk**: The IP loopback check might break access if running in certain complex Docker networking modes (e.g., proxied).
- **Rollback**: Revert `crates/web.rs` and `crates/mcp/server/handlers_system.rs`.

## 11. Open Questions
- Should we implement a full Auth token layer for the shell WS instead of just IP loopback?

## 12. Next Steps
- Implement full `axum` middleware for session authentication.
- Add integration tests for the new native `ask_payload` logic.