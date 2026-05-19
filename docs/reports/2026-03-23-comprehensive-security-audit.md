# Comprehensive Security Audit: axon_rust
**Date:** 2026-03-23
**Auditor:** Security Audit (DevSecOps)
**Scope:** Full codebase at `/home/jmagar/workspace/axon_rust`
**Branch:** `feat/pulse-shell-and-hybrid-search`

---

## Executive Summary

The axon_rust codebase demonstrates **strong security engineering fundamentals** across most attack surfaces. SSRF protection is thorough with defense-in-depth, subprocess execution guards are well-designed, and authentication uses constant-time comparison. The codebase shows evidence of prior security reviews with fixes properly applied.

**Critical findings: 0** -- No remotely exploitable critical vulnerabilities were identified.
**High findings: 3** -- DNS rebinding TOCTOU, debug build auth bypass, shell PTY exposure.
**Medium findings: 6** -- Configuration risks, credential scoping, and hardening gaps.
**Low findings: 5** -- Informational items and defense-in-depth improvements.

---

## Findings

### SSRF Protection (CWE-918)

**Assessment: STRONG**

The `validate_url()` function in `crates/core/http/ssrf.rs` is well-implemented:

- Blocks loopback (127.0.0.0/8, ::1), link-local (169.254.0.0/16, fe80::/10), RFC-1918 private ranges, unique-local IPv6 (fc00::/7)
- Blocks `.internal` and `.local` TLDs, `localhost` hostnames
- Handles IPv4-mapped IPv6 bypass (`::ffff:127.0.0.1`) via recursive `check_ip()` -- this was a prior finding that has been fixed
- Property-based tests (`proptest_tests.rs`) cover full IP range exhaustion across all blocked ranges
- Defense-in-depth via `ssrf_blacklist_patterns()` applied to spider's crawl discovery
- Scheme validation blocks `file://`, `ftp://`, `data:` URIs

The `normalize_url()` function prepends `https://` to bare hostnames, which is correct (forces HTTPS default).

**Coverage gaps:**

| Test surface | Status |
|-------------|--------|
| IPv4 private ranges | Covered (proptest) |
| IPv4-mapped IPv6 bypass | Covered (proptest) |
| Loopback ranges | Covered (proptest) |
| Link-local / AWS metadata | Covered |
| Scheme filtering | Covered |
| TLD blocking | Covered |
| DNS rebinding | Documented TOCTOU -- see H-1 |
| `0.0.0.0` (unspecified) | Covered via `is_unspecified()` |

---

### H-1: DNS Rebinding TOCTOU Window (CWE-367)

**Severity:** High
**Location:** `crates/core/http/ssrf.rs:49-62` (documented in comments)

**Description:** `validate_url()` checks the resolved IP at parse time, but `reqwest` resolves DNS independently at connect time. An attacker with a short-TTL DNS record can pass validation (first resolution returns a public IP), then rebind before connection (second resolution returns `127.0.0.1`).

**Attack scenario:**
1. Attacker registers `evil.example.com` with TTL=0
2. First DNS query returns `93.184.216.34` (public) -- passes `validate_url()`
3. `reqwest` opens connection, resolves DNS again -- now returns `127.0.0.1`
4. Request reaches internal service (metadata endpoint, Redis, Postgres, etc.)

**Risk assessment:** The codebase is self-hosted internal tooling, not a public-facing SaaS. Attacker-controlled URLs come from CLI input and MCP tool calls, both of which require authenticated access. The blast radius is limited to the infrastructure services on the same network (Qdrant on port 53333, TEI on port 52000, Redis on port 53379, RabbitMQ on port 45535, Postgres on port 53432) -- all unauthenticated by default.

**Remediation:**
```rust
// Option A: Use hickory-resolver for pre-resolution + connection pinning
// Option B: Use reqwest's resolve() to pin the validated IP
let client = reqwest::Client::builder()
    .resolve("target.example.com", validated_ip_addr)
    .build()?;
```

**Current mitigation:** The risk is documented in the code comments with a clear explanation. The `ssrf_blacklist_patterns()` provides a secondary defense layer at the spider crawl level.

---

### H-2: Debug Build Auth Bypass (CWE-287)

**Severity:** High
**Location:** `crates/web/tailscale_auth.rs:86-97`

