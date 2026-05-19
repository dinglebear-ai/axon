# Security Audit: crates/core
**Date:** 2026-03-15
**Auditor:** Security Auditor (DevSecOps)
**Scope:** `/home/jmagar/workspace/axon_rust/crates/core/` -- shared infrastructure crate
**Methodology:** Manual source code review, OWASP Top 10 mapping, CWE classification

---

## Executive Summary

The `crates/core` crate provides foundational infrastructure for the Axon CLI: configuration parsing, HTTP client with SSRF protection, content transformation, logging, and health checks. The codebase demonstrates strong security awareness -- SSRF protection is well-implemented with defense-in-depth (validate_url + redirect policy + blacklist patterns), Debug redaction prevents casual credential leakage, and property-based testing covers IP range validation exhaustively.

However, the audit identified **13 findings** across 4 severity levels:

| Severity | Count |
|----------|-------|
| High     | 3     |
| Medium   | 5     |
| Low      | 4     |
| Info     | 1     |

The most significant issues are: (1) credentials stored as plain `String` in the `Config` struct despite a `Secret<T>` wrapper being available but unused, (2) the SSRF TOCTOU gap from DNS rebinding, and (3) `unsafe` env var mutation in tests which is undefined behavior in multi-threaded Rust since 1.66.

---

## Findings

### CORE-SEC-01: Secret<T> Wrapper Exists But Is Not Integrated Into Config
**Severity:** High
**CWE:** CWE-312 (Cleartext Storage of Sensitive Information)
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/core/config/types/config.rs` (lines 148-249)
- `/home/jmagar/workspace/axon_rust/crates/core/config/secret.rs`

**Description:**
A well-designed `Secret<T>` wrapper exists at `config/secret.rs` with redacted Debug/Display, constant-time comparison for auth checks, and explicit `.expose()` access. However, every credential field in the `Config` struct remains a plain `String`:

```rust
// config/types/config.rs
pub pg_url: String,          // contains password in URL
pub redis_url: String,        // contains password in URL
pub amqp_url: String,         // contains password in URL
pub openai_api_key: String,   // API key
pub tavily_api_key: String,   // API key
pub neo4j_password: String,   // database password
pub github_token: Option<String>,
pub reddit_client_secret: Option<String>,
```

The manual `fmt::Debug` implementation in `config_impls.rs` redacts these fields, but this is fragile -- any new code path that formats a `Config` via a method other than `Debug` (e.g., serialization, error messages, structured logging with `{:?}` on a containing struct) will leak credentials. The `subconfigs.rs` file (lines 22-23, 33, 58, 63, 66) explicitly documents this as a tracked TODO (A-M-07) but it remains unimplemented.

**Attack Scenario:**
A developer adds a struct containing `Config` and derives `Debug`, or a panic unwind captures a `Config` value -- credentials appear in log files, crash reports, or terminal output.

**Remediation:**
Complete the A-M-07 migration. Replace all credential fields with `Secret<String>`:

```rust
pub pg_url: Secret<String>,
pub redis_url: Secret<String>,
pub amqp_url: Secret<String>,
pub openai_api_key: Secret<String>,
pub tavily_api_key: Secret<String>,
pub neo4j_password: Secret<String>,
pub github_token: Option<Secret<String>>,
pub reddit_client_secret: Option<Secret<String>>,
```

This makes credential access explicit (`.expose()`) and redaction automatic regardless of formatting context. The `constant_time_eq` method on `Secret<String>` should be used for all auth token comparisons. Search for `.expose()` calls during code review to audit all secret access points.

---

### CORE-SEC-02: SSRF TOCTOU Gap -- DNS Rebinding Bypass
**Severity:** High
**CWE:** CWE-367 (Time-of-Check Time-of-Use Race Condition)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/http/ssrf.rs` (lines 49-62)

**Description:**
`validate_url()` parses the URL and checks the hostname against blocked IP ranges at parse time. However, `reqwest` performs its own independent DNS resolution at connect time. An attacker controlling a domain with TTL-0 DNS records can bypass the SSRF guard:

