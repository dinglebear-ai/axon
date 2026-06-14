# Codex App Server LLM Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `AXON_LLM_BACKEND=codex-app-server` a real, tested, hardened Axon LLM backend for ask/summarize/evaluate/suggest/extract/research synthesis using the existing `codex app-server` adapter.

**Architecture:** Activate the orphaned `src/core/llm/codex_app_server.rs` backend through the same `core::llm` facade used by Gemini headless and OpenAI-compatible backends. Add only the minimal config fields and model-selection plumbing needed for Codex synthesis, keep provider profiles out of scope because that code is currently orphaned, and use the existing child-process stdio app-server adapter rather than introducing desktop socket transport in this slice. Ask/RAG calls should use the same direct synthesis prompt as OpenAI-compatible providers because the Codex adapter deliberately prepares an isolated `CODEX_HOME` without skills, hooks, apps, or MCP servers; this plan hardens that subprocess boundary before exposing it through `serve`/MCP paths.

**Tech Stack:** Rust 2024, Tokio subprocess/stdout streaming, existing Axon `core::llm` facade, existing `Config` parsing, Qdrant/RAG ask services, sidecar Rust unit tests, markdown docs.

---

## Scope Decisions

- Build the child-process `codex app-server` backend that already exists in `src/core/llm/codex_app_server.rs`.
- Do not implement provider profiles in this plan. `src/core/config/parse/build_config/provider_overlay.rs` and `src/cli/commands/config/provider.rs` are stale/orphaned and should remain untouched unless a compile failure forces a narrow fix.
- Do not implement desktop Unix socket transport in this plan. Add a docs note that the backend spawns `codex app-server` per completion. A future transport can reuse the protocol parser after this backend is green.
- Do not copy raw user Codex hooks, MCP config, app config, or skills into the isolated runtime. Preserve the isolation behavior in `codex_app_server/home.rs`.
- Add only one Codex model knob in this slice: `codex_model` from `AXON_SYNTHESIS_CODEX_MODEL` with legacy alias `AXON_CODEX_MODEL`. Defer `AXON_CHAT_CODEX_MODEL` until there is a concrete Codex chat surface.
- Add a Codex-specific completion concurrency cap with a conservative default because this backend spawns a child app-server per completion.

## File Structure

- Modify: `src/core/llm.rs` - export `codex_app_server`, dispatch completions, and add a limiter key branch.
- Modify: `src/core/llm/types.rs` - add `CodexAppServer`, Codex config fields, model selection, timeout helper, and Codex-specific concurrency selection.
- Modify: `src/core/llm/concurrency.rs` - add a Codex limiter key variant.
- Modify: `src/core/llm/codex_app_server.rs` - adapt to the finalized config shape if needed.
- Modify: `src/core/config/types/config.rs` - add Codex command/home/model/concurrency fields.
- Modify: `src/core/config/types/config_impls.rs` - add defaults and debug fields.
- Modify: `src/core/config/parse/build_config/config_literal.rs` - parse `AXON_CODEX_CMD`, `AXON_CODEX_HOME`, `AXON_SYNTHESIS_CODEX_MODEL`, `AXON_CODEX_MODEL`, and `AXON_CODEX_COMPLETION_CONCURRENCY`.
- Modify: `src/core/config/parse/env_registry/runtime.rs` and `src/core/config/parse/env_registry/advanced.rs` - classify the Codex runtime env vars consistently.
- Modify: `src/jobs/config_snapshot.rs` - preserve Codex backend fields across async job replay.
- Modify: `src/vector/ops/commands/ask.rs` - validate Codex config for ask.
- Modify: `src/vector/ops/commands/ask/context.rs` - include Codex model in high-context detection.
- Modify: `src/vector/ops/commands/ask/synthesis_prompt.rs` - use the direct synthesis prompt for Codex.
- Modify: `src/core/config/parse/tuning.rs` - derive model tier from the configured backend model, including Codex.
- Modify: `src/services/search/synthesis/source.rs` - preserve full research sources for Codex/GPT-class models.
- Modify: `.env.example`, `config.example.toml`, `docs/guides/configuration.md`, `docs/reference/env-matrix.md`, `CLAUDE.md` - document the new backend truthfully.
- Test: `src/core/llm/types_tests.rs`, `src/core/llm/llm_backend_tests.rs`, `src/core/llm/codex_app_server_tests.rs`, `src/core/llm/codex_app_server/home_tests.rs`, `src/core/llm/codex_app_server/protocol_tests.rs`, `src/core/config/parse/build_config/tests/env_required.rs`, `src/jobs/config_snapshot_tests.rs`, `src/core/config/parse/tuning_tests.rs`, `src/services/search/synthesis_tests.rs`, `src/vector/ops/commands/ask_tests.rs`, `src/vector/ops/commands/streaming_tests.rs`, `src/vector/ops/commands/ask/context_tests.rs`, `src/vector/ops/commands/ask/synthesis_prompt_tests.rs`.

## Task 1: Backend Enum, Config Shape, and Model Selection

**Files:**
- Modify: `src/core/llm/types.rs`
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/config_impls.rs`
- Test: `src/core/llm/types_tests.rs`

- [ ] **Step 1: Write failing enum and config tests**

Add tests to `src/core/llm/types_tests.rs`:

```rust
#[test]
fn backend_kind_parses_codex_app_server_aliases() {
    for alias in ["codex-app-server", "codex_app_server", "codex"] {
        assert_eq!(
            LlmBackendKind::parse(alias).unwrap(),
            LlmBackendKind::CodexAppServer
        );
    }
}