**Description:** When `AXON_WEB_API_TOKEN` is not set and the binary is compiled with `debug_assertions` (the default for `cargo build` without `--release`), the `check_auth()` function returns `AuthOutcome::Token` (authenticated) for all requests -- effectively disabling authentication.

```rust
#[cfg(any(debug_assertions, test))]
{
    // ...
    AuthOutcome::Token  // <-- All requests pass auth in debug builds
}
```

**Attack scenario:** If a developer runs `cargo run --bin axon -- serve` (debug build) without setting `AXON_WEB_API_TOKEN`, the web server accepts all WebSocket connections and HTTP requests without authentication, including shell PTY access.

**Risk assessment:** The server binds to `127.0.0.1` by default (`AXON_SERVE_HOST`), limiting exposure to the local machine. However, if `AXON_SERVE_HOST` is set to `0.0.0.0` (common for Docker), the entire API surface including the shell is exposed.

**Remediation:**
- Log a prominent startup warning when auth is disabled in debug builds (already done via `OnceLock`)
- Consider requiring an explicit opt-in flag like `--allow-unauthenticated` rather than silently bypassing based on build profile
- The release build correctly returns `AuthOutcome::Denied(DenyReason::NoAuthConfigured)` -- this is the correct production behavior

---

### H-3: Shell PTY — Full System Access via WebSocket (CWE-78)

**Severity:** High
**Location:** `crates/web/shell.rs:17-150`, `crates/web.rs:372-437`

**Description:** The `/ws/shell` endpoint spawns a full interactive PTY shell (`$SHELL` or `/bin/bash`) with the privileges of the axon process. Once authenticated, there are no further restrictions on what commands can be executed -- the user has equivalent access to an SSH session.

**Existing controls (well-implemented):**
- Token authentication required (same `check_auth()` as `/ws`)
- Origin validation via `effective_shell_allowed_origins()`
- Connection limit: `AXON_MAX_SHELL_CONNECTIONS` (default 10)
- Input size guard: 64 KiB per message (`MAX_SHELL_INPUT_BYTES`)
- Frame-level message size cap via `ws.max_message_size(65_536)`
- Keepalive timeout with ping/pong (30s timeout, 2 unanswered pings = disconnect)

**Residual risks:**
1. No audit logging of shell commands -- commands executed via PTY are not captured
2. No session recording for forensic review
3. The shell inherits the full environment of the axon process (env vars with secrets)
4. No command filtering or restricted shell mode

**Remediation:**
- Add structured audit logging of shell session lifecycle (connect, disconnect, duration)
- Consider `script` or `ttyrec` for session recording in production
- Apply environment variable filtering to the shell subprocess (similar to `ACP_ENV_ALLOWLIST` pattern used for ACP adapters)

---

### M-1: format!() SQL Table Name Interpolation (CWE-89)

**Severity:** Medium
**Location:** `crates/jobs/common/job_ops.rs:26-34` and throughout

**Description:** SQL queries use `format!()` to interpolate table names from `JobTable::as_str()` and status values from `JobStatus::as_str()`. While these are currently sourced from trusted enums (not user input), the pattern is fragile.

```rust
let query = format!(
    r#"WITH n AS (
        SELECT id FROM {table} WHERE status='{pending}' ...
    )"#,
    pending = JobStatus::Pending.as_str(),
    running = JobStatus::Running.as_str(),
);
```

**Current safety:** `JobTable` is a closed enum with fixed string representations (`"axon_crawl_jobs"`, `"axon_extract_jobs"`, etc.). `JobStatus` returns fixed strings (`"pending"`, `"running"`, etc.). No user input reaches these values.

**Risk:** If a future developer adds a `JobTable` variant with special characters or if `as_str()` is refactored to accept dynamic input, this becomes a SQL injection vector. The pattern also prevents using prepared statement benefits for query plan caching.

**Remediation:** This is acceptable as-is given the enum constraints. Add a compile-time assertion or comment documenting the safety invariant:
```rust
// SAFETY: table_name comes from JobTable enum — a closed set of
// compile-time constants. This MUST remain true; never derive
// table names from user input.
```

---

### M-2: ACP Adapter Command from Environment Variable (CWE-78)

**Severity:** Medium
**Location:** `crates/services/acp_llm/runner.rs:124-145`, `crates/services/acp/mapping/validation.rs:13-90`

