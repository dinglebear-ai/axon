# axon.json Config File — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **📁 Organization Note:** When this plan is fully implemented and verified, move this file to `docs/superpowers/plans/complete/` to keep the plans folder organized.

**Goal:** Load `axon.json` as a JSON config layer between env vars and hardcoded defaults, so non-secret settings can live in a committed file instead of `.env`.

**Architecture:** New `AxonConfig` serde struct tree loaded once at the top of `into_config()`. New `_or` helper variants in `helpers.rs` accept `Option<T>` JSON defaults. All ~50 env-var call sites in `build_config.rs` gain a JSON fallback layer. TypeScript side gets a server-only singleton loading the same file.

**Tech Stack:** Rust / `serde_json` (already in `Cargo.toml`), TypeScript / `dotenv` (Next.js transitive dep), JSON Schema (editor tooling only — no runtime use)

**Spec:** `docs/superpowers/specs/2026-03-22-axon-json-config-design.md`

---

## File Map

| File | Action | Notes |
|------|--------|-------|
| `crates/core/config/axon_config.rs` | **Create** | ~270-line serde struct tree |
| `crates/core/config/parse/axon_config_loader.rs` | **Create** | ~25-line load function |
| `crates/core/config/parse/helpers.rs` | **Modify** | Add 5 `_or` helper variants |
| `crates/core/config/parse/build_config.rs` | **Modify** | Call loader + migrate ~50 call sites |
| `crates/core/config/parse.rs` | **Modify** | Declare `axon_config_loader` module |
| `crates/core/config.rs` | **Modify** | Re-export `AxonConfig` |
| `apps/web/lib/axon-config-types.ts` | **Create** | TypeScript types mirroring JSON shape |
| `apps/web/lib/axon-config.ts` | **Create** | Server-only config singleton |
| `apps/web/next.config.ts` | **Modify** | Add dotenv root `.env` load |
| `.env.example` | **Modify** | Strip ~85 vars moved to `axon.json` |
| `apps/web/.env.local` | **Delete** | Vars already in root `.env` |

---

## Task 1: `AxonConfig` Serde Struct Tree

**Files:**
- Create: `crates/core/config/axon_config.rs`

- [ ] **Step 1: Write the failing test**

Add a `#[cfg(test)]` block at the bottom of the new file (write the test before the structs):

```rust
// crates/core/config/axon_config.rs  — write test first, structs second

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_deserializes_without_error() {
        let json = r#"{ "search": { "hybrid_candidates": 77 } }"#;
        let cfg: AxonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.search.hybrid_candidates, Some(77));
        assert!(cfg.ask.is_none() || cfg.ask.unwrap().chunk_limit.is_none());
    }

    #[test]
    fn empty_object_deserializes_to_all_none() {
        let cfg: AxonConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.services.qdrant_url.is_none());
        assert!(cfg.llm.model.is_none());
    }

    #[test]
    fn unknown_keys_are_silently_ignored() {
        let json = r#"{ "future_section": { "new_key": 42 }, "search": { "hybrid_enabled": false } }"#;
        let cfg: AxonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.search.hybrid_enabled, Some(false));
    }
}
```

- [ ] **Step 2: Run test — expect compile failure** (struct not defined yet)

```bash
cargo test -p axon-core axon_config 2>&1 | head -20
```

Expected: `error[E0433]: failed to resolve: use of undeclared type 'AxonConfig'`

- [ ] **Step 3: Write the struct tree**

Create `crates/core/config/axon_config.rs`:

```rust
use serde::Deserialize;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct AxonConfig {
    pub services: ServicesConfig,
    pub llm:      LlmConfig,
    pub tei:      TeiConfig,
    pub search:   SearchConfig,
    pub ask:      AskConfig,
    pub embed:    EmbedConfig,
    pub queues:   QueuesConfig,
    pub workers:  WorkersConfig,
    pub graph:    GraphConfig,
    pub acp:      AcpConfig,
    pub web:      WebConfig,
    pub mcp:      McpConfig,
    pub serve:    ServeConfig,
    pub chrome:   ChromeConfig,
    pub logging:  LoggingConfig,
    pub output:   OutputConfig,
    pub ingest:   IngestConfig,
    pub oauth:    OAuthConfig,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct ServicesConfig {
    pub qdrant_url:        Option<String>,
    pub tei_url:           Option<String>,
    pub chrome_remote_url: Option<String>,
    pub chrome_url:        Option<String>,
    pub neo4j_url:         Option<String>,
    pub neo4j_user:        Option<String>,
    pub backend_url:       Option<String>,
    pub workers_ws_url:    Option<String>,
    pub backend_hostname:  Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct LlmConfig {
    pub base_url: Option<String>,
    pub model:    Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct TeiConfig {
    pub max_retries:             Option<usize>,
    pub request_timeout_ms:      Option<u64>,
    pub max_client_batch_size:   Option<usize>,
    pub http_port:               Option<u16>,
    pub embedding_model:         Option<String>,
    pub max_concurrent_requests: Option<usize>,
    pub max_batch_tokens:        Option<usize>,
    pub max_batch_requests:      Option<usize>,
    pub pooling:                 Option<String>,
    pub tokenization_workers:    Option<usize>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct SearchConfig {
    pub hybrid_enabled:        Option<bool>,
    pub hybrid_candidates:     Option<usize>,
    pub ask_hybrid_candidates: Option<usize>,
    pub hnsw_ef:               Option<usize>,
    pub hnsw_ef_legacy:        Option<usize>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct AskConfig {
    pub max_context_chars:        Option<usize>,
    pub candidate_limit:          Option<usize>,
    pub chunk_limit:              Option<usize>,
    pub full_docs:                Option<usize>,
    pub backfill_chunks:          Option<usize>,
    pub doc_fetch_concurrency:    Option<usize>,
    pub doc_chunk_limit:          Option<usize>,
    pub min_relevance_score:      Option<f64>,
    pub authoritative_domains:    Option<Vec<String>>,
    pub authoritative_boost:      Option<f64>,
    pub authoritative_allowlist:  Option<Vec<String>>,
    pub min_citations_nontrivial: Option<usize>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct EmbedConfig {
    pub collection:       Option<String>,
    pub doc_concurrency:  Option<usize>,
    pub doc_timeout_secs: Option<usize>,
    pub strict_predelete: Option<bool>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct QueuesConfig {
    pub crawl:   Option<String>,
    pub extract: Option<String>,
    pub embed:   Option<String>,
    pub ingest:  Option<String>,
    pub refresh: Option<String>,
    pub graph:   Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct WorkersConfig {
    pub ingest_lanes:              Option<usize>,
    pub max_pending_crawl_jobs:    Option<usize>,
    pub crawl_size_warn_threshold: Option<usize>,
    pub job_stale_timeout_secs:    Option<u64>,
    pub job_stale_confirm_secs:    Option<u64>,
    pub pg_pool_size:              Option<u32>,
    pub max_ws_connections:        Option<usize>,
    pub max_shell_connections:     Option<usize>,
    pub max_sync_concurrent:       Option<usize>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct GraphConfig {
    pub concurrency:          Option<usize>,
    pub llm_model:            Option<String>,
    pub similarity_threshold: Option<f64>,
    pub similarity_limit:     Option<usize>,
    pub context_max_chars:    Option<usize>,
    pub taxonomy_path:        Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct AcpConfig {
    pub adapter_cmd:             Option<String>,
    pub adapter_args:            Option<String>,
    pub prewarm:                 Option<bool>,
    pub auto_approve:            Option<bool>,
    pub max_concurrent_sessions: Option<usize>,
    pub turn_timeout_ms:         Option<u64>,
    pub allowed_claude_betas:    Option<String>,
    pub agents:                  AgentsConfig,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct AgentsConfig {
    pub claude: AgentEntry,
    pub codex:  AgentEntry,
    pub gemini: AgentEntry,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct AgentEntry {
    pub cmd:  Option<String>,
    pub args: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct WebConfig {
    pub allowed_origins:           Option<Vec<String>>,
    pub allow_insecure_dev:        Option<bool>,
    pub allow_query_token:         Option<bool>,
    pub trust_proxy:               Option<bool>,
    pub pulse_chat_timeout_ms:     Option<u64>,
    pub shell_allowed_origins:     Option<Vec<String>>,
    pub shell_server_host:         Option<String>,
    pub shell_server_port:         Option<u16>,
    pub docker_socket_path:        Option<String>,
    pub enable_docker_socket_logs: Option<bool>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct McpConfig {
    pub transport:              Option<String>,
    pub http_host:              Option<String>,
    pub http_port:              Option<u16>,
    pub artifact_dir:           Option<String>,
    pub inline_bytes_threshold: Option<usize>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct ServeConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct ChromeConfig {
    pub diagnostics:            Option<bool>,
    pub diagnostics_dir:        Option<String>,
    pub diagnostics_events:     Option<bool>,
    pub diagnostics_screenshot: Option<bool>,
    pub proxy:                  Option<String>,
    pub user_agent:             Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct LoggingConfig {
    pub file:      Option<String>,
    pub max_bytes: Option<u64>,
    pub max_files: Option<usize>,
    pub no_color:  Option<bool>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct OutputConfig {
    pub dir:                           Option<String>,
    pub extract_est_cost_per_1k_tokens: Option<f64>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct IngestConfig {
    pub github_max_issues:   Option<usize>,
    pub github_max_prs:      Option<usize>,
    pub download_max_bytes:  Option<u64>,
    pub download_max_files:  Option<usize>,
    pub domains_detailed:    Option<bool>,
    pub domains_facet_limit: Option<usize>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct OAuthConfig {
    pub auth_url:        Option<String>,
    pub token_url:       Option<String>,
    pub redirect_uri:    Option<String>,
    pub redirect_host:   Option<String>,
    pub redirect_path:   Option<String>,
    pub redirect_policy: Option<String>,
    pub scopes:          Option<String>,
    pub required_scopes: Option<String>,
    pub redis_prefix:    Option<String>,
    pub broker_issuer:   Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_deserializes_without_error() {
        let json = r#"{ "search": { "hybrid_candidates": 77 } }"#;
        let cfg: AxonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.search.hybrid_candidates, Some(77));
        assert!(cfg.ask.chunk_limit.is_none());
    }

    #[test]
    fn empty_object_deserializes_to_all_none() {
        let cfg: AxonConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.services.qdrant_url.is_none());
        assert!(cfg.llm.model.is_none());
    }

    #[test]
    fn unknown_keys_are_silently_ignored() {
        let json = r#"{ "future_section": { "new_key": 42 }, "search": { "hybrid_enabled": false } }"#;
        let cfg: AxonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.search.hybrid_enabled, Some(false));
    }

    #[test]
    fn nested_acp_agents_deserialize() {
        let json = r#"{ "acp": { "adapter_cmd": "claude", "agents": { "claude": { "cmd": "/usr/bin/claude" } } } }"#;
        let cfg: AxonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.acp.adapter_cmd.as_deref(), Some("claude"));
        assert_eq!(cfg.acp.agents.claude.cmd.as_deref(), Some("/usr/bin/claude"));
        assert!(cfg.acp.agents.codex.cmd.is_none());
    }
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p axon-core axon_config -- --nocapture
```