#[test]
fn backend_config_accepts_codex_fields() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "/usr/local/bin/codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        codex_home: Some(PathBuf::from("/home/example/.codex")),
        codex_completion_concurrency: 1,
        ..Config::default()
    };

    let backend = LlmBackendConfig::from_config(&cfg);

    assert_eq!(backend.kind, LlmBackendKind::CodexAppServer);
    assert_eq!(backend.codex_cmd, "/usr/local/bin/codex");
    assert_eq!(backend.codex_model.as_deref(), Some("gpt-5.5"));
    assert_eq!(backend.codex_home, Some(PathBuf::from("/home/example/.codex")));
    assert_eq!(backend.completion_concurrency, 1);
}

#[test]
fn configured_model_uses_codex_backend_model() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        headless_gemini_model: "gemini-should-not-win".to_string(),
        openai_model: "openai-should-not-win".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    let synthesis = CompletionRequest::new("hello").backend_from_config(&cfg);

    assert_eq!(synthesis.model.as_deref(), Some("gpt-5.5"));
}

#[test]
fn completion_timeout_is_at_least_one_second_on_backend_config() {
    let zero = LlmBackendConfig {
        completion_timeout_secs: 0,
        ..LlmBackendConfig::default()
    };
    assert_eq!(zero.completion_timeout(), std::time::Duration::from_secs(1));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test core::llm::tests core::llm::types_tests --lib -- --nocapture
```

Expected: FAIL with missing `CodexAppServer`, `codex_cmd`, `codex_model`, `codex_home`, `codex_completion_concurrency`, or `completion_timeout`.

- [ ] **Step 3: Add Codex variants and backend config fields**

In `src/core/llm/types.rs`, extend the backend enum and config:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmBackendKind {
    GeminiHeadless,
    OpenAiCompat,
    CodexAppServer,
}

impl LlmBackendKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "" | "gemini-headless" | "gemini" | "headless" => Ok(Self::GeminiHeadless),
            "openai-compat" | "openai_compat" => Ok(Self::OpenAiCompat),
            "codex-app-server" | "codex_app_server" | "codex" => Ok(Self::CodexAppServer),
            other => Err(format!(
                "AXON_LLM_BACKEND must be 'gemini-headless', 'openai-compat', or 'codex-app-server' (got '{other}')"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBackendConfig {
    pub kind: LlmBackendKind,
    pub gemini_cmd: String,
    pub gemini_model: Option<String>,
    pub gemini_home: Option<PathBuf>,
    pub openai_base_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_model: Option<String>,
    pub codex_cmd: String,
    pub codex_model: Option<String>,
    pub codex_home: Option<PathBuf>,
    pub completion_concurrency: usize,
    pub completion_timeout_secs: u64,
    pub configured: bool,
}
```

Also add:

```rust
impl LlmBackendConfig {
    #[must_use]
    pub fn completion_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.completion_timeout_secs.max(1))
    }
}
```

- [ ] **Step 4: Add Config fields and defaults**

In `src/core/config/types/config.rs`, add fields next to the existing LLM backend fields:

```rust
/// Codex CLI command for app-server LLM synthesis. Env: `AXON_CODEX_CMD`.
pub codex_cmd: String,

/// Source CODEX_HOME for Codex auth isolation. Env: `AXON_CODEX_HOME`.
pub codex_home: Option<PathBuf>,

/// Codex-specific model override for synthesis. Env: `AXON_SYNTHESIS_CODEX_MODEL` or `AXON_CODEX_MODEL`.
pub codex_model: String,

/// Max concurrent Codex app-server completions. Env: `AXON_CODEX_COMPLETION_CONCURRENCY`.
pub codex_completion_concurrency: usize,
```

In `src/core/config/types/config_impls.rs`, set defaults:

```rust
codex_cmd: "codex".to_string(),
codex_home: None,
codex_model: String::new(),
codex_completion_concurrency: 1,
```

Add these fields to the `Debug` implementation beside the Gemini/OpenAI fields.

- [ ] **Step 5: Wire model selection**

In `configured_model_for_config`, add:

```rust
LlmBackendKind::CodexAppServer => match purpose {
    LlmModelPurpose::Synthesis => non_empty(cfg.codex_model.clone()),
    LlmModelPurpose::Chat => non_empty(cfg.codex_model.clone()),
},
```

In `LlmBackendConfig::from_config`, set:

```rust
codex_cmd: non_empty(cfg.codex_cmd.clone()).unwrap_or_else(|| "codex".to_string()),
codex_model: non_empty(cfg.codex_model.clone()),
codex_home: cfg.codex_home.clone(),
completion_concurrency: match cfg.llm_backend {
    LlmBackendKind::CodexAppServer => cfg.codex_completion_concurrency.clamp(1, tokio::sync::Semaphore::MAX_PERMITS),
    _ => cfg.llm_completion_concurrency.clamp(1, tokio::sync::Semaphore::MAX_PERMITS),
},
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test core::llm --lib -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/core/llm/types.rs src/core/llm/types_tests.rs src/core/config/types/config.rs src/core/config/types/config_impls.rs
git commit -m "feat(llm): add codex app-server backend config"
```

## Task 2: Activate Codex Backend Dispatch

**Files:**
- Modify: `src/core/llm.rs`
- Modify: `src/core/llm/concurrency.rs`
- Modify: `src/core/llm/codex_app_server.rs`
- Test: `src/core/llm/llm_backend_tests.rs`
- Test: `src/core/llm/codex_app_server_tests.rs`

- [ ] **Step 1: Write failing dispatch and limiter tests**

Add tests to `src/core/llm/llm_backend_tests.rs`:

```rust
#[test]
fn limiter_key_distinguishes_codex_command_and_model() {
    let req = CompletionRequest::new("hello").backend_from_config(&Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "/opt/codex/bin/codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    });

    assert_eq!(
        completion_limiter_key(&req),
        CompletionKey::Codex {
            cmd: "/opt/codex/bin/codex".to_string(),
            model: "gpt-5.5".to_string(),
        }
    );
}
```

Keep the existing `src/core/llm/codex_app_server_tests.rs` tests; after this task they must be compiled and run by `cargo test`.

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test codex_app_server limiter_key_distinguishes_codex --lib -- --nocapture
```

Expected: FAIL because the module and limiter key variant are not wired.

- [ ] **Step 3: Add Codex limiter key**

In `src/core/llm/concurrency.rs`, add a variant to `CompletionKey`:

```rust
Codex {
    cmd: String,
    model: String,
},
```

Do not change semaphore behavior; Codex uses the same keyed concurrency limiter as the other backends.

- [ ] **Step 4: Export and dispatch the backend**

In `src/core/llm.rs`, add:

```rust
pub mod codex_app_server;
```

Add match arms:

```rust
LlmBackendKind::CodexAppServer => codex_app_server::complete_text(req).await,
```

and:

```rust
LlmBackendKind::CodexAppServer => codex_app_server::complete_streaming(req, on_delta).await,
```

Add limiter key branch:

```rust
LlmBackendKind::CodexAppServer => CompletionKey::Codex {
    cmd: req.backend.codex_cmd.clone(),
    model: req.model.clone()
        .or_else(|| req.backend.codex_model.clone())
        .unwrap_or_default(),
},
```

- [ ] **Step 5: Adapt `codex_app_server.rs` to final config shape**

If needed, update `src/core/llm/codex_app_server.rs` so these existing references compile:

```rust
let model = req.model.clone().or_else(|| req.backend.codex_model.clone());
let timeout = req.backend.completion_timeout();
let mut command = Command::new(&backend.codex_cmd);
```

Do not weaken `validate_codex_cmd`; keep explicit-path symlink and executable checks.

- [ ] **Step 6: Run focused backend tests**

Run:

```bash
cargo test core::llm codex_app_server --lib -- --nocapture
```

Expected: PASS, and `codex_app_server` tests must be nonzero.

- [ ] **Step 7: Commit**

```bash
git add src/core/llm.rs src/core/llm/concurrency.rs src/core/llm/llm_backend_tests.rs src/core/llm/codex_app_server.rs src/core/llm/codex_app_server_tests.rs
git commit -m "feat(llm): dispatch codex app-server completions"
```

## Task 3: Harden Codex Subprocess Boundary

**Files:**
- Modify: `src/core/llm/codex_app_server.rs`
- Modify: `src/core/llm/codex_app_server/home.rs`
- Modify: `src/core/llm/codex_app_server/protocol.rs`
- Test: `src/core/llm/codex_app_server_tests.rs`
- Test: `src/core/llm/codex_app_server/home_tests.rs`
- Test: `src/core/llm/codex_app_server/protocol_tests.rs`

- [ ] **Step 1: Write failing home-isolation tests**

Add tests to `src/core/llm/codex_app_server/home_tests.rs`:

```rust
#[test]
fn codex_child_env_rehomes_home_and_xdg_dirs_to_isolated_home() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("env");

    apply_codex_env_allowlist(&mut command);
    apply_codex_home_env(&mut command, dir.path());

    let envs: std::collections::BTreeMap<_, _> = command
        .as_std()
        .get_envs()
        .filter_map(|(key, value)| value.map(|v| (key.to_os_string(), v.to_os_string())))
        .collect();

    assert_eq!(envs.get(std::ffi::OsStr::new("HOME")).unwrap(), dir.path());
    assert_eq!(envs.get(std::ffi::OsStr::new("CODEX_HOME")).unwrap(), dir.path());
    assert_eq!(envs.get(std::ffi::OsStr::new("XDG_CONFIG_HOME")).unwrap(), &dir.path().join(".config"));
    assert_eq!(envs.get(std::ffi::OsStr::new("XDG_CACHE_HOME")).unwrap(), &dir.path().join(".cache"));
    assert_eq!(envs.get(std::ffi::OsStr::new("XDG_DATA_HOME")).unwrap(), &dir.path().join(".local/share"));
}