1. Attacker's DNS returns `93.184.216.34` (public IP) for `evil.attacker.com`
2. `validate_url("http://evil.attacker.com/")` passes -- public IP
3. Before `reqwest` connects, attacker's DNS rebinds to `127.0.0.1` or `169.254.169.254`
4. `reqwest` connects to the internal/metadata endpoint

The code already documents this risk thoroughly (lines 49-62) and has defense-in-depth via `ssrf_blacklist_patterns()` applied to discovered URLs. The redirect policy in `build_client()` also validates each redirect target.

**Attack Scenario:**
An attacker submits `http://evil.attacker.com/` as a crawl target. Through DNS rebinding, the crawler accesses `169.254.169.254` (cloud metadata), potentially leaking IAM credentials or instance metadata.

**Remediation:**
Implement DNS pre-resolution with connection pinning using `hickory-resolver`:

```rust
use hickory_resolver::TokioAsyncResolver;

async fn resolve_and_validate(url: &str) -> Result<IpAddr, HttpError> {
    let parsed = Url::parse(url)?;
    let host = parsed.host_str().ok_or(HttpError::InvalidUrl(url.to_string()))?;
    let resolver = TokioAsyncResolver::tokio_from_system_conf()?;
    let response = resolver.lookup_ip(host).await?;
    let ip = response.iter().next().ok_or(HttpError::Dns("no records".into()))?;
    check_ip(ip)?;
    Ok(ip)
}
```

Then configure `reqwest` to use the pre-resolved IP via `resolve()`:

```rust
reqwest::Client::builder()
    .resolve(host, SocketAddr::new(resolved_ip, port))
    .build()
```

This eliminates the TOCTOU window entirely. For the self-hosted deployment model where attacker-controlled domains are less likely, the existing defense-in-depth is acceptable as a risk-acknowledged residual -- but the fix should be prioritized if the tool ever processes untrusted URL inputs from external users.

---

### CORE-SEC-03: Secrets Passable Via CLI Arguments -- Visible in Process List
**Severity:** High
**CWE:** CWE-214 (Invocation of Process Using Visible Sensitive Information)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/config/cli/global_args.rs` (lines 170-207)

**Description:**
Credentials can be passed as CLI arguments:

```rust
#[arg(global = true, long)]
pub(in crate::crates::core::config) pg_url: Option<String>,

#[arg(global = true, long)]
pub(in crate::crates::core::config) redis_url: Option<String>,

#[arg(global = true, long)]
pub(in crate::crates::core::config) amqp_url: Option<String>,