Expected: `test axon_config::tests::partial_json_deserializes_without_error ... ok` (×4)

- [ ] **Step 5: Lint check**

```bash
cargo clippy -p axon-core 2>&1 | grep -E "^error"
```

Expected: no errors

- [ ] **Step 6: Commit**

```bash
git add crates/core/config/axon_config.rs
git commit -m "feat(config): add AxonConfig serde struct tree for axon.json"
```

---

## Task 2: `load_axon_config()` Loader + Module Wiring

**Files:**
- Create: `crates/core/config/parse/axon_config_loader.rs`
- Modify: `crates/core/config/parse.rs` (declare module)
- Modify: `crates/core/config.rs` (re-export)

- [ ] **Step 1: Write the failing test**

The test file is at the bottom of the new loader file. Write it first:

```rust
// crates/core/config/parse/axon_config_loader.rs — test block only first pass

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_returns_default() {
        // AXON_CONFIG pointing to nonexistent file → silent default
        unsafe { std::env::set_var("AXON_CONFIG", "/tmp/axon_test_nonexistent_99999.json") };
        let cfg = load_axon_config();
        unsafe { std::env::remove_var("AXON_CONFIG") };
        assert!(cfg.search.hybrid_candidates.is_none());
    }

    #[test]
    fn partial_file_loads_present_keys_only() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{ "search": {{ "hybrid_candidates": 77 }} }}"#).unwrap();
        unsafe { std::env::set_var("AXON_CONFIG", f.path()) };
        let cfg = load_axon_config();
        unsafe { std::env::remove_var("AXON_CONFIG") };
        assert_eq!(cfg.search.hybrid_candidates, Some(77));
        assert!(cfg.ask.chunk_limit.is_none());
    }

    #[test]
    fn axon_config_env_override_loads_correct_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{ "llm": {{ "model": "test-model" }} }}"#).unwrap();
        unsafe { std::env::set_var("AXON_CONFIG", f.path()) };
        let cfg = load_axon_config();
        unsafe { std::env::remove_var("AXON_CONFIG") };
        assert_eq!(cfg.llm.model.as_deref(), Some("test-model"));
    }
}
```

- [ ] **Step 2: Run test — expect compile failure**

```bash
cargo test -p axon-core axon_config_loader 2>&1 | head -20
```

Expected: `error[E0433]: failed to resolve: use of undeclared module`

- [ ] **Step 3: Add `tempfile` dev-dependency**

In `crates/core/Cargo.toml`, add:

```toml
[dev-dependencies]
tempfile = "3"
```

Check first that it isn't already there:
```bash
grep -n "tempfile" crates/core/Cargo.toml
```

- [ ] **Step 4: Write the loader**

Create `crates/core/config/parse/axon_config_loader.rs`:

```rust
use crate::crates::core::config::axon_config::AxonConfig;

/// Load `axon.json` from the path in `AXON_CONFIG` env var, or `axon.json` in CWD.
///
/// - Missing file: silently returns `AxonConfig::default()`
/// - Parse error: prints error to stderr and calls `process::exit(1)`
/// - `AXON_CONFIG` value: resolved as-is (absolute path recommended)
pub(crate) fn load_axon_config() -> AxonConfig {
    let path = std::env::var("AXON_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("axon.json"));

    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return AxonConfig::default(), // missing file is fine
    };

    match serde_json::from_str::<AxonConfig>(&contents) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("ERROR: axon.json parse error ({}): {e}", path.display());
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_returns_default() {
        unsafe { std::env::set_var("AXON_CONFIG", "/tmp/axon_test_nonexistent_99999.json") };
        let cfg = load_axon_config();
        unsafe { std::env::remove_var("AXON_CONFIG") };
        assert!(cfg.search.hybrid_candidates.is_none());
    }

    #[test]
    fn partial_file_loads_present_keys_only() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{ "search": {{ "hybrid_candidates": 77 }} }}"#).unwrap();
        unsafe { std::env::set_var("AXON_CONFIG", f.path()) };
        let cfg = load_axon_config();
        unsafe { std::env::remove_var("AXON_CONFIG") };
        assert_eq!(cfg.search.hybrid_candidates, Some(77));
        assert!(cfg.ask.chunk_limit.is_none());
    }

    #[test]
    fn axon_config_env_override_loads_correct_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{ "llm": {{ "model": "test-model" }} }}"#).unwrap();
        unsafe { std::env::set_var("AXON_CONFIG", f.path()) };
        let cfg = load_axon_config();
        unsafe { std::env::remove_var("AXON_CONFIG") };
        assert_eq!(cfg.llm.model.as_deref(), Some("test-model"));
    }
}
```

