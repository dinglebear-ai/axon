# Security Model
Last Modified: 2026-03-09

Version: 1.0.0
Last Updated: 01:26:53 | 02/25/2026 EST

## Table of Contents

1. Scope
2. Threat Model
3. Security Controls
4. ACP Permission Security
5. Secrets Management
6. Network Exposure
7. API and Command Surface Hardening
8. Residual Risks
9. Operational Security Checklist
10. Source Map

## Scope

This document captures security controls present in code and deployment configuration for Axon.

## Threat Model

In scope:

- SSRF attempts through user-provided URLs
- Path traversal attempts in file/download APIs
- Command injection attempts through websocket `execute`
- Secret leakage through repository commits and logs
- Local service exposure beyond host boundary

Out of scope:

- Host kernel compromise
- Supply-chain integrity beyond pinned images/dependencies
- Full multi-tenant isolation (system is designed for trusted self-hosted operation)

## Security Controls

### URL Validation and SSRF Controls

Implemented in `crates/core/http.rs`:

- scheme allowlist: `http` and `https` only
- blocked hosts: `localhost`, `.localhost`, `.internal`, `.local`
- blocked IP ranges:
  - loopback
  - link-local
  - RFC-1918 private ranges
  - IPv4-mapped IPv6 private/loopback
  - IPv6 unique-local
- additional crawler blacklist patterns for defense-in-depth

### File Path Safety

`/output/{*path}` (`crates/web.rs`):

- rejects `..` and NUL bytes
- canonicalizes base and target
- enforces target path under output root

`/download/{job_id}/...` (`crates/web/download.rs`):

- validates job id format
- resolves files only from registered job output dirs
- path traversal guarded
- max files limit enforced via `AXON_DOWNLOAD_MAX_FILES`

`/api/omnibox/files` (`apps/web/app/api/omnibox/files/route.ts`):

- id parsing with `source:path` model
- rejects unsafe ids and `..`
- resolves within source root only

### WebSocket Authentication Gate

`/ws` upgrade path (`crates/web.rs`):

- Gate is active when `AXON_WEB_API_TOKEN` is set; disabled (open) when unset
- One token type: `AXON_WEB_API_TOKEN` — the same static secret used by `proxy.ts` for `/api/*` routes
- Browser sends it as `?token=` query param (appended by `hooks/use-axon-ws.ts`)
- MCP OAuth clients (`atk_` tokens) do not have access to `/ws` — they use the MCP tool API instead
- Non-loopback connections to `/ws/shell` rejected with 403; IPv4-mapped loopback (`::ffff:127.0.0.1`) accepted correctly
- Rejected upgrades return 401 before the WebSocket handshake completes

### Command Surface Hardening

WebSocket command execution (`crates/web/execute.rs`):

- explicit `ALLOWED_MODES` list
- explicit `ALLOWED_FLAGS` list
- blocked forwarding of sensitive infra flags (db/redis/amqp/openai/qdrant/tei URL flags)
- asynchronous mode semantics controlled server-side

## ACP Permission Security

### SEC-7: Session-Scoped Permission Routing

Implemented across `crates/services/acp/`, `crates/web.rs`, and the execute bridge.

**Problem addressed:** When multiple ACP sessions run concurrently (e.g. two open browser tabs both active in Pulse), permission responses routed only by `tool_call_id` can bleed across sessions. If two sessions happen to produce the same `tool_call_id`, the wrong session's frontend could receive or consume the other session's permission prompt.

**Fix:** The `PermissionResponderMap` keys on `(session_id, tool_call_id)` — a composite tuple — instead of `tool_call_id` alone. Two concurrent sessions with colliding `tool_call_id` values cannot route to each other's responder channels.

**Type definition** (`crates/services/acp.rs`):

```rust
/// Key is (session_id, tool_call_id) to prevent cross-session collisions (SEC-7).
pub type PermissionResponderMap =
    Arc<dashmap::DashMap<(String, String), tokio::sync::oneshot::Sender<String>>>;
```

`DashMap` is used instead of `Arc<Mutex<HashMap<...>>>` to eliminate lock contention when the bridge concurrently inserts and removes responders from multiple async tasks (shard-level locking).

**Wire path:**

```
WS handler (crates/web.rs)
  └─ init_permission_responders()         ← one map per WS connection
       └─ execute bridge (crates/web/execute.rs)
            └─ sync_mode dispatch
                 └─ AcpBridgeClient::request_permission()
                      ├─ auto-approve path  (AXON_ACP_AUTO_APPROVE=true, default)
                      └─ interactive path   → handle_interactive_permission()
                           └─ insert (session_id, tool_call_id) → oneshot sender
                           └─ WS message routes response back by (session_id, tool_call_id)
                           └─ cleanup on receive OR timeout
```

**Auto-approve behavior:**

- `AXON_ACP_AUTO_APPROVE` env var (default: `true`) — when set, permission requests are granted immediately without waiting for frontend input. This is the expected default for self-hosted, trusted deployments.
- Set `AXON_ACP_AUTO_APPROVE=false` to require explicit frontend approval for every ACP tool-use permission request.

**Interactive path timeout:**

When auto-approve is disabled, `handle_interactive_permission()` inserts a oneshot sender into the map and waits up to **60 seconds** for the frontend to respond. On timeout:

1. The map entry is removed (no leak).
2. The outcome is `Cancelled` — the ACP agent receives a cancellation, not a hang.
3. A warning is logged: `ACP permission request timed out after 60s`.

**Cross-session bleed prevention (SEC-7):**

The `send_permission_response()` helper in `crates/web.rs` looks up the responder by `(session_id, tool_call_id)`. A permission response from session A can only resolve a pending responder that was registered under session A's `session_id`. Session B's responders are unreachable regardless of `tool_call_id` value.

## Secrets Management

Required practice:

- secrets in `.env` only
- `.env` is gitignored
- `.env.example` is the tracked template

Do not:

- commit real credentials
- print API keys in logs
- hardcode endpoint credentials in source

## Network Exposure

Most infra services bind to loopback (`127.0.0.1`) in compose:

- Postgres
- Redis
- RabbitMQ
- Qdrant
- Chrome management/CDP endpoints

`axon-web` is published as `49010:49010` by default (host-accessible unless firewall/reverse-proxy constrained).

Hardening guidance:
- For local-only web UI, publish `127.0.0.1:49010:49010`.
- If exposed externally, enforce TLS + auth at reverse proxy.
- Keep worker/internal service ports loopback-bound unless explicitly required.

## API and Command Surface Hardening

Pulse/Copilot API routes:

- schema validation with Zod
- explicit error mapping (for example `400`, `401`, `408`, `500`)
- timeout on upstream LLM calls

Worker startup:

- validates required env vars for DB/Redis/AMQP before long-running lane execution

## Residual Risks

1. DNS rebinding TOCTOU window — **MITIGATED** (v0.32.4):
- `SsrfBlockingResolver` (in `crates/core/http/ssrf.rs`) is wired into the reqwest client via `ClientBuilder::dns_resolver()`. It re-runs `check_ip()` on every IP returned by the OS resolver at the moment TCP connects — the same instant reqwest would dial. A TTL-0 DNS record that flips to `127.0.0.1` after `validate_url()` is caught at connection time.
- Two-layer defence: `validate_url()` blocks literal IPs, hostile TLDs, and `localhost` at parse time; `SsrfBlockingResolver` blocks any hostname whose DNS resolution produces a blocked IP at connect time.

2. WebSocket auth requires explicit env config:
- Gate is disabled if `AXON_WEB_API_TOKEN` is not set — any client can connect to `/ws`.
- For production / externally-exposed deployments, always set `AXON_WEB_API_TOKEN` to activate the gate.

3. Upstream model endpoints:
- security posture depends on TEI/LLM deployment hardening outside this repo.

4. **`AXON_WEB_ALLOW_INSECURE_DEV` loopback bypass:** When set to `true`, requests from `127.0.0.1`/`::1` bypass the `AXON_WEB_API_TOKEN` check on `/api/*` and `/ws/shell` — even when the token IS configured. This flag must never be set to `true` in any deployment accessible from untrusted networks. Default is `false`. See `.env.example`.

## Operational Security Checklist

Before deploy:

1. Confirm `.env` exists and is not tracked.
2. Confirm no secrets in changed files:

```bash
git diff -- . ':!*.lock'
```

Note: `git diff` only checks current uncommitted/staged changes. It does not detect
secrets already present in commit history.

For history scans, run a dedicated secret scanner on recent commits, for example:

```bash
gitleaks detect --source=. --log-opts="HEAD~5..HEAD"
```

3. Validate local-only bindings in compose.
4. Confirm `AXON_WEB_ALLOW_INSECURE_DEV=false` before any network-accessible deployment.
5. Run `./scripts/axon doctor`.

After deploy:

1. Confirm healthy containers.
2. Check logs for repeated auth/network failures.
3. Ensure API routes return expected status codes on invalid requests.

## Source Map

- `crates/core/http.rs` — SSRF / URL validation
- `crates/web.rs` — WS OAuth gate, shell WS loopback restriction, output file path safety, `init_permission_responders()`, `send_permission_response()` (SEC-7)
- `crates/web/download.rs` — download path safety
- `crates/web/execute.rs` — ALLOWED_MODES / ALLOWED_FLAGS command surface, `PermissionResponderMap` wire-through
- `crates/web/execute/cancel.rs` — cancel mode guard (H-04)
- `crates/web/execute/sync_mode/dispatch.rs` — `PermissionResponderMap` passed to ACP bridge
- `crates/services/acp.rs` — `PermissionResponderMap` type definition (SEC-7 composite key)
- `crates/services/acp/bridge.rs` — `handle_interactive_permission()`, `resolve_acp_auto_approve()`, 60s timeout + cleanup
- `crates/services/acp/runtime.rs` — `PermissionResponderMap` threaded through session init
- `crates/mcp/server/oauth_google/` — MCP OAuth server (issues `atk_` tokens; separate from WS auth)
- `apps/web/hooks/use-axon-ws.ts` — WS URL construction with `?token=` passthrough
- `apps/web/proxy.ts` — `/api/*` origin check + API token validation helpers
- `apps/web/app/api/omnibox/files/route.ts`
- `apps/web/app/api/pulse/chat/route.ts`
- `apps/web/app/api/ai/copilot/route.ts`
- `docker-compose.yaml`
- `.gitignore`