#[arg(global = true, long)]
pub(in crate::crates::core::config) openai_api_key: Option<String>,
```

When passed as `axon --pg-url "postgresql://user:password@host/db" scrape ...`, the full connection string (including password) is visible to any user on the system via `ps aux`, `/proc/PID/cmdline`, or process monitoring tools.

**Attack Scenario:**
A multi-user system or CI environment where other processes/users can read `/proc/*/cmdline`. The database password, API keys, and AMQP credentials are exposed in plaintext.

**Remediation:**
1. Remove `--openai-api-key` from CLI args entirely -- it should only come from env vars or a secrets file
2. For service URLs that embed credentials (`--pg-url`, `--redis-url`, `--amqp-url`), add a deprecation warning when they contain credentials:

```rust
if pg_url.contains('@') && pg_url.contains(':') {
    log_warn("passing credentials via --pg-url is insecure; use AXON_PG_URL env var instead");
}
```

3. Document that all secrets should be provided via `.env` file or environment variables, never CLI flags

---

### CORE-SEC-04: Unsafe env::set_var / env::remove_var in Tests (UB Since Rust 1.66)
**Severity:** Medium
**CWE:** CWE-362 (Concurrent Execution Using Shared Resource with Improper Synchronization)
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs` (lines 608-648)
- `/home/jmagar/workspace/axon_rust/crates/core/health.rs` (lines 113-185)

**Description:**
Both files use `unsafe { env::set_var(...) }` and `unsafe { env::remove_var(...) }` in test code, with a `Mutex` guard attempting to serialize access. However, since Rust 1.66, `env::set_var` and `env::remove_var` are marked `unsafe` because the C runtime's `setenv`/`unsetenv` are not thread-safe. The `Mutex` only prevents concurrent test execution within the same test module -- it cannot prevent other threads (tokio runtime threads, other test modules running in parallel) from calling `env::var` simultaneously.

```rust
// build_config.rs:615-617
unsafe {
    env::set_var(WEB, " https://axon.example.com , http://localhost:49010 ");
    env::set_var(SHELL, " http://localhost:49011 ");
}
```

```rust
// health.rs:127-132
unsafe {
    env::remove_var("AXON_CHROME_DIAGNOSTICS");
    env::remove_var("AXON_CHROME_DIAGNOSTICS_SCREENSHOT");
    // ...
}
```

**Attack Scenario:**
Not directly exploitable in production (test-only code), but can cause flaky test failures, memory corruption in ASAN builds, or false MIRI reports. The `#[allow(unsafe_code)]` annotations silence lints that would otherwise flag this.

**Remediation:**
Replace `env::set_var` with a test-specific approach that avoids modifying the process environment:

Option A -- Use `temp_env` crate:
```rust
temp_env::with_vars(
    [("AXON_WEB_ALLOWED_ORIGINS", Some("https://example.com"))],
    || { /* test body */ }
);
```

Option B -- Refactor `into_config` to accept an env reader trait:
```rust
trait EnvReader {
    fn get(&self, key: &str) -> Option<String>;
}

struct RealEnv;
impl EnvReader for RealEnv {
    fn get(&self, key: &str) -> Option<String> { env::var(key).ok() }
}

// In tests:
struct MockEnv(HashMap<String, String>);
impl EnvReader for MockEnv {
    fn get(&self, key: &str) -> Option<String> { self.0.get(key).cloned() }
}
```

Option B is more work but eliminates all process-level env mutation and makes tests fully deterministic and parallelizable.

---

### CORE-SEC-05: Neo4j Client Creates Per-Query TCP Connection (No Pooling)
**Severity:** Medium
**CWE:** CWE-400 (Uncontrolled Resource Consumption)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/neo4j.rs`

**Description:**
`Neo4jClient::from_parts()` clones the shared HTTP client from `http_client()`, which is good for connection reuse at the HTTP level. However, the client is constructed once and holds a single `reqwest::Client`. Under high concurrency (graph extraction with `graph_concurrency: 4`), all Neo4j queries share the same client but there is no connection pool management, no retry logic, and no timeout specific to Neo4j operations.

The `send()` method (line 76-98) has no timeout -- it inherits the global 30-second timeout from `build_client(30)`. A slow Neo4j query that takes 29 seconds will block a worker lane without being identified as problematic.

**Attack Scenario:**
A large graph extraction job with many concurrent queries can exhaust the Neo4j connection pool server-side. A slow or hung Neo4j instance will silently block all graph operations for up to 30 seconds per query with no circuit-breaking.

**Remediation:**
1. Add a Neo4j-specific timeout (e.g., 10 seconds for health checks, 60 seconds for extraction queries)
2. Add retry with backoff for transient Neo4j errors (connection reset, 503)
3. Consider using `bb8` or `deadpool` for connection pooling if Neo4j load increases

```rust
pub async fn send_with_timeout(&self, cypher: &str, params: Value, timeout: Duration) -> Neo4jResult<Value> {
    let body = build_request_body(cypher, params);
    let mut request = self.http.post(&self.endpoint).json(&body).timeout(timeout);
    // ...
}
```

---