#[cfg(unix)]
#[test]
fn copy_auth_rejects_symlinked_auth_json() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    symlink(outside.path(), source.path().join("auth.json")).unwrap();

    let err = copy_auth(source.path(), dest.path()).unwrap_err();

    assert!(err.to_string().contains("auth.json must not be a symlink"));
}
```

Add a non-symlink auth copy permission test on Unix:

```rust
#[cfg(unix)]
#[test]
fn copy_auth_writes_destination_auth_json_0600() {
    use std::os::unix::fs::PermissionsExt;

    let source = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("auth.json"), "{}").unwrap();

    copy_auth(source.path(), dest.path()).unwrap();

    let mode = std::fs::metadata(dest.path().join("auth.json"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}
```

- [ ] **Step 2: Write failing protocol redaction tests**

Add to `src/core/llm/codex_app_server/protocol_tests.rs`:

```rust
#[test]
fn malformed_protocol_error_is_bounded_and_redacted() {
    let mut state = CodexStreamState::new(None, "prompt", "/tmp", "test");
    let secret = format!("{{ bad json {} }}", "sk-abcdefghijklmnopqrstuvwxyz0123456789");

    let err = state.handle_line(&secret, &mut |_| Ok(())).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("[REDACTED]"));
    assert!(text.len() < 600);
    assert!(!text.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
}

#[test]
fn json_rpc_error_is_summarized_without_raw_payload_echo() {
    let mut state = CodexStreamState::new(None, "prompt", "/tmp", "test");
    let line = r#"{"id":2,"error":{"message":"failed with sk-abcdefghijklmnopqrstuvwxyz0123456789","data":{"prompt":"secret prompt"}}}"#;

    let err = state.handle_line(line, &mut |_| Ok(())).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("[REDACTED]"));
    assert!(!text.contains("secret prompt"));
    assert!(!text.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
}
```

- [ ] **Step 3: Write failing full-timeout test**

Add to `src/core/llm/codex_app_server_tests.rs`:

```rust
#[tokio::test]
async fn codex_completion_timeout_covers_slow_child_before_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("slow-codex");
    std::fs::write(
        &script,
        "#!/bin/sh\nsleep 5\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let req = CompletionRequest::new("hello").backend(LlmBackendConfig {
        kind: LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        completion_timeout_secs: 1,
        configured: true,
        ..LlmBackendConfig::default()
    });

    let started = std::time::Instant::now();
    let err = complete_text(req).await.unwrap_err();

    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert!(err.to_string().contains("timed out"));
}
```

If `CompletionRequest` has no direct `backend` builder, construct the request mutably:

```rust
let mut req = CompletionRequest::new("hello");
req.backend = LlmBackendConfig { /* same fields */ };
```

- [ ] **Step 4: Isolate HOME and XDG dirs**

In `home.rs`, remove `HOME` from `ALLOWED_ENV_KEYS` and add a helper:

```rust
pub(super) fn apply_codex_home_env(command: &mut Command, home: &std::path::Path) {
    command.env("CODEX_HOME", home);
    command.env("HOME", home);
    command.env("XDG_CONFIG_HOME", home.join(".config"));
    command.env("XDG_CACHE_HOME", home.join(".cache"));
    command.env("XDG_DATA_HOME", home.join(".local/share"));
}
```

In `spawn_codex_child`, replace the direct `command.env("CODEX_HOME", home.path())` with:

```rust
home::apply_codex_home_env(&mut command, home.path());
```

- [ ] **Step 5: Harden auth copy**

In `copy_auth`, use `symlink_metadata` on `auth.json`, reject symlinks and non-files, cap file size at 1 MiB, and write destination permissions as `0600` on Unix:

```rust
const MAX_CODEX_AUTH_JSON_BYTES: u64 = 1024 * 1024;