**Description:** The ACP adapter subprocess is spawned from `AXON_ACP_ADAPTER_CMD` (environment variable) with args from `AXON_ACP_ADAPTER_ARGS` (pipe-delimited). These execute arbitrary binaries via `execvp`.

**Existing controls (comprehensive):**
- `validate_adapter_command()` blocks known shell interpreters (sh, bash, zsh, fish, dash, ksh, csh, tcsh, cmd, powershell, pwsh) -- case insensitive, handles `.exe` suffix
- Symlink resolution via `canonicalize()` catches `/tmp/safe_name -> /bin/bash`
- `env_clear()` + `ACP_ENV_ALLOWLIST` limits environment variables passed to subprocess (27 specific keys)
- `OPENAI_*` vars intentionally excluded from allowlist -- prevents leaking internal LLM proxy credentials to adapters
- `kill_on_drop(true)` prevents orphan processes
- `AdapterGuard` RAII pattern ensures cleanup on all error paths
- `validate_model_string()` restricts model names to `[a-zA-Z0-9-_./: ]` -- prevents injection via model parameter
- Arguments passed via `execvp` (no shell expansion) -- `format!("model=\"{model}\"")` is safe because there is no shell to interpret quotes

**Residual risks:**
1. `parse_adapter_args()` splits on `|` -- a pipe character in `AXON_ACP_ADAPTER_ARGS` could unintentionally split a single argument. This is a configuration footgun, not a security vulnerability (env vars are admin-controlled).
2. The blocked shells list does not include `python`, `node`, `ruby`, `perl` -- these can execute arbitrary code too. However, the blocklist is designed to prevent *accidental* shell injection, not to sandbox untrusted adapter binaries. The admin controls what binary `AXON_ACP_ADAPTER_CMD` points to.

**Assessment:** Well-defended. The defense-in-depth approach (blocklist + env allowlist + no shell expansion + RAII cleanup) is sound. No changes recommended.

---

### M-3: `accept_invalid_certs` TLS Bypass (CWE-295)

**Severity:** Medium
**Location:** `crates/crawl/scrape.rs:66-68`, `crates/core/config/cli/global_args.rs:316`

**Description:** The `--accept-invalid-certs` CLI flag disables TLS certificate verification for spider crawl and scrape operations. When enabled, the crawler is vulnerable to MITM attacks.

**Existing controls:**
- Defaults to `false`
- One-time warning logged via `OnceLock` when enabled
- Opt-in only (requires explicit flag)

**Risk:** If enabled in a shared worker configuration (e.g., via `.env`), all crawl operations become vulnerable to TLS interception. Credentials sent to HTTPS sites (via `--header` custom headers) could be intercepted.

**Remediation:** Consider restricting this flag to development builds only, or requiring `--yes` confirmation when combined with `--header` (custom auth headers + disabled cert verification is a dangerous combination).

---

### M-4: Infrastructure Services Exposed Without Authentication (CWE-306)

**Severity:** Medium
**Location:** `docker-compose.services.yaml` (ports 53432, 53379, 45535, 53333, 52000)

**Description:** All infrastructure services are exposed on high-numbered ports bound to `127.0.0.1`:
- **Postgres** (53432): password-protected via connection string
- **Redis** (53379): password-protected (per `AXON_REDIS_URL`)
- **RabbitMQ** (45535): credentials in `AXON_AMQP_URL`
- **Qdrant** (53333): **no authentication** -- any local process can read/write vectors
- **TEI** (52000): **no authentication** -- any local process can generate embeddings

**Attack scenario:** A malicious process on the same host (or a compromised container on the Docker network) can directly access Qdrant (read all indexed documents, delete collections, inject poisoned embeddings) and TEI (consume GPU resources, exfiltrate through timing channels).

**Remediation:**
- Qdrant supports API key authentication -- enable it via `QDRANT__SERVICE__API_KEY`
- TEI does not have built-in auth -- consider placing it behind an auth proxy or restricting access via Docker network policies
- For self-hosted single-user setups, the current `127.0.0.1` binding provides adequate isolation

---

### M-5: Reddit OAuth2 Client Credentials in Process Memory (CWE-798)

**Severity:** Medium
**Location:** `crates/ingest/reddit/client.rs:25-47`

**Description:** Reddit `client_id` and `client_secret` are passed directly to `basic_auth()` and the resulting access token is held in memory as a plain `String`. The `Config` struct's `Debug` implementation redacts these values (confirmed at `config_impls.rs:259-260`), which is correct.

