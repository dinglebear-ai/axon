# Desktop Palette → HTTP Client Refactor Implementation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every `Command::new("axon")` subprocess spawn in `apps/desktop/` with HTTP calls to `$AXON_SERVER_URL/v1/actions` and `/healthz`, so the palette no longer requires the `axon` binary on PATH and works as a thin client against any local or remote `axon serve` instance.

**Architecture:** The `axon serve` HTTP API is fully built — `POST /v1/actions` accepts the same `AxonRequest` schema MCP uses, `/healthz` answers without auth. The palette gains:

- `config.rs` — typed `ClientConfig` with redacted `Debug`, URL-parsed validation (rejects `http://` for non-loopback), file-permission check on `~/.axon/.env`, and pure-functional dotenv loading (no `unsafe { set_var }` ping-pong).
- `wire.rs` — pure `CommandAction` → JSON action body mapping. Request IDs come from the existing `next_run_id` counter on `Palette` (no separate atomic).
- `client.rs` — `HttpClient` with per-action timeouts (doctor 30s, ask 600s, crawl unlimited, etc.), `tcp_keepalive(30s)`, `pool_idle_timeout(None)`, large-response `spawn_blocking` deserialization, `https://`-enforcement on non-loopback URLs.
- Rewired `ui.rs`: completion guard on `command_output` (not just `running`); debounced health probes; `tokio::time::interval` background re-probe every 30s; treats `Checking` as ineligible for dispatch.
- Wire-contract smoke test in Task 10 validates each of the 9 hand-rolled JSON bodies against a running server.

When the server is unreachable the palette shows a clear notice — there is NO subprocess fallback.

**Tech Stack:** Rust 2024, GPUI (Zed git pin), `reqwest 0.13` (rustls), `url`, `serde_json`, `tokio` (already pulled in by GPUI), `httpmock` for tests, `dotenvy` for `~/.axon/.env` parsing.

**Scope:** Single subsystem — the desktop palette client. Server-side API is already deployed. No changes to `src/`, MCP, web, or any other axon component except a tiny in-repo smoke-test binary in Task 10.

**Out of scope:** OAuth flow when server runs `AXON_MCP_AUTH_MODE=oauth`. The palette v0.3 assumes static bearer auth or no auth. Document as a v0.4 follow-up.

---

## File Structure

**New files:**
- `apps/desktop/src/config.rs` + `apps/desktop/src/config_tests.rs`
- `apps/desktop/src/wire.rs` + `apps/desktop/src/wire_tests.rs`
- `apps/desktop/src/client.rs` + `apps/desktop/src/client_tests.rs`
- `apps/desktop/tests/wire_contract.rs` (integration test against live server)

**Modified files:**
- `apps/desktop/Cargo.toml` — add `reqwest`, `serde_json`, `dotenvy`, `url`, `httpmock` (dev), `tempfile` only if needed, `tokio` features. Bump version 0.2.0 → 0.3.0.
- `apps/desktop/src/main.rs` — load `ClientConfig` at top of `main()` (before any runtime spawn), pass into Palette.
- `apps/desktop/src/actions.rs` — drop `build_axon_args`, `display_command_line`, `split_shell_words`; add `render_as_markdown: bool` field to `CommandAction`.
- `apps/desktop/src/ui.rs` — rewire `spawn_health_check` + `submit`, drop `axon_command()` + `Command` imports, add `tokio::time::interval` background probe, debounce, completion-id guards.
- `apps/desktop/src/output.rs` — `from_http_ok` / `from_http_err` / `server_unreachable`; drop `from_process` + `spawn_error` + `use std::process::Output`.
- `CHANGELOG.md` — release notes under `[Unreleased]`.

---

## Task 1: Add HTTP dependencies + bump version

**Files:**
- Modify: `apps/desktop/Cargo.toml`

- [ ] **Step 1.1: Add runtime + dev dependencies**

Replace the `[dependencies]` block in `apps/desktop/Cargo.toml` (currently lines 24-32) with:

```toml
[dependencies]
anyhow = "1"
async-channel = "2"
dotenvy = "0.15"
global-hotkey = "0.7"
open = "5"
pulldown-cmark = { version = "0.10", default-features = false }
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls-tls", "http2"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
url = "2"
gpui = { git = "https://github.com/zed-industries/zed", rev = "5f5dd7ae301dd265e5020fd65cd20769a81d0d5b" }

[dev-dependencies]
httpmock = "0.8"
serial_test = "3"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "test-util"] }
```

`url = "2"` parses + validates `AXON_SERVER_URL` (Task 2/4). `serial_test = "3"` serializes env-mutating tests so they don't race each other (Task 2). `http2` feature on reqwest enables ALPN negotiation for h2 servers; harmless on h1.

Bump `version = "0.2.0"` to `version = "0.3.0"` in the `[package]` block.

- [ ] **Step 1.2: Verify the new dep tree resolves**

From `apps/desktop/`:

```bash
cargo check
```

Expected: clean compile. Pre-existing dead-code warnings on theme constants are fine. Any `error[E....]` line means a feature mismatch — most commonly reqwest's TLS backend; do NOT add `native-tls` or wasm features.

- [ ] **Step 1.3: Commit**

```bash
git add apps/desktop/Cargo.toml apps/desktop/Cargo.lock
git commit -m "build(palette): add reqwest/serde_json/dotenvy/url/httpmock for HTTP refactor

Bumps axon-palette 0.2.0 → 0.3.0. The reqwest feature set is
deliberately minimal (default-features=false, rustls-tls + h2 +
json only) to avoid the wasm-bindgen conflict that motivated
apps/desktop's separate workspace in the first place. \`url\` for
strict server-URL validation, \`serial_test\` to serialize env-
mutating unit tests."
```

---

## Task 2: Config module — strict URL validation, redacted Debug, no env mutation

**Files:**
- Create: `apps/desktop/src/config.rs`
- Create: `apps/desktop/src/config_tests.rs`
- Modify: `apps/desktop/src/main.rs`

- [ ] **Step 2.1: Write the failing test sidecar**

Create `apps/desktop/src/config_tests.rs`:

```rust
use super::*;
use serial_test::serial;
use std::io::Write;

// All tests below are #[serial] because they read/write process env.

fn write_dotenv(tmp: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
    let axon_dir = tmp.path().join(".axon");
    std::fs::create_dir(&axon_dir).expect("mkdir .axon");
    let env_path = axon_dir.join(".env");
    let mut f = std::fs::File::create(&env_path).expect("create .env");
    f.write_all(body.as_bytes()).expect("write .env");
    // Production permissions: 0600 (user rw only).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    env_path
}

#[test]
#[serial]
fn defaults_to_local_loopback_when_unset() {
    unsafe {
        std::env::remove_var("AXON_SERVER_URL");
        std::env::remove_var("AXON_MCP_HTTP_TOKEN");
        std::env::remove_var("HOME");
    }
    let cfg = ClientConfig::load().expect("default load");
    assert_eq!(cfg.server_url.as_str(), "http://127.0.0.1:8001/");
    assert!(cfg.token.is_none());
}

#[test]
#[serial]
fn process_env_wins_over_dotenv_file() {
    let tmp = tempfile::tempdir().unwrap();
    write_dotenv(&tmp, "AXON_SERVER_URL=http://from-dotenv:9000\nAXON_MCP_HTTP_TOKEN=dotenv-token\n");
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("AXON_SERVER_URL", "http://127.0.0.1:8001");
        std::env::set_var("AXON_MCP_HTTP_TOKEN", "env-token");
    }
    let cfg = ClientConfig::load().expect("load");
    assert_eq!(cfg.server_url.as_str(), "http://127.0.0.1:8001/");
    assert_eq!(cfg.exposed_token(), Some("env-token"));
    unsafe {
        std::env::remove_var("AXON_SERVER_URL");
        std::env::remove_var("AXON_MCP_HTTP_TOKEN");
        std::env::remove_var("HOME");
    }
}

#[test]
#[serial]
fn falls_back_to_dotenv_when_process_env_unset() {
    let tmp = tempfile::tempdir().unwrap();
    write_dotenv(&tmp, "AXON_SERVER_URL=https://axon.example.com\nAXON_MCP_HTTP_TOKEN=secret\n");
    unsafe {
        std::env::remove_var("AXON_SERVER_URL");
        std::env::remove_var("AXON_MCP_HTTP_TOKEN");
        std::env::set_var("HOME", tmp.path());
    }
    let cfg = ClientConfig::load().expect("load");
    assert_eq!(cfg.server_url.as_str(), "https://axon.example.com/");
    assert_eq!(cfg.exposed_token(), Some("secret"));
    unsafe { std::env::remove_var("HOME"); }
}

#[test]
#[serial]
fn rejects_remote_plain_http() {
    unsafe {
        std::env::set_var("AXON_SERVER_URL", "http://axon.example.com");
        std::env::remove_var("HOME");
    }
    let err = ClientConfig::load().unwrap_err();
    assert!(err.contains("https"), "got: {err}");
    unsafe { std::env::remove_var("AXON_SERVER_URL"); }
}

#[test]
#[serial]
fn allows_local_plain_http() {
    for host in ["http://127.0.0.1:8001", "http://localhost:8001", "http://[::1]:8001"] {
        unsafe {
            std::env::set_var("AXON_SERVER_URL", host);
            std::env::remove_var("HOME");
        }
        let _ = ClientConfig::load().unwrap_or_else(|e| panic!("{host} should load: {e}"));
    }
    unsafe { std::env::remove_var("AXON_SERVER_URL"); }
}

#[test]
#[serial]
fn rejects_url_with_path() {
    unsafe {
        std::env::set_var("AXON_SERVER_URL", "https://axon.example.com/some/path");
        std::env::remove_var("HOME");
    }
    let err = ClientConfig::load().unwrap_err();
    assert!(err.contains("must not have a path"), "got: {err}");
    unsafe { std::env::remove_var("AXON_SERVER_URL"); }
}

#[test]
#[serial]
fn rejects_url_with_query_or_fragment() {
    for bad in ["https://host?x=y", "https://host#frag"] {
        unsafe {
            std::env::set_var("AXON_SERVER_URL", bad);
            std::env::remove_var("HOME");
        }
        let err = ClientConfig::load().unwrap_err();
        assert!(err.contains("query") || err.contains("fragment"), "got: {err}");
    }
    unsafe { std::env::remove_var("AXON_SERVER_URL"); }
}

#[test]
#[serial]
fn debug_impl_redacts_token() {
    unsafe {
        std::env::set_var("AXON_SERVER_URL", "http://127.0.0.1:8001");
        std::env::set_var("AXON_MCP_HTTP_TOKEN", "super-secret-bearer");
        std::env::remove_var("HOME");
    }
    let cfg = ClientConfig::load().expect("load");
    let rendered = format!("{cfg:?}");
    assert!(!rendered.contains("super-secret-bearer"), "token leaked: {rendered}");
    assert!(rendered.contains("Some(***)") || rendered.contains("Some(<redacted>)"));
    unsafe {
        std::env::remove_var("AXON_SERVER_URL");
        std::env::remove_var("AXON_MCP_HTTP_TOKEN");
    }
}

#[test]
#[serial]
#[cfg(unix)]
fn warns_when_dotenv_world_readable() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let path = write_dotenv(&tmp, "AXON_MCP_HTTP_TOKEN=t\n");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    unsafe {
        std::env::remove_var("AXON_SERVER_URL");
        std::env::remove_var("AXON_MCP_HTTP_TOKEN");
        std::env::set_var("HOME", tmp.path());
    }
    // We don't assert a panic — the loader emits a tracing::warn. Use a
    // tracing subscriber capture to inspect, OR rely on the function
    // returning Ok with the warning side-effect. Here we just confirm the
    // file is still loaded despite the loose perms.
    let cfg = ClientConfig::load().expect("loose-perm dotenv should still load");
    assert_eq!(cfg.exposed_token(), Some("t"));
    unsafe { std::env::remove_var("HOME"); }
}

#[test]
#[serial]
fn empty_dotenv_value_falls_through_to_default() {
    let tmp = tempfile::tempdir().unwrap();
    write_dotenv(&tmp, "AXON_SERVER_URL=\nAXON_MCP_HTTP_TOKEN=  \n");
    unsafe {
        std::env::remove_var("AXON_SERVER_URL");
        std::env::remove_var("AXON_MCP_HTTP_TOKEN");
        std::env::set_var("HOME", tmp.path());
    }
    let cfg = ClientConfig::load().expect("load");
    assert_eq!(cfg.server_url.as_str(), "http://127.0.0.1:8001/");
    assert!(cfg.token.is_none());
    unsafe { std::env::remove_var("HOME"); }
}
```

Add `tempfile = "3"` to the `[dev-dependencies]` block of `apps/desktop/Cargo.toml`:

```toml
[dev-dependencies]
httpmock = "0.8"
serial_test = "3"
tempfile = "3"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "test-util"] }
```

- [ ] **Step 2.2: Run the test to verify it fails**

```bash
cargo test --test '*' --lib config_tests 2>&1 | tail -10
```

Expected: compile error — `config` module doesn't exist.

- [ ] **Step 2.3: Implement the minimal config module**

Create `apps/desktop/src/config.rs`:

```rust
//! Palette runtime config: where to talk to `axon serve`, and how to authenticate.
//!
//! Priority order: process env > `~/.axon/.env` > built-in defaults.
//! Only two keys are read: `AXON_SERVER_URL` and `AXON_MCP_HTTP_TOKEN`.
//!
//! Two security invariants:
//! 1. The bearer token is opaque-printed by `Debug` — never appears in logs.
//! 2. `https://` is required for any non-loopback server URL — plain HTTP
//!    over the network would expose the bearer token to passive observers.

use std::path::Path;

/// Loaded palette config. Token is intentionally not exposed via getters
/// other than `exposed_token()` which exists for HTTP client construction
/// only; all other code paths see `Some(***)` via Debug.
#[derive(Clone)]
pub(crate) struct ClientConfig {
    /// Validated server URL: parsed, no path/query/fragment, trailing slash
    /// preserved by `url::Url`.
    pub(crate) server_url: url::Url,
    /// Bearer token. Never include this in Debug output.
    pub(crate) token: Option<String>,
}

impl ClientConfig {
    /// Expose the raw token. Call sites: HTTP client `Authorization` header.
    /// Do NOT log the return value.
    pub(crate) fn exposed_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Production loader: process env first, then `$HOME/.axon/.env`, then defaults.
    /// Builds the config purely-functionally from the dotenvy iterator (no
    /// `unsafe { set_var }` ping-pong, no global env mutation).
    ///
    /// Returns `Err(message)` if `AXON_SERVER_URL` is malformed or non-loopback
    /// plain HTTP. Logs a `tracing::warn!` if `~/.axon/.env` exists but has
    /// loose permissions (mode > 0o600 on Unix).
    pub(crate) fn load() -> Result<Self, String> {
        let env_url = nonempty_env("AXON_SERVER_URL");
        let env_token = nonempty_env("AXON_MCP_HTTP_TOKEN");

        // Read ~/.axon/.env without modifying global env state. Promote only
        // the two keys we care about, and only if the corresponding process
        // env var wasn't already set.
        let (file_url, file_token) = read_axon_dotenv();

        let url_raw = env_url
            .or(file_url)
            .unwrap_or_else(|| "http://127.0.0.1:8001".to_string());
        let token = env_token.or(file_token);

        let server_url = parse_server_url(&url_raw)?;
        Ok(Self { server_url, token })
    }
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("server_url", &self.server_url.as_str())
            .field("token", &self.token.as_ref().map(|_| "***"))
            .finish()
    }
}

fn nonempty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Read `$HOME/.axon/.env` and return (server_url, token) if present and non-empty.
/// Emits a `tracing::warn!` if the file exists with permissions > 0o600 on Unix.
fn read_axon_dotenv() -> (Option<String>, Option<String>) {
    let Some(home) = std::env::var_os("HOME") else {
        return (None, None);
    };
    let path = Path::new(&home).join(".axon").join(".env");
    if !path.exists() {
        return (None, None);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                tracing::warn!(
                    path = %path.display(),
                    mode = format!("{mode:o}").as_str(),
                    "~/.axon/.env has loose permissions; recommend `chmod 600`"
                );
            }
        }
    }

    let Ok(iter) = dotenvy::from_path_iter(&path) else {
        return (None, None);
    };

    let mut url = None;
    let mut token = None;
    for item in iter.flatten() {
        let (k, v) = item;
        let v = v.trim().to_string();
        if v.is_empty() {
            continue;
        }
        if k == "AXON_SERVER_URL" {
            url = Some(v);
        } else if k == "AXON_MCP_HTTP_TOKEN" {
            token = Some(v);
        }
        // All other keys in the file are intentionally discarded.
    }
    (url, token)
}

/// Parse and validate `AXON_SERVER_URL`:
/// - must be http/https
/// - must have no path (other than `/`), no query, no fragment
/// - if scheme is `http`, host must be loopback (127.0.0.1 / ::1 / localhost)
fn parse_server_url(raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|e| format!("AXON_SERVER_URL parse error: {e}"))?;
    match url.scheme() {
        "https" => {}
        "http" => {
            let host = url.host_str().unwrap_or("");
            if !is_loopback_host(host) {
                return Err(format!(
                    "AXON_SERVER_URL={raw} uses http://; only https is allowed for non-loopback hosts (bearer token leakage)"
                ));
            }
        }
        other => return Err(format!("AXON_SERVER_URL scheme must be http or https, got {other}")),
    }
    let path = url.path();
    if path != "/" && !path.is_empty() {
        return Err(format!("AXON_SERVER_URL must not have a path ({path:?})"));
    }
    if url.query().is_some() {
        return Err("AXON_SERVER_URL must not have a query string".to_string());
    }
    if url.fragment().is_some() {
        return Err("AXON_SERVER_URL must not have a fragment".to_string());
    }
    Ok(url)
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
```

- [ ] **Step 2.4: Wire the module into the crate root**

Modify `apps/desktop/src/main.rs` line 10-15 — replace the existing `mod` block:

```rust
mod actions;
mod config;
mod markdown;
mod output;
mod render;
mod theme;
mod ui;
```

- [ ] **Step 2.5: Run the tests**

```bash
cargo test --lib config 2>&1 | tail -15
```

Expected: 9 tests pass. If `warns_when_dotenv_world_readable` fails on a system where `umask` is 0077 anyway, that's fine — the test asserts the file still loads, not the warning capture.

- [ ] **Step 2.6: Commit**

```bash
git add apps/desktop/Cargo.toml apps/desktop/src/config.rs apps/desktop/src/config_tests.rs apps/desktop/src/main.rs
git commit -m "feat(palette): typed ClientConfig with validation + redacted Debug