fn copy_auth(source: &std::path::Path, dest: &std::path::Path) -> Result<(), BoxError> {
    let auth = source.join("auth.json");
    let metadata = match fs::symlink_metadata(&auth) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("failed to inspect codex auth.json: {err}").into()),
    };
    if metadata.file_type().is_symlink() {
        return Err("codex auth.json must not be a symlink".into());
    }
    if !metadata.is_file() {
        return Err("codex auth.json must be a regular file".into());
    }
    if metadata.len() > MAX_CODEX_AUTH_JSON_BYTES {
        return Err("codex auth.json is larger than 1 MiB".into());
    }
    let dest_auth = dest.join("auth.json");
    fs::copy(&auth, &dest_auth).map_err(|err| format!("failed to copy codex auth.json: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest_auth, fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("failed to chmod isolated codex auth.json: {err}"))?;
    }
    Ok(())
}
```

Apply the same non-symlink directory validation to fallback `CODEX_HOME` and `$HOME/.codex` candidates by routing them through `validate_source_home` when they exist.

- [ ] **Step 6: Sanitize protocol errors**

In `protocol.rs`, add a small bounded error helper using the existing headless redaction behavior or a local equivalent:

```rust
const PROTOCOL_ERROR_LIMIT: usize = 512;

fn sanitize_protocol_error(text: &str) -> String {
    let mut redacted = crate::core::llm::headless::common::redact_for_error(text);
    if redacted.len() > PROTOCOL_ERROR_LIMIT {
        redacted.truncate(PROTOCOL_ERROR_LIMIT);
        redacted.push_str("...");
    }
    redacted
}
```

If `redact_for_error` is not public, make a narrow `pub(crate)` helper in `headless/common.rs` that reuses the existing `redact_secrets` logic. Use the sanitized helper for malformed-line errors and JSON-RPC errors. For JSON-RPC errors, prefer the `error.message` field and do not print arbitrary `error.data`.

- [ ] **Step 7: Cover setup/spawn in timeout**

Refactor `complete_streaming` so `req.backend.completion_timeout()` wraps the whole operation from home preparation through process cleanup. One acceptable shape:

```rust
pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    let timeout = req.backend.completion_timeout();
    match tokio::time::timeout(timeout, complete_streaming_inner(req, on_delta)).await {
        Ok(result) => result,
        Err(_) => Err(format!("codex app-server timed out after {}s", timeout.as_secs()).into()),
    }
}
```

Move the current body into `complete_streaming_inner`. Keep the existing child cleanup behavior inside the inner function so a timed-out handshake still kills the child; for a timeout before spawn finishes, there may be no child yet.

- [ ] **Step 8: Run hardening tests**

Run:

```bash
cargo test codex_app_server --lib -- --nocapture
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/core/llm/codex_app_server.rs src/core/llm/codex_app_server/home.rs src/core/llm/codex_app_server/protocol.rs src/core/llm/codex_app_server_tests.rs src/core/llm/codex_app_server/home_tests.rs src/core/llm/codex_app_server/protocol_tests.rs src/core/llm/headless/common.rs
git commit -m "fix(llm): harden codex app-server subprocess isolation"
```

## Task 4: Parse Codex Environment Configuration

**Files:**
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Modify: `src/core/config/parse/env_registry/runtime.rs`
- Modify: `src/core/config/parse/env_registry/advanced.rs`
- Test: `src/core/config/parse/build_config/tests/env_required.rs`

- [ ] **Step 1: Write failing env parse test**

Add to `src/core/config/parse/build_config/tests/env_required.rs`:

```rust
#[allow(unsafe_code)]
#[test]
fn into_config_reads_codex_app_server_env_settings() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(
        &[
            "AXON_LLM_BACKEND",
            "AXON_CODEX_CMD",
            "AXON_CODEX_HOME",
            "AXON_SYNTHESIS_CODEX_MODEL",
            "AXON_CODEX_MODEL",
            "AXON_CODEX_COMPLETION_CONCURRENCY",
        ],
        || unsafe {
            env::set_var("AXON_LLM_BACKEND", "codex-app-server");
            env::set_var("AXON_CODEX_CMD", "/opt/codex/bin/codex");
            env::set_var("AXON_CODEX_HOME", "/home/example/.codex");
            env::set_var("AXON_CODEX_MODEL", "legacy-model");
            env::set_var("AXON_SYNTHESIS_CODEX_MODEL", "gpt-5.5");
            env::set_var("AXON_CODEX_COMPLETION_CONCURRENCY", "2");

            let cfg = into_config_via_args(&["status"]).expect("status config");
            let backend = crate::core::llm::LlmBackendConfig::from_config(&cfg);

            assert_eq!(backend.kind, crate::core::llm::LlmBackendKind::CodexAppServer);
            assert_eq!(backend.codex_cmd, "/opt/codex/bin/codex");
            assert_eq!(backend.codex_home.as_deref(), Some(std::path::Path::new("/home/example/.codex")));
            assert_eq!(backend.codex_model.as_deref(), Some("gpt-5.5"));
            assert_eq!(backend.completion_concurrency, 2);
        },
    );
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test into_config_reads_codex_app_server_env_settings --lib -- --nocapture
```

Expected: FAIL because Codex env parsing is missing.

- [ ] **Step 3: Parse Codex env vars**

In `populate_services_and_ask_basics`, after Gemini home parsing, add:

```rust
cfg.codex_cmd = non_empty_env("AXON_CODEX_CMD").unwrap_or_else(|| "codex".to_string());
cfg.codex_home = non_empty_env("AXON_CODEX_HOME").map(std::path::PathBuf::from);
cfg.codex_model = non_empty_env("AXON_SYNTHESIS_CODEX_MODEL")
    .or_else(|| non_empty_env("AXON_CODEX_MODEL"))
    .unwrap_or_default();