**Assessment:** This is standard practice for OAuth2 client credentials flow. The credentials are properly redacted in debug output. The Reddit client uses `https_only(true)` which prevents credential leakage over HTTP. No significant risk beyond the inherent nature of secret management in process memory.

---

### M-6: MCP OAuth In-Memory State Without Redis (CWE-613)

**Severity:** Medium
**Location:** `crates/mcp/server/oauth_google/state.rs:63-109`

**Description:** When Redis is not available, OAuth state (pending states, auth codes, access tokens, refresh tokens) is stored in in-memory `HashMap`s. This means:
1. Tokens do not survive process restarts
2. No shared state between multiple worker instances
3. The `MAX_OAUTH_STATE_ENTRIES` (10,000) cap prevents memory exhaustion, which is good

**Existing controls:**
- TTL-based expiry with 60-second background reaper
- `guarded_insert()` enforces capacity limit with cleanup-before-reject pattern
- Rate limiting per client IP (`rate_limits` map)
- Auth codes consumed atomically (removed from memory regardless of Redis result)

**Assessment:** The in-memory fallback is a usability trade-off for self-hosted deployments without Redis. The capacity limits and TTL eviction prevent DoS. Acceptable for the deployment model.

---

### L-1: Subprocess Argument Injection Guards (CWE-88)

**Severity:** Low (well-mitigated)
**Locations:**
- `crates/ingest/youtube.rs:155-178` (yt-dlp)
- `crates/ingest/github/wiki.rs:160-161` (git clone)

**Assessment: WELL-DEFENDED**

Both subprocess invocations use `"--"` to separate flags from URL arguments, preventing argument injection:
```rust
// yt-dlp
command.args(["...", "--no-exec", "--", safe_url]);

// git clone
cmd.args(["clone", "--depth=1", "--", &clone_url, &tmp_path]);
```

Additional protections:
- YouTube: `extract_video_id()` canonicalizes to `https://www.youtube.com/watch?v={id}` before passing to yt-dlp -- bare video IDs (11 alphanumeric chars) cannot inject arguments
- YouTube: `--no-exec` prevents yt-dlp post-processing command execution
- GitHub wiki: `validate_url()` is called on the clone URL before invoking git
- GitHub wiki: Token is passed via `GIT_CONFIG_VALUE_0` environment variable, not embedded in the clone URL (prevents token leakage in process args visible via `/proc`)
- Both use `kill_on_drop(true)` and `SUBPROCESS_TIMEOUT` (300s)
- File size guard: `MAX_INGEST_FILE_BYTES` (50 MiB) prevents reading oversized output into memory

---

### L-2: WebSocket Execute Mode Allowlist (CWE-20)

**Severity:** Low (well-mitigated)
**Location:** `crates/web/execute/constants.rs:5-33`, `crates/web/execute/args.rs:12-87`

**Assessment: WELL-DEFENDED**

The execute pipeline enforces strict allowlists:
- `ALLOWED_MODES`: 23 explicitly permitted subcommands
- `ALLOWED_FLAGS`: 33 key-value pairs mapping JSON keys to CLI flags -- unknown keys are silently dropped
- Input sanitization: `trimmed.trim_start_matches('-')` prevents flag injection via the input field
- Path traversal guard on `--output-dir`: rejects values containing `..` (ParentDir component)
- Output file serving: uses `canonicalize()` + `starts_with()` to enforce directory containment
- Null byte injection blocked: `file_path.contains('\0')` check

---

### L-3: Path Traversal Protection in Output File Serving (CWE-22)

**Severity:** Low (well-mitigated)
**Location:** `crates/web.rs:224-298`

**Assessment: WELL-DEFENDED**

The `serve_output_file` handler has three layers of defense:
1. **Pre-check:** `Path::components().any(|c| c == ParentDir)` rejects `..` traversal
2. **Null byte check:** `file_path.contains('\0')` blocks null byte injection
3. **Canonicalization:** `tokio::fs::canonicalize()` resolves symlinks, then `canonical_file.starts_with(&canonical_base)` verifies containment

The `build_args()` function in `execute/args.rs` also blocks path traversal in `--output-dir` values.

---

### L-4: Credential Redaction in Debug Output (CWE-532)

**Severity:** Low (well-handled)
**Location:** `crates/core/config/types/config_impls.rs:243-278`

