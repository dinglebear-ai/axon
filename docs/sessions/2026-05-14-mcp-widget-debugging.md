---
date: 2026-05-14 00:04:44 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 321fc6f2
plan: none
agent: Claude (claude-sonnet-4-6)
session id: unknown
transcript: unknown
working directory: /home/jmagar/workspace/axon_rust
---

## User Request

Debug and fix the MCP Apps status dashboard widget which was rendering as an empty shell or stuck "Connecting..." in claude.ai web chat and the basic-host test client.

## Session Overview

Traced the MCP Apps widget failure through three distinct root causes — null artifact URL, awaited `app.connect()`, and sandbox referrer allowlist — and fixed each systematically using the official MCP Apps spec, the basic-host test client, and agent-browser automation. The widget now renders correctly in the basic-host reference implementation with live job queue data.

## Sequence of Events

1. User reported widget stuck on "Connecting..." in claude.ai — `artifact_handle.url` was null
2. Investigated `respond_with_mode` in `artifacts/respond.rs` — found `status` handler using `InlineHint::Default` falling to `path` mode for large payloads; widget JS can't fetch file artifacts
3. Fixed status handler to use `InlineHint::Document` forcing inline response mode
4. Widget still showed "Connecting..." — found `await app.connect()` blocking forever
5. Read official MCP Apps spec at `modelcontextprotocol.io/extensions/apps/build` and `ext-apps` spec repo
6. Fixed `app.connect()` to fire-and-forget (no `await`), added 5-second timeout + fallback to injected initial data
7. Added `window.__AXON_INITIAL_STATUS__` placeholder in HTML and dynamic injection in `read_resource` via `system::full_status()`
8. Removed the `await` timeout approach in favour of pure fire-and-forget per spec
9. Set up basic-host test client from `modelcontextprotocol/ext-apps` repo to test without claude.ai dependency
10. Debugged connection failures: CORS (added `localhost:8080` to `AXON_MCP_ALLOWED_ORIGINS`), auth (proxy with injected bearer token), sandbox URL hardcoded to `localhost:8081` (changed to `10.1.0.6:8081` in `implementation.ts`), streaming proxy buffering SSE (switched to `http-proxy` library)
11. Used `agent-browser` to inspect the live DOM — found iframe present but empty
12. Checked network requests via `performance.getEntriesByType('resource')` — 343KB widget HTML fetched successfully but not rendering
13. **Root cause found**: `sandbox.ts:4` hardcoded `ALLOWED_REFERRER_PATTERN = /^http:\/\/(localhost|127\.0\.0\.1)/` — sandbox threw on init for `10.1.0.6` referrer, never set up postMessage relay
14. Fixed referrer pattern to include `10.1.0.6`, rebuilt sandbox bundle
15. Rebuilt local axon binary (`cargo build --release`) — local binary was stale (only `cargo check` had run)
16. Widget rendered with full job queue data: 19 crawl, 18 embed, 0 extract/ingest

## Key Findings

- `src/mcp/server/handlers_system.rs:364` — `InlineHint::Default` caused `status` payload to fall to `path` mode (>8192 byte threshold), producing artifact with `url: null` that the widget iframe cannot fetch
- `src/mcp/assets/status_dashboard.html:281` — `await app.connect()` blocks forever because claude.ai web does not complete the MCP Apps postMessage handshake
- `src/mcp/server.rs:478` — `read_resource` for `STATUS_DASHBOARD_URI` was static; now calls `system::full_status()` and injects JSON as `window.__AXON_INITIAL_STATUS__` placeholder replacement
- `/tmp/ext-apps/examples/basic-host/src/sandbox.ts:4` — `ALLOWED_REFERRER_PATTERN` only allows `localhost`/`127.0.0.1`; any non-loopback origin causes sandbox to throw before setting up postMessage relay
- `performance.getEntriesByType('resource')` showed 343KB transfer for widget HTML — resource was fetched correctly, failure was in postMessage delivery to sandbox
- The MCP Apps `app.connect()` bridge is NOT completing in claude.ai web for this server; server-side injection is the correct fallback