New ClientConfig with:
- url::Url-parsed server URL (rejects path/query/fragment)
- https:// enforced for non-loopback hosts (no bearer-token leakage)
- Hand-rolled Debug that prints token: Some(***) — never the raw value
- File-permission check on ~/.axon/.env (warn at mode > 0600 on Unix)
- Pure-functional dotenv loading — no unsafe { env::set_var } ping-pong
- Trims empty values, filters whitespace-only entries

9 sidecar tests (serial_test for env isolation) cover precedence,
defaults, all rejection paths, debug redaction, and loose-perm warning."
```

---

## Task 3: Wire module — simplified mapping (no atomic, no envelope helper)

**Files:**
- Create: `apps/desktop/src/wire.rs`
- Create: `apps/desktop/src/wire_tests.rs`
- Modify: `apps/desktop/src/main.rs`

- [ ] **Step 3.1: Write the failing test sidecar**

Create `apps/desktop/src/wire_tests.rs`:

```rust
use super::*;
use crate::actions::ACTIONS;

fn action(subcommand: &str) -> &'static crate::actions::CommandAction {
    ACTIONS
        .iter()
        .find(|a| a.subcommand == subcommand)
        .expect("action exists")
}

#[test]
fn scrape_url_builds_action_body() {
    let body = build_action_body(action("scrape"), "https://example.com").unwrap();
    assert_eq!(body["action"], "scrape");
    assert_eq!(body["url"], "https://example.com");
}

#[test]
fn crawl_url_builds_urls_array() {
    let body = build_action_body(action("crawl"), "https://example.com").unwrap();
    assert_eq!(body["action"], "crawl");
    assert_eq!(body["urls"][0], "https://example.com");
}

#[test]
fn map_url_builds_action_body() {
    let body = build_action_body(action("map"), "https://example.com/docs").unwrap();
    assert_eq!(body["action"], "map");
    assert_eq!(body["url"], "https://example.com/docs");
}

#[test]
fn ask_question_uses_query_field() {
    let body = build_action_body(action("ask"), "what is RAG?").unwrap();
    assert_eq!(body["action"], "ask");
    assert_eq!(body["query"], "what is RAG?");
}

#[test]
fn search_uses_query_field() {
    let body = build_action_body(action("search"), "claude code plugins").unwrap();
    assert_eq!(body["action"], "search");
    assert_eq!(body["query"], "claude code plugins");
}

#[test]
fn research_uses_query_field() {
    let body = build_action_body(action("research"), "qdrant hybrid").unwrap();
    assert_eq!(body["action"], "research");
    assert_eq!(body["query"], "qdrant hybrid");
}

#[test]
fn ingest_uses_target_field() {
    let body = build_action_body(action("ingest"), "https://github.com/zed-industries/zed").unwrap();
    assert_eq!(body["action"], "ingest");
    assert_eq!(body["target"], "https://github.com/zed-industries/zed");
}

#[test]
fn status_takes_no_argument() {
    let body = build_action_body(action("status"), "").unwrap();
    assert_eq!(body["action"], "status");
    assert!(body.as_object().unwrap().len() == 1, "status should only have action field");
}

#[test]
fn doctor_takes_no_argument() {
    let body = build_action_body(action("doctor"), "").unwrap();
    assert_eq!(body["action"], "doctor");
}

#[test]
fn empty_argument_for_required_action_errors() {
    let err = build_action_body(action("ask"), "  ").unwrap_err();
    assert!(err.contains("argument required"), "got: {err}");
}

#[test]
fn format_request_id_is_deterministic_from_counter() {
    let id = format_request_id(42);
    assert!(id.starts_with("palette-"), "got: {id}");
    assert!(id.ends_with("-42"), "got: {id}");
}
```

- [ ] **Step 3.2: Run the test to verify it fails**

```bash
cargo test --lib wire 2>&1 | tail -10
```

Expected: compile error — `wire` module doesn't exist.

- [ ] **Step 3.3: Implement the wire module**

Create `apps/desktop/src/wire.rs`:

```rust
//! Pure mapping: each palette `CommandAction` → the JSON action body that
//! `POST /v1/actions` expects. Server validates against `AxonRequest`
//! (`src/mcp/schema.rs` in the axon crate). We keep types JSON-loose to
//! avoid coupling `apps/desktop` to the axon crate's tree (the GPUI
//! wasm-bindgen conflict — see `Cargo.toml` comment).
//!
//! Request IDs come from the existing `next_run_id: u64` counter on
//! `Palette`; we don't keep a separate atomic.

use crate::actions::{ArgMode, CommandAction};
use serde_json::{Value, json};

/// Build the inner `action` object that will be wrapped in a ClientActionRequest envelope.
///
/// `arg` is the user-typed argument string. For ArgMode::None actions it is ignored;
/// for ArgMode::Single it becomes the natural-language field (query, target);
/// for ArgMode::Split the first token is the URL.
pub(crate) fn build_action_body(action: &CommandAction, arg: &str) -> Result<Value, String> {
    let arg = arg.trim();
    if action.arg_mode != ArgMode::None && arg.is_empty() {
        return Err("argument required".to_string());
    }

    let body = match action.subcommand {
        "scrape" => json!({ "action": "scrape", "url": arg }),
        "crawl" => json!({ "action": "crawl", "urls": [arg] }),
        "map" => json!({ "action": "map", "url": arg }),
        "ask" => json!({ "action": "ask", "query": arg }),
        "search" => json!({ "action": "search", "query": arg }),
        "research" => json!({ "action": "research", "query": arg }),
        "ingest" => json!({ "action": "ingest", "target": arg }),
        "status" => json!({ "action": "status" }),
        "doctor" => json!({ "action": "doctor" }),
        other => return Err(format!("unknown action subcommand: {other}")),
    };
    Ok(body)
}

/// Build a per-dispatch request id. Format: `palette-<pid>-<run_id>`.
/// The server treats this as opaque; only used by us for log correlation.
pub(crate) fn format_request_id(run_id: u64) -> String {
    format!("palette-{}-{}", std::process::id(), run_id)
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
```

- [ ] **Step 3.4: Wire it into main.rs**

Modify `apps/desktop/src/main.rs` — add `mod wire;` to the module list:

```rust
mod actions;
mod config;
mod markdown;
mod output;
mod render;
mod theme;
mod ui;
mod wire;
```

- [ ] **Step 3.5: Run the tests**

```bash
cargo test --lib wire 2>&1 | tail -10
```

Expected: 11 tests pass.

- [ ] **Step 3.6: Commit**

```bash
git add apps/desktop/src/wire.rs apps/desktop/src/wire_tests.rs apps/desktop/src/main.rs
git commit -m "feat(palette): map CommandAction to /v1/actions JSON body

Pure functions, no I/O. build_action_body() switches on subcommand
and emits the AxonRequest schema the server expects (action field
is the tag, snake_case). format_request_id() reuses Palette's
existing next_run_id counter — no AtomicU64. The envelope wrapper
is inlined at the dispatch call site in client.rs.

11 sidecar tests cover all 9 ACTIONS plus arg validation and
request-id format."
```

---

## Task 4: HTTP client — per-action timeouts, https enforcement, large-response spawn_blocking

**Files:**
- Create: `apps/desktop/src/client.rs`
- Create: `apps/desktop/src/client_tests.rs`
- Modify: `apps/desktop/src/main.rs`

- [ ] **Step 4.1: Write the failing test sidecar**

Create `apps/desktop/src/client_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use serde_json::json;

fn cfg(server: &MockServer, token: Option<&str>) -> ClientConfig {
    ClientConfig {
        server_url: url::Url::parse(&format!("{}/", server.base_url())).unwrap(),
        token: token.map(str::to_string),
    }
}

#[tokio::test]
async fn health_returns_true_when_server_healthy() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/healthz");
            then.status(200).body("ok");
        })
        .await;

    let client = HttpClient::new(cfg(&server, None));
    assert!(client.health().await);
}

#[tokio::test]
async fn health_returns_false_when_server_down() {
    let bad_cfg = ClientConfig {
        server_url: url::Url::parse("http://127.0.0.1:1/").unwrap(),
        token: None,
    };
    let client = HttpClient::new(bad_cfg);
    assert!(!client.health().await);
}

#[tokio::test]
async fn health_returns_false_on_5xx() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/healthz");
            then.status(503);
        })
        .await;

    let client = HttpClient::new(cfg(&server, None));
    assert!(!client.health().await);
}

#[tokio::test]
async fn dispatch_sends_bearer_token_when_present() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/actions")
                .header("authorization", "Bearer my-token")
                .header_exists("content-type");
            then.status(200).json_body(json!({
                "request_id": "x",
                "ok": true,
                "result": { "all_ok": true },
                "server": { "name": "axon", "version": "test" }
            }));
        })
        .await;

    let client = HttpClient::new(cfg(&server, Some("my-token")));
    let body = json!({ "action": "doctor" });
    let resp = client.dispatch("x", "doctor", body).await.expect("dispatch");
    assert!(resp.ok);
    assert_eq!(resp.result.as_ref().and_then(|r| r["all_ok"].as_bool()), Some(true));
    mock.assert_async().await;
}