### CORE-SEC-06: constant_time_eq Length Short-Circuit Leaks Secret Length
**Severity:** Medium
**CWE:** CWE-208 (Observable Timing Discrepancy)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/config/secret.rs` (lines 73-84)

**Description:**
The `constant_time_eq` method correctly performs XOR-fold comparison for equal-length strings, but short-circuits on length mismatch:

```rust
pub fn constant_time_eq(&self, other: &str) -> bool {
    let a = self.0.as_bytes();
    let b = other.as_bytes();
    if a.len() != b.len() {
        return false;  // <-- timing leak: reveals secret length
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
```

An attacker making many authentication attempts can measure response times to determine the exact length of the secret token. Once the length is known, the constant-time comparison is only protecting against content leakage, not length leakage.

**Attack Scenario:**
An attacker sends tokens of varying lengths (1 byte, 2 bytes, ..., 64 bytes) against an auth endpoint. The length-mismatch path returns slightly faster. After enough samples, the attacker knows the secret is exactly N bytes long, reducing the brute-force search space.

**Remediation:**
Hash both values before comparison, or pad to equal length:

```rust
pub fn constant_time_eq(&self, other: &str) -> bool {
    use std::hash::{Hash, Hasher, SipHasher};
    // Hash both to fixed-size, then compare hashes in constant time.
    // This hides the length of both the secret and the candidate.
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    self.0.as_bytes().hash(&mut h1);
    other.as_bytes().hash(&mut h2);
    let d1 = h1.finish().to_ne_bytes();
    let d2 = h2.finish().to_ne_bytes();
    d1.iter().zip(d2.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
```

Or better, use the `subtle` crate which provides a battle-tested `ConstantTimeEq` trait:

```rust
use subtle::ConstantTimeEq;

pub fn constant_time_eq(&self, other: &str) -> bool {
    // HMAC both with a fixed key to equalize length, then compare
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let key = b"axon-auth-comparison-key"; // fixed, non-secret
    let mut mac1 = HmacSha256::new_from_slice(key).unwrap();
    let mut mac2 = HmacSha256::new_from_slice(key).unwrap();
    mac1.update(self.0.as_bytes());
    mac2.update(other.as_bytes());
    mac1.finalize().into_bytes().ct_eq(&mac2.finalize().into_bytes()).into()
}
```

---

### CORE-SEC-07: MCP HTTP Transport Defaults to 0.0.0.0 (All Interfaces)
**Severity:** Medium
**CWE:** CWE-1188 (Initialization with an Insecure Default)
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/core/config/types/config_impls.rs` (line 153-154)
- `/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs` (line 520)

**Description:**
The MCP HTTP transport binds to `0.0.0.0` by default:

```rust
// config_impls.rs:153
mcp_http_host: "0.0.0.0".to_string(),
mcp_http_port: 8001,

// build_config.rs:520
mcp_http_host: env::var("AXON_MCP_HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
```

This exposes the MCP server on all network interfaces, including external-facing ones. The MCP protocol provides full access to crawl/scrape/embed/query operations.

**Attack Scenario:**
On a machine with a public IP or connected to a shared network, the MCP server is accessible from any host. An attacker on the same network can invoke arbitrary crawl/embed/query commands.

**Remediation:**
Change the default to `127.0.0.1` (loopback only):

```rust
mcp_http_host: "127.0.0.1".to_string(),
```

Users who need network-accessible MCP can explicitly set `AXON_MCP_HTTP_HOST=0.0.0.0`.

---

### CORE-SEC-08: accept_invalid_certs Flag Disables TLS Verification
**Severity:** Medium
**CWE:** CWE-295 (Improper Certificate Validation)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/config/cli/global_args.rs` (lines 292-293)

**Description:**
The `--accept-invalid-certs` flag is available as a global CLI argument:

```rust
#[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
pub(in crate::crates::core::config) accept_invalid_certs: bool,
```

When enabled, this is passed to Spider's `with_danger_accept_invalid_certs(true)`, disabling all TLS certificate validation. While this is opt-in (default false), there is no warning emitted when it is enabled, and it applies globally to all connections in a crawl session.

**Attack Scenario:**
A user enables `--accept-invalid-certs` for a staging site, then forgets to disable it. All subsequent crawl traffic is vulnerable to MITM attacks. An attacker performing ARP spoofing or DNS poisoning can intercept all crawled content.

**Remediation:**
1. Emit a prominent warning when the flag is enabled:
```rust
if cfg.accept_invalid_certs {
    log_warn("TLS certificate validation DISABLED -- all connections are vulnerable to MITM attacks");
}
```
2. Consider making this per-domain rather than global
3. Add a confirmation prompt (unless `--yes` is set) when this flag is used

---

### CORE-SEC-09: extract_meta_description Uses Byte Indexing on Potentially Multi-Byte UTF-8
**Severity:** Low
**CWE:** CWE-135 (Incorrect Calculation of Multi-Byte String Length)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/content.rs` (lines 181-197)

**Description:**
The function already has a mitigation using `.get()` (line 189), but the 8192-byte fallback for `head_end` (line 186) could land mid-character in a multi-byte UTF-8 sequence. The `.get()` returns `None` and falls back to the full `html` string, which is correct but potentially expensive for very large documents.

```rust
let head_end = html
    .find("</head>")
    .or_else(|| html.find("</HEAD>"))
    .unwrap_or(html.len().min(8192));
let head = html.get(..head_end).unwrap_or(html);
```

**Attack Scenario:**
Not directly exploitable -- the fallback to `html` is safe. The risk is performance: a malicious HTML document with no `</head>` tag and multi-byte characters at exactly byte 8192 causes the function to process the entire document instead of the first 8KB.

**Remediation:**
Use a UTF-8-aware truncation:

```rust
let head_end = html
    .find("</head>")
    .or_else(|| html.find("</HEAD>"))
    .unwrap_or_else(|| {
        // Find the nearest char boundary at or before 8192
        let max = html.len().min(8192);
        html.floor_char_boundary(max) // nightly; or manually: while !html.is_char_boundary(max) { max -= 1; }
    });
```

---

### CORE-SEC-10: Log File Path From Environment Without Sanitization
**Severity:** Low
**CWE:** CWE-22 (Improper Limitation of a Pathname to a Restricted Directory)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/logging.rs` (lines 357-370)

**Description:**
`resolve_json_log_file()` reads `AXON_LOG_FILE` and `AXON_DATA_DIR` from environment variables and uses them directly as file paths without sanitization:

```rust
fn resolve_json_log_file() -> String {
    std::env::var("AXON_LOG_FILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("AXON_DATA_DIR")
                .ok()
                .map(|d| format!("{d}/axon/logs/axon.log"))
        })
        .unwrap_or_else(|| "logs/axon.log".to_string())
}
```

Setting `AXON_LOG_FILE=/etc/cron.d/backdoor` would cause the application to write structured JSON log data to an arbitrary file path. If the process runs as root, this could overwrite system files.

**Attack Scenario:**
An attacker with control over environment variables (e.g., via a container escape, CI injection, or shared hosting) sets `AXON_LOG_FILE` to a sensitive system path. The rotating log writer creates and writes to that path.

**Remediation:**
1. Validate that the log path is within an expected directory
2. Refuse absolute paths unless they fall under `AXON_DATA_DIR` or a known safe prefix
3. Document that the process should never run as root

```rust
fn resolve_json_log_file() -> String {
    let path = std::env::var("AXON_LOG_FILE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "logs/axon.log".to_string());

    // Reject paths that traverse outside the expected directory
    if path.contains("..") {
        eprintln!("warning: AXON_LOG_FILE contains '..', using default");
        return "logs/axon.log".to_string();
    }
    path
}
```

---

### CORE-SEC-11: ACP Adapter Command From Environment Without Validation
**Severity:** Low
**CWE:** CWE-78 (Improper Neutralization of Special Elements in OS Command)
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/core/config/types/config.rs` (lines 229-235)
- `/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs` (lines 363-370)

**Description:**
The `acp_adapter_cmd` and `acp_adapter_args` fields are read from environment variables and stored without validation:

```rust
acp_adapter_cmd: env::var("AXON_ACP_ADAPTER_CMD")
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty()),
acp_adapter_args: env::var("AXON_ACP_ADAPTER_ARGS")
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty()),
```

`acp_adapter_args` uses pipe-delimited splitting (`|`), and the resulting values are passed to a subprocess. If a caller (MCP, web UI) allows these to be set per-request rather than only at startup, command injection is possible.

**Attack Scenario:**
If `AXON_ACP_ADAPTER_ARGS` is controllable (directly or through a config override path), an attacker could inject arbitrary subprocess arguments. The pipe-delimiter prevents shell metacharacter injection, but argument injection (e.g., `--config|/etc/shadow`) is possible depending on the adapter binary.

**Remediation:**
1. Validate `acp_adapter_cmd` is an absolute path to a known binary
2. Validate `acp_adapter_args` entries against an allowlist of known flags
3. Ensure these values cannot be overridden per-request via MCP or web UI -- they should be startup-only configuration

---

### CORE-SEC-12: HTTP Client Has No Certificate Pinning or Custom CA Bundle
**Severity:** Low
**CWE:** CWE-295 (Improper Certificate Validation)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/http/client.rs` (lines 32-46)

**Description:**
The `build_client()` function creates a reqwest client with default TLS configuration:

```rust
pub fn build_client(timeout_secs: u64) -> Result<reqwest::Client, HttpError> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::custom(|attempt| { ... }))
        .build()?)
}
```

For internal service communication (TEI, Qdrant, Neo4j, LLM endpoints), there is no option to configure a custom CA bundle or certificate pinning. All internal services are trusted based on the system CA store.

**Attack Scenario:**
On a compromised system where the CA store has been tampered with, or in a network where a corporate proxy performs TLS interception, all internal service traffic could be intercepted without detection.

**Remediation:**
Add optional CA bundle configuration:

```rust
let mut builder = reqwest::Client::builder()
    .timeout(Duration::from_secs(timeout_secs))
    .redirect(reqwest::redirect::Policy::custom(|attempt| { ... }));

if let Ok(ca_path) = env::var("AXON_CA_BUNDLE") {
    let cert = std::fs::read(ca_path)?;
    let cert = reqwest::Certificate::from_pem(&cert)?;
    builder = builder.add_root_certificate(cert);
}
```

This is low priority for a self-hosted deployment but becomes important if the tool is deployed in untrusted network environments.

---

### CORE-SEC-13: Docker URL Rewriting Trusts /.dockerenv Existence
**Severity:** Info
**CWE:** CWE-693 (Protection Mechanism Failure)
**File:** `/home/jmagar/workspace/axon_rust/crates/core/config/parse/docker.rs` (line 26)

**Description:**
Docker detection relies on the existence of `/.dockerenv`:

```rust
if std::path::Path::new("/.dockerenv").exists() {
    return url;
}
```

An attacker with filesystem write access could create `/.dockerenv` to prevent URL rewriting, causing the CLI to attempt connections to Docker-internal hostnames (`axon-postgres`, `axon-redis`, etc.) that do not resolve outside Docker. This would cause denial-of-service rather than data compromise.

Conversely, if running inside Docker but `/.dockerenv` is absent (some container runtimes), the CLI would rewrite internal URLs to localhost, causing connection failures.

**Attack Scenario:**
Minimal impact -- filesystem write access implies broader compromise. The worst case is service connectivity failure, not data exfiltration.

**Remediation:**
Add a secondary detection method:

```rust
fn is_inside_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
        || std::fs::read_to_string("/proc/1/cgroup")
            .map(|c| c.contains("docker") || c.contains("containerd"))
            .unwrap_or(false)
}
```

---

## Positive Findings (Security Strengths)

The following security practices are well-implemented and should be preserved:

1. **SSRF Defense-in-Depth**: Three layers of protection -- `validate_url()` on seed URLs, redirect policy validation in `build_client()`, and `ssrf_blacklist_patterns()` on discovered URLs during crawl.

2. **IPv4-Mapped IPv6 Handling**: `check_ip()` correctly extracts embedded IPv4 from `::ffff:x.x.x.x` addresses and recursively applies private range checks (line 122-127 of ssrf.rs). This was a prior production bug that has been thoroughly fixed.

3. **Property-Based SSRF Testing**: `proptest_tests.rs` generates adversarial inputs across full IP ranges (all of 10.0.0.0/8, 127.0.0.0/8, 192.168.0.0/16, 169.254.0.0/16, 172.16-31.x.x, ::ffff: mapped addresses). This is significantly better than hand-written test cases alone.

4. **Parameterized Neo4j Queries**: All Cypher queries use `$variable` parameters (neo4j.rs line 17-23), preventing Cypher injection. No string interpolation is used for query construction.

5. **URL Credential Redaction**: `redact_url()` in content.rs replaces userinfo in URLs with `***` before logging. The `Debug` impl for `Config` redacts all 10 secret fields. Custom headers are also redacted (values replaced with `[REDACTED]`).

6. **HTTP Client Singleton**: `LazyLock<Result<reqwest::Client, String>>` prevents per-call client construction that would exhaust sockets and bypass connection pooling.

7. **Content Transform Safety**: The `readability: false` and `clean_html: false` settings in `build_transform_config()` are well-documented as the result of confirmed production regressions with clear "do not change" warnings.

8. **Test Isolation for SSRF Bypass**: `ALLOW_LOOPBACK` thread-local flag is `#[cfg(test)]`-gated, ensuring test bypass logic never compiles into production builds.