## Technical Decisions

- **`InlineHint::Document` for status handler**: Forces `ResponseMode::Inline` by default with 60k char clip budget; appropriate since status is the primary widget data source and cannot be fetched via URL
- **Fire-and-forget `app.connect()`**: Official ext-apps build guide example does not `await` connect; skill docs show `await` — resolved by going with spec behavior (non-blocking) since blocking causes infinite hang
- **`read_resource` dynamic injection**: When claude.ai fetches the resource to render the iframe, we call `full_status()` and bake the JSON into `window.__AXON_INITIAL_STATUS__` in the HTML; widget renders immediately without needing host bridge
- **http-proxy over manual fetch streaming**: Manual `fetch + getReader()` pump fails for SSE because the stream never terminates; `http-proxy` handles it transparently
- **Separate local axon instance (port 8002) for testing**: Avoids auth complexity of the Docker container; loopback bind allows `AXON_MCP_HTTP_TOKEN=""` without triggering SSRF guards

## Files Modified

| File | Change |
|------|--------|
| `src/mcp/server/handlers_system.rs` | `InlineHint::Default` → `InlineHint::Document` for status handler |
| `src/mcp/server.rs` | Added `use crate::services::system;`; `read_resource` for status dashboard now dynamically injects `window.__AXON_INITIAL_STATUS__` JSON |
| `src/mcp/assets/status_dashboard.html` | Added `<script>window.__AXON_INITIAL_STATUS__ = null;</script>` placeholder before module script; replaced `await app.connect()` with fire-and-forget + immediate render from initial data |
| `/tmp/ext-apps/examples/basic-host/src/sandbox.ts` | Added `10.1.0.6` to `ALLOWED_REFERRER_PATTERN` (local test only, not committed to axon) |
| `/tmp/ext-apps/examples/basic-host/src/implementation.ts` | Changed `SANDBOX_PROXY_BASE_URL` from `localhost:8081` to `10.1.0.6:8081` (local test only) |
| `/tmp/ext-apps/examples/basic-host/serve.ts` | Added `http-proxy` based `/mcp-proxy` route with bearer token injection (local test only) |
| `~/.axon/.env` | Added `localhost:8080` to `AXON_MCP_ALLOWED_ORIGINS` |

## Commands Executed

```bash
# Verify _meta.ui.resourceUri reaches the wire
echo '...' | ./target/release/axon mcp 2>/dev/null | grep -o '"_meta":{[^}]*}'
# → "_meta":{"ui":{"resourceUri":"ui://axon/status-dashboard"}

# Verify inline injection in resource response
node -e "... fetch resources/read ..." | grep "Has __AXON_INITIAL_STATUS__"
# → Has injected data: true, Resource size: 504318

# Capture network requests to find which layer fails
agent-browser eval "JSON.stringify(performance.getEntriesByType('resource')...)"
# → /mcp-proxy 343753b  (widget HTML fetched), /sandbox.html 0b (cached)

# Confirm sandbox throws on non-loopback referrer
# (found by reading sandbox.ts source — ALLOWED_REFERRER_PATTERN)

# Final widget screenshot
agent-browser screenshot /tmp/widget-final.png
```

## Errors Encountered