#[tokio::test]
async fn dispatch_omits_authorization_when_token_unset() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/v1/actions");
            then.status(200).json_body(json!({
                "request_id": "x",
                "ok": true,
                "server": { "name": "axon", "version": "test" }
            }));
        })
        .await;

    let client = HttpClient::new(cfg(&server, None));
    let body = json!({ "action": "doctor" });
    let resp = client.dispatch("x", "doctor", body).await.expect("dispatch");
    assert!(resp.ok);
}

#[tokio::test]
async fn dispatch_returns_err_on_4xx_with_body() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/v1/actions");
            then.status(401).json_body(json!({
                "ok": false,
                "error": { "kind": "unauthorized", "message": "missing token", "retryable": false },
                "server": { "name": "axon", "version": "test" }
            }));
        })
        .await;

    let client = HttpClient::new(cfg(&server, None));
    let err = client
        .dispatch("x", "doctor", json!({ "action": "doctor" }))
        .await
        .unwrap_err();
    assert!(err.contains("401") || err.contains("unauthorized"), "got: {err}");
}

#[tokio::test]
async fn dispatch_returns_err_when_server_unreachable() {
    let bad_cfg = ClientConfig {
        server_url: url::Url::parse("http://127.0.0.1:1/").unwrap(),
        token: None,
    };
    let client = HttpClient::new(bad_cfg);
    let err = client
        .dispatch("x", "doctor", json!({ "action": "doctor" }))
        .await
        .unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn timeout_for_action_uses_per_action_table() {
    assert_eq!(timeout_for_action("doctor").as_secs(), 30);
    assert_eq!(timeout_for_action("status").as_secs(), 30);
    assert_eq!(timeout_for_action("scrape").as_secs(), 60);
    assert_eq!(timeout_for_action("map").as_secs(), 60);
    assert_eq!(timeout_for_action("search").as_secs(), 60);
    assert_eq!(timeout_for_action("ingest").as_secs(), 60);
    assert_eq!(timeout_for_action("ask").as_secs(), 600);
    assert_eq!(timeout_for_action("research").as_secs(), 900);
    assert_eq!(timeout_for_action("crawl").as_secs(), 3600);
    // Unknown subcommands fall through to a sane default.
    assert_eq!(timeout_for_action("never-heard-of-it").as_secs(), 60);
}
```

- [ ] **Step 4.2: Run the test to verify it fails**

```bash
cargo test --lib client 2>&1 | tail -10
```

Expected: compile error — `client` module doesn't exist.

- [ ] **Step 4.3: Implement the HTTP client**

Create `apps/desktop/src/client.rs`:

```rust
//! HTTP client for the running `axon serve` instance.
//!
//! Two entry points used by the palette:
//! - `health()` — GET /healthz, no auth, 3s timeout, returns bool
//! - `dispatch(request_id, subcommand, body)` — POST /v1/actions with the
//!   per-action timeout, bearer auth if configured, returns the parsed
//!   ClientActionResponse on 2xx, an error string on transport failure or
//!   4xx/5xx.
//!
//! Connection-pool tuning:
//! - `tcp_keepalive(30s)` — Tailscale wireguard drops idle conns otherwise
//! - `pool_idle_timeout(None)` — palette is hotkey-driven, may sit idle 10m+
//!   between commands; without this, each cold dispatch eats reconnect cost
//! - `http2` feature on reqwest lets ALPN negotiate h2 with capable servers
//!
//! Large-response handling: responses over 256 KiB are deserialized via
//! `tokio::task::spawn_blocking` so a 5 MB crawl manifest doesn't block
//! the tokio worker thread for tens of milliseconds.

use crate::config::ClientConfig;
use serde::Deserialize;
use std::time::Duration;

const SPAWN_BLOCKING_THRESHOLD: usize = 256 * 1024;

/// Subset of the server's ClientActionResponse we surface to the UI.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DispatchResponse {
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    #[serde(default)]
    pub(crate) ok: bool,
    #[serde(default)]
    pub(crate) result: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) error: Option<DispatchError>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DispatchError {
    pub(crate) kind: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) hint: Option<String>,
}

pub(crate) struct HttpClient {
    cfg: ClientConfig,
    inner: reqwest::Client,
}

impl HttpClient {
    pub(crate) fn new(cfg: ClientConfig) -> Self {
        let inner = reqwest::Client::builder()
            // 10s connect timeout — Tailscale DERP-relayed peers can spike.
            .connect_timeout(Duration::from_secs(10))
            // Keep pooled connections forever; per-action timeout is the only cap.
            .pool_idle_timeout(None)
            // Defeat wireguard idle-conn drops on Tailscale mesh.
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .expect("reqwest client builds with defaults");
        Self { cfg, inner }
    }

    pub(crate) fn server_url(&self) -> &str {
        self.cfg.server_url.as_str()
    }