---

## Recommendations Priority Matrix

| Priority | Finding | Effort | Impact |
|----------|---------|--------|--------|
| P1 | CORE-SEC-01: Integrate Secret<T> into Config | Medium | High -- eliminates entire class of credential leaks |
| P1 | CORE-SEC-07: MCP default bind to 127.0.0.1 | Trivial | High -- one-line change, prevents network exposure |
| P2 | CORE-SEC-03: Warn on CLI credential args | Low | Medium -- defense-in-depth for process-list exposure |
| P2 | CORE-SEC-06: Fix constant_time_eq length leak | Low | Medium -- use subtle crate for timing safety |
| P2 | CORE-SEC-04: Replace unsafe env::set_var in tests | Medium | Medium -- eliminates UB, enables test parallelism |
| P3 | CORE-SEC-02: DNS pre-resolution for SSRF | High | High -- but existing mitigations reduce urgency |
| P3 | CORE-SEC-05: Neo4j query timeout + retry | Low | Medium -- resilience improvement |
| P3 | CORE-SEC-08: Warn when accept_invalid_certs used | Trivial | Low -- user awareness |
| P4 | CORE-SEC-10: Sanitize log file path | Trivial | Low -- defense-in-depth |
| P4 | CORE-SEC-11: Validate ACP adapter command | Low | Low -- startup-only config |
| P4 | CORE-SEC-12: Optional CA bundle support | Low | Low -- self-hosted context |
| P4 | CORE-SEC-09: UTF-8 safe byte truncation | Trivial | Low -- already handled by fallback |
| P4 | CORE-SEC-13: Improve Docker detection | Trivial | Minimal -- info only |

---

## Dependency Note

`cargo audit` is not installed in this environment. It should be added to CI and run before every release:

```bash
cargo install cargo-audit
cargo audit
```

Known areas to watch:
- `spider` crate (large dependency surface, custom URL/IP handling)
- `reqwest` + `hyper` (HTTP stack -- track CVEs actively)
- `html5gum` (HTML parsing -- potential for parser differentials)
- `chromiumoxide` (CDP protocol -- deserialization surface)

---

*End of audit report.*