- [ ] **Step 5: Declare module in `parse.rs`**

In `crates/core/config/parse.rs`, add the new module declaration alongside the existing ones:

```rust
// BEFORE (line 1):
mod build_config;
pub(crate) mod docker;
pub(crate) mod excludes;
pub(crate) mod helpers;
mod performance;

// AFTER:
mod axon_config_loader;
mod build_config;
pub(crate) mod docker;
pub(crate) mod excludes;
pub(crate) mod helpers;
mod performance;

pub(crate) use axon_config_loader::load_axon_config;
```

- [ ] **Step 6: Re-export `AxonConfig` from `crates/core/config.rs`**

Add to `crates/core/config.rs` (it's a re-export shim — find the existing `pub use` lines and add):

```rust
pub use axon_config::AxonConfig;
```

Also declare the module at the top of `config.rs`:

```rust
pub mod axon_config;
```

Verify the exact current content first:
```bash
head -20 crates/core/config.rs
```

- [ ] **Step 7: Run tests — expect pass**

```bash
cargo test -p axon-core axon_config_loader -- --nocapture
```

Expected: 3 tests pass

- [ ] **Step 8: Lint**

```bash
cargo clippy -p axon-core 2>&1 | grep "^error"
```

- [ ] **Step 9: Commit**

```bash
git add crates/core/config/parse/axon_config_loader.rs \
        crates/core/config/parse.rs \
        crates/core/config.rs \
        crates/core/Cargo.toml
git commit -m "feat(config): add load_axon_config() loader with AXON_CONFIG override"
```

---

## Task 3: `_or` Helper Variants in `helpers.rs`

**Files:**
- Modify: `crates/core/config/parse/helpers.rs`

These are the new precedence-chain helpers: `env → json → fallback`. They sit alongside the existing `env_bool` in `helpers.rs`.

- [ ] **Step 1: Write the failing tests**

Append a new `mod or_helpers` test block to `helpers.rs`:

```rust
#[cfg(test)]
mod or_helpers_tests {
    use super::*;
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn usize_or_env_wins_over_json() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::set_var("TEST_USIZE_OR_A", "200") };
        let result = env_usize_clamped_or("TEST_USIZE_OR_A", Some(150), 100, 10, 500);
        unsafe { std::env::remove_var("TEST_USIZE_OR_A") };
        assert_eq!(result, 200);
    }

    #[test]
    fn usize_or_json_wins_when_env_unset() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TEST_USIZE_OR_B") };
        let result = env_usize_clamped_or("TEST_USIZE_OR_B", Some(150), 100, 10, 500);
        assert_eq!(result, 150);
    }

    #[test]
    fn usize_or_fallback_when_both_absent() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TEST_USIZE_OR_C") };
        let result = env_usize_clamped_or("TEST_USIZE_OR_C", None, 100, 10, 500);
        assert_eq!(result, 100);
    }

    #[test]
    fn usize_or_empty_env_falls_through_to_json() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::set_var("TEST_USIZE_OR_D", "") };
        let result = env_usize_clamped_or("TEST_USIZE_OR_D", Some(150), 100, 10, 500);
        unsafe { std::env::remove_var("TEST_USIZE_OR_D") };
        assert_eq!(result, 150); // empty string parse fails → fall through to json
    }

    #[test]
    fn bool_or_env_wins() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::set_var("TEST_BOOL_OR_A", "false") };
        let result = env_bool_or("TEST_BOOL_OR_A", Some(true), true);
        unsafe { std::env::remove_var("TEST_BOOL_OR_A") };
        assert!(!result);
    }

    #[test]
    fn bool_or_json_wins_when_env_unset() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TEST_BOOL_OR_B") };
        let result = env_bool_or("TEST_BOOL_OR_B", Some(false), true);
        assert!(!result);
    }

    #[test]
    fn str_or_env_set_empty_uses_empty_not_json() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::set_var("TEST_STR_OR_A", "") };
        let result = env_str_or("TEST_STR_OR_A", Some("from-json".to_string()), "fallback");
        unsafe { std::env::remove_var("TEST_STR_OR_A") };
        assert_eq!(result, ""); // explicitly empty string wins over JSON
    }

    #[test]
    fn str_or_json_wins_when_env_unset() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TEST_STR_OR_B") };
        let result = env_str_or("TEST_STR_OR_B", Some("from-json".to_string()), "fallback");
        assert_eq!(result, "from-json");
    }

    #[test]
    fn opt_str_or_env_wins() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::set_var("TEST_OPT_STR_OR_A", "env-val") };
        let result = env_opt_str_or("TEST_OPT_STR_OR_A", Some("json-val".to_string()));
        unsafe { std::env::remove_var("TEST_OPT_STR_OR_A") };
        assert_eq!(result.as_deref(), Some("env-val"));
    }

    #[test]
    fn opt_str_or_json_wins_when_env_unset() {
        let _g = LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TEST_OPT_STR_OR_B") };
        let result = env_opt_str_or("TEST_OPT_STR_OR_B", Some("json-val".to_string()));
        assert_eq!(result.as_deref(), Some("json-val"));
    }
}
```

- [ ] **Step 2: Run tests — expect compile failure** (functions not defined yet)

```bash
cargo test -p axon-core or_helpers_tests 2>&1 | head -20
```

- [ ] **Step 3: Implement the `_or` variants**

Append to `crates/core/config/parse/helpers.rs` (after the last existing function, before `#[cfg(test)]`):

```rust
/// env → json → fallback, with clamp. Empty string env var falls through (parse fails → None).
pub(crate) fn env_usize_clamped_or(
    var: &str,
    json: Option<usize>,
    fallback: usize,
    min: usize,
    max: usize,
) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok()) // empty string → None → fall through
        .or(json)
        .unwrap_or(fallback)
        .clamp(min, max)
}

/// env → json → fallback, with clamp. Empty string env var falls through.
pub(crate) fn env_f64_clamped_or(
    var: &str,
    json: Option<f64>,
    fallback: f64,
    min: f64,
    max: f64,
) -> f64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .or(json)
        .unwrap_or(fallback)
        .clamp(min, max)
}

/// env → json → fallback for booleans. Unset or empty env var falls through to json.
pub(crate) fn env_bool_or(var: &str, json: Option<bool>, fallback: bool) -> bool {
    match std::env::var(var).ok().as_deref().map(str::trim) {
        None | Some("") => json.unwrap_or(fallback),
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => true,
            "0" | "false" | "no" | "n" | "off" => false,
            _ => json.unwrap_or(fallback),
        },
    }
}

/// env → json → fallback for strings.
/// IMPORTANT: `Ok("")` (env var explicitly set to empty) returns `""` — does NOT fall through to json.
/// Only an unset env var (`Err`) falls through to json.
pub(crate) fn env_str_or(var: &str, json: Option<String>, fallback: &str) -> String {
    match std::env::var(var) {
        Ok(v) => v,
        Err(_) => json.unwrap_or_else(|| fallback.to_string()),
    }
}

/// Like `env_str_or` but returns `Option<String>`. Unset env var falls through to json.
pub(crate) fn env_opt_str_or(var: &str, json: Option<String>) -> Option<String> {
    match std::env::var(var) {
        Ok(v) => Some(v),
        Err(_) => json,
    }
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p axon-core or_helpers_tests -- --nocapture
```

Expected: 10 tests pass

- [ ] **Step 5: Lint**

```bash
cargo clippy -p axon-core 2>&1 | grep "^error"
```

- [ ] **Step 6: Commit**

```bash
git add crates/core/config/parse/helpers.rs
git commit -m "feat(config): add env_*_or helper variants for JSON config precedence"
```

---

## Task 4: Wire `AxonConfig` into `build_config.rs` — Services, LLM, ACP, Queues

**Files:**
- Modify: `crates/core/config/parse/build_config.rs`

This task migrates the call sites that read service URLs, LLM settings, ACP settings, and queue names.

- [ ] **Step 1: Add `load_axon_config` call at top of `into_config()`**

In `build_config.rs`, update the imports and add the loader call:

At the top of `into_config()`, after `let global = cli.global;`, add:

```rust
// Load axon.json once. Env vars (step 2) override these values (step 3).
// Missing file or missing keys silently return Option::None → hardcoded defaults.
let ac = super::load_axon_config();
```

Also add to the `use` block at the top of the file:

```rust
use super::helpers::{
    env_bool, env_bool_or, env_opt_str_or, env_str_or, parse_viewport,
    positional_from_graph_subcommand, positional_from_job,
    positional_from_refresh_subcommand, positional_from_watch_subcommand,
};
```

- [ ] **Step 2: Migrate service URL fields**

Replace these call sites in the `Config { ... }` literal:

```rust
// BEFORE — qdrant_url:
qdrant_url: global
    .qdrant_url
    .or_else(|| env::var("QDRANT_URL").ok())
    .map(normalize_local_service_url)
    .unwrap_or_else(|| "http://127.0.0.1:53333".to_string()),

// AFTER:
qdrant_url: global
    .qdrant_url
    .or_else(|| env::var("QDRANT_URL").ok())
    .or_else(|| ac.services.qdrant_url.clone())
    .map(normalize_local_service_url)
    .unwrap_or_else(|| "http://127.0.0.1:53333".to_string()),
```

```rust
// BEFORE — tei_url:
tei_url: global
    .tei_url
    .or_else(|| env::var("TEI_URL").ok())
    .map(normalize_local_service_url)
    .unwrap_or_default(),

// AFTER:
tei_url: global
    .tei_url
    .or_else(|| env::var("TEI_URL").ok())
    .or_else(|| ac.services.tei_url.clone())
    .map(normalize_local_service_url)
    .unwrap_or_default(),
```

```rust
// BEFORE — chrome_remote_url:
chrome_remote_url: global
    .chrome_remote_url
    .or_else(|| env::var("AXON_CHROME_REMOTE_URL").ok())
    .map(normalize_local_service_url),

// AFTER:
chrome_remote_url: global
    .chrome_remote_url
    .or_else(|| env::var("AXON_CHROME_REMOTE_URL").ok())
    .or_else(|| ac.services.chrome_remote_url.clone())
    .map(normalize_local_service_url),
```

```rust
// BEFORE — neo4j_url:
neo4j_url: env::var("AXON_NEO4J_URL").ok().unwrap_or_default(),

// AFTER:
neo4j_url: env_str_or("AXON_NEO4J_URL", ac.services.neo4j_url.clone(), ""),
```

```rust
// BEFORE — neo4j_user:
neo4j_user: env::var("AXON_NEO4J_USER")
    .ok()
    .unwrap_or_else(|| "neo4j".to_string()),

// AFTER:
neo4j_user: env_str_or("AXON_NEO4J_USER", ac.services.neo4j_user.clone(), "neo4j"),
```

- [ ] **Step 3: Migrate LLM fields**

```rust
// BEFORE — openai_base_url:
openai_base_url: global
    .openai_base_url
    .or_else(|| env::var("OPENAI_BASE_URL").ok())
    .unwrap_or_default(),

// AFTER:
openai_base_url: global
    .openai_base_url
    .or_else(|| env::var("OPENAI_BASE_URL").ok())
    .or_else(|| ac.llm.base_url.clone())
    .unwrap_or_default(),
```

```rust
// BEFORE — openai_model:
openai_model: global
    .openai_model
    .or_else(|| env::var("OPENAI_MODEL").ok())
    .unwrap_or_default(),

// AFTER:
openai_model: global
    .openai_model
    .or_else(|| env::var("OPENAI_MODEL").ok())
    .or_else(|| ac.llm.model.clone())
    .unwrap_or_default(),
```

- [ ] **Step 4: Migrate ACP fields**

```rust
// BEFORE — acp_adapter_cmd:
acp_adapter_cmd: env::var("AXON_ACP_ADAPTER_CMD")
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
    .or_else(|| Some("codex-acp".to_string())),

// AFTER:
acp_adapter_cmd: env::var("AXON_ACP_ADAPTER_CMD")
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
    .or_else(|| ac.acp.adapter_cmd.clone().filter(|v| !v.is_empty()))
    .or_else(|| Some("codex-acp".to_string())),
```

```rust
// BEFORE — acp_adapter_args:
acp_adapter_args: env::var("AXON_ACP_ADAPTER_ARGS")
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty()),

// AFTER:
acp_adapter_args: env::var("AXON_ACP_ADAPTER_ARGS")
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
    .or_else(|| ac.acp.adapter_args.clone().filter(|v| !v.is_empty())),
```

```rust
// BEFORE — acp_prewarm:
acp_prewarm: env_bool("AXON_ACP_PREWARM", true),

// AFTER:
acp_prewarm: env_bool_or("AXON_ACP_PREWARM", ac.acp.prewarm, true),
```

- [ ] **Step 5: Migrate queue name fields**

```rust
// BEFORE — crawl_queue:
crawl_queue: global
    .crawl_queue
    .or_else(|| env::var("AXON_CRAWL_QUEUE").ok())
    .unwrap_or_else(|| "axon.crawl.jobs".to_string()),

// AFTER:
crawl_queue: global.crawl_queue.unwrap_or_else(||
    env_str_or("AXON_CRAWL_QUEUE", ac.queues.crawl.clone(), "axon.crawl.jobs")),
```

Apply the same pattern to `refresh_queue`, `extract_queue`, `embed_queue`, `ingest_queue`:

```rust
refresh_queue: global.refresh_queue.unwrap_or_else(||
    env_str_or("AXON_REFRESH_QUEUE", ac.queues.refresh.clone(), "axon.refresh.jobs")),

extract_queue: global.extract_queue.unwrap_or_else(||
    env_str_or("AXON_EXTRACT_QUEUE", ac.queues.extract.clone(), "axon.extract.jobs")),

embed_queue: global.embed_queue.unwrap_or_else(||
    env_str_or("AXON_EMBED_QUEUE", ac.queues.embed.clone(), "axon.embed.jobs")),

ingest_queue: global.ingest_queue.unwrap_or_else(||
    env_str_or("AXON_INGEST_QUEUE", ac.queues.ingest.clone(), "axon.ingest.jobs")),
```

For `graph_queue` (not a CLI flag, env-only):

```rust
// BEFORE:
graph_queue: env::var("AXON_GRAPH_QUEUE")
    .ok()
    .unwrap_or_else(|| "axon.graph.jobs".to_string()),

// AFTER:
graph_queue: env_str_or("AXON_GRAPH_QUEUE", ac.queues.graph.clone(), "axon.graph.jobs"),
```

- [ ] **Step 6: Run full test suite**

```bash
cargo test -p axon-core 2>&1 | tail -10
```

Expected: all tests pass (no regressions)

- [ ] **Step 7: Lint**

```bash
cargo clippy -p axon-core 2>&1 | grep "^error"
```

- [ ] **Step 8: Commit**

```bash
git add crates/core/config/parse/build_config.rs
git commit -m "feat(config): wire AxonConfig into build_config for services, llm, acp, queues"
```

---

## Task 5: Wire `AxonConfig` — Graph, Ask, Search, Ingest

**Files:**
- Modify: `crates/core/config/parse/build_config.rs`
- Note: `performance::env_usize_clamped` and `performance::env_f64_clamped` are the existing functions in `performance.rs`; the new `env_usize_clamped_or` / `env_f64_clamped_or` are in `helpers.rs`. Add the new imports.

- [ ] **Step 1: Add new helper imports**

Add to the `use super::helpers::{...}` block:

```rust
use super::helpers::{
    env_bool, env_bool_or, env_f64_clamped_or, env_opt_str_or, env_str_or,
    env_usize_clamped_or, parse_viewport, positional_from_graph_subcommand,
    positional_from_job, positional_from_refresh_subcommand, positional_from_watch_subcommand,
};
```

- [ ] **Step 2: Migrate graph fields**

```rust
// BEFORE:
graph_concurrency: env::var("AXON_GRAPH_CONCURRENCY")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(4),

// AFTER:
graph_concurrency: env_usize_clamped_or(
    "AXON_GRAPH_CONCURRENCY", ac.graph.concurrency, 4, 1, 256),
```

```rust
// BEFORE:
graph_llm_model: env::var("AXON_GRAPH_LLM_MODEL")
    .ok()
    .unwrap_or_else(|| "qwen3.5:4b".to_string()),

// AFTER:
graph_llm_model: env_str_or("AXON_GRAPH_LLM_MODEL", ac.graph.llm_model.clone(), "qwen3.5:4b"),
```

```rust
// BEFORE:
graph_similarity_threshold: env::var("AXON_GRAPH_SIMILARITY_THRESHOLD")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(0.75),

// AFTER:
graph_similarity_threshold: env_f64_clamped_or(
    "AXON_GRAPH_SIMILARITY_THRESHOLD", ac.graph.similarity_threshold, 0.75, 0.0, 1.0),
```

```rust
// BEFORE:
graph_similarity_limit: env::var("AXON_GRAPH_SIMILARITY_LIMIT")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(20),

// AFTER:
graph_similarity_limit: env_usize_clamped_or(
    "AXON_GRAPH_SIMILARITY_LIMIT", ac.graph.similarity_limit, 20, 1, 1000),
```

```rust
// BEFORE:
graph_context_max_chars: env::var("AXON_GRAPH_CONTEXT_MAX_CHARS")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(2_000),

// AFTER:
graph_context_max_chars: env_usize_clamped_or(
    "AXON_GRAPH_CONTEXT_MAX_CHARS", ac.graph.context_max_chars, 2_000, 100, 100_000),
```

```rust
// BEFORE:
graph_taxonomy_path: env::var("AXON_GRAPH_TAXONOMY_PATH").ok().unwrap_or_default(),

// AFTER:
graph_taxonomy_path: env_str_or("AXON_GRAPH_TAXONOMY_PATH", ac.graph.taxonomy_path.clone(), ""),
```

- [ ] **Step 3: Migrate ask fields** (replace all `performance::env_usize_clamped` / `performance::env_f64_clamped` calls for ask fields)

```rust
ask_max_context_chars: env_usize_clamped_or(
    "AXON_ASK_MAX_CONTEXT_CHARS", ac.ask.max_context_chars, 120_000, 20_000, 400_000),

ask_candidate_limit: env_usize_clamped_or(
    "AXON_ASK_CANDIDATE_LIMIT", ac.ask.candidate_limit, 64, 8, 200),

ask_chunk_limit: env_usize_clamped_or(
    "AXON_ASK_CHUNK_LIMIT", ac.ask.chunk_limit, 10, 3, 40),

ask_full_docs: env_usize_clamped_or(
    "AXON_ASK_FULL_DOCS", ac.ask.full_docs, 4, 1, 20),

ask_backfill_chunks: env_usize_clamped_or(
    "AXON_ASK_BACKFILL_CHUNKS", ac.ask.backfill_chunks, 3, 0, 20),

ask_doc_fetch_concurrency: env_usize_clamped_or(
    "AXON_ASK_DOC_FETCH_CONCURRENCY", ac.ask.doc_fetch_concurrency, 4, 1, 16),

ask_doc_chunk_limit: env_usize_clamped_or(
    "AXON_ASK_DOC_CHUNK_LIMIT", ac.ask.doc_chunk_limit, 192, 8, 2000),

ask_min_relevance_score: env_f64_clamped_or(
    "AXON_ASK_MIN_RELEVANCE_SCORE", ac.ask.min_relevance_score, 0.45, -1.0, 2.0),

ask_authoritative_boost: env_f64_clamped_or(
    "AXON_ASK_AUTHORITATIVE_BOOST", ac.ask.authoritative_boost, 0.0, 0.0, 0.5),

ask_min_citations_nontrivial: env_usize_clamped_or(
    "AXON_ASK_MIN_CITATIONS_NONTRIVIAL", ac.ask.min_citations_nontrivial, 2, 1, 5),
```

For the Vec<String> ask fields (authoritative_domains, authoritative_allowlist), use a custom fallback:

```rust
ask_authoritative_domains: env::var("AXON_ASK_AUTHORITATIVE_DOMAINS")
    .ok()
    .map(|raw| parse_csv_env(&raw, |s| s.to_ascii_lowercase()))
    .or_else(|| ac.ask.authoritative_domains.clone())
    .unwrap_or_default(),

ask_authoritative_allowlist: env::var("AXON_ASK_AUTHORITATIVE_ALLOWLIST")
    .ok()
    .map(|raw| parse_csv_env(&raw, |s| s.to_ascii_lowercase()))
    .or_else(|| ac.ask.authoritative_allowlist.clone())
    .unwrap_or_default(),
```

- [ ] **Step 4: Migrate search/hybrid fields**

```rust
// BEFORE:
hybrid_search_enabled: env_bool("AXON_HYBRID_SEARCH", true),

// AFTER:
hybrid_search_enabled: env_bool_or("AXON_HYBRID_SEARCH", ac.search.hybrid_enabled, true),
```

```rust
// BEFORE:
hybrid_search_candidates: performance::env_usize_clamped("AXON_HYBRID_CANDIDATES", 100, 10, 500),

// AFTER:
hybrid_search_candidates: env_usize_clamped_or(
    "AXON_HYBRID_CANDIDATES", ac.search.hybrid_candidates, 100, 10, 500),
```

```rust
// BEFORE:
ask_hybrid_candidates: performance::env_usize_clamped("AXON_ASK_HYBRID_CANDIDATES", 150, 10, 500),

// AFTER:
ask_hybrid_candidates: env_usize_clamped_or(
    "AXON_ASK_HYBRID_CANDIDATES", ac.search.ask_hybrid_candidates, 150, 10, 500),
```

- [ ] **Step 5: Migrate ingest fields**

```rust
// BEFORE (at top of into_config, the github_max_* muts):
let mut github_max_issues: usize = env::var("GITHUB_MAX_ISSUES")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(100);
let mut github_max_prs: usize = env::var("GITHUB_MAX_PRS")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(100);

// AFTER (replace the two let mut lines):
let mut github_max_issues: usize = env_usize_clamped_or(
    "GITHUB_MAX_ISSUES", ac.ingest.github_max_issues, 100, 0, 10_000);
let mut github_max_prs: usize = env_usize_clamped_or(
    "GITHUB_MAX_PRS", ac.ingest.github_max_prs, 100, 0, 10_000);
```

- [ ] **Step 6: Migrate MCP fields**

```rust
// BEFORE — mcp_http_host:
mcp_http_host: env::var("AXON_MCP_HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),

// AFTER:
mcp_http_host: env_str_or("AXON_MCP_HTTP_HOST", ac.mcp.http_host.clone(), "0.0.0.0"),
```

For `mcp_http_port`, the existing code uses a custom `parse_mcp_http_port` error handler — keep that logic but add JSON fallback:

```rust
// BEFORE:
mcp_http_port: env::var("AXON_MCP_HTTP_PORT")
    .ok()
    .as_deref()
    .map(parse_mcp_http_port)
    .transpose()?
    .unwrap_or(8001),

// AFTER:
mcp_http_port: env::var("AXON_MCP_HTTP_PORT")
    .ok()
    .as_deref()
    .map(parse_mcp_http_port)
    .transpose()?
    .or(ac.mcp.http_port.map(u16::from))
    .unwrap_or(8001),
```

Wait — `ac.mcp.http_port` is `Option<u16>` and `mcp_http_port` in Config is `u16`. No cast needed:

```rust
    .or(ac.mcp.http_port)
    .unwrap_or(8001),
```

- [ ] **Step 7: Migrate web fields (Vec<String> pattern)**

```rust
// BEFORE:
web_allowed_origins: env::var("AXON_WEB_ALLOWED_ORIGINS")
    .ok()
    .map(|raw| parse_origin_allowlist(&raw))
    .unwrap_or_default(),

// AFTER:
web_allowed_origins: env::var("AXON_WEB_ALLOWED_ORIGINS")
    .ok()
    .map(|raw| parse_origin_allowlist(&raw))
    .or_else(|| ac.web.allowed_origins.clone())
    .unwrap_or_default(),
```

```rust
// BEFORE:
shell_allowed_origins: env::var("AXON_SHELL_ALLOWED_ORIGINS")
    .ok()
    .map(|raw| parse_origin_allowlist(&raw))
    .unwrap_or_default(),

// AFTER:
shell_allowed_origins: env::var("AXON_SHELL_ALLOWED_ORIGINS")
    .ok()
    .map(|raw| parse_origin_allowlist(&raw))
    .or_else(|| ac.web.shell_allowed_origins.clone())
    .unwrap_or_default(),
```

- [ ] **Step 8: Run full test suite**

```bash
cargo test 2>&1 | tail -15
```

Expected: all tests pass

- [ ] **Step 9: Lint**

```bash
cargo clippy 2>&1 | grep "^error"
```

Expected: no errors. If `performance::env_usize_clamped` is now unused, the compiler will warn — either remove it or add `#[allow(dead_code)]` with a comment.

- [ ] **Step 10: Commit**

```bash
git add crates/core/config/parse/build_config.rs
git commit -m "feat(config): wire AxonConfig into build_config for graph, ask, search, ingest, mcp, web"
```

---

## Task 6: TypeScript Types + Server Singleton

**Files:**
- Create: `apps/web/lib/axon-config-types.ts`
- Create: `apps/web/lib/axon-config.ts`

- [ ] **Step 1: Create TypeScript types**

Create `apps/web/lib/axon-config-types.ts`:

```typescript
/** Mirror of axon.json structure. All fields optional — missing keys fall back to defaults. */

export type AxonConfig = {
  services?: ServicesConfig
  llm?: LlmConfig
  tei?: TeiConfig
  search?: SearchConfig
  ask?: AskConfig
  embed?: EmbedConfig
  queues?: QueuesConfig
  workers?: WorkersConfig
  graph?: GraphConfig
  acp?: AcpConfig
  web?: WebConfig
  mcp?: McpConfig
  serve?: ServeConfig
  chrome?: ChromeConfig
  logging?: LoggingConfig
  output?: OutputConfig
  ingest?: IngestConfig
  oauth?: OAuthConfig
}

export type ServicesConfig = {
  qdrant_url?: string
  tei_url?: string
  chrome_remote_url?: string
  chrome_url?: string
  neo4j_url?: string
  neo4j_user?: string
  backend_url?: string
  workers_ws_url?: string
  backend_hostname?: string
}

export type LlmConfig = {
  base_url?: string
  model?: string
}

export type TeiConfig = {
  max_retries?: number
  request_timeout_ms?: number
  max_client_batch_size?: number
  http_port?: number
  embedding_model?: string
  max_concurrent_requests?: number
  max_batch_tokens?: number
  max_batch_requests?: number
  pooling?: string
  tokenization_workers?: number
}

export type SearchConfig = {
  hybrid_enabled?: boolean
  hybrid_candidates?: number
  ask_hybrid_candidates?: number
  hnsw_ef?: number
  hnsw_ef_legacy?: number
}

export type AskConfig = {
  max_context_chars?: number
  candidate_limit?: number
  chunk_limit?: number
  full_docs?: number
  backfill_chunks?: number
  doc_fetch_concurrency?: number
  doc_chunk_limit?: number
  min_relevance_score?: number
  authoritative_domains?: string[]
  authoritative_boost?: number
  authoritative_allowlist?: string[]
  min_citations_nontrivial?: number
}

export type EmbedConfig = {
  collection?: string
  doc_concurrency?: number | null
  doc_timeout_secs?: number
  strict_predelete?: boolean
}

export type QueuesConfig = {
  crawl?: string
  extract?: string
  embed?: string
  ingest?: string
  refresh?: string
  graph?: string
}

export type WorkersConfig = {
  ingest_lanes?: number
  max_pending_crawl_jobs?: number
  crawl_size_warn_threshold?: number
  job_stale_timeout_secs?: number
  job_stale_confirm_secs?: number
  pg_pool_size?: number | null
  max_ws_connections?: number | null
  max_shell_connections?: number | null
  max_sync_concurrent?: number | null
}

export type GraphConfig = {
  concurrency?: number
  llm_model?: string
  similarity_threshold?: number
  similarity_limit?: number
  context_max_chars?: number
  taxonomy_path?: string
}

export type AgentEntry = {
  cmd?: string
  args?: string
}

export type AgentsConfig = {
  claude?: AgentEntry
  codex?: AgentEntry
  gemini?: AgentEntry
}

export type AcpConfig = {
  adapter_cmd?: string
  adapter_args?: string
  prewarm?: boolean
  auto_approve?: boolean
  max_concurrent_sessions?: number
  turn_timeout_ms?: number
  allowed_claude_betas?: string
  agents?: AgentsConfig
}

export type WebConfig = {
  allowed_origins?: string[]
  allow_insecure_dev?: boolean
  allow_query_token?: boolean
  trust_proxy?: boolean
  pulse_chat_timeout_ms?: number
  shell_allowed_origins?: string[]
  shell_server_host?: string
  shell_server_port?: number | null
  docker_socket_path?: string
  enable_docker_socket_logs?: boolean
}

export type McpConfig = {
  transport?: 'http' | 'stdio'
  http_host?: string
  http_port?: number
  artifact_dir?: string
  inline_bytes_threshold?: number
}

export type ServeConfig = {
  host?: string
  port?: number
}

export type ChromeConfig = {
  diagnostics?: boolean
  diagnostics_dir?: string
  diagnostics_events?: boolean
  diagnostics_screenshot?: boolean
  proxy?: string
  user_agent?: string
}

export type LoggingConfig = {
  file?: string
  max_bytes?: number
  max_files?: number
  no_color?: boolean
}

export type OutputConfig = {
  dir?: string
  extract_est_cost_per_1k_tokens?: number | null
}

export type IngestConfig = {
  github_max_issues?: number
  github_max_prs?: number
  download_max_bytes?: number | null
  download_max_files?: number | null
  domains_detailed?: boolean
  domains_facet_limit?: number | null
}

export type OAuthConfig = {
  auth_url?: string
  token_url?: string
  redirect_uri?: string
  redirect_host?: string
  redirect_path?: string
  redirect_policy?: string
  scopes?: string
  required_scopes?: string
  redis_prefix?: string
  broker_issuer?: string
}
```

- [ ] **Step 2: Create the server-only singleton**

Create `apps/web/lib/axon-config.ts`:

```typescript
import 'server-only' // prevents accidental client-side import
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import type { AxonConfig } from './axon-config-types'

// Module-level singleton — loaded once on first import, not per-request.
function loadAxonConfig(): AxonConfig {
  const configPath = process.env.AXON_CONFIG ?? join(process.cwd(), 'axon.json')
  try {
    return JSON.parse(readFileSync(configPath, 'utf8')) as AxonConfig
  } catch {
    return {}
  }
}

export const axonConfig: AxonConfig = loadAxonConfig()
```

- [ ] **Step 3: Check `server-only` is available**

```bash
grep -r "server-only" apps/web/package.json apps/web/node_modules/.package-lock.json 2>/dev/null | head -3
```

If not present: `cd apps/web && pnpm add server-only`

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd apps/web && pnpm exec tsc --noEmit --skipLibCheck 2>&1 | grep "axon-config" | head -10
```

Expected: no errors for these files

- [ ] **Step 5: Update `service-url.ts` to use axon-config for backend URL**

In `apps/web/lib/server/service-url.ts`, the Next.js server-side code uses `process.env.AXON_BACKEND_URL`. After this task, server-side callers can optionally adopt the JSON config for non-secret URL settings.

This is an optional incremental adoption — do NOT modify all callers now. The spec says: migrate at own pace. The singleton is now available for callers to adopt.

- [ ] **Step 6: Commit**

```bash
git add apps/web/lib/axon-config-types.ts apps/web/lib/axon-config.ts
git commit -m "feat(web): add axon-config TypeScript types and server-only singleton"
```

---

## Task 7: `next.config.ts` Dotenv Root Load

**Files:**
- Modify: `apps/web/next.config.ts`

This enables local `pnpm dev` to find all vars from root `.env` without needing `apps/web/.env.local`.

- [ ] **Step 1: Check current top of `next.config.ts`**

```bash
head -10 apps/web/next.config.ts
```

- [ ] **Step 2: Add dotenv root load at the top**

Add before any other imports in `apps/web/next.config.ts`:

```typescript
// Local dev: load root .env so `pnpm dev` and `next build` find all vars.
// Docker: no-op — compose injects vars into the container env before Node starts.
// override: false ensures container-injected vars win over file values.
import { config as loadDotenv } from 'dotenv'
import { resolve } from 'node:path'

loadDotenv({ path: resolve(__dirname, '../../.env'), override: false })
```

`dotenv` is a transitive Next.js dependency — no new install required. Verify:

```bash
ls apps/web/node_modules/dotenv 2>/dev/null && echo "present" || echo "missing"
```

If missing: `cd apps/web && pnpm add dotenv`

- [ ] **Step 3: Verify dev server starts without `apps/web/.env.local`**

```bash
# Temporarily rename .env.local to verify the root .env is sufficient
mv apps/web/.env.local apps/web/.env.local.bak
cd apps/web && timeout 15 pnpm dev 2>&1 | grep -E "(Ready|Error|ENOENT)" | head -5
mv apps/web/.env.local.bak apps/web/.env.local  # restore
```

Expected: `Ready` (or no ENOENT errors related to missing vars)

- [ ] **Step 4: Commit**

```bash
git add apps/web/next.config.ts
git commit -m "feat(web): load root .env in next.config.ts for pnpm dev without .env.local"
```

---

## Task 8: Env File Cleanup

**Files:**
- Modify: `.env.example`
- Modify: `.env` (remove vars that now have defaults in `axon.json`)
- Delete: `apps/web/.env.local`

> **⚠️ Read carefully before editing.** The spec has an explicit list of what stays in `.env`. Do not remove secrets, DSNs, or NEXT_PUBLIC_* vars.

- [ ] **Step 1: Read the current `.env.example`**

```bash
cat .env.example
```

- [ ] **Step 2: Add pointer to `axon.json` at the top of `.env.example`**

At the top of `.env.example`, add:

```bash
# Non-secret configuration (service URLs, model names, tuning knobs, feature flags)
# lives in axon.json at the repo root. See axon.schema.json for all available fields.
# This file contains ONLY: credentials, DSNs with passwords, API keys, tokens,
# Docker Compose interpolation vars, and NEXT_PUBLIC_* build-time vars.
```

- [ ] **Step 3: Remove vars from `.env.example` that moved to `axon.json`**

Variables to remove from `.env.example` (they now have defaults in `axon.json`):

```
# Remove these — they have defaults in axon.json:
AXON_CRAWL_QUEUE, AXON_EXTRACT_QUEUE, AXON_EMBED_QUEUE, AXON_INGEST_QUEUE,
AXON_GRAPH_QUEUE, AXON_GRAPH_CONCURRENCY, AXON_GRAPH_LLM_MODEL,
AXON_GRAPH_SIMILARITY_THRESHOLD, AXON_GRAPH_SIMILARITY_LIMIT,
AXON_GRAPH_CONTEXT_MAX_CHARS, AXON_GRAPH_TAXONOMY_PATH,
AXON_ACP_PREWARM, AXON_HYBRID_SEARCH, AXON_HYBRID_CANDIDATES,
AXON_ASK_HYBRID_CANDIDATES, AXON_ASK_MAX_CONTEXT_CHARS, AXON_ASK_CANDIDATE_LIMIT,
AXON_ASK_CHUNK_LIMIT, AXON_ASK_FULL_DOCS, AXON_ASK_BACKFILL_CHUNKS,
AXON_ASK_DOC_FETCH_CONCURRENCY, AXON_ASK_DOC_CHUNK_LIMIT,
AXON_ASK_MIN_RELEVANCE_SCORE, AXON_ASK_AUTHORITATIVE_DOMAINS,
AXON_ASK_AUTHORITATIVE_BOOST, AXON_ASK_AUTHORITATIVE_ALLOWLIST,
AXON_ASK_MIN_CITATIONS_NONTRIVIAL, AXON_WEB_ALLOWED_ORIGINS,
AXON_SHELL_ALLOWED_ORIGINS, AXON_MCP_HTTP_HOST, AXON_MCP_HTTP_PORT,
GITHUB_MAX_ISSUES, GITHUB_MAX_PRS, AXON_INGEST_LANES,
AXON_EMBED_DOC_TIMEOUT_SECS, AXON_EMBED_STRICT_PREDELETE,
AXON_JOB_STALE_TIMEOUT_SECS, AXON_JOB_STALE_CONFIRM_SECS,
AXON_MAX_PENDING_CRAWL_JOBS, AXON_CRAWL_SIZE_WARN_THRESHOLD
```

**Keep in `.env.example` (secrets / credentials / compose vars / NEXT_PUBLIC_*):** Everything in the spec's "What Stays in `.env`" section.

- [ ] **Step 4: Delete `apps/web/.env.local`**

```bash
git rm apps/web/.env.local
```

- [ ] **Step 5: Verify nothing is broken**

```bash
cargo check 2>&1 | grep "^error" | head -10
```

- [ ] **Step 6: Commit**

```bash
git add .env.example
git commit -m "chore(config): strip axon.json vars from .env.example, delete apps/web/.env.local"
```

---

## Task 9: Precedence Integration Tests

**Files:**
- Modify: `crates/core/config/parse/build_config.rs` (add tests to the existing `#[cfg(test)] mod tests` block)

- [ ] **Step 1: Write the tests**

Add to the existing test module in `build_config.rs`:

```rust
#[allow(unsafe_code)]
#[test]
fn json_hybrid_candidates_used_when_env_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    const VAR: &str = "AXON_HYBRID_CANDIDATES";
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    const CFG: &str = "AXON_CONFIG";

    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, b"{\"search\":{\"hybrid_candidates\":77}}").unwrap();

    unsafe {
        env::remove_var(VAR);
        env::set_var(CFG, f.path());
        env::set_var(PG, "postgresql://axon:postgres@127.0.0.1:53432/axon"); <!-- gitleaks:allow -->
        env::set_var(REDIS, "redis://127.0.0.1:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@127.0.0.1:45535/%2f"); <!-- gitleaks:allow -->
    }

    let cli = Cli::parse_from(["axon", "status"]);
    let cfg = into_config(cli).expect("status should parse");
    assert_eq!(cfg.hybrid_search_candidates, 77);

    unsafe {
        env::remove_var(CFG);
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
}

#[allow(unsafe_code)]
#[test]
fn env_hybrid_candidates_wins_over_json() {
    let _guard = ENV_LOCK.lock().unwrap();
    const VAR: &str = "AXON_HYBRID_CANDIDATES";
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    const CFG: &str = "AXON_CONFIG";

    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, b"{\"search\":{\"hybrid_candidates\":77}}").unwrap();

    unsafe {
        env::set_var(VAR, "200");
        env::set_var(CFG, f.path());
        env::set_var(PG, "postgresql://axon:postgres@127.0.0.1:53432/axon"); <!-- gitleaks:allow -->
        env::set_var(REDIS, "redis://127.0.0.1:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@127.0.0.1:45535/%2f"); <!-- gitleaks:allow -->
    }

    let cli = Cli::parse_from(["axon", "status"]);
    let cfg = into_config(cli).expect("status should parse");
    assert_eq!(cfg.hybrid_search_candidates, 200); // env wins

    unsafe {
        env::remove_var(VAR);
        env::remove_var(CFG);
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
}

#[allow(unsafe_code)]
#[test]
fn service_url_from_json_when_env_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    const CFG: &str = "AXON_CONFIG";
    const QDRANT: &str = "QDRANT_URL";

    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(
        &mut f,
        b"{\"services\":{\"qdrant_url\":\"http://127.0.0.1:19333\"}}",
    )
    .unwrap();

    unsafe {
        env::remove_var(QDRANT);
        env::set_var(CFG, f.path());
        env::set_var(PG, "postgresql://axon:postgres@127.0.0.1:53432/axon"); <!-- gitleaks:allow -->
        env::set_var(REDIS, "redis://127.0.0.1:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@127.0.0.1:45535/%2f"); <!-- gitleaks:allow -->
    }

    let cli = Cli::parse_from(["axon", "status"]);
    let cfg = into_config(cli).expect("status should parse");
    // url is non-docker so normalize_local_service_url leaves it unchanged
    assert!(cfg.qdrant_url.contains("19333"), "expected JSON qdrant url, got {}", cfg.qdrant_url);

    unsafe {
        env::remove_var(CFG);
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
}
```

Add to `[dev-dependencies]` in `crates/core/Cargo.toml` if not already present:

```toml
tempfile = "3"
```

- [ ] **Step 2: Run the new tests**

```bash
cargo test -p axon-core json_hybrid_candidates env_hybrid_candidates service_url_from_json -- --nocapture
```

Expected: 3 tests pass

- [ ] **Step 3: Run full test suite**

```bash
cargo test 2>&1 | tail -5
```

Expected: all tests pass

- [ ] **Step 4: Run `just verify`**

```bash
just verify
```

Expected: fmt ✓, clippy ✓, check ✓, test ✓

- [ ] **Step 5: Commit**

```bash
git add crates/core/config/parse/build_config.rs crates/core/Cargo.toml
git commit -m "test(config): add precedence integration tests for axon.json JSON layer"
```

---

## Task 10: Verification + CLAUDE.md Update

**Files:**
- Modify: `crates/core/CLAUDE.md` (document new AxonConfig load order)
- Modify: `CLAUDE.md` (root — document axon.json approach)

- [ ] **Step 1: Smoke test — JSON value appears in config**

```bash
# Set a test value in axon.json
tmp=$(mktemp)
echo '{"search":{"hybrid_candidates":42}}' > "$tmp"
AXON_CONFIG="$tmp" ./scripts/axon doctor 2>&1 | head -20
rm "$tmp"
```

Expected: doctor runs without error (the value is wired internally; it won't print hybrid_candidates unless logging is added)

A more targeted test:

```bash
cargo test -p axon-core json_hybrid_candidates -- --nocapture 2>&1
```

Expected: PASS

- [ ] **Step 2: Smoke test — env var beats JSON**

```bash
cargo test -p axon-core env_hybrid_candidates -- --nocapture 2>&1
```

Expected: PASS

- [ ] **Step 3: Update `crates/core/CLAUDE.md`**

Add to the `into_config()` section:

```markdown
### AxonConfig load order (added 2026-03-22)

`into_config()` calls `load_axon_config()` as its **first action** — before reading CLI flags.
This populates `AxonConfig` (`crates/core/config/axon_config.rs`) from `axon.json`
(or the path in `AXON_CONFIG` env var).

Precedence for each field: `CLI flag > env var > axon.json > hardcoded default`

The `_or` helper variants in `helpers.rs` implement steps 2–4:
- `env_usize_clamped_or(var, json_val, fallback, min, max)`
- `env_f64_clamped_or(var, json_val, fallback, min, max)`
- `env_bool_or(var, json_val, fallback)`
- `env_str_or(var, json_val, fallback)` — `Ok("")` uses `""`, NOT json
- `env_opt_str_or(var, json_val)` — `Err` falls through to json

`AxonConfig` uses `Option<T>` fields throughout + `#[serde(default)]` so missing JSON keys
never cause parse errors. Unknown JSON keys are silently ignored (no `deny_unknown_fields`).
```

- [ ] **Step 4: Update root `CLAUDE.md` with axon.json mention**

Find the Environment Variables section and add a note:

```markdown
## axon.json Config File

Non-secret configuration lives in `axon.json` at the repo root (committed). Secrets stay in `.env`.

Precedence: `CLI flag > env var > axon.json > hardcoded default`

- Schema: `axon.schema.json` (editor validation only, not used at runtime)
- All ~85 settings documented in `docs/superpowers/specs/2026-03-22-axon-json-config-design.md`
- `AXON_CONFIG` env var overrides the file path (useful for Docker multi-config setups)
```

- [ ] **Step 5: Final `just verify`**

```bash
just verify
```

Expected: all checks pass

- [ ] **Step 6: Final commit**

```bash
git add crates/core/CLAUDE.md CLAUDE.md
git commit -m "docs(config): document axon.json load order in CLAUDE.md files"
```

---

## Checklist Summary

| Task | Description | Status |
|------|-------------|--------|
| 1 | `AxonConfig` serde struct tree | ☐ |
| 2 | `load_axon_config()` loader + module wiring | ☐ |
| 3 | `_or` helper variants in `helpers.rs` | ☐ |
| 4 | Wire into `build_config.rs` — services, llm, acp, queues | ☐ |
| 5 | Wire into `build_config.rs` — graph, ask, search, ingest, mcp, web | ☐ |
| 6 | TypeScript types + server singleton | ☐ |
| 7 | `next.config.ts` dotenv root load | ☐ |
| 8 | Env file cleanup + delete `.env.local` | ☐ |
| 9 | Precedence integration tests | ☐ |
| 10 | Smoke tests + CLAUDE.md docs | ☐ |