cfg.codex_completion_concurrency =
    parse_positive_usize_env("AXON_CODEX_COMPLETION_CONCURRENCY", 1)?;
```

Do not default `AXON_CODEX_HOME` to `HOME`; when unset, `codex_app_server/home.rs` should choose the source-home behavior it already owns.

- [ ] **Step 4: Classify env registry entries**

Move or add these runtime-facing entries to `src/core/config/parse/env_registry/runtime.rs`:

```rust
spec("AXON_CODEX_MODEL", KeepEnv, Both, None, Canonical, false),
spec("AXON_SYNTHESIS_CODEX_MODEL", KeepEnv, Both, None, Canonical, false),
spec("AXON_CODEX_COMPLETION_CONCURRENCY", KeepEnv, Both, None, Canonical, false),
```

Keep `AXON_CODEX_CMD` and `AXON_CODEX_HOME` classified as trusted bootstrap/host-only in `advanced.rs` unless an existing registry test requires a different grouping.

- [ ] **Step 5: Run config tests**

Run:

```bash
cargo test core::config::parse::build_config::tests::env_required --lib -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/core/config/parse/build_config/config_literal.rs src/core/config/parse/env_registry/runtime.rs src/core/config/parse/env_registry/advanced.rs src/core/config/parse/build_config/tests/env_required.rs
git commit -m "feat(config): parse codex app-server llm settings"
```

## Task 5: Preserve Codex Config in Async Jobs and Model Tuning

**Files:**
- Modify: `src/jobs/config_snapshot.rs`
- Modify: `src/core/config/parse/tuning.rs`
- Modify: `src/services/search/synthesis/source.rs`
- Test: `src/jobs/config_snapshot_tests.rs`
- Test: `src/core/config/parse/tuning_tests.rs`
- Test: `src/services/search/synthesis_tests.rs`

- [ ] **Step 1: Write failing config snapshot tests**

Add to `src/jobs/config_snapshot_tests.rs`:

```rust
#[test]
fn config_snapshot_preserves_codex_llm_backend_fields() {
    let mut cfg = Config::default();
    cfg.llm_backend = crate::core::llm::LlmBackendKind::CodexAppServer;
    cfg.codex_cmd = "/opt/codex/bin/codex".to_string();
    cfg.codex_home = Some(PathBuf::from("/home/example/.codex"));
    cfg.codex_model = "gpt-5.5".to_string();
    cfg.codex_completion_concurrency = 2;

    let json = config_snapshot_json(&cfg).expect("snapshot json");
    let mut restored = Config::default();
    apply_config_snapshot_json(&mut restored, &json).expect("apply snapshot");

    assert_eq!(restored.llm_backend, crate::core::llm::LlmBackendKind::CodexAppServer);
    assert_eq!(restored.codex_cmd, "/opt/codex/bin/codex");
    assert_eq!(restored.codex_home, Some(PathBuf::from("/home/example/.codex")));
    assert_eq!(restored.codex_model, "gpt-5.5");
    assert_eq!(restored.codex_completion_concurrency, 2);
}
```

If the existing snapshot test helper names differ, use the local equivalents already used in this file; keep the assertions identical.

- [ ] **Step 2: Write failing model-tier and source-preservation tests**

Add to `src/core/config/parse/tuning_tests.rs`:

```rust
#[test]
fn codex_backend_gets_medium_context_budget() {
    let cfg = Config {
        llm_backend: crate::core::llm::LlmBackendKind::CodexAppServer,
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    assert_eq!(super::model_context_char_budget(&cfg), 400_000);
}
```

Add to `src/services/search/synthesis_tests.rs` or the closest existing source-preservation test module:

```rust
#[test]
fn codex_backend_preserves_full_research_sources() {
    let cfg = Config {
        llm_backend: crate::core::llm::LlmBackendKind::CodexAppServer,
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    assert!(super::source::preserve_full_research_sources(&cfg));
}
```

If `preserve_full_research_sources` is private, make it `pub(super)` for the sibling test path rather than duplicating the logic.

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
cargo test config_snapshot_preserves_codex codex_backend_gets_medium_context_budget codex_backend_preserves_full_research_sources --lib -- --nocapture
```

Expected: FAIL because snapshots and model-tier detection do not include Codex yet.

- [ ] **Step 4: Snapshot Codex fields**

In `src/jobs/config_snapshot.rs`, add fields to `ConfigSnapshot`:

```rust
codex_cmd: Option<String>,
codex_home: Option<PathBuf>,
codex_model: Option<String>,
codex_completion_concurrency: Option<usize>,
```

In `ConfigSnapshot::from_config`, set:

```rust
codex_cmd: Some(cfg.codex_cmd.clone()),
codex_home: cfg.codex_home.clone(),
codex_model: Some(cfg.codex_model.clone()),
codex_completion_concurrency: Some(cfg.codex_completion_concurrency),
```

In the snapshot apply path, restore those fields exactly the same way Gemini/OpenAI fields are restored. In `llm_backend_snapshot`, add:

```rust
crate::core::llm::LlmBackendKind::CodexAppServer => "codex-app-server".to_string(),
```

- [ ] **Step 5: Use configured backend model for tuning**

In `src/core/config/parse/tuning.rs`, replace direct `cfg.openai_model` sniffing with:

```rust
let model = crate::core::llm::configured_model_from_config(cfg)
    .unwrap_or_default()
    .to_ascii_lowercase();
```

Keep `GeminiHeadless` as large even when the model string is empty.

- [ ] **Step 6: Preserve full research sources for Codex/GPT models**

In `src/services/search/synthesis/source.rs`, update `preserve_full_research_sources`:

```rust
matches!(cfg.llm_backend, LlmBackendKind::GeminiHeadless)
    || configured.contains("gemini")
    || configured.contains("opus")
    || configured.contains("codex")
    || configured.starts_with("gpt-")
    || configured.contains("/gpt-")
```

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test config_snapshot_preserves_codex codex_backend_gets_medium_context_budget codex_backend_preserves_full_research_sources --lib -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/jobs/config_snapshot.rs src/jobs/config_snapshot_tests.rs src/core/config/parse/tuning.rs src/core/config/parse/tuning_tests.rs src/services/search/synthesis/source.rs src/services/search/synthesis_tests.rs
git commit -m "feat(llm): preserve codex backend config in async paths"
```

## Task 6: Wire Ask/RAG Validation and Prompt Behavior

**Files:**
- Modify: `src/vector/ops/commands/ask.rs`
- Modify: `src/vector/ops/commands/ask/context.rs`
- Modify: `src/vector/ops/commands/ask/synthesis_prompt.rs`
- Test: `src/vector/ops/commands/ask_tests.rs`
- Test: `src/vector/ops/commands/streaming_tests.rs`
- Test: `src/vector/ops/commands/ask/context_tests.rs`
- Test: `src/vector/ops/commands/ask/synthesis_prompt_tests.rs`

- [ ] **Step 1: Write failing ask validation and prompt tests**

Add to `src/vector/ops/commands/ask_tests.rs`:

```rust
#[test]
fn validate_ask_llm_config_accepts_codex_app_server_config() {
    let cfg = Config {
        llm_backend: llm::LlmBackendKind::CodexAppServer,
        codex_cmd: "codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    validate_ask_llm_config(&cfg).expect("codex config should validate");
}

#[test]
fn validate_ask_llm_config_rejects_empty_codex_cmd() {
    let cfg = Config {
        llm_backend: llm::LlmBackendKind::CodexAppServer,
        codex_cmd: "   ".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    let err = validate_ask_llm_config(&cfg).unwrap_err();
    assert!(err.to_string().contains("AXON_CODEX_CMD"));
}
```

Add to `src/vector/ops/commands/ask/synthesis_prompt_tests.rs`:

```rust
#[test]
fn codex_app_server_uses_direct_synthesis_prompt() {
    let prompt = synthesis_prompt_for_backend(crate::core::llm::LlmBackendKind::CodexAppServer);

    assert!(!prompt.contains("Use the axon-rag-synthesize skill"));
    assert!(prompt.contains("Use the provided context"));
}
```

Add to `src/vector/ops/commands/streaming_tests.rs` near the OpenAI-compatible direct-prompt test:

```rust
#[tokio::test]
async fn ask_llm_streaming_with_runner_uses_direct_prompt_for_codex_app_server() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };
    let runner = MockRunner::with_streaming(&["The answer [S1]."], "The answer [S1].");

    let answer = ask_llm_streaming_with_runner(&runner, &cfg, "How?", "Context block", false)
        .await
        .unwrap();

    assert_eq!(answer, "The answer [S1].");
    let observed = runner.observed.lock().expect("lock poisoned");
    let req = observed.last().expect("request captured");
    assert_eq!(req.backend.kind, LlmBackendKind::CodexAppServer);
    assert!(req.system_prompt.as_deref().unwrap_or("").contains(
        "Every sentence containing factual content must end with one or more source citations."
    ));
    assert!(!req.system_prompt.as_deref().unwrap_or("").contains(
        "Use the axon-rag-synthesize skill"
    ));
}
```

If there is no context sidecar test module, create `src/vector/ops/commands/ask/context_tests.rs` and wire it from `context.rs` with:

```rust
#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
```

Add:

```rust
#[test]
fn high_context_detection_uses_codex_model_name() {
    let cfg = Config {
        llm_backend: crate::core::llm::LlmBackendKind::CodexAppServer,
        codex_model: "gpt-5.5".to_string(),
        ask_max_context_chars: 50_000,
        ..Config::default()
    };

    assert!(super::high_context_synthesis_model(&cfg));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test validate_ask_llm_config_accepts_codex codex_app_server_uses_direct_synthesis_prompt high_context_detection_uses_codex --lib -- --nocapture
```

Expected: FAIL because ask match arms do not include Codex.

- [ ] **Step 3: Validate Codex ask config**

In `validate_ask_llm_config`, add:

```rust
llm::LlmBackendKind::CodexAppServer => {
    llm::codex_app_server::validate_config(&backend).map_err(|e| anyhow::anyhow!("{e}"))
}
```

Do not require a Codex model. The Codex CLI config can provide its default model when `AXON_SYNTHESIS_CODEX_MODEL` is unset.

- [ ] **Step 4: Include Codex model in high-context detection**

In `high_context_synthesis_model`, add:

```rust
let codex_model = cfg.codex_model.to_ascii_lowercase();
let model = match cfg.llm_backend {
    LlmBackendKind::GeminiHeadless => headless_model.as_str(),
    LlmBackendKind::OpenAiCompat => openai_model.as_str(),
    LlmBackendKind::CodexAppServer => codex_model.as_str(),
};
```

Keep the existing high-context predicate.

- [ ] **Step 5: Use direct prompt for Codex**

In `synthesis_prompt_for_backend`, add:

```rust
crate::core::llm::LlmBackendKind::CodexAppServer => synthesis_prompt_for_openai_compat(),
```

- [ ] **Step 6: Run ask tests**

Run:

```bash
cargo test vector::ops::commands::ask --lib -- --nocapture
cargo test ask_llm_streaming_with_runner_uses_direct_prompt_for_codex_app_server --lib -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/vector/ops/commands/ask.rs src/vector/ops/commands/ask/context.rs src/vector/ops/commands/ask/context_tests.rs src/vector/ops/commands/ask/synthesis_prompt.rs src/vector/ops/commands/ask_tests.rs src/vector/ops/commands/streaming_tests.rs src/vector/ops/commands/ask/synthesis_prompt_tests.rs
git commit -m "feat(ask): support codex app-server synthesis backend"
```

## Task 7: Documentation and Examples

**Files:**
- Modify: `.env.example`
- Modify: `config.example.toml`
- Modify: `docs/guides/configuration.md`
- Modify: `docs/reference/env-matrix.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update `.env.example`**

In the LLM section, document:

```dotenv
# Codex app-server backend (spawns `codex app-server` per completion):
# AXON_LLM_BACKEND=codex-app-server
# AXON_CODEX_CMD=codex
# AXON_CODEX_HOME=/home/you/.codex
# AXON_SYNTHESIS_CODEX_MODEL=gpt-5.5
# AXON_CODEX_COMPLETION_CONCURRENCY=1
```

Make clear that `AXON_CODEX_HOME` is optional when the default Codex home has auth, that the backend is not OpenAI-compatible HTTP, and that `OPENAI_API_KEY` is intentionally forwarded only to the isolated Codex child as an optional Codex auth path.

- [ ] **Step 2: Update configuration guide**

In `docs/guides/configuration.md`, change the LLM runtime table so `AXON_LLM_BACKEND` says:

```markdown
Completion backend. Supported: `gemini-headless`, `openai-compat`, `codex-app-server`.
```

Add rows:

```markdown
| `AXON_CODEX_CMD` | `codex` | Codex CLI command used when `AXON_LLM_BACKEND=codex-app-server`. Explicit paths must be executable and non-symlinked. |
| `AXON_CODEX_HOME` | -- | Optional source Codex home used for auth isolation. The backend creates a throwaway runtime home and does not load user hooks, MCP servers, apps, or skills. |
| `AXON_SYNTHESIS_CODEX_MODEL` | -- | Optional synthesis model for Codex app-server. If unset, Codex uses its configured default. Legacy alias: `AXON_CODEX_MODEL`. |
| `AXON_CODEX_COMPLETION_CONCURRENCY` | `1` | Max concurrent Codex app-server completions. Defaults lower than HTTP backends because this backend spawns a child app-server per completion. |
```

- [ ] **Step 3: Update `CLAUDE.md` source of truth**

In `CLAUDE.md`, update the LLM backend section to list `codex-app-server` and state:

```markdown
- **`codex-app-server`** - spawns `codex app-server` through the Codex CLI over stdio with an isolated throwaway `CODEX_HOME` and rehomed `HOME`/XDG directories. Configure with `AXON_CODEX_CMD`, optional `AXON_CODEX_HOME`, optional `AXON_SYNTHESIS_CODEX_MODEL`, and `AXON_CODEX_COMPLETION_CONCURRENCY` (default `1`). This is not an OpenAI-compatible HTTP endpoint and does not connect to the desktop Unix socket in this slice.
```

Do not edit `AGENTS.md` or `GEMINI.md` directly; they should remain symlinks to `CLAUDE.md`.

- [ ] **Step 4: Update env matrix and TOML comments**

In `docs/reference/env-matrix.md`, ensure Codex env vars are listed in the LLM/runtime area with accurate source files. If `OPENAI_API_KEY` remains in the Codex child allowlist, update its row so it is no longer described as external/test-only for the Codex backend.

In `config.example.toml`, add comments explaining that Codex command/home remain env-only because they are runtime/bootstrap values; do not add `[llm]` TOML keys for Codex command/home.

- [ ] **Step 5: Run docs consistency checks**

Run:

```bash
test -L AGENTS.md
test -L GEMINI.md
rg -n "codex-app-server|AXON_CODEX|AXON_LLM_BACKEND" .env.example config.example.toml docs/guides/configuration.md docs/reference/env-matrix.md CLAUDE.md
```

Expected: both symlink checks pass, and every documented backend list includes Codex.

- [ ] **Step 6: Commit**

```bash
git add .env.example config.example.toml docs/guides/configuration.md docs/reference/env-matrix.md CLAUDE.md
git commit -m "docs(llm): document codex app-server backend"
```

## Task 8: Verification and Optional Live Smoke

**Files:**
- Modify docs only if verification reveals behavior differs from this plan.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --check
```

Expected: PASS.

- [ ] **Step 2: Run focused tests**

Run:

```bash
cargo test core::llm --lib -- --nocapture
cargo test core::config::parse::build_config::tests::env_required --lib -- --nocapture
cargo test config_snapshot_preserves_codex codex_backend_gets_medium_context_budget codex_backend_preserves_full_research_sources --lib -- --nocapture
cargo test vector::ops::commands::ask --lib -- --nocapture
cargo test ask_llm_streaming_with_runner_uses_direct_prompt_for_codex_app_server --lib -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Run full compile/test gate**

Run:

```bash
cargo check --all-targets
cargo test
```

Expected: PASS.

- [ ] **Step 4: Run optional live Codex smoke when auth is available**

Only run this if `codex --version` succeeds and the machine has valid Codex auth:

```bash
AXON_LLM_BACKEND=codex-app-server \
AXON_CODEX_CMD=codex \
AXON_SYNTHESIS_CODEX_MODEL=gpt-5.5 \
./scripts/axon summarize https://example.com --json
```

Expected: command exits 0 and returns a JSON summary. If it fails due auth/quota/model availability, record the exact error and do not block the code merge if all unit/integration gates pass.

- [ ] **Step 5: Confirm orphaned provider code remains intentionally out of scope**

Run:

```bash
rg -n "mod provider_overlay|mod provider;" src/core/config/parse/build_config.rs src/cli/commands/config.rs
```

Expected: no active module declarations for the orphaned provider overlay or config-provider CLI. Do not wire provider profiles as part of this task unless earlier tasks accidentally made them compile and fail, and if that happens, make the smallest compile fix without adding provider UX.

- [ ] **Step 6: Commit verification-only docs fix if needed**

If verification required docs changes:

```bash
git add docs/guides/configuration.md docs/reference/env-matrix.md CLAUDE.md .env.example config.example.toml
git commit -m "docs(llm): align codex verification notes"
```

If no changes were required, do not make an empty commit.

## Deferred Follow-Up Beads

- Desktop socket transport for connecting to the already-running Codex desktop app-server Unix socket instead of spawning a child `codex app-server`.
- Provider profiles and `axon config provider` UX, including `provider_overlay.rs` and `src/cli/commands/config/provider.rs`.
- A gated live integration test for Codex app-server using `AXON_TEST_CODEX_APP_SERVER=1`.
- Shared app-server protocol conformance tests generated from `codex app-server generate-json-schema`.

## Self-Review

Spec coverage:
- The existing orphaned Codex backend is activated by Tasks 1 and 2.
- Config/env parsing is covered by Task 4.
- Async job replay and model tuning are covered by Task 5.
- Ask/RAG synthesis behavior is covered by Task 6.
- Documentation and source-of-truth memory are covered by Task 7.
- Compile, test, and live-smoke verification are covered by Task 8.

Placeholder scan:
- The plan intentionally contains no placeholder strings. Deferred work is listed as explicit follow-up scope, not implementation gaps.

Type consistency:
- `CodexAppServer`, `codex_cmd`, `codex_model`, `codex_home`, and `codex_completion_concurrency` are used consistently across `Config`, `LlmBackendConfig`, parser tests, snapshot tests, and ask/RAG tests.

Risk controls:
- Codex command validation preserves explicit-path symlink/executable checks.
- The isolated `CODEX_HOME` behavior remains owned by the existing `codex_app_server/home.rs`.
- Codex uses the direct synthesis prompt because the isolated home intentionally does not expose skills.
- Provider profiles and desktop socket transport are deferred so the first backend slice is small, reviewable, and shippable.