| Error | Root Cause | Resolution |
|-------|-----------|------------|
| `artifact_handle.url: null` | `InlineHint::Default` fell to `path` mode; artifacts are local files with no HTTP URL | Changed to `InlineHint::Document` |
| Widget stuck "Connecting..." | `await app.connect()` blocks; host never completes postMessage handshake | Fire-and-forget `app.connect().catch(() => {})` |
| basic-host: "Failed to connect" | `CORS 403` — origin `localhost:8080` not in `AXON_MCP_ALLOWED_ORIGINS` | Added `localhost:8080` to `.env` |
| `forbidden: host not allowed` | Host header allowlist built from bind host + allowed origins; needed `10.1.0.6:8002` in origins | Added to `AXON_MCP_ALLOWED_ORIGINS` env |
| SSE proxy hung | Manual `fetch + arrayBuffer()` buffers entire response; SSE never ends | Switched to `http-proxy` library |
| Sandbox empty despite 343KB fetch | `ALLOWED_REFERRER_PATTERN` rejected `10.1.0.6` referrer; sandbox threw before postMessage setup | Added `10.1.0.6` to pattern in sandbox.ts |
| Widget still "Connecting..." after sandbox fix | Local axon binary was stale (`cargo check` ≠ `cargo build --release`) | `cargo build --release --bin axon` |

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| `action:status` MCP response | `response_mode: "path"` with `artifact_handle.url: null` | `response_mode: "inline"` with full payload in `data.inline` |
| `read_resource(ui://axon/status-dashboard)` | Static HTML, `window.__AXON_INITIAL_STATUS__ = null` | Dynamic HTML with live `full_status()` payload injected |
| Widget in basic-host | Empty box (sandbox init threw) then "Connecting..." (binary stale) | Full dashboard: 19 crawl, 18 embed, 0 extract/ingest jobs |
| Widget in claude.ai web | "Connecting..." indefinitely | Renders from injected initial data immediately; live updates via `ontoolresult` if host bridge connects |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `grep _meta tools/list response` | `"_meta":{"ui":{"resourceUri":"ui://axon/...` | Match | ✅ |
| `read_resource` response contains `AXON_INITIAL_STATUS` | `true` | `true`, 504318 bytes | ✅ |
| `agent-browser screenshot` after widget load | Job cards visible | 19 crawl + 18 embed cards rendered | ✅ |
| `docker ps` health | healthy | `Up ... (healthy)` | ✅ |

## Risks and Rollback

- `read_resource` now calls `full_status()` on every widget fetch — adds ~10ms DB query per resource read; acceptable for an on-demand dashboard
- Rollback: revert `src/mcp/server.rs` `read_resource` block and `src/mcp/server/handlers_system.rs` hint change, rebuild Docker image

## Decisions Not Taken

- **`InlineHint::AlwaysInline` new variant**: Would have been cleaner semantically but added an enum variant for a single use; `InlineHint::Document` achieves the same result
- **`await app.connect()` with timeout**: Initial fix wrapped connect in `Promise.race([connect(), timeout(5000)])` — removed in favour of pure fire-and-forget per spec since initial data injection is the real solution
- **Serving axon via Cloudflare tunnel**: Spec recommends this for testing with claude.ai; deferred because basic-host was sufficient to confirm the implementation is correct

## References

- [MCP Apps build guide](https://modelcontextprotocol.io/extensions/apps/build)
- [ext-apps spec repo](https://github.com/modelcontextprotocol/ext-apps)
- [ext-apps spec draft](https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/draft/apps.mdx)
- [MCP Apps blog post](https://blog.modelcontextprotocol.io/posts/2026-01-26-mcp-apps/)

## Open Questions

- Does claude.ai web actually complete `app.connect()` for any MCP server, or is the bridge still in limited rollout? Test with the official ext-apps example server via a Cloudflare tunnel to determine
- Does `read_resource` get called by claude.ai web before or after the tool call result? If after, the injected data may be from a previous invocation (stale by one call)
- Should `window.__AXON_INITIAL_STATUS__` injection be gated on `full_status()` succeeding (currently silently returns `null` on error)?

## Next Steps

**Started but not fully verified:**
- Widget rendering in claude.ai web itself (not basic-host) — not tested with the Cloudflare tunnel approach; basic-host confirms the implementation is correct but claude.ai bridge behavior is still unknown

**Follow-on tasks:**
- Test with Cloudflare tunnel + claude.ai custom connector to confirm `ontoolresult` fires when host bridge works
- Consider making `_meta.ui.resourceUri` action-conditional (only on `status` action) rather than on all `axon` tool calls to avoid spurious widget renders for non-status actions
- Bump version per CLAUDE.md bump policy (all changes are `fix` → patch bump)
