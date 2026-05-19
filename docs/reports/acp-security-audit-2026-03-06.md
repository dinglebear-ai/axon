# ACP (Agent Client Protocol) Security Audit

**Date:** 2026-03-06
**Auditor:** Security Review (DevSecOps)
**Scope:** ACP client scaffold, adapter spawning, prompt turn/probe execution, WS bridge, frontend Pulse chat
**Classification:** Read-only audit -- no changes applied

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Critical Findings](#critical-findings)
3. [High Findings](#high-findings)
4. [Medium Findings](#medium-findings)
5. [Low Findings](#low-findings)
6. [Informational Findings](#informational-findings)
7. [Positive Security Controls](#positive-security-controls)

---

## Executive Summary

The ACP implementation was audited across the Rust backend (`crates/services/acp.rs`, `crates/web/execute/sync_mode.rs`, `crates/web/execute/events.rs`) and the TypeScript/Next.js frontend (`apps/web/lib/pulse/`, `apps/web/app/api/pulse/`, `apps/web/hooks/`). The review focused on command injection, path traversal, environment isolation, input validation, authentication bypass, SSRF, denial of service, information disclosure, and XSS vectors.

**Summary of findings:**

| Severity | Count |
|----------|-------|
| Critical | 2 |
| High | 4 |
| Medium | 5 |
| Low | 4 |
| Informational | 3 |

The two critical findings involve argument injection through the Codex model override and unsanitized `--tools` pass-through. The high findings cover silent event loss, missing adapter lifecycle timeouts, full environment inheritance by child processes, and the `--dangerously-skip-permissions` default. Several medium findings address path traversal edge cases, timing-based auth concerns, and missing rate limiting.

---

## Critical Findings

### SEC-01: Argument Injection via Codex Model Override

**Severity:** Critical (CVSS 8.6)
**CWE:** CWE-88 (Improper Neutralization of Argument Delimiters)
**Location:** `crates/services/acp.rs:231-245`

**Description:**

The function `append_codex_model_override()` takes user-supplied model text from the frontend and injects it directly into the adapter's CLI arguments:

```rust
fn append_codex_model_override(
    adapter: &AcpAdapterCommand,
    requested_model: Option<&str>,
) -> AcpAdapterCommand {
    let Some(model) = normalized_requested_model(requested_model) else {
        return adapter.clone();
    };
    // ...
    let mut next = adapter.clone();
    next.args.push("-c".to_string());
    next.args.push(format!("model=\"{model}\""));
    next
}
```

The `normalized_requested_model()` function only trims whitespace and filters empty strings or the literal `"default"`. It does not validate the model string against an allowlist or sanitize shell metacharacters.

**Attack scenario:**

A user sends a `pulse_chat` request with:
```json
{ "model": "gpt-4\"\n--dangerously-allow-all" }
```

The resulting argument becomes `model="gpt-4"\n--dangerously-allow-all"`, which depending on how the Codex CLI parses its `-c` flag, could inject additional configuration directives or flags. While `tokio::process::Command` passes arguments as an array (not through a shell), the `-c` flag itself is a configuration override mechanism in Codex -- injecting arbitrary key=value pairs through the model string is the real attack surface.

A more practical attack: `model: "gpt-4\"\nsomething_dangerous=\"true"` could set arbitrary Codex config keys through the `-c` flag.

**Remediation:**

```rust
fn normalized_requested_model(model: Option<&str>) -> Option<String> {
    let value = model?.trim();
    if value.is_empty() || value == "default" {
        return None;
    }
    // Allowlist: only alphanumeric, hyphens, dots, underscores, forward slashes
    // (covers model IDs like "gpt-4o", "claude-3.5-sonnet", "o1-mini")
    if !value.chars().all(|c| c.is_alphanumeric() || "-._/".contains(c)) {
        log::warn!("ACP: rejected model name with invalid characters: {:?}", value);
        return None;
    }
    if value.len() > 128 {
        return None;
    }
    Some(value.to_string())
}
```

---

### SEC-02: Unsanitized `--tools` / `toolsRestrict` Pass-Through

**Severity:** Critical (CVSS 8.1)
**CWE:** CWE-88 (Improper Neutralization of Argument Delimiters)
**Location:** `apps/web/app/api/pulse/chat/claude-stream-types.ts:209-211`

**Description:**

The `buildClaudeArgs()` function passes `toolsRestrict` directly to `--tools` without any validation:

```typescript
if (extra?.toolsRestrict) {
    args.push('--tools', extra.toolsRestrict)
}
```

Compare this with `allowedTools` and `disallowedTools` on lines 173-192, which are filtered through `TOOL_ENTRY_RE = /^[a-zA-Z][a-zA-Z0-9_*(),:]*$/`. The `toolsRestrict` field bypasses this sanitization entirely.

The Zod schema in `types.ts:99-101` does apply a regex:
```typescript
toolsRestrict: z.string().regex(/^[a-zA-Z0-9,\-.:]*$/).optional()
```

However, this regex is more permissive than `TOOL_ENTRY_RE` and allows characters like `.` and `:` which could have special meaning to the Claude CLI argument parser. More critically, the Zod validation happens at the HTTP boundary but the WS path (`pulse_chat` mode in `sync_mode.rs`) does NOT apply the same Zod schema -- it passes `model` and other flags directly from the WS JSON payload without frontend Zod validation.

**Attack scenario:**

Through the WS path, a crafted message could send arbitrary strings as tool restrictions. Even through the HTTP path, the regex mismatch between the Zod schema and `TOOL_ENTRY_RE` means some values pass Zod validation but would be rejected by the stricter regex if it were applied.

**Remediation:**

Apply the same `TOOL_ENTRY_RE` filter used for `allowedTools`:

```typescript
if (extra?.toolsRestrict) {
    const filtered = extra.toolsRestrict
        .split(',')
        .map((t) => t.trim())
        .filter((t) => TOOL_ENTRY_RE.test(t))
        .join(',')
    if (filtered) {
        args.push('--tools', filtered)
    }
}
```

---

## High Findings

### SEC-03: Silent Event Loss via `try_send` on Bounded Channel

**Severity:** High (CVSS 7.1)
**CWE:** CWE-223 (Omission of Security-relevant Information)
**Location:** `crates/services/events.rs:10-14`

**Description:**

The `emit()` function uses `try_send()` which silently drops events when the channel is full:

```rust
pub fn emit(tx: &Option<mpsc::Sender<ServiceEvent>>, event: ServiceEvent) {
    if let Some(sender) = tx {
        let _ = sender.try_send(event);
    }
}
```

The channel is created with capacity 32 (`mpsc::channel::<ServiceEvent>(32)` in `sync_mode.rs:728,772`). The `let _ =` discards the `Err(TrySendError::Full(_))` result.

This means critical events like `TurnResult` (which carries the final assistant response and session ID) or `PermissionRequest` (which carries tool approval requests) can be silently dropped if the consumer is slow or if a burst of `AssistantDelta` events fills the buffer.

**Impact:**

- A dropped `TurnResult` means the frontend never receives the final response, leading to a permanently "loading" state.
- A dropped `PermissionRequest` means a tool execution request is silently denied (since the default is `Cancelled`) AND the user is never informed that a permission was requested.
- Dropped `ConfigOptionsUpdate` events could cause the UI to show stale model/config options.

**Remediation:**

For critical events, use `.send().await` (blocking) instead of `try_send()`. Alternatively, increase channel capacity and add logging on drop:

```rust
pub fn emit(tx: &Option<mpsc::Sender<ServiceEvent>>, event: ServiceEvent) {
    if let Some(sender) = tx {
        if let Err(err) = sender.try_send(event) {
            match &err {
                mpsc::error::TrySendError::Full(dropped) => {
                    log::error!(
                        "ACP event channel full -- dropped event: {:?}",
                        std::mem::discriminant(dropped)
                    );
                }
                mpsc::error::TrySendError::Closed(_) => {
                    log::warn!("ACP event channel closed");
                }
            }
        }
    }
}
```

For the `TurnResult` specifically, consider sending it via a dedicated oneshot channel that cannot be dropped.

---

### SEC-04: No Timeout on ACP Adapter Lifecycle

**Severity:** High (CVSS 7.0)
**CWE:** CWE-400 (Uncontrolled Resource Consumption)
**Location:** `crates/services/acp.rs:120-162` (`start_prompt_turn`), `crates/services/acp.rs:164-202` (`start_session_probe`)

**Description:**

The `start_prompt_turn` and `start_session_probe` methods spawn a child process and await its completion with no timeout:

```rust
let join = tokio::task::spawn_blocking(move || {
    // ...
    local.block_on(&rt, run_prompt_turn(adapter, initialize, session_setup, req_owned, tx))
}).await
```

The `spawn_blocking` call blocks a thread from tokio's blocking pool indefinitely. If the adapter process hangs (network issue, infinite loop in the LLM, deadlocked I/O), the thread is consumed permanently. With repeated requests to a hung adapter, the entire blocking thread pool can be exhausted, causing a system-wide denial of service affecting ALL `spawn_blocking` callers (database operations, file I/O, etc.).

The child process kill at the end (`child.kill().await`) only executes on the happy path -- if the ACP protocol exchange hangs at `conn.initialize()`, `conn.new_session()`, or `conn.prompt()`, the kill is never reached.

**Remediation:**

Wrap the entire operation in a `tokio::time::timeout`:

```rust
pub async fn start_prompt_turn(
    &self,
    req: &AcpPromptTurnRequest,
    cwd: impl AsRef<Path>,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<(), Box<dyn Error>> {
    const ACP_TURN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

    let result = tokio::time::timeout(ACP_TURN_TIMEOUT, async {
        // ... existing logic ...
    }).await;

    match result {
        Ok(inner) => inner,
        Err(_elapsed) => {
            Err("ACP prompt turn timed out after 600 seconds".into())
        }
    }
}
```

Additionally, the child process should be killed in a `Drop` guard or `scopeguard` to ensure cleanup on all exit paths.

---

### SEC-05: Full Environment Inheritance by ACP Adapter

**Severity:** High (CVSS 6.8)
**CWE:** CWE-526 (Exposure of Sensitive Information Through Environmental Variables)
**Location:** `crates/services/acp.rs:54-78` (`spawn_adapter`)

**Description:**

The `spawn_adapter()` method removes only four specific environment variables:

```rust
command.env_remove("OPENAI_BASE_URL");
command.env_remove("OPENAI_API_KEY");
command.env_remove("OPENAI_MODEL");
command.env_remove("CLAUDECODE");
```

The child process inherits ALL other environment variables from the parent, including:

- `AXON_PG_URL` -- full Postgres connection string with credentials
- `AXON_REDIS_URL` -- Redis connection string with password
- `AXON_AMQP_URL` -- RabbitMQ connection string with credentials
- `AXON_WEB_API_TOKEN` -- the API authentication token
- `TAVILY_API_KEY` -- third-party API key
- `GITHUB_TOKEN` -- GitHub access token
- `REDDIT_CLIENT_SECRET` -- Reddit OAuth secret
- `QDRANT_URL` -- Qdrant vector DB endpoint
- Any other secrets in the process environment

The adapter (Claude CLI or Codex CLI) could potentially access these credentials through tool use, environment inspection commands, or if the adapter itself has a security vulnerability.

**Remediation:**

Use `env_clear()` and then explicitly set only the variables the adapter needs:

```rust
pub fn spawn_adapter(&self) -> Result<tokio::process::Child, Box<dyn Error>> {
    self.validate_adapter()?;
    let mut command = tokio::process::Command::new(&self.adapter.program);
    command.args(&self.adapter.args);
    if let Some(cwd) = &self.adapter.cwd {
        command.current_dir(cwd);
    }

    // Start with a clean environment -- adapters should not see infrastructure secrets.
    command.env_clear();

    // Explicitly pass through only what the adapter needs.
    let passthrough = ["PATH", "HOME", "USER", "LANG", "TERM", "XDG_CONFIG_HOME",
                       "XDG_DATA_HOME", "XDG_CACHE_HOME", "ANTHROPIC_API_KEY",
                       "OPENAI_API_KEY_CODEX"];  // adapter-specific keys only
    for key in &passthrough {
        if let Some(val) = std::env::var_os(key) {
            command.env(key, val);
        }
    }

    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let child = command.spawn()?;
    Ok(child)
}
```

---

### SEC-06: `--dangerously-skip-permissions` Enabled by Default

**Severity:** High (CVSS 6.5)
**CWE:** CWE-269 (Improper Privilege Management)
**Location:** `apps/web/app/api/pulse/chat/claude-stream-types.ts:137-139`

**Description:**

The Claude CLI subprocess is spawned with `--dangerously-skip-permissions` by default:

```typescript
...(process.env.PULSE_SKIP_PERMISSIONS !== 'false' ? ['--dangerously-skip-permissions'] : []),
```

This means the Claude agent can execute file reads/writes, shell commands, and other tool actions without any human approval. The only control is the `PULSE_SKIP_PERMISSIONS` environment variable, which must be explicitly set to the string `"false"` to disable the bypass.

Combined with the `permissionLevel: 'bypass-permissions'` option in the frontend Zod schema (`types.ts:28,73`), any authenticated user can instruct the agent to perform arbitrary actions on the host filesystem and network.

The ACP path (`crates/services/acp.rs:1046-1055`) correctly defaults to `RequestPermissionOutcome::Cancelled`, but the HTTP Pulse chat path bypasses this entirely by using `--dangerously-skip-permissions` as a CLI flag.

**Impact:**

Any authenticated user with access to the Pulse chat interface can instruct Claude to:
- Read/write arbitrary files within the container
- Execute shell commands
- Make network requests to internal services
- Exfiltrate data via tool use

**Remediation:**

1. Invert the default: require `PULSE_SKIP_PERMISSIONS=true` to enable the bypass, not `=false` to disable it.
2. Add defense-in-depth: even with `--dangerously-skip-permissions`, Claude CLI respects `--disallowedTools`. Consider blocking `Bash(*)` and `Write` by default unless the user explicitly enables them.
3. Document the risk prominently in deployment guides.

---

## Medium Findings

### SEC-07: Path Traversal via `cwd` -- Insufficient Validation

**Severity:** Medium (CVSS 5.3)
**CWE:** CWE-22 (Improper Limitation of a Pathname to a Restricted Directory)
**Location:** `crates/services/acp.rs:333-338`

**Description:**

The `validate_session_cwd()` function only checks that the path is absolute:

```rust
pub fn validate_session_cwd(cwd: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if !cwd.is_absolute() {
        return Err("ACP session cwd must be an absolute path".into());
    }
    Ok(cwd.to_path_buf())
}
```

This allows any absolute path, including `/etc`, `/root`, `/proc`, or any other sensitive directory. While the `cwd` is ultimately set via `env::current_dir()` in `sync_mode.rs:741,776` (not from user input in the current code), the function's signature accepts any `impl AsRef<Path>`, and future callers could pass user-controlled values.

Additionally, the function does not resolve symlinks or normalize `..` components. A path like `/workspace/../etc/shadow` would pass validation.

**Remediation:**

```rust
pub fn validate_session_cwd(cwd: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if !cwd.is_absolute() {
        return Err("ACP session cwd must be an absolute path".into());
    }
    // Canonicalize resolves symlinks and .. components.
    let canonical = cwd.canonicalize().map_err(|e| {
        format!("ACP session cwd does not exist or is inaccessible: {e}")
    })?;
    // Optionally: check against an allowlist of workspace directories.
    Ok(canonical)
}
```

---

### SEC-08: WS Token Comparison is Not Constant-Time

**Severity:** Medium (CVSS 4.8)
**CWE:** CWE-208 (Observable Timing Discrepancy)
**Location:** `crates/web.rs:189`

**Description:**

The WS auth token comparison uses direct string equality:

```rust
if token != expected.as_str() {
    log::warn!("ws upgrade rejected: invalid token from {}", addr.ip());
    return (axum::http::StatusCode::UNAUTHORIZED, "invalid token").into_response();
}
```

Standard `!=` comparison on strings short-circuits on the first differing byte, leaking information about the token prefix through timing side-channels. While exploitation requires high-precision timing measurements, it is a well-documented attack vector (CWE-208).

**Remediation:**

Use a constant-time comparison:

```rust
use subtle::ConstantTimeEq;
// Or manually:
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
```

---

### SEC-09: Unchecked `response.body!` Non-Null Assertion

**Severity:** Medium (CVSS 4.3)
**CWE:** CWE-476 (NULL Pointer Dereference)
**Location:** `apps/web/lib/pulse/chat-api.ts:57`

**Description:**

The `readNdjsonStream()` function uses a TypeScript non-null assertion on `response.body`:

```typescript
const reader = response.body!.getReader()
```

If `response.body` is `null` (which can happen with certain HTTP responses, opaque responses, or when the response has already been consumed), this will throw a runtime `TypeError` that is not caught by the caller.

The caller in `runChatPrompt()` (line 208) does check `response.body` before calling:
```typescript
if (isNdjson && response.body) {
    return readNdjsonStream(response, onEvent)
}
```

However, `readNdjsonStream` is exported and could be called from other locations without the guard.

**Remediation:**

```typescript
async function readNdjsonStream(
    response: Response,
    onEvent?: (event: ChatStreamEvent) => void,
): Promise<PulseChatResponse> {
    if (!response.body) {
        throw new Error('Response body is null -- cannot read NDJSON stream')
    }
    const reader = response.body.getReader()
    // ...
}
```

---

### SEC-10: No Rate Limiting on ACP Adapter Spawning

**Severity:** Medium (CVSS 5.9)
**CWE:** CWE-770 (Allocation of Resources Without Limits or Throttling)
**Location:** `crates/web/execute/sync_mode.rs:712-760` (`handle_pulse_chat`)

**Description:**

Each `pulse_chat` or `pulse_chat_probe` request spawns a new child process (Claude CLI or Codex CLI adapter) via `spawn_adapter()`. There is no rate limiting, no concurrent request cap, and no per-user throttling.

An attacker with a valid API token could send rapid `pulse_chat` requests, each spawning a child process that:
- Consumes a thread from tokio's blocking pool (`spawn_blocking`)
- Consumes a PID
- Consumes memory (each Claude/Codex process can use significant RAM)
- Potentially spawns its own subprocesses

The default tokio blocking pool has 512 threads. With 512 concurrent requests, the pool would be exhausted, blocking all other `spawn_blocking` callers system-wide.

**Remediation:**

Add a semaphore to limit concurrent ACP sessions:

```rust
use tokio::sync::Semaphore;
use std::sync::LazyLock;

static ACP_CONCURRENCY: LazyLock<Semaphore> = LazyLock::new(|| {
    let max = std::env::var("AXON_ACP_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    Semaphore::new(max)
});
```

---

### SEC-11: `validateAddDir` Symlink Race Condition (TOCTOU)

**Severity:** Medium (CVSS 4.2)
**CWE:** CWE-367 (Time-of-Check Time-of-Use Race Condition)
**Location:** `apps/web/app/api/pulse/chat/claude-stream-types.ts:100-115`

**Description:**

The `validateAddDir()` function resolves symlinks at validation time:

```typescript
function validateAddDir(dir: string): string | null {
    let real: string
    try {
        real = fs.realpathSync(path.resolve(dir))
    } catch {
        real = path.resolve(dir)
    }
    if (ALLOWED_DIR_ROOTS.some((root) => real.startsWith(root + path.sep) || real === root)) {
        return real
    }
    return null
}
```

Between the `realpathSync` check and the actual use of the path by the Claude CLI, an attacker with local access could:
1. Create `/tmp/safe` pointing to `/tmp/workspace` (passes validation)
2. After validation, replace `/tmp/safe` symlink to point to `/etc` or `/root`
3. Claude CLI follows the new symlink target

The `catch` fallback to `path.resolve(dir)` for non-existent paths is also concerning: a non-existent path could be created as a symlink after validation passes.

**Remediation:**

Pass the resolved real path to `--add-dir` (which the code already does by returning `real`). Additionally, consider using `O_NOFOLLOW` semantics or documenting that the `--add-dir` targets should be directories owned by the service user, not world-writable locations.

---

## Low Findings

### SEC-12: Unsafe `as` Casts from localStorage Without Runtime Validation

**Severity:** Low (CVSS 3.1)
**CWE:** CWE-704 (Incorrect Type Conversion)
**Location:** `apps/web/hooks/use-ws-messages.ts:221-234`

**Description:**

Values from `localStorage` are cast directly to union types without runtime validation:

```typescript
const a = localStorage.getItem('axon.web.pulse-agent') as PulseWorkspaceAgent
if (a && ['claude', 'codex'].includes(a)) setPulseAgent(a)
const m = localStorage.getItem('axon.web.pulse-model') as PulseWorkspaceModel
if (m && typeof m === 'string' && m.length > 0) setPulseModel(m)
const p = localStorage.getItem('axon.web.pulse-permission') as PulseWorkspacePermission
if (p && ['plan', 'accept-edits', 'bypass-permissions'].includes(p)) {
    setPulsePermissionLevel(p)
}
```

The `as` casts are technically type-unsafe but the subsequent `includes()` checks provide runtime guards for `agent` and `permission`. The `model` value has no allowlist check -- any string from localStorage is accepted and eventually sent as a flag value to the backend.

An attacker who can write to a user's localStorage (via XSS on the same origin) could set `axon.web.pulse-model` to an arbitrary string that is then forwarded to the ACP adapter. This is mitigated by the Zod validation on the HTTP path, but the WS path may not apply the same validation.

**Remediation:**

Use Zod parsing instead of `as` casts:

```typescript
const agentResult = PulseAgent.safeParse(localStorage.getItem('axon.web.pulse-agent'))
if (agentResult.success) setPulseAgent(agentResult.data)
```

---

### SEC-13: Permission Request Always Returns `Cancelled`

**Severity:** Low (CVSS 3.5)
**CWE:** CWE-284 (Improper Access Control)
**Location:** `crates/services/acp.rs:1046-1055`

**Description:**

The ACP `Client::request_permission` implementation always returns `Cancelled`:

```rust
async fn request_permission(
    &self,
    args: RequestPermissionRequest,
) -> agent_client_protocol::Result<RequestPermissionResponse> {
    emit(&self.tx, map_permission_request_event(&args));
    Ok(RequestPermissionResponse::new(
        RequestPermissionOutcome::Cancelled,
    ))
}
```

While this is secure (default-deny), it means the ACP adapter path has NO functional permission system. The UI receives a `permission_request` event but has no mechanism to respond. Combined with SEC-06 (the HTTP path uses `--dangerously-skip-permissions`), the result is an all-or-nothing security model: either everything is auto-approved (HTTP path) or everything is denied (ACP path).

**Impact:** Functional rather than security -- the adapter cannot use tools that require permission, limiting the utility of the ACP path. This is flagged as Low because the default-deny is the correct security posture, but it should be documented and eventually replaced with interactive approval.

**Remediation:**

Implement a bidirectional permission flow:
1. Send the `PermissionRequest` event to the frontend via the WS channel
2. Wait for a user response on a dedicated channel (with timeout)
3. Return the user's decision to the ACP SDK

---

### SEC-14: Adapter stderr Forwarded to Client Without Sanitization

**Severity:** Low (CVSS 3.3)
**CWE:** CWE-209 (Generation of Error Message Containing Sensitive Information)
**Location:** `crates/services/acp.rs:518-541`

**Description:**

The adapter's stderr output is forwarded directly to the WS client as log events:

```rust
emit(
    &stderr_tx,
    ServiceEvent::Log {
        level: "warn".to_string(),
        message: format!("ACP adapter stderr: {trimmed}"),
    },
);
```

Adapter stderr could contain:
- File paths revealing directory structure
- API keys or tokens if the adapter logs them on error
- Internal error messages from the LLM provider
- Stack traces revealing implementation details

**Remediation:**

Truncate and sanitize stderr before forwarding:

```rust
let sanitized = trimmed.chars().take(500).collect::<String>();
// Optionally: filter known patterns like API keys, file paths
```

---

### SEC-15: Error Messages Leak Internal Details to Client

**Severity:** Low (CVSS 3.1)
**CWE:** CWE-209 (Generation of Error Message Containing Sensitive Information)
**Location:** `apps/web/app/api/pulse/config/route.ts:80-84`, `apps/web/app/api/pulse/chat/route.ts:515,531-534`

**Description:**

Error responses include internal error messages that could reveal implementation details:

```typescript
return apiError(502, 'ACP config probe failed', {
    code: 'pulse_config_probe_failed',
    errorId,
    detail: probeErrorMessage,  // raw internal error
})
```

And in the chat route:
```typescript
emitErrorAndClose(
    `Pulse chat worker failed: ${truncateForLog(message)}`,
    'pulse_chat_command_error',
)
```

While `truncateForLog` limits length, the content itself is unfiltered. Internal messages like "failed to connect to postgresql://axon:postgres@..." would leak credentials. <!-- gitleaks:allow -->

**Remediation:**

Map internal errors to generic user-facing messages. Log the full error server-side with the `errorId` for correlation.

---

## Informational Findings

### SEC-16: `readability: false` Guard Should Have Test Coverage

**Severity:** Informational
**Location:** `crates/core/content.rs` (referenced in CLAUDE.md)

The CLAUDE.md documents that `readability: false` is a critical setting that caused a production regression when changed to `true`. This invariant should be enforced with a unit test to prevent future regressions.

---

### SEC-17: Codex Config File Read Uses Blocking I/O

**Severity:** Informational
**CWE:** CWE-400 (Uncontrolled Resource Consumption)
**Location:** `crates/services/acp.rs:253-269` (`read_codex_default_model`), `crates/services/acp.rs:271-299` (`read_codex_cached_model_options`)

Both functions use `std::fs::read_to_string` (blocking I/O) within code paths that may be called from async contexts. While these are currently called from within `spawn_blocking`, any future refactoring that moves them to an async context would block the tokio runtime thread.

---

### SEC-18: No CSRF Protection on Pulse Chat POST Endpoints

**Severity:** Informational
**CWE:** CWE-352 (Cross-Site Request Forgery)
**Location:** `apps/web/app/api/pulse/chat/route.ts`, `apps/web/app/api/pulse/config/route.ts`

The API routes rely on bearer token authentication (`AXON_WEB_API_TOKEN`) but do not implement CSRF tokens. Since the token is sent via headers (not cookies), traditional CSRF attacks cannot succeed. However, if the `AXON_WEB_ALLOW_INSECURE_DEV=true` bypass is enabled, the auth gate is presumably disabled, which could expose the endpoints to CSRF from any origin. The CSP headers in `next.config.ts` provide some mitigation.

---

## Positive Security Controls

The following security controls were identified as well-implemented:

1. **ALLOWED_MODES / ALLOWED_FLAGS allowlists** (`crates/web/execute/constants.rs`): The WS bridge enforces strict mode and flag allowlists before spawning any subprocess. Unknown modes or flags are rejected without process creation.

2. **Zod schema validation** (`apps/web/lib/pulse/types.ts`): The `PulseChatRequestSchema` applies comprehensive input validation including:
   - Prompt length limits (1-8000 chars)
   - Session ID format regex (`/^[0-9a-f-]{8,64}$/i`)
   - Collection name limits (1-100 chars, max 10 collections)
   - Conversation history caps (max 50 entries, 8000 chars each)
   - Betas allowlist regex (`/^[a-zA-Z0-9,\-.:]*$/`)

3. **`validateAddDir` path allowlist** (`claude-stream-types.ts:92-115`): Directory access for Claude CLI is restricted to an allowlist of root paths with symlink resolution.

4. **`sanitizeBetas` allowlist** (`claude-stream-types.ts:84-90`): Beta flags are validated against a configurable allowlist.

5. **`TOOL_ENTRY_RE` for allowedTools/disallowedTools** (`claude-stream-types.ts:173`): Tool identifiers are validated with a strict regex before being passed to the CLI.

6. **WS auth gate** (`crates/web.rs:177-194`): The WebSocket upgrade path enforces token authentication when `AXON_WEB_API_TOKEN` is configured.

7. **Default-deny permission model** (`acp.rs:1053`): The ACP `request_permission` handler defaults to `Cancelled`, preventing unauthorized tool execution through the ACP path.

8. **Environment variable stripping** (`acp.rs:63-70`): The four most dangerous environment variables are explicitly removed from the adapter subprocess.

9. **Shell WS localhost restriction** (`web.rs:197-216`): The shell WebSocket upgrade is restricted to loopback addresses with proper IPv4-mapped IPv6 handling.

---

## Remediation Priority

| Priority | Finding | Effort | Risk Reduction |
|----------|---------|--------|----------------|
| P0 | SEC-01: Codex model argument injection | Low | Critical |
| P0 | SEC-02: `toolsRestrict` unsanitized pass-through | Low | Critical |
| P1 | SEC-05: Full environment inheritance | Medium | High |
| P1 | SEC-06: `--dangerously-skip-permissions` default | Low | High |
| P1 | SEC-04: No adapter lifecycle timeout | Medium | High |
| P2 | SEC-03: Silent event loss via `try_send` | Medium | High |
| P2 | SEC-10: No rate limiting on adapter spawning | Medium | Medium |
| P3 | SEC-07: Path traversal -- insufficient cwd validation | Low | Medium |
| P3 | SEC-08: Non-constant-time token comparison | Low | Medium |
| P3 | SEC-09: Unchecked response.body assertion | Low | Medium |
| P4 | SEC-11: `validateAddDir` TOCTOU | Low | Medium |
| P4 | SEC-12: Unsafe localStorage casts | Low | Low |
| P4 | SEC-13: Permission always cancelled | High | Low |
| P4 | SEC-14: Stderr forwarding without sanitization | Low | Low |
| P4 | SEC-15: Error message information leakage | Low | Low |

---

*End of audit report.*