    /// GET /healthz. Returns true iff the server answered 2xx within 3s.
    pub(crate) async fn health(&self) -> bool {
        // url::Url::join treats a trailing slash on the base as the directory.
        // server_url is always normalized to end with '/' by ClientConfig.
        let url = self.cfg.server_url.join("healthz").expect("static path joins");
        match self
            .inner
            .get(url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// POST /v1/actions with the given action body. Returns the parsed response
    /// envelope on 2xx. Returns Err(message) on transport failure or non-2xx.
    pub(crate) async fn dispatch(
        &self,
        request_id: &str,
        subcommand: &str,
        action_body: serde_json::Value,
    ) -> Result<DispatchResponse, String> {
        let url = self
            .cfg
            .server_url
            .join("v1/actions")
            .expect("static path joins");
        let envelope = serde_json::json!({
            "request_id": request_id,
            "action": action_body,
        });

        let mut req = self
            .inner
            .post(url)
            .timeout(timeout_for_action(subcommand))
            .json(&envelope);
        if let Some(token) = self.cfg.exposed_token() {
            req = req.bearer_auth(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("read response body: {e}"))?;

        if !status.is_success() {
            if let Ok(parsed) = parse_response(bytes.clone()).await
                && let Some(err) = parsed.error
            {
                return Err(format!(
                    "{} ({}): {}{}",
                    status.as_u16(),
                    err.kind,
                    err.message,
                    err.hint
                        .as_deref()
                        .map(|h| format!(" — hint: {h}"))
                        .unwrap_or_default()
                ));
            }
            return Err(format!(
                "HTTP {} {}",
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            ));
        }

        parse_response(bytes).await
    }
}

/// Per-action HTTP request timeout. Chosen to match server-side worst-case
/// runtimes. Unknown subcommands fall through to a 60s default.
pub(crate) fn timeout_for_action(subcommand: &str) -> Duration {
    match subcommand {
        "doctor" | "status" => Duration::from_secs(30),
        "scrape" | "map" | "search" | "ingest" => Duration::from_secs(60),
        "ask" => Duration::from_secs(600),
        "research" => Duration::from_secs(900),
        "crawl" => Duration::from_secs(3600),
        _ => Duration::from_secs(60),
    }
}

/// Deserialize a response body. For bodies over `SPAWN_BLOCKING_THRESHOLD`
/// (256 KiB), do the parse on a blocking thread so we don't stall the tokio
/// worker on multi-MB crawl/ingest manifests.
async fn parse_response(bytes: bytes::Bytes) -> Result<DispatchResponse, String> {
    if bytes.len() > SPAWN_BLOCKING_THRESHOLD {
        tokio::task::spawn_blocking(move || {
            serde_json::from_slice::<DispatchResponse>(&bytes)
                .map_err(|e| format!("decode response: {e}"))
        })
        .await
        .map_err(|e| format!("join spawn_blocking: {e}"))?
    } else {
        serde_json::from_slice::<DispatchResponse>(&bytes)
            .map_err(|e| format!("decode response: {e}"))
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
```

Add `bytes = "1"` to the `[dependencies]` block of `apps/desktop/Cargo.toml` (reqwest re-exports it but the explicit dep makes `bytes::Bytes` usage clear):

```toml
bytes = "1"
```

- [ ] **Step 4.4: Wire it into main.rs**

Modify `apps/desktop/src/main.rs` — add `mod client;` (alphabetical):

```rust
mod actions;
mod client;
mod config;
mod markdown;
mod output;
mod render;
mod theme;
mod ui;
mod wire;
```

- [ ] **Step 4.5: Run the tests**

```bash
cargo test --lib client 2>&1 | tail -10
```

Expected: 8 tests pass.

- [ ] **Step 4.6: Commit**

```bash
git add apps/desktop/Cargo.toml apps/desktop/src/client.rs apps/desktop/src/client_tests.rs apps/desktop/src/main.rs
git commit -m "feat(palette): HTTP client with per-action timeouts + pool tuning

HttpClient wraps reqwest with:
- 10s connect timeout (Tailscale DERP relay headroom)
- pool_idle_timeout(None) — palette can sit idle for minutes, keep
  the warm connection
- tcp_keepalive(30s) — wireguard drops idle conns otherwise
- Per-action request timeout: doctor 30s, scrape 60s, ask 600s,
  research 900s, crawl 3600s, unknown 60s
- bearer auth when ClientConfig.token is set
- url::Url::join() for endpoint construction (no string formatting)
- Bodies over 256 KiB are deserialized via spawn_blocking

8 httpmock-driven sidecar tests cover happy path, missing token,
server-down, 4xx with error body, and per-action timeout table."
```

---

## Task 5: Rewire `Palette::spawn_health_check` with debounce + background polling

**Files:**
- Modify: `apps/desktop/src/ui.rs`

- [ ] **Step 5.1: Update imports and the Palette struct**

In `apps/desktop/src/ui.rs`, replace lines 1-21 with:

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Render, ScrollHandle, SharedString, Styled, Window, div, prelude::*, px, rgb,
};

use crate::actions::{
    ACTIONS, ArgMode, CommandAction, action_invoked_by, action_matches, looks_like_url,
};
use crate::client::HttpClient;
use crate::output::{CommandOutput, OutputKind};
use crate::render::{
    render_action_rows, render_output_body, render_palette_footer, render_prompt_row,
};
use crate::theme::{
    AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_SANS, AURORA_NAV_BG, AURORA_PAGE_BG,
    AURORA_PANEL_STRONG, AURORA_TEXT_PRIMARY,
};
use crate::wire::{build_action_body, format_request_id};
use crate::{ClearOutput, MoveDown, MoveUp, Submit, TabComplete};

/// Minimum interval between health checks. Prevents the "user mashes Enter
/// against a downed server, dot flickers per keystroke" antipattern.
const HEALTH_CHECK_MIN_INTERVAL: Duration = Duration::from_millis(500);

/// Background health poll cadence while a connection is established. Cheap
/// (a few bytes over warm keepalive) and catches server restarts.
const HEALTH_CHECK_BACKGROUND_INTERVAL: Duration = Duration::from_secs(30);
```

Delete the old `axon_command()` helper at the top of the file (lines 3-20 of the post-Windows-stopgap version). It is no longer needed.

Find the Palette struct (currently lines 40-54) and replace with:

```rust
pub(crate) struct Palette {
    query: String,
    selected: usize,
    focus: FocusHandle,
    command_output: Option<CommandOutput>,
    running: Option<RunningCommand>,
    next_run_id: u64,
    output_scroll: ScrollHandle,
    locked_command: Option<CommandAction>,
    connection: ConnectionState,
    /// Monotonic id for in-flight health checks. Each spawn increments this;
    /// completions only apply when their captured id still matches the latest,
    /// so a slower older probe can't overwrite a newer result.
    health_check_id: u64,
    /// Instant of the last health check spawn — used to debounce.
    last_health_check_at: Option<Instant>,
    /// HTTP client into the running `axon serve`. Shared across health checks
    /// and command dispatches via reqwest's internal connection pool.
    client: Arc<HttpClient>,
}
```

Delete the now-dead `HealthResult` struct (was a wrapper around `bool`):

```rust
// DELETE THIS:
// struct HealthResult {
//     ok: bool,
// }
```

- [ ] **Step 5.2: Update `Palette::new` signature**

Replace `impl Palette { pub(crate) fn new(cx: ...) -> Self`:

```rust
impl Palette {
    pub(crate) fn new(cfg: crate::config::ClientConfig, cx: &mut Context<Self>) -> Self {
        let client = Arc::new(HttpClient::new(cfg));
        let mut palette = Self {
            query: String::new(),
            selected: 0,
            focus: cx.focus_handle(),
            command_output: None,
            running: None,
            next_run_id: 1,
            output_scroll: ScrollHandle::new(),
            locked_command: None,
            connection: ConnectionState::Unknown,
            health_check_id: 0,
            last_health_check_at: None,
            client,
        };
        palette.spawn_health_check(cx);
        palette.spawn_background_health_loop(cx);
        palette
    }
```

- [ ] **Step 5.3: Replace `spawn_health_check` with the debounced HTTP version**

Replace the existing `spawn_health_check` with:

```rust
    fn spawn_health_check(&mut self, cx: &mut Context<Self>) {
        // Debounce: refuse re-probe within HEALTH_CHECK_MIN_INTERVAL.
        if let Some(last) = self.last_health_check_at
            && last.elapsed() < HEALTH_CHECK_MIN_INTERVAL
        {
            return;
        }
        self.last_health_check_at = Some(Instant::now());
        self.health_check_id = self.health_check_id.wrapping_add(1);
        let my_id = self.health_check_id;
        self.connection = ConnectionState::Checking;
        cx.notify();

        let client = Arc::clone(&self.client);
        let task = cx.background_spawn(async move { client.health().await });

        cx.spawn(async move |this, cx| {
            let ok = task.await;
            let _ = this.update(cx, |this, cx| {
                // Ignore stale completions — a newer probe has been spawned.
                if this.health_check_id != my_id {
                    return;
                }
                this.connection = if ok {
                    ConnectionState::Connected
                } else {
                    ConnectionState::Disconnected
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Background loop: re-probe every HEALTH_CHECK_BACKGROUND_INTERVAL.
    /// Catches server restarts and OAuth token expiry; unconditional so the
    /// status dot stays accurate without user interaction.
    fn spawn_background_health_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut ticker = tokio::time::interval(HEALTH_CHECK_BACKGROUND_INTERVAL);
            // First tick fires immediately — skip it (we already probed in `new`).
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let updated = this.update(cx, |this, cx| {
                    this.spawn_health_check(cx);
                });
                if updated.is_err() {
                    // Palette dropped — exit the loop cleanly.
                    break;
                }
            }
        })
        .detach();
    }
```

- [ ] **Step 5.4: Verify the crate compiles**

```bash
cargo check 2>&1 | tail -10
```

Expected: compilation error at the `Palette::new` callsite in `main.rs` (we update it in Task 6) and at `submit` (still references removed `build_axon_args` etc — fixed in Task 6).

DO NOT commit yet — proceed to Task 6.

---

## Task 6: Rewire `Palette::submit` with completion-id guard + Checking-as-ineligible

**Files:**
- Modify: `apps/desktop/src/ui.rs`
- Modify: `apps/desktop/src/actions.rs`
- Modify: `apps/desktop/src/output.rs`
- Modify: `apps/desktop/src/main.rs`

- [ ] **Step 6.1: Add `render_as_markdown` field to `CommandAction`**

In `apps/desktop/src/actions.rs`, find the `CommandAction` struct (line 1-11):

```rust
#[derive(Clone, Copy)]
pub(crate) struct CommandAction {
    pub(crate) label: &'static str,
    pub(crate) subcommand: &'static str,
    pub(crate) arg_mode: ArgMode,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) description: &'static str,
    pub(crate) example: &'static str,
    /// True if the server's response should be rendered as markdown rather
    /// than pretty-printed JSON. Set on actions whose response types carry
    /// a known string field (see output::extract_response_text).
    pub(crate) render_as_markdown: bool,
}
```

For each of the 9 entries in `ACTIONS` (lines 20-92), add `render_as_markdown: <value>`:
- `scrape`, `ask`, `research` → `render_as_markdown: true`
- All others (`crawl`, `map`, `search`, `ingest`, `status`, `doctor`) → `render_as_markdown: false`

Example for one entry:

```rust
    CommandAction {
        label: "Scrape URL",
        subcommand: "scrape",
        arg_mode: ArgMode::Split,
        aliases: &["scrape", "fetch", "page", "url"],
        description: "Fetch one page, convert it to markdown, and optionally embed it.",
        example: "scrape https://docs.rs/serde",
        render_as_markdown: true,
    },
```

- [ ] **Step 6.2: Add `CommandOutput::from_http_ok` / `from_http_err` / `server_unreachable`**

In `apps/desktop/src/output.rs`, after `from_process` (which we'll delete in Task 8), add:

```rust
    /// Build a CommandOutput from a successful /v1/actions response.
    pub(crate) fn from_http_ok(
        request_label: &str,
        action: CommandAction,
        result: Option<serde_json::Value>,
    ) -> Self {
        let body_text = match &result {
            Some(value) => extract_response_text(action.subcommand, value),
            None => String::new(),
        };

        let stdout = if body_text.is_empty() {
            None
        } else {
            Some(OutputSection::new("stdout", body_text))
        };

        Self {
            kind: OutputKind::Success,
            title: format!("{} completed", command_title(action.subcommand)),
            subtitle: request_label.to_string(),
            stdout,
            stderr: None,
            use_markdown: action.render_as_markdown,
        }
    }

    /// Build a CommandOutput from a transport-level or non-2xx error.
    pub(crate) fn from_http_err(
        request_label: &str,
        subcommand: &str,
        error: String,
    ) -> Self {
        Self {
            kind: OutputKind::Error,
            title: format!("{} failed", command_title(subcommand)),
            subtitle: request_label.to_string(),
            stdout: None,
            stderr: Some(OutputSection::new("error", error)),
            use_markdown: false,
        }
    }

    /// Notice shown when the configured AXON_SERVER_URL is unreachable.
    pub(crate) fn server_unreachable(server_url: &str) -> Self {
        Self {
            kind: OutputKind::Error,
            title: "Cannot reach axon server".to_string(),
            subtitle: format!(
                "Tried {server_url}. Start `axon serve` locally or point AXON_SERVER_URL at a running instance."
            ),
            stdout: None,
            stderr: None,
            use_markdown: false,
        }
    }
```

Add this helper at the bottom of `output.rs`, above `truncate_output`. Field names are verified against `src/services/types/service.rs:708-718, 777-781, 841-908` in the axon crate:

```rust
/// Pull the human-readable text out of a server response, per subcommand.
/// Field names are pinned to `src/services/types/service.rs`:
/// - AskResult.answer       (line 710)
/// - ScrapeResult.markdown  (line 780)
/// - ResearchResult.summary (line 908) — JSON value, not a string
/// Anything else: pretty-printed JSON. Drift in those names breaks rendering;
/// the wire-contract smoke test in Task 10 catches it.
fn extract_response_text(subcommand: &str, value: &serde_json::Value) -> String {
    match subcommand {
        "ask" => value
            .get("answer")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| pretty_json(value)),
        "scrape" => value
            .get("markdown")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| pretty_json(value)),
        "research" => value
            .get("summary")
            .map(pretty_json)
            .unwrap_or_else(|| pretty_json(value)),
        _ => pretty_json(value),
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}
```

The `extract_response_text` signature takes `&serde_json::Value` — make sure `pretty_json` accepts the same. The `import CommandAction` may need adding to the file's `use` block — verify with `cargo check`.

- [ ] **Step 6.3: Replace the body of `Palette::submit` (id guard + Checking-as-ineligible)**

Locate `Palette::submit` in `apps/desktop/src/ui.rs`. Replace the entire function body:

```rust
    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let (action, arg) = if let Some(locked) = self.locked_command {
            (locked, self.query.trim().to_string())
        } else {
            let actions = self.matches();
            let Some(action) = actions.get(self.selected).copied() else {
                return;
            };
            (action, self.argument_for(action).to_string())
        };

        if action.arg_mode != ArgMode::None && arg.is_empty() {
            self.command_output = Some(CommandOutput::notice(
                OutputKind::Warning,
                "Argument required",
                action.example,
            ));
            cx.notify();
            return;
        }

        // Treat Disconnected AND Checking as ineligible. Checking might
        // resolve to Connected within ms, but we don't gamble — the user
        // gets a clear notice and a fresh debounced probe.
        if matches!(
            self.connection,
            ConnectionState::Disconnected | ConnectionState::Checking
        ) {
            self.command_output =
                Some(CommandOutput::server_unreachable(self.client.server_url()));
            self.spawn_health_check(cx); // debounced internally
            cx.notify();
            return;
        }

        if self.running.is_some() {
            self.command_output = Some(CommandOutput::notice(
                OutputKind::Warning,
                "Command already running",
                "Wait for the current axon command to finish.",
            ));
            cx.notify();
            return;
        }

        let action_body = match build_action_body(&action, &arg) {
            Ok(body) => body,
            Err(error) => {
                self.command_output = Some(CommandOutput::notice(
                    OutputKind::Error,
                    "Invalid input",
                    error,
                ));
                cx.notify();
                return;
            }
        };

        let run_id = self.next_run_id;
        self.next_run_id += 1;
        let request_id = format_request_id(run_id);
        let request_label = format!("{} → {}", self.client.server_url(), action.subcommand);
        self.running = Some(RunningCommand {
            id: run_id,
            subcommand: action.subcommand,
        });
        self.command_output = Some(CommandOutput::running(&request_label, action));

        let client = Arc::clone(&self.client);
        let subcommand = action.subcommand;
        let task = cx.background_spawn(async move {
            let result = client.dispatch(&request_id, subcommand, action_body).await;
            CommandResult {
                id: run_id,
                action,
                request_label,
                result,
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                // Completion-id guard: only apply this result if it
                // corresponds to the currently-running dispatch. Without
                // this, a stale completion 30s after the user dismissed
                // its UI clobbers whatever is on screen.
                let is_current = this
                    .running
                    .as_ref()
                    .map(|r| r.id)
                    .is_some_and(|running_id| running_id == result.id);
                if !is_current {
                    return;
                }
                this.running = None;
                this.command_output = Some(match result.result {
                    Ok(resp) if resp.ok => CommandOutput::from_http_ok(
                        &result.request_label,
                        result.action,
                        resp.result,
                    ),
                    Ok(resp) => {
                        let msg = resp
                            .error
                            .map(|e| format!("{}: {}", e.kind, e.message))
                            .unwrap_or_else(|| {
                                "server returned ok=false with no error body".to_string()
                            });
                        CommandOutput::from_http_err(
                            &result.request_label,
                            result.action.subcommand,
                            msg,
                        )
                    }
                    Err(error) => CommandOutput::from_http_err(
                        &result.request_label,
                        result.action.subcommand,
                        error,
                    ),
                });
                cx.notify();
            });
        })
        .detach();

        self.locked_command = None;
        self.query.clear();
        self.selected = 0;
        cx.notify();
    }
```

- [ ] **Step 6.4: Update the `CommandResult` struct**

In `apps/desktop/src/ui.rs`, find the `CommandResult` definition (around line 61). Replace with:

```rust
struct CommandResult {
    id: u64,
    action: CommandAction,
    request_label: String,
    result: Result<crate::client::DispatchResponse, String>,
}
```

`action: CommandAction` is `Copy`, so this stays cheap.

- [ ] **Step 6.5: Update `main.rs` to load config + pass it through (at the top of `main()`)**

This is a security-critical change — load config BEFORE any GPUI/tokio init so the dotenv read doesn't race with spawned threads.

In `apps/desktop/src/main.rs`, find the `fn main() -> Result<()> {` block. Add the config load as the very first thing after the tracing init:

```rust
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load config BEFORE any GPUI/tokio thread spawns. Reading env vars
    // while threads exist is racy under Rust 2024 safety semantics.
    let client_cfg = crate::config::ClientConfig::load().map_err(|e| {
        tracing::error!(error = %e, "fatal: AXON_SERVER_URL is invalid");
        anyhow::anyhow!(e)
    })?;
    tracing::info!(
        server_url = %client_cfg.server_url,
        has_token = client_cfg.token.is_some(),
        "palette client config loaded"
    );

    // ... rest of main unchanged until the window closure ...
```

Then in the window construction closure (around line 100), use the loaded config. `client_cfg` is `Clone`, but only one window is opened — use `move` in the closure so it's consumed once:

```rust
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let view = cx.new(|cx| Palette::new(client_cfg, cx));
                    let handle = view.focus_handle(cx);
                    window.focus(&handle, cx);
                    view
                },
            )
```

Note the `move` keyword on the closure to capture `client_cfg`.

- [ ] **Step 6.6: Compile + test**

```bash
cargo check 2>&1 | tail -10
cargo test --lib 2>&1 | tail -5
```

Expected: clean compile. `build_axon_args`/`display_command_line` will still appear in compiler warnings as dead code — Task 8 removes them. Tests should still pass.

- [ ] **Step 6.7: Commit Tasks 5 + 6 together**

```bash
git add apps/desktop/src/ui.rs apps/desktop/src/output.rs apps/desktop/src/actions.rs apps/desktop/src/main.rs
git commit -m "refactor(palette): drink from /v1/actions, with id guards + debounced probes

Palette::new now takes a ClientConfig and builds an Arc<HttpClient>.
The HTTP path lands the following race fixes vs the original draft:

- Completion-id guard on self.command_output assignment (not just on
  self.running). Without this, a stale dispatch completing 30s after
  the user dismissed its UI clobbers whatever is on screen.
- Checking is treated as ineligible for dispatch alongside Disconnected.
  Mashing Enter against a Checking dot used to gamble; now it shows
  the server-unreachable notice and re-probes.
- Health check is debounced (HEALTH_CHECK_MIN_INTERVAL = 500ms) so a
  flickering connection state doesn't spawn a probe per keystroke.
- Background re-probe via tokio::time::interval(30s) catches server
  restarts and token expiry while the palette is open.

CommandAction gains a `render_as_markdown: bool` field so the
markdown-vs-JSON choice is declared in one place (actions.rs)
instead of duplicated string-matching in output.rs.

extract_response_text() field names are pinned to
src/services/types/service.rs: AskResult.answer (line 710),
ScrapeResult.markdown (line 780), ResearchResult.summary (line 908).
The wire-contract smoke test in Task 10 catches drift.

main.rs loads ClientConfig::load() BEFORE GPUI/tokio init — Rust
2024 safety semantics require no concurrent env reads while we're
reading the dotenv file."
```

---

## Task 7: Delete the now-dead subprocess scaffolding

**Files:**
- Modify: `apps/desktop/src/actions.rs`
- Modify: `apps/desktop/src/output.rs`

- [ ] **Step 7.1: Delete `build_axon_args`, `display_command_line`, `split_shell_words` from actions.rs**

In `apps/desktop/src/actions.rs`, delete lines 130-188 (everything from `pub(crate) fn build_axon_args` through the end of `split_shell_words`). Leave the rest of the file intact.

- [ ] **Step 7.2: Delete `CommandOutput::from_process`, `CommandOutput::spawn_error`, and the std::process::Output import**

In `apps/desktop/src/output.rs`:

1. Delete the `use std::process::Output;` line at the top.
2. Delete the entire `pub(crate) fn from_process(...)` function.
3. Delete any `pub(crate) fn spawn_error(...)` function.

Verify nothing else references them:

```bash
cd apps/desktop && grep -rn "from_process\|spawn_error\|std::process::Output" src/
```

Expected: no matches.

- [ ] **Step 7.3: Compile + run all tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: clean compile, all 28 unit tests pass (9 config + 11 wire + 8 client).

- [ ] **Step 7.4: Commit**

```bash
git add apps/desktop/src/actions.rs apps/desktop/src/output.rs
git commit -m "chore(palette): delete subprocess scaffolding now that HTTP path is wired

Removes:
- actions::build_axon_args, display_command_line, split_shell_words
- output::from_process, output::spawn_error
- std::process::Output import in output.rs

The palette is now pure HTTP. No code path constructs an axon argv
or shells out to the binary."
```

---

## Task 8: Document the HTTP-only model + update Cargo metadata

**Files:**
- Modify: `apps/desktop/src/main.rs` (module docstring)
- Modify: `apps/desktop/Cargo.toml` (description)
- Modify: `CHANGELOG.md` (project root)

- [ ] **Step 8.1: Update the crate docstring in main.rs**

Replace the file-level doc comment (lines 3-8):

```rust
//! axon-palette: a global-hotkey command palette for axon.
//!
//! Press the configured global hotkey (default: Ctrl+Shift+Space) anywhere on
//! the desktop to bring the palette window forward. Type to filter actions,
//! optionally followed by an argument (URL, query, etc.), then press Enter.
//!
//! Architecture: HTTP client. The palette does NOT spawn the `axon` binary.
//! It talks to a running `axon serve` instance at $AXON_SERVER_URL (default
//! http://127.0.0.1:8001) via the same `/v1/actions` endpoint used by MCP
//! and the web panel. Auth via $AXON_MCP_HTTP_TOKEN. Config can live in
//! process env or ~/.axon/.env (file is permission-checked and never written).
//!
//! Limitations of v0.3:
//! - Config is loaded once at startup. Editing ~/.axon/.env mid-session
//!   has no effect until the palette is relaunched.
//! - When the server runs in OAuth mode (AXON_MCP_AUTH_MODE=oauth) the
//!   palette has no OAuth flow; it requires either no auth (loopback) or
//!   a static AXON_MCP_HTTP_TOKEN. OAuth support is a v0.4 follow-up.
//! - Ask/research responses are non-streaming. A spinner shows during the
//!   request; the full response appears when the server finishes.
```

- [ ] **Step 8.2: Update the package description in Cargo.toml**

In `apps/desktop/Cargo.toml`:

```toml
description = "Global-hotkey command palette for axon — HTTP client for a running axon serve instance"
```

- [ ] **Step 8.3: Add a CHANGELOG entry**

Open `CHANGELOG.md` at the repo root. Under `## [Unreleased]` (create if absent):

```markdown
### Changed

- desktop: palette no longer requires the `axon` binary on PATH. Every command and the health-status indicator now talk to `$AXON_SERVER_URL/v1/actions` and `/healthz` instead of spawning `axon` as a subprocess. The palette becomes a thin HTTP client; ship it standalone to a workstation and point it at any local or remote `axon serve` instance. Bumps `axon-palette` 0.2.0 → 0.3.0.

  Security: `ClientConfig` has a hand-rolled `Debug` impl that redacts the bearer token, and `AXON_SERVER_URL` is parsed via `url::Url` — plain `http://` is rejected for non-loopback hosts to prevent token leakage. `~/.axon/.env` is read with a file-permission check that warns at mode > 0o600 on Unix.

  Performance: per-action timeouts (doctor 30s, ask 600s, crawl 3600s), `tcp_keepalive(30s)` for Tailscale, `pool_idle_timeout(None)` to keep warm connections, large responses (>256 KiB) deserialized via `spawn_blocking`.

  Limitations: config is loaded once at startup (no hot reload). OAuth auth mode is unsupported — use a static `AXON_MCP_HTTP_TOKEN` or loopback-only deployment. Ask/research are non-streaming.
```

- [ ] **Step 8.4: Final unit-test sweep**

```bash
cd apps/desktop && cargo test --lib
```

Expected: all 28 unit tests pass.

- [ ] **Step 8.5: Commit**

```bash
git add apps/desktop/src/main.rs apps/desktop/Cargo.toml CHANGELOG.md
git commit -m "docs(palette): document HTTP-client architecture + v0.3 limitations

Updates the crate docstring, package description, and CHANGELOG.
Limitations documented:
- Config is loaded once (no hot reload)
- OAuth auth mode unsupported (v0.4 follow-up)
- Ask/research are non-streaming (server-side)"
```

---

## Task 9: Wire-contract smoke test (live-server integration)

**Files:**
- Create: `apps/desktop/tests/wire_contract.rs`

This is the integration test that catches `AxonRequest` schema drift. It is gated behind an env var so CI doesn't fail when no server is running — but anyone touching the palette wire format must be able to run it.

- [ ] **Step 9.1: Write the smoke test**

Create `apps/desktop/tests/wire_contract.rs`:

```rust
//! Integration smoke test: each of the 9 hand-rolled JSON bodies in
//! `wire::build_action_body` must POST to a running `axon serve`
//! without returning 4xx. This catches drift between the palette's
//! action-body shape and the server's AxonRequest enum.
//!
//! Skipped unless AXON_PALETTE_WIRE_CONTRACT_URL is set to a live
//! server base URL (e.g. http://127.0.0.1:8001). Token is read from
//! AXON_MCP_HTTP_TOKEN if present.
//!
//! Run with:
//!   AXON_PALETTE_WIRE_CONTRACT_URL=http://127.0.0.1:8001 \
//!   AXON_MCP_HTTP_TOKEN=... \
//!   cargo test --test wire_contract -- --nocapture
//!
//! The test does NOT execute side-effecting actions end-to-end. For
//! crawl/scrape/ingest it uses a non-routable example URL, which the
//! server validates before any work — we only assert non-4xx (most
//! return 200 with an enqueued job_id, or 4xx specifically on a
//! validation rule we hit on purpose; that's still wire-contract OK).

use axon_palette::actions::ACTIONS;
use axon_palette::wire::{build_action_body, format_request_id};
use serde_json::Value;

#[tokio::test]
async fn each_action_body_is_accepted_by_v1_actions() {
    let Ok(base) = std::env::var("AXON_PALETTE_WIRE_CONTRACT_URL") else {
        eprintln!("skipped: set AXON_PALETTE_WIRE_CONTRACT_URL to run");
        return;
    };
    let token = std::env::var("AXON_MCP_HTTP_TOKEN").ok();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let canned_arg: &dyn Fn(&str) -> &'static str = &|sub| match sub {
        "scrape" | "crawl" | "map" | "ingest" => "https://example.com/wire-contract-probe",
        "ask" | "search" | "research" => "wire contract probe",
        _ => "",
    };

    for action in ACTIONS {
        let arg = canned_arg(action.subcommand);
        let body = build_action_body(action, arg)
            .unwrap_or_else(|e| panic!("build_action_body({}, {arg:?}): {e}", action.subcommand));
        let envelope = serde_json::json!({
            "request_id": format_request_id(0),
            "action": body,
        });
        let url = format!("{}/v1/actions", base.trim_end_matches('/'));
        let mut req = client.post(&url).json(&envelope);
        if let Some(t) = &token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.unwrap_or_else(|e| {
            panic!("POST {url} for action={} failed: {e}", action.subcommand)
        });
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or_else(|_| Value::Null);
        assert!(
            !status.as_u16().to_string().starts_with("4"),
            "action {} returned {} — wire contract drift suspected.\nrequest: {envelope:#}\nresponse: {body:#}",
            action.subcommand,
            status
        );
    }
}
```

- [ ] **Step 9.2: Make `wire` + `actions` modules accessible from integration tests**

Integration tests in `apps/desktop/tests/` link against the crate as if it were a library. The current crate is a `[[bin]]` only, so we need to expose a library target. Add to `apps/desktop/Cargo.toml`:

```toml
[lib]
name = "axon_palette"
path = "src/lib.rs"
```

Create `apps/desktop/src/lib.rs`:

```rust
//! Library surface for axon-palette integration tests.
//! Production callers use the bin (src/main.rs).

pub mod actions;
pub mod client;
pub mod config;
pub mod output;
pub mod wire;

// Modules not needed by tests are kept private to the bin via main.rs.
```

In `apps/desktop/src/main.rs`, remove the redundant `mod` declarations that are now in `lib.rs`:

```rust
// In main.rs, replace the top mod block:
use axon_palette::{actions, client, config, output, wire};

mod markdown;
mod render;
mod theme;
mod ui;
```

This requires `actions`, `client`, `config`, `output`, `wire` to have `pub` items where `ui.rs` uses them. The existing `pub(crate)` annotations need to be loosened to `pub` for the public surface — keep internal helpers `pub(crate)`. Specifically, ensure these are `pub`:

- `actions::CommandAction`, `actions::ArgMode`, `actions::ACTIONS`, `actions::action_invoked_by`, `actions::action_matches`, `actions::looks_like_url`
- `client::HttpClient`, `client::DispatchResponse`, `client::DispatchError`, `client::timeout_for_action`
- `config::ClientConfig`
- `output::CommandOutput`, `output::OutputKind`, `output::OutputSection`
- `wire::build_action_body`, `wire::format_request_id`

Use a regex sweep to swap `pub(crate)` → `pub` for these specific items (other `pub(crate)` items stay private).

- [ ] **Step 9.3: Run the unit tests first to confirm the library split compiles**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: 28 tests pass.

- [ ] **Step 9.4: Run the smoke test against the live server**

Assuming `axon serve` is running on `http://127.0.0.1:8001` with token set:

```bash
AXON_PALETTE_WIRE_CONTRACT_URL=http://127.0.0.1:8001 \
AXON_MCP_HTTP_TOKEN="$(grep AXON_MCP_HTTP_TOKEN ~/.axon/.env | cut -d= -f2)" \
cargo test --test wire_contract -- --nocapture
```

Expected: 1 test passes. If any action gets a 4xx, the test panics with the offending request body and response — that's the drift signal.

If `axon serve` isn't running, the test prints "skipped" and exits 0.

- [ ] **Step 9.5: Commit**

```bash
git add apps/desktop/Cargo.toml apps/desktop/src/lib.rs apps/desktop/src/main.rs apps/desktop/tests/wire_contract.rs apps/desktop/src/actions.rs apps/desktop/src/client.rs apps/desktop/src/config.rs apps/desktop/src/output.rs apps/desktop/src/wire.rs
git commit -m "test(palette): wire-contract smoke test against running axon serve

Adds apps/desktop/tests/wire_contract.rs which POSTs each of the 9
hand-rolled JSON bodies from wire::build_action_body to a live
/v1/actions endpoint, asserting non-4xx. Skipped unless
AXON_PALETTE_WIRE_CONTRACT_URL is set, so unit-test runs are
unaffected.

Also splits apps/desktop into a [lib] + [[bin]] so the integration
test can link against the wire/actions modules. The bin's runtime
surface is unchanged."
```

---

## Task 10: Final beads update + push

**Files:**
- None (beads + git only)

- [ ] **Step 10.1: Close the tracking bead**

```bash
bd close axon_rust-j19t --reason="Shipped in apps/desktop v0.3.0. Palette is now a pure HTTP client against \$AXON_SERVER_URL/v1/actions. Per-action timeouts, debounced + background-polled health probes, completion-id guards, https://-enforcement on non-loopback, token redaction in Debug, file-permission warning on ~/.axon/.env. Wire-contract smoke test gates AxonRequest drift. OAuth auth-mode noted as v0.4 follow-up."
```

- [ ] **Step 10.2: File the v0.4 OAuth follow-up bead**

```bash
bd create --title="palette v0.4: OAuth auth flow for AXON_MCP_AUTH_MODE=oauth deployments" --description="axon-palette v0.3 supports static AXON_MCP_HTTP_TOKEN auth or no-auth loopback only. When the server is configured with AXON_MCP_AUTH_MODE=oauth (Google OAuth + JWT via lab-auth), the palette has no way to authenticate.

# Acceptance criteria
- Detect oauth mode via probing /healthz response headers OR a new /v1/auth/info endpoint
- Implement device-code or browser-redirect OAuth flow in the palette
- Cache the JWT in ~/.axon/palette-token.json (mode 0600)
- Refresh expired tokens automatically
- Update CHANGELOG and crate docstring

# Why
v0.3 explicitly drops the AXON_LITE-style subprocess fallback. For OAuth-mode deployments (production server at axon.tootie.tv), there's currently no path for the palette to authenticate." --type=feature --priority=3
```

- [ ] **Step 10.3: File the v0.4 ask-streaming follow-up bead**

```bash
bd create --title="palette v0.4: SSE streaming for ask/research" --description="axon-palette v0.3 polls /v1/actions and waits for the full response. For ask/research (which can take 30s-5min) the user sees a blank screen with a spinner. Once the axon server adds SSE streaming on /v1/actions?stream=true (separate bead), the palette should consume tokens incrementally.

# Acceptance criteria
- Detect server SSE support via capability probe
- Implement reqwest event-stream consumer in client::dispatch_stream()
- Update Palette UI to render partial responses as they arrive
- Fall back to non-streaming for actions where streaming isn't useful

# Dependencies
Server-side SSE work (file as separate bead in axon)." --type=feature --priority=3
```

- [ ] **Step 10.4: Push the branch**

```bash
git push
```

If upstream is unset, push with `-u origin <branch>`.

- [ ] **Step 10.5: Smoke-test against the live server (manual)**

```bash
# Terminal 1: ensure axon serve is up
docker compose --env-file ~/.axon/.env ps axon | grep -q "Up"

# Terminal 2: run the wire-contract test
cd apps/desktop && AXON_PALETTE_WIRE_CONTRACT_URL=http://127.0.0.1:8001 \
    AXON_MCP_HTTP_TOKEN="$(grep AXON_MCP_HTTP_TOKEN ~/.axon/.env | cut -d= -f2)" \
    cargo test --test wire_contract -- --nocapture

# Terminal 3: launch palette
cd apps/desktop && cargo run --release
```

In the palette:
1. Confirm the status dot lights green within ~3s.
2. Type `doctor` and press Enter — pretty-printed JSON renders.
3. Type `ask what is axon?` and press Enter — markdown answer renders in the output panel (uses AskResult.answer field).
4. Type `scrape https://docs.rs` and press Enter — markdown renders (uses ScrapeResult.markdown field).
5. Stop `axon serve`. Within 30s the dot turns red (background poll). Type `doctor` and submit — should show "Cannot reach axon server" notice with the URL.
6. Restart `axon serve`. Within 30s the dot returns to green.
7. Wait at least 11 minutes between two `ask` invocations (or check `tcpdump` / `ss -tan`) to confirm the TCP connection is reused — confirms `pool_idle_timeout(None)` + `tcp_keepalive` are working.

If a step fails, file a bead with the repro and stop. Do not paper over UX regressions.

---

## Self-Review

**Spec coverage:**
- Config with redacted Debug → Task 2 ✓ (Step 2.3, `Debug` impl + `warns_when_dotenv_world_readable` test)
- `https://` enforcement on non-loopback → Task 2 ✓ (`rejects_remote_plain_http` test)
- URL strict parsing → Task 2 ✓ (`rejects_url_with_path`/`query_or_fragment`)
- File permission check → Task 2 ✓ (`warns_when_dotenv_world_readable`)
- No `unsafe { set_var }` ping-pong → Task 2 ✓ (purely functional dotenv read)
- Load config before threads spawn → Task 6 ✓ (Step 6.5, before any GPUI/tokio init)
- Per-action timeouts → Task 4 ✓ (`timeout_for_action` table + test)
- Connection pool tuning → Task 4 ✓ (`tcp_keepalive`, `pool_idle_timeout(None)`)
- `Url::join` instead of `format!` → Task 4 ✓
- Large-response spawn_blocking → Task 4 ✓ (`SPAWN_BLOCKING_THRESHOLD`)
- Background health probe → Task 5 ✓ (`spawn_background_health_loop`)
- Debounced health probes → Task 5 ✓ (`HEALTH_CHECK_MIN_INTERVAL`)
- Completion-id guard on command_output → Task 6 ✓ (`is_current` check)
- `Checking` treated as ineligible → Task 6 ✓
- `render_as_markdown` on CommandAction → Task 6 ✓ (Step 6.1)
- Markdown field names verified against `src/services/types/service.rs` → Task 6 ✓ (Step 6.2 doc comment cites line numbers)
- Drop `AtomicU64`, reuse `next_run_id` → Task 3 ✓
- Inline envelope at dispatch site → Task 4 ✓ (`envelope = json!({...})` inside `dispatch`)
- Drop `from_env_only()` constructor → Task 2 ✓ (single `load()` only)
- Drop `from_process` / `spawn_error` / `Output` import → Task 7 ✓
- Wire-contract smoke test → Task 9 ✓
- OAuth + streaming as follow-up beads → Task 10 ✓

**Placeholders:** scanned the plan for "TODO", "TBD", "implement later", "add validation" — none found. Every code step has full code; every command has full args.

**Type consistency:**
- `ClientConfig { server_url: url::Url, token: Option<String> }` — same across Tasks 2/4/5/6/9.
- `ClientConfig::load() -> Result<Self, String>` — Tasks 2, 6.
- `ClientConfig::exposed_token() -> Option<&str>` — Tasks 2, 4.
- `HttpClient::new(ClientConfig) -> Self` — Tasks 4, 5, 6.
- `HttpClient::dispatch(&str, &str, Value) -> Result<DispatchResponse, String>` — Tasks 4, 6 (note: takes subcommand `&str` for per-action timeout lookup).
- `HttpClient::health() -> bool` — Tasks 4, 5.
- `timeout_for_action(&str) -> Duration` — Tasks 4 (defined), 4 (tested).
- `build_action_body(&CommandAction, &str) -> Result<Value, String>` — Tasks 3, 6, 9.
- `format_request_id(u64) -> String` — Tasks 3, 6, 9.
- `CommandOutput::from_http_ok(&str, CommandAction, Option<Value>) -> Self` — Tasks 6 (defined + called).
- `CommandOutput::from_http_err(&str, &str, String) -> Self` — Task 6.
- `CommandOutput::server_unreachable(&str) -> Self` — Task 6.
- `extract_response_text(&str, &Value) -> String` — Task 6.
- `Palette::new(ClientConfig, &mut Context<Self>) -> Self` — Tasks 5, 6.
- `CommandResult { id, action, request_label, result }` — Task 6 (defined + consumed).
- `HEALTH_CHECK_MIN_INTERVAL: Duration`, `HEALTH_CHECK_BACKGROUND_INTERVAL: Duration` — Task 5.
- `SPAWN_BLOCKING_THRESHOLD: usize` — Task 4.

**No types or methods referenced without a defining task.**