**Assessment: PROPERLY IMPLEMENTED**

The `Config` struct's `fmt::Debug` implementation redacts 10 sensitive fields:
- `pg_url`, `redis_url`, `amqp_url` (connection strings with credentials)
- `github_token`, `reddit_client_id`, `reddit_client_secret`
- `openai_api_key`, `tavily_api_key`
- `neo4j_url`, `neo4j_password`
- Custom headers: redacted to `"name: [REDACTED]"` format

The `content.rs::redact_url()` function strips `username:password@` from URLs.

**Gap:** `acp_adapter_cmd` and `acp_adapter_args` are not redacted -- these contain binary paths and flags, not secrets, so this is acceptable.

---

### L-5: `cargo-deny` Advisory Policy (CWE-1395)

**Severity:** Low
**Location:** `deny.toml:7-12`

**Description:** One advisory is suppressed:
- `RUSTSEC-2023-0071`: Marvin Attack timing side-channel in `rsa` crate (via `octocrab -> jsonwebtoken -> rsa v0.9.x`)

**Assessment:** The suppression is justified -- the `rsa` crate is used for JWT verification (not key generation), which limits the attack surface. The advisory comment explains the rationale. No upgrade path is available due to the transitive dependency chain.

The `deny.toml` configuration is otherwise strict:
- `unknown-registry = "deny"` -- blocks crates from unauthorized registries
- `unknown-git = "deny"` -- blocks crates from unknown git sources
- `unmaintained = "workspace"` -- checks only workspace-local crates

---

## Authentication Architecture Review

### Web API Auth Stack

| Surface | Auth mechanism | Constant-time | Notes |
|---------|---------------|---------------|-------|
| `/api/*` (Next.js proxy) | `proxy.ts` -- Bearer / x-api-key / ?token | Yes (`timingSafeEqual`) | Two-tier: `AXON_WEB_API_TOKEN` + optional `AXON_WEB_BROWSER_API_TOKEN` |
| `/ws` (Rust) | `check_auth()` -- Bearer / x-api-key / ?token | Yes (`subtle::ConstantTimeEq`) | Same token as API |
| `/ws/shell` (Rust) | `check_auth()` -- same as `/ws` | Yes | Additional origin validation |
| `/output/*` (Rust) | `check_auth()` -- same as `/ws` | Yes | Path traversal protection |
| MCP OAuth | `atk_` tokens via Google OAuth2 | N/A (separate system) | Does not gate `/ws` or `/api/*` |

**Strengths:**
- Both Rust and Node.js layers use constant-time comparison
- Header-based auth takes precedence over query parameter (reduces URL logging exposure)
- Connection limits prevent resource exhaustion (100 WS, 10 shell, 8 ACP sessions)
- Rate limiting on execute operations (per-IP, survives reconnects)
- Session ownership tracking prevents cross-connection session hijacking
- CORS middleware with origin allowlists on both WS surfaces

**Weakness noted in H-2:** Debug builds bypass all auth when token is unset.

---

## Dependency Security

### `Cargo.lock` Analysis

The project uses a `path` dependency for `spider_agent` pointing to `../spider/spider_agent`. This is documented in CLAUDE.md with registry fallback instructions for CI. The `deny.toml` policy with `unknown-git = "deny"` provides protection against supply chain attacks from unknown sources, but the `path` dependency bypasses this check entirely (path deps are local, not fetched).

### Security-Relevant Dependencies

| Dependency | Purpose | Risk |
|-----------|---------|------|
| `subtle` (constant-time) | Token comparison | Low -- well-audited crate |
| `reqwest` (HTTP client) | All outbound HTTP | Medium -- large attack surface, well-maintained |
| `sqlx` (Postgres) | Job persistence | Low -- parameterized queries for user data |
| `lapin` (AMQP) | Job queue | Low -- message content is trusted (internal) |
| `qdrant-client` | Vector store | Medium -- unauthenticated by default |
| `octocrab` | GitHub API | Low -- HTTPS only |
| `portable-pty` | Shell PTY | High impact -- grants full shell access (auth-gated) |

---

## Configuration Security Summary

