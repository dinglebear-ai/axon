# Security Audit: apps/web + crates/web Integration Layer

**Date:** 2026-03-13
**Auditor:** Security audit (Phase 2 deep-dive)
**Scope:** WebSocket bridge, auth gates, file serving, shell PTY, ACP integration, SSRF guards, CSP, input validation
**Codebase Version:** `main` @ `fe11a78d`

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Critical Findings](#critical-findings)
3. [High Findings](#high-findings)
4. [Medium Findings](#medium-findings)
5. [Low Findings](#low-findings)
6. [Informational / Defense-in-Depth](#informational)
7. [Positive Security Observations](#positive-observations)

---

## Executive Summary

The web integration layer demonstrates strong security engineering in several areas: constant-time token comparison, proper canonicalization-based path traversal prevention, MCP command blocklisting, and Zod-validated inputs on the TypeScript side. However, the audit identified **2 critical**, **3 high**, **5 medium**, and **4 low** severity findings.

The most urgent issues are (1) the Rust WS/output auth gate ignoring HTTP header-based tokens, forcing all auth through query parameters (which are logged in access logs and browser history), and (2) debug builds silently disabling all authentication on the Rust backend.

---

## Critical Findings

### C-1: Rust `check_auth` Ignores HTTP Header Tokens (WS + Output Routes)

**Severity:** Critical (CVSS 8.1)
**CWE:** CWE-287 (Improper Authentication)
**Location:** `crates/web/tailscale_auth.rs:41-65`, `crates/web.rs:145-151`

**Description:**

The Rust `check_auth()` function takes `_headers: &HeaderMap` (note the underscore prefix indicating the parameter is unused) and only validates the `query_token` parameter. For `/ws` and `/output/*` routes, the caller (`http_auth` in `web.rs:145-151`) passes only `params.token` (the `?token=` query parameter):

```rust
fn http_auth(
    req_headers: &HeaderMap,
    query_token: Option<&str>,
    api_token: Option<&str>,
) -> AuthOutcome {
    check_auth(req_headers, query_token, api_token) // headers are UNUSED inside
}
```

Meanwhile, the download routes in `download.rs:34-50` correctly extract tokens from `x-api-key` / `Authorization: Bearer` headers before falling back to query params -- but this extraction happens outside `check_auth()` and is passed as the `query_token` parameter.

**Impact:**

1. For `/ws` and `/output/*`, only `?token=` query parameter auth works. Header-based auth (`Authorization: Bearer`, `x-api-key`) is silently ignored.
2. The `?token=` parameter appears in server access logs, browser history, referrer headers, and proxy logs -- exposing the API token broadly.
3. The Next.js client (`use-axon-ws.ts`) appends `?token=` to the WS URL, so the token is visible in any network inspection or proxy log.
4. Inconsistent auth behavior between download routes (headers work) and WS/output routes (headers silently fail) can confuse operators into thinking header auth works everywhere.

**Remediation:**

1. Update `check_auth()` to extract tokens from `x-api-key` and `Authorization: Bearer` headers (like `auth_download` already does), not just the query token parameter.
2. Make `?token=` query parameter auth opt-in via an env var (it is inherently less secure).
3. Document the auth extraction priority: headers first, query param fallback.

---

### C-2: Debug Builds Silently Disable All Authentication

**Severity:** Critical (CVSS 9.1)
**CWE:** CWE-489 (Active Debug Code), CWE-306 (Missing Authentication)
**Location:** `crates/web/tailscale_auth.rs:57-64`

**Description:**

When `AXON_WEB_API_TOKEN` is not set, the Rust `check_auth()` function uses a `#[cfg(debug_assertions)]` gate to auto-grant access:

```rust
#[cfg(any(debug_assertions, test))]
{
    AuthOutcome::Token  // <-- full access granted with zero credentials
}
#[cfg(not(any(debug_assertions, test)))]
{
    AuthOutcome::Denied(DenyReason::NoAuthConfigured)
}
```

A `cargo build` (without `--release`) produces a binary with `debug_assertions` enabled. Any debug build deployed without `AXON_WEB_API_TOKEN` set has **zero authentication** on all WebSocket, download, and output file routes.

**Impact:**

- If a developer or CI pipeline deploys a debug build (which is the default `cargo build`), all routes are open to unauthenticated access.
- The server log message at line 80 says "WS gate: open in debug/test builds" but this can easily be missed.
- Combined with the shell PTY loopback bypass (see M-1), any attacker on the local network gets unrestricted shell access.

**Remediation:**

1. Remove the `#[cfg(debug_assertions)]` auto-grant. Require `AXON_WEB_API_TOKEN` in all environments.
2. If a dev bypass is needed, use an explicit env var like `AXON_WEB_ALLOW_INSECURE_DEV=true` (matching the TypeScript side) rather than a compile-time flag.
3. Refuse to start the server if no auth mechanism is configured, or bind to `127.0.0.1` only when unauth'd.

---

## High Findings

### H-1: Shell PTY Has No Input Size Limit or Rate Limiting

**Severity:** High (CVSS 7.5)
**CWE:** CWE-400 (Uncontrolled Resource Consumption), CWE-770 (Allocation of Resources Without Limits)
**Location:** `crates/web/shell.rs:96-134`

**Description:**

The shell PTY handler (`run_shell`) reads WebSocket messages in a loop and writes them directly to the PTY stdin with no input size validation:

```rust
Ok(ShellClientMsg::Input { data }) => {
    let _ = pty_in_tx.send(data.into_bytes()).await;
}
```

The `data` field is a `String` deserialized from JSON with no maximum length constraint. The `ShellClientMsg` struct has no `#[serde(deserialize_with)]` or custom length validation.

**Impact:**

1. An attacker can send arbitrarily large `data` payloads, consuming memory in the channel buffer (256 entries, unbounded message size).
2. There is no rate limiting on input messages -- a rapid stream of input messages can saturate the PTY writer thread.
3. The `Resize` message accepts `cols` and `rows` as `u16` but there is no validation of reasonable ranges (e.g., `cols: 65535, rows: 65535` may cause display buffer issues).

**Remediation:**

1. Add a maximum size limit to `ShellClientMsg::Input::data` (e.g., 64KB per message) via a custom deserializer or post-parse check.
2. Implement rate limiting (e.g., sliding window of messages per second).
3. Clamp `Resize` dimensions to sane ranges (e.g., 1-500 cols, 1-200 rows).

---

### H-2: Session ID in `acp_resume` / `permission_response` Is Not Authenticated

**Severity:** High (CVSS 7.1)
**CWE:** CWE-639 (Authorization Bypass Through User-Controlled Key)
**Location:** `crates/web/ws_handler.rs:220-253`, `crates/web/ws_handler.rs:259-294`

**Description:**

The `acp_resume` and `permission_response` WS message types accept a `session_id` from the client with no verification that the requesting WS connection owns that session:

```rust
"acp_resume" => {
    handle_acp_resume(conn, &client_msg.session_id).await;
}
"permission_response" => {
    route_permission_response(
        &conn.permission_responders,
        client_msg.tool_call_id,
        client_msg.option_id,
        client_msg.session_id,  // <-- client-provided, unvalidated
    );
}
```

The `route_permission_response` function first checks the per-WS connection responders map, but then falls back to the global `SESSION_CACHE`:

```rust
if let Some(cached) = SESSION_CACHE.get_by_session_id_sync(&session_id)
    && let Some((_, sender)) = cached.permission_responders.remove(...)
{
    let _ = sender.send(option_id);
}
```

**Impact:**

If two browser tabs or users share the same WS backend (which they do -- any authenticated WS connection reaches the same server), one user can:
1. Replay or hijack another user's ACP session by guessing/observing the session_id.
2. Approve or deny permission requests for another user's tool executions.
3. Resume and receive buffered events from another user's sessions.

Session IDs appear to be UUIDs or hash-based keys, so guessing is unlikely for a single session but becomes feasible with many concurrent sessions.

**Remediation:**

1. Bind each ACP session to a specific WS connection ID or authenticated principal.
2. Validate that the requesting WS connection created or owns the session before allowing resume or permission responses.
3. Add a per-session secret token that must accompany all session operations.

---

### H-3: CORS Preflight Reflects Arbitrary Request Headers

**Severity:** High (CVSS 6.5)
**CWE:** CWE-346 (Origin Validation Error)
**Location:** `crates/web/cors.rs:60-81`

**Description:**

The CORS preflight handler reflects whatever the client sends in `Access-Control-Request-Headers` back as `Access-Control-Allow-Headers`:

```rust
let requested_headers = request
    .headers()
    .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
    .cloned()
    .unwrap_or_else(|| HeaderValue::from_static(DEFAULT_CORS_ALLOW_HEADERS));
response
    .headers_mut()
    .insert(header::ACCESS_CONTROL_ALLOW_HEADERS, requested_headers);
```

**Impact:**

While origin validation is still enforced (so this only matters for allowed origins), reflecting arbitrary headers weakens the security posture:
1. Any allowed origin can send any custom header (e.g., `X-Forwarded-For`, `X-Real-IP`) that backend infrastructure might trust.
2. This effectively makes the CORS header allowlist a wildcard `*`, bypassing the intent of restricting headers.

**Remediation:**

Replace the reflection with a static allowlist of permitted headers:
```rust
let allowed = HeaderValue::from_static("authorization, content-type, x-api-key");
```
Or validate the requested headers against an explicit allowlist.

---

## Medium Findings

### M-1: Shell PTY Loopback Bypass With No Audit Trail

**Severity:** Medium (CVSS 6.1)
**CWE:** CWE-778 (Insufficient Logging), CWE-288 (Authentication Bypass Using an Alternate Path)
**Location:** `crates/web.rs:269-294`

**Description:**

The shell WebSocket upgrade handler skips all authentication for loopback connections:

```rust
let is_loopback = match addr.ip() {
    IpAddr::V4(v4) => v4.is_loopback(),
    IpAddr::V6(v6) => {
        v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
    }
};
if !is_loopback {
    // auth check here
}
// no else: loopback silently passes through
```

No log entry is emitted when auth is bypassed via loopback. Combined with C-2 (debug builds disable auth entirely), this means:
1. Any process on the same host gets unrestricted shell access.
2. If the server binds to `0.0.0.0` (configurable via `AXON_SERVE_HOST`), any host behind a reverse proxy that sets `X-Forwarded-For` or uses PROXY protocol will appear as loopback at the TCP layer from the proxy's perspective.

**Impact:**

- SSRF-like attacks from other services on the same host can reach the shell PTY.
- Container escape scenarios: another container on the same Docker bridge network is NOT loopback, but misconfigurations (host networking, `network_mode: host`) would make it loopback.

**Remediation:**

1. Log all loopback bypass events at `info` level for audit trail.
2. Make the loopback bypass configurable via an env var (default off in production).
3. Consider removing the loopback bypass entirely -- if a local process needs shell access, it should use the token.

---

### M-2: `acp_resume` Session ID String Injection in JSON Response

**Severity:** Medium (CVSS 5.3)
**CWE:** CWE-116 (Improper Encoding or Escaping of Output)
**Location:** `crates/web/ws_handler.rs:238-252`

**Description:**

The `acp_resume` handler interpolates the `session_id` directly into a JSON string using `format!`:

```rust
let _ = tx.send(format!(
    r#"{{"type":"acp_resume_result","success":true,"session_id":"{session_id}","replayed":{replayed}}}"#
)).await;
```

And similarly for the failure case:
```rust
r#"{{"type":"acp_resume_result","success":false,"reason":"session not found","session_id":"{session_id}"}}"#
```

If `session_id` contains a double quote or backslash, this produces invalid or injectable JSON. While the `WsClientMsg` struct deserializes `session_id` as a `String` from JSON (so it would typically be a valid string), the raw string could still contain characters like `"`, `\`, or control characters.

**Impact:**

A crafted `session_id` like `foo","injected":"bar` would break the JSON structure, potentially causing parsing errors or injection in the frontend.

**Remediation:**

Use `serde_json::json!()` macro or `serde_json::to_string()` for all JSON construction instead of `format!` string interpolation.

---

### M-3: WebSocket Origin Check Allows No-Origin Requests

**Severity:** Medium (CVSS 5.0)
**CWE:** CWE-346 (Origin Validation Error)
**Location:** `crates/web/cors.rs:104-119`

**Description:**

The `websocket_origin_is_allowed` function returns `true` when no `Origin` header is present:

```rust
let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
else {
    return true;  // No origin = allowed
};
```

**Impact:**

Non-browser clients (curl, custom scripts, other services) can connect to `/ws` and `/ws/shell` without an `Origin` header and bypass the origin check entirely. While these clients still need a valid token (when token auth is configured), this means origin-based access control is effectively optional.

WebSocket connections from browser extensions or tabs using `fetch` in no-cors mode may also omit the `Origin` header in some edge cases.

**Remediation:**

1. For the shell PTY endpoint specifically, consider requiring an `Origin` header.
2. Document that origin checking is a defense-in-depth measure, not a primary auth mechanism.
3. Add a strict mode env var that rejects connections without an `Origin` header.

---

### M-4: MCP Config `args` Array Passed Directly to Subprocess

**Severity:** Medium (CVSS 5.9)
**CWE:** CWE-78 (OS Command Injection)
**Location:** `crates/web/execute/mcp_config.rs:121-127`

**Description:**

MCP server configurations read from `mcp.json` have their `args` arrays passed directly to the ACP adapter subprocess:

```rust
Some(AcpMcpServerConfig::Stdio {
    name,
    command: cmd,
    args: entry.args.unwrap_or_default(),  // untrusted user input from mcp.json
    env: entry.env.unwrap_or_default().into_iter().collect(),
})
```

While `is_safe_mcp_command` blocks shell interpreters (bash, sh, etc.) and requires absolute paths for path-containing commands, there is no validation on the `args` array or `env` map:

1. The `args` could include shell metacharacters or flags that alter the behavior of the command (e.g., `--exec`, `--shell`, `--run`).
2. The `env` map is passed without any sanitization -- it could override `PATH`, `LD_PRELOAD`, `NODE_OPTIONS`, or other execution-altering environment variables.

**Impact:**

If an attacker gains write access to `mcp.json` (via the `/api/mcp` POST route or filesystem access), they can:
1. Pass arbitrary arguments to any non-shell binary.
2. Override environment variables to alter execution behavior (e.g., `LD_PRELOAD` for shared library injection).

**Remediation:**

1. Validate `args` entries against an allowlist of safe flag patterns (or at minimum, reject entries starting with `--exec`, `--shell`, `--eval`, `-c`).
2. Sanitize the `env` map -- block security-sensitive keys like `LD_PRELOAD`, `LD_LIBRARY_PATH`, `NODE_OPTIONS`, `PYTHONPATH`, `PATH` (or allowlist safe keys).
3. Consider running MCP server subprocesses in a restricted environment (seccomp, namespaces).

---

### M-5: `PulseSourceResponse` Leaks Subprocess Command in API Response

**Severity:** Medium (CVSS 4.3)
**CWE:** CWE-209 (Generation of Error Message Containing Sensitive Information)
**Location:** `apps/web/app/api/pulse/source/route.ts:135-139`

**Description:**

The `/api/pulse/source` response includes the exact command that was executed:

```typescript
return NextResponse.json({
    indexed: urls,
    command: `./scripts/axon scrape ${urls.join(' ')} --json`,
    markdownBySrc: result.markdownBySrc,
} satisfies PulseSourceResponse)
```

**Impact:**

Exposes internal file paths (`./scripts/axon`), binary names, and flag patterns to the client. This information aids reconnaissance.

**Remediation:**

Remove the `command` field from the response. If debugging info is needed, log it server-side.

---

## Low Findings

### L-1: SSRF Guard Does Not Block Cloud Metadata Endpoints by IP

**Severity:** Low (CVSS 3.7)
**CWE:** CWE-918 (Server-Side Request Forgery)
**Location:** `apps/web/lib/server/url-validation.ts:163-199`

**Description:**

The SSRF validation blocks private IP ranges (10.x, 172.16-31.x, 192.168.x, 127.x, 169.254.x) and loopback hostnames. However, it does not block:

1. **Decimal/octal IP representations**: `http://2130706433` (127.0.0.1 as decimal integer) -- `URL` constructor may resolve these.
2. **DNS rebinding**: As documented in the code's own comment, a hostname resolving to a public IP at validation time may rebind to a private IP at fetch time.

Since this project is self-hosted (not cloud), the cloud metadata endpoint risk (169.254.169.254) is lower, and the SSRF guard is a defense-in-depth layer before the subprocess. The risk is acceptably low.

**Remediation:**

1. Consider adding decimal/octal IP detection for defense-in-depth.
2. Document the DNS rebinding limitation and accept the residual risk.

---

### L-2: `output_dir` Traversal Check Uses String Contains Instead of Component Analysis

**Severity:** Low (CVSS 3.1)
**CWE:** CWE-22 (Path Traversal)
**Location:** `crates/web.rs:172`

**Description:**

The `serve_output_file` handler checks for path traversal with a string-based check:

```rust
if file_path.contains("..") || file_path.contains('\0') {
    return (StatusCode::BAD_REQUEST, "invalid path").into_response();
}
```

While this is followed by a proper canonicalize-and-starts-with check (lines 179-187), the string-based check is an imprecise early guard. A filename literally containing `..` (e.g., `notes..txt`) would be falsely rejected by the string check but would pass the canonicalization check.

**Impact:** Minor false positive potential. The security is correct due to the canonicalization check that follows.

**Remediation:**

Use path component analysis for the early check:
```rust
use std::path::Component;
if Path::new(&file_path).components().any(|c| c == Component::ParentDir) || file_path.contains('\0') {
```

---

### L-3: No Maximum WS Connection Limit

**Severity:** Low (CVSS 3.7)
**CWE:** CWE-400 (Uncontrolled Resource Consumption)
**Location:** `crates/web.rs:104-113`

**Description:**

The Axum router accepts unlimited WebSocket connections. Each connection spawns a forward task, subscribes to the stats broadcast channel (64 slots), and creates two mpsc channels (256 each). There is no connection limit.

**Impact:**

A connection flood from authenticated clients (or from any client if C-2 applies) can exhaust memory and file descriptors.

**Remediation:**

Add a concurrent connection limit using a tower middleware or a semaphore gate before the WS upgrade handler.

---

### L-4: CSP Allows `'unsafe-inline'` for Scripts

**Severity:** Low (CVSS 3.5)
**CWE:** CWE-79 (Cross-site Scripting)
**Location:** `apps/web/lib/server/csp.ts:71`

**Description:**

The CSP includes `script-src 'self' 'unsafe-inline'`:

```typescript
`script-src 'self' 'unsafe-inline'${isDev ? " 'unsafe-eval'" : ''}`,
```

`'unsafe-inline'` allows inline `<script>` tags and event handlers, which reduces the effectiveness of CSP against XSS.

**Impact:**

If any XSS vector exists in the application (stored XSS in crawl results, reflected XSS in error messages), `'unsafe-inline'` allows the injected script to execute.

**Remediation:**

1. Use nonce-based CSP (`'nonce-<random>'`) for legitimate inline scripts.
2. Move all inline scripts to external files.
3. As a minimum, add `'strict-dynamic'` which ignores `'unsafe-inline'` in supporting browsers.

---

## Informational / Defense-in-Depth {#informational}

### I-1: ACP Permission Flags Silently Dropped (Confirmed C-3 from Phase 1)

**Location:** `crates/web/execute/sync_mode/acp_adapter.rs:40-48`

The `resolve_acp_adapter_command_from_values` function hardcodes `enable_fs: true` and `enable_terminal: true` regardless of frontend-sent flags. These flags (`enable_fs`, `enable_terminal`, `permission_timeout_secs`, `adapter_timeout_secs`) are not in `ALLOWED_FLAGS` and are silently dropped at the arg-building layer.

The frontend may send these flags expecting them to restrict permissions, but they have no effect. This is a correctness issue that could have security implications if users rely on these controls.

### I-2: `DefaultHasher` for MCP Server Fingerprinting Is Non-Deterministic

**Location:** `crates/web/execute/sync_mode/pulse_chat.rs:204-211`

`DefaultHasher` is not guaranteed to produce the same output across Rust versions or process restarts (it is SipHash with randomized keys in some configurations). For a cache key, this means that after a process restart, the same MCP config may produce a different fingerprint, causing unnecessary session re-creation. Not a security issue, but worth noting.

### I-3: `env` Extension Included in Workspace File Browsing Allowlist

**Location:** `apps/web/app/api/workspace/route.ts:37`

The TEXT_EXTENSIONS set includes `.env`, meaning the workspace browser can read `.env` files if they exist under the workspace root. The path validation prevents reading outside the workspace, but `.env` files within the workspace may contain secrets.

---

## Positive Security Observations {#positive-observations}

The following security measures are well-implemented:

1. **Constant-time token comparison** -- Both Rust (`tailscale_auth.rs:29-38`) and TypeScript (`proxy.ts:110-118`, `shell-auth.mjs:100-103`) use constant-time comparison for token validation, preventing timing oracle attacks.

2. **Path traversal prevention via canonicalization** -- All file-serving routes (`serve_output_file`, `serve_file`, `handle_read_file`, workspace API) use `canonicalize()` + `starts_with()` after the initial string checks, which is the correct defense.

3. **MCP command safety checks** -- `is_safe_mcp_command()` blocks shell interpreters by basename and rejects relative paths, preventing the most obvious command injection vectors.

4. **WS mode/flag allowlisting** -- The `ALLOWED_MODES` and `ALLOWED_FLAGS` whitelists in `execute.rs` prevent unknown modes from reaching the subprocess, and unknown flag keys are rejected with an error before processing.

5. **Subprocess `--wait` flag stripping for async modes** -- `build_args` correctly strips the `--wait` flag for async modes, preventing clients from converting fire-and-forget operations into blocking ones.

6. **Input leading-dash stripping** -- `build_args` strips leading dashes from user input (`trimmed.trim_start_matches('-')`), preventing flag injection through the input field.

7. **Download route validation** -- Job IDs are validated as UUID format before filesystem operations, filenames are sanitized for Content-Disposition, and manifest paths are validated as safe relative paths.

8. **Shell env allowlisting** -- Both the Rust PTY (`shell.rs`) and Node.js shell server (`shell-auth.mjs`) use explicit env var allowlists, preventing secret leakage to shell child processes.

9. **ACP session semaphore** -- The `ACP_SESSION_SEMAPHORE` (default 8) prevents unbounded thread consumption from concurrent pulse_chat sessions.

10. **Zod schema validation** -- Frontend request bodies are validated with strict Zod schemas (`PulseChatRequestSchema`, `PulseSourceRequestSchema`) with appropriate size limits on fields.

11. **SSRF guard coverage** -- IPv4 private ranges, IPv6 ULA/link-local/mapped, and known loopback hostnames are all blocked, with a well-structured IPv6 parser.

12. **CSP consistency** -- Both `proxy.ts` and `next.config.ts` use the same shared `buildCspHeader()` function, eliminating the CSP divergence risk.

---

## Summary of Recommendations by Priority

| Priority | Finding | Action |
|----------|---------|--------|
| **Immediate** | C-1: Header auth ignored in Rust WS gate | Fix `check_auth` to extract from headers |
| **Immediate** | C-2: Debug builds disable auth | Replace `#[cfg(debug_assertions)]` with explicit env var |
| **This Sprint** | H-1: Shell PTY no input size limit | Add size + rate limits to shell input |
| **This Sprint** | H-2: Session ID not connection-bound | Bind ACP sessions to WS connections |
| **This Sprint** | H-3: CORS reflects arbitrary headers | Use static header allowlist |
| **Next Sprint** | M-1: Shell loopback bypass unlogged | Add audit logging |
| **Next Sprint** | M-2: JSON string interpolation | Use serde_json for all JSON construction |
| **Next Sprint** | M-3: No-origin WS allowed | Add strict mode option |
| **Next Sprint** | M-4: MCP args/env unsanitized | Validate args, blocklist dangerous env vars |
| **Next Sprint** | M-5: Command leaked in response | Remove `command` field from API response |
| **Backlog** | L-1 through L-4 | See individual remediation steps |