| Setting | Default | Secure? | Notes |
|---------|---------|---------|-------|
| `accept_invalid_certs` | `false` | Yes | Requires explicit opt-in |
| `respect_robots` | `false` | N/A | Ethical, not security |
| `include_subdomains` | `false` | Yes | Changed from `true` after scope-escape bug |
| `AXON_SERVE_HOST` | `127.0.0.1` | Yes | Binds to loopback only by default |
| `AXON_WEB_ALLOW_INSECURE_DEV` | `false` | Yes | Requires explicit opt-in |
| `AXON_WEB_ALLOW_QUERY_TOKEN` | `false` | Yes | Token in URL is off by default |
| Debug build auth bypass | Implicit | **No** | See H-2 |
| `embed` (auto-embed) | `true` | N/A | Functional, not security |

---

## AMQP / Job Queue Security

**Job ID handling:** UUIDs are generated server-side (`Uuid::new_v4()`), never from user input. Job IDs in cancel/status operations are bound via `$1` parameterized queries.

**Cancel key construction:** Redis cancel keys use format `axon:crawl:cancel:{id}` where `id` is a UUID. No injection vector exists since UUIDs are generated internally.

**Message validation:** AMQP messages carry job UUIDs. Workers fetch full job details from Postgres by ID. There is no direct execution of AMQP message content -- messages are identifiers, not commands.

**Stale job recovery:** The watchdog marks jobs as stale/failed after configurable timeouts, preventing indefinite resource holding.

---

## Recommendations (Priority Order)

1. **[H-1] DNS Rebinding:** Implement connection pinning via `reqwest::Client::resolve()` for URLs that pass `validate_url()`. This closes the TOCTOU window without requiring an external DNS resolver.

2. **[H-2] Debug Auth Bypass:** Add a startup log line at WARN level that is impossible to miss (e.g., bright red ANSI). Consider requiring `--allow-unauthenticated` flag rather than implicit bypass based on build profile.

3. **[H-3] Shell Audit Logging:** Add structured logs for shell session lifecycle: connection source IP, duration, bytes transferred. This enables forensic review without session recording overhead.

4. **[M-4] Qdrant Auth:** Enable `QDRANT__SERVICE__API_KEY` on the Qdrant instance and pass the key via `cfg.qdrant_url` or a dedicated config field.

5. **[M-3] TLS Bypass Guard:** Warn or require confirmation when `--accept-invalid-certs` is combined with `--header` (custom auth headers).

---

## Files Reviewed

| File | Purpose |
|------|---------|
| `crates/core/http/ssrf.rs` | SSRF guard implementation |
| `crates/core/http/normalize.rs` | URL normalization |
| `crates/core/http/tests.rs` | SSRF test suite (38 tests) |
| `crates/core/http/proptest_tests.rs` | Property-based SSRF tests |
| `crates/web/tailscale_auth.rs` | Token auth + constant-time comparison |
| `crates/web/shell.rs` | PTY shell WebSocket handler |
| `crates/web.rs` | Web server routing, auth gates, connection limits |
| `crates/web/execute/args.rs` | CLI arg allowlist builder |
| `crates/web/execute/constants.rs` | Allowed modes/flags |
| `crates/services/acp.rs` | ACP scaffold, env allowlist, subprocess spawn |
| `crates/services/acp/adapters.rs` | Model validation, adapter detection |
| `crates/services/acp/runtime.rs` | ACP session lifecycle, AdapterGuard RAII |
| `crates/services/acp/session.rs` | Session CWD validation, adapter I/O wiring |
| `crates/services/acp/mapping/validation.rs` | Adapter command validation, shell blocklist |
| `crates/services/acp_llm/runner.rs` | One-shot ACP completion runner |
| `crates/ingest/youtube.rs` | yt-dlp subprocess, arg injection guards |
| `crates/ingest/subprocess.rs` | Shared subprocess timeout + kill_on_drop |
| `crates/ingest/github/wiki.rs` | git clone subprocess, token via env |
| `crates/ingest/reddit/client.rs` | OAuth2 credential handling |
| `crates/jobs/common/job_ops.rs` | SQL job lifecycle operations |
| `crates/mcp/config.rs` | MCP config loader |
| `crates/mcp/server/oauth_google/state.rs` | MCP OAuth state management |
| `crates/vector/ops/ranking/snippet.rs` | Ranking code (unwrap safety check) |
| `crates/core/config/types/config_impls.rs` | Secret redaction in Debug |
| `apps/web/proxy.ts` | Next.js API auth proxy |
| `deny.toml` | Dependency advisory policy |
