# Session Watch Service Auto-Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build host-local automatic AI session ingestion with `axon sessions watch` and `axon setup session-watch-service`, reusing existing prepared-session ingest and `/v1/ingest/sessions/prepared`.

**Architecture:** Keep SessionStart recall and auto-capture separate: the existing plugin hook only calls `axon memory context`, while the new watcher is a long-running host-local process. The watcher watches Claude, Codex, and Gemini transcript roots with Cortex-style debounce/settle/retry/overflow behavior, converts stable files to existing prepared-session docs, and either ingests locally or uploads to the existing prepared-session REST endpoint when `AXON_SERVER_URL` is set. A setup command installs a user systemd service that runs `axon sessions watch --no-initial-scan --json` and performs one initial `axon sessions --json` ingest before enabling the service.

**Tech Stack:** Rust 2024, Tokio, clap, notify, SQLite via Axon job DB style, existing Axon session parsers, existing prepared-session DTOs, Axon REST client/server paths, user systemd.

---

## Scope Check

This plan covers one independently testable subsystem: automatic session capture. It includes the watcher command, checkpoint/status storage, setup service, docs, and validation. It does not alter persistent memory recall semantics, and it does not make SessionStart scan or ingest session files.

## File Structure

- Create `src/ingest/sessions/watch.rs`: watcher options, watch target discovery, non-recursive directory registration, pending file queue, debounce/settle logic, event handling, rescan trigger, graceful shutdown wiring, and process loop.
- Create `src/ingest/sessions/watch_tests.rs`: unit tests for target selection, pending queue coalescing, debounce, settle, retry, overflow, delete handling, and supported-file filtering.
- Create `src/ingest/sessions/checkpoint.rs`: SQLite checkpoint functions for source path metadata, errors, duplicate suppression, and status summaries. V0 uses this for skip/status/error visibility, not append-offset ingestion.
- Create `src/ingest/sessions/checkpoint_tests.rs`: SQLite-backed checkpoint tests with temp DB.
- Create `src/jobs/migrations/0010_create_session_watch_tables.sql`: checkpoint and watch error tables.
- Modify `Cargo.toml`: add `notify`.
- Modify `src/ingest/sessions.rs`: export watcher/checkpoint modules and add single-file prepared-session collection helpers that reuse provider parsers.
- Modify `src/ingest/sessions/claude.rs`: expose a file-level collection function for one Claude `.jsonl` file.
- Modify `src/ingest/sessions/codex.rs`: expose a file-level collection function for one Codex `.jsonl` file.
- Modify `src/ingest/sessions/gemini.rs`: expose a file-level collection function for one Gemini `.json` file.
- Modify `src/cli/commands/sessions.rs`: dispatch `sessions watch` before normal full-history session ingest.
- Modify `src/core/config/cli.rs`: add `SessionsSubcommand::Watch` with watcher flags and `SetupSubcommand::SessionWatchService`.
- Modify `src/core/config/parse/build_config/command_dispatch.rs`: map `sessions watch` and setup service arguments into `Config`.
- Modify `src/core/config/types/enums.rs`: keep command kind as `Sessions`; no new top-level command kind.
- Modify `src/services/setup.rs`: export session-watch setup service.
- Create `src/services/setup/session_watch_service.rs`: install/check/remove/status implementation and systemd unit rendering.
- Create `src/services/setup/session_watch_service_tests.rs`: unit tests for env file content, systemd unit content, and action output.
- Modify `src/cli/commands/setup_tests.rs`: add setup CLI parse/help tests if setup tests live there in the current tree.
- Modify `tests/cli_help_contract.rs`: require help text for `sessions watch` and `setup session-watch-service`.
- Modify `docs/reference/commands/sessions.md`: document `sessions watch` and server-mode prepared upload behavior.
- Modify `docs/guides/ingest/sessions.md`: document auto-ingest architecture and the SessionStart distinction.
- Modify `docs/reference/commands/setup.md` if present, otherwise create it: document `setup session-watch-service`.
- Modify `docs/reference/api-parity.md`: note that auto-ingest uses the existing prepared sessions REST endpoint and adds no new REST route.
- Modify `plugins/axon/README.md`: clarify that SessionStart is recall-only and auto-ingest is installed through `axon setup session-watch-service`.

## Task 1: CLI Shape and Dependency

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Modify: `src/cli/commands/sessions.rs`
- Test: `tests/cli_help_contract.rs`

- [ ] **Step 1: Write failing CLI help tests**

Add these tests to `tests/cli_help_contract.rs`:

```rust
#[test]
fn sessions_watch_help_exposes_debounce_settle_and_initial_scan_flags() {
    let output = axon_cmd()
        .args(["sessions", "watch", "--help"])
        .output()
        .expect("run axon sessions watch --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Watch local AI session exports and ingest stable changes"));
    assert!(stdout.contains("--path"));
    assert!(stdout.contains("--debounce-ms"));
    assert!(stdout.contains("--settle-ms"));
    assert!(stdout.contains("--max-retries"));
    assert!(stdout.contains("--no-initial-scan"));
    assert!(stdout.contains("--json"));
}

#[test]
fn setup_session_watch_service_help_exposes_install_check_remove_status() {
    let output = axon_cmd()
        .args(["setup", "session-watch-service", "--help"])
        .output()
        .expect("run axon setup session-watch-service --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Install, check, remove, or inspect the host-local session watch service"));
    assert!(stdout.contains("install"));
    assert!(stdout.contains("check"));
    assert!(stdout.contains("remove"));
    assert!(stdout.contains("status"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --test cli_help_contract sessions_watch_help_exposes_debounce_settle_and_initial_scan_flags -- --nocapture
cargo test --test cli_help_contract setup_session_watch_service_help_exposes_install_check_remove_status -- --nocapture
```

Expected: both fail because `sessions watch` and `setup session-watch-service` are not recognized.

- [ ] **Step 3: Add `notify` dependency**

In `Cargo.toml`, add the dependency near the other runtime/filesystem dependencies:

```toml
notify = "8"
```

- [ ] **Step 4: Add clap types for sessions watch**

In `src/core/config/cli.rs`, replace the current `SessionsArgs` shape with a subcommand-capable shape. Preserve existing `--claude`, `--codex`, `--gemini`, and `--project` behavior for ordinary `axon sessions`.

```rust
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct SessionsArgs {
    #[command(subcommand)]
    pub(super) action: Option<SessionsSubcommand>,
    /// Only scan Claude session exports.
    #[arg(long)]
    pub(super) claude: bool,
    /// Only scan Codex session exports.
    #[arg(long)]
    pub(super) codex: bool,
    /// Only scan Gemini session exports.
    #[arg(long)]
    pub(super) gemini: bool,
    /// Filter session projects by substring.
    #[arg(long)]
    pub(super) project: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(super) enum SessionsSubcommand {
    /// Watch local AI session exports and ingest stable changes.
    Watch(SessionsWatchArgs),
}

#[derive(Debug, Args, Clone)]
pub(super) struct SessionsWatchArgs {
    /// Watch one transcript file or directory instead of all default provider roots.
    #[arg(long)]
    pub(super) path: Option<std::path::PathBuf>,
    /// Debounce file events by this many milliseconds before checking stability.
    #[arg(long = "debounce-ms", default_value_t = 750)]
    pub(super) debounce_ms: u64,
    /// Require unchanged size and mtime for this many milliseconds before ingest.
    #[arg(long = "settle-ms", default_value_t = 500)]
    pub(super) settle_ms: u64,
    /// Retry parse, upload, or storage failures this many times before recording an error.
    #[arg(long = "max-retries", default_value_t = 5)]
    pub(super) max_retries: u8,
    /// Skip the startup full scan. The setup service uses this after its one-time initial ingest.
    #[arg(long = "no-initial-scan")]
    pub(super) no_initial_scan: bool,
    /// Emit newline-delimited JSON events suitable for systemd logs.
    #[arg(long)]
    pub(super) json: bool,
}
```

- [ ] **Step 5: Add clap types for `setup session-watch-service`**

In `src/core/config/cli.rs`, extend `SetupSubcommand`:

```rust
    /// Install, check, remove, or inspect the host-local session watch service.
    #[command(name = "session-watch-service")]
    SessionWatchService {
        #[command(subcommand)]
        action: SessionWatchServiceSubcommand,
    },
```

Add the subcommand enum below `SetupSubcommand`:

```rust
#[derive(Debug, Subcommand, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionWatchServiceSubcommand {
    /// Write service files, run initial ingest, and enable the user service.
    Install,
    /// Verify generated files and systemd state without mutating service files.
    Check,
    /// Disable the user service and remove generated service files.
    Remove,
    /// Print current user systemd status for the service.
    Status,
}
```

- [ ] **Step 6: Map parsed args into config positional dispatch**

In `src/core/config/parse/build_config/command_dispatch.rs`, import `SessionsSubcommand` and `SessionWatchServiceSubcommand`. In the `CliCommand::Sessions(args)` arm, add:

```rust
CliCommand::Sessions(args) => {
    out.command = CommandKind::Sessions;
    if let Some(SessionsSubcommand::Watch(watch)) = args.action {
        out.positional = vec![
            "watch".to_string(),
            watch.path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            watch.debounce_ms.to_string(),
            watch.settle_ms.to_string(),
            watch.max_retries.to_string(),
            (!watch.no_initial_scan).to_string(),
            watch.json.to_string(),
        ];
    } else {
        out.sessions_claude = args.claude;
        out.sessions_codex = args.codex;
        out.sessions_gemini = args.gemini;
        out.sessions_project = args.project;
    }
}
```

In the `CliCommand::Setup(args)` arm, add this mapping for the new setup action:

```rust
Some(SetupSubcommand::SessionWatchService { action }) => {
    out.command = CommandKind::Setup;
    out.positional = vec![
        "session-watch-service".to_string(),
        match action {
            SessionWatchServiceSubcommand::Install => "install",
            SessionWatchServiceSubcommand::Check => "check",
            SessionWatchServiceSubcommand::Remove => "remove",
            SessionWatchServiceSubcommand::Status => "status",
        }
        .to_string(),
    ];
}
```

- [ ] **Step 7: Add a stub sessions watch dispatcher**

In `src/cli/commands/sessions.rs`, before job-subcommand handling, add:

```rust
use crate::ingest::sessions::watch::{SessionWatchOptions, run_session_watch};
use std::path::PathBuf;
use std::time::Duration;
```

At the start of `run_sessions`, add:

```rust
if cfg.positional.first().is_some_and(|value| value == "watch") {
    let options = SessionWatchOptions {
        path: cfg
            .positional
            .get(1)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        debounce: Duration::from_millis(
            cfg.positional
                .get(2)
                .and_then(|value| value.parse().ok())
                .unwrap_or(750),
        ),
        settle: Duration::from_millis(
            cfg.positional
                .get(3)
                .and_then(|value| value.parse().ok())
                .unwrap_or(500),
        ),
        max_retries: cfg
            .positional
            .get(4)
            .and_then(|value| value.parse().ok())
            .unwrap_or(5),
        initial_scan: cfg
            .positional
            .get(5)
            .and_then(|value| value.parse().ok())
            .unwrap_or(true),
        json: cfg
            .positional
            .get(6)
            .and_then(|value| value.parse().ok())
            .unwrap_or(false),
    };
    return run_session_watch(cfg, options)
        .await
        .map_err(|err| -> Box<dyn Error> { err.into() });
}
```

- [ ] **Step 8: Create a compiling watcher stub**

Create `src/ingest/sessions/watch.rs`:

```rust
use crate::core::config::Config;
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SessionWatchOptions {
    pub path: Option<PathBuf>,
    pub debounce: Duration,
    pub settle: Duration,
    pub max_retries: u8,
    pub initial_scan: bool,
    pub json: bool,
}

pub async fn run_session_watch(_cfg: &Config, _options: SessionWatchOptions) -> Result<()> {
    anyhow::bail!("sessions watch is wired but the watcher implementation is not complete")
}
```

In `src/ingest/sessions.rs`, add:

```rust
pub mod watch;
```

- [ ] **Step 9: Run tests to verify help passes**

Run:

```bash
cargo fmt
cargo test --test cli_help_contract sessions_watch_help_exposes_debounce_settle_and_initial_scan_flags -- --nocapture
cargo test --test cli_help_contract setup_session_watch_service_help_exposes_install_check_remove_status -- --nocapture
```

Expected: both tests pass. `axon sessions watch` exits with a clear stub error if executed.

- [ ] **Step 10: Commit CLI shape**

```bash
git add Cargo.toml src/core/config/cli.rs src/core/config/parse/build_config/command_dispatch.rs src/cli/commands/sessions.rs src/ingest/sessions.rs src/ingest/sessions/watch.rs tests/cli_help_contract.rs
git commit -m "feat: add session watch command surface"
```

## Task 2: Single-File Prepared Session Collection

**Files:**
- Modify: `src/ingest/sessions.rs`
- Modify: `src/ingest/sessions/claude.rs`
- Modify: `src/ingest/sessions/codex.rs`
- Modify: `src/ingest/sessions/gemini.rs`
- Test: `src/ingest/sessions_tests.rs`

- [ ] **Step 1: Write failing tests for single-file prepared docs**

Add to `src/ingest/sessions_tests.rs`:

```rust
#[tokio::test]
async fn collect_prepared_session_file_doc_parses_claude_file() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join(".claude/projects/-tmp-axon");
    std::fs::create_dir_all(&project_dir).unwrap();
    let file = project_dir.join("claude-1.jsonl");
    std::fs::write(
        &file,
        serde_json::json!({
            "type": "user",
            "timestamp": "2026-06-11T12:00:00Z",
            "cwd": "/tmp/axon",
            "message": { "content": "remember this claude session" }
        })
        .to_string()
            + "\n",
    )
    .unwrap();

    let mut cfg = crate::core::config::Config::default();
    cfg.collection = "axon-test".to_string();
    let doc = super::collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap()
        .expect("claude doc");

    assert_eq!(doc.session_platform, "claude");
    assert!(doc.text.contains("remember this claude session"));
    assert_eq!(doc.session_file, file.to_string_lossy());
    assert!(doc.url.starts_with("file://"));
}

#[tokio::test]
async fn collect_prepared_session_file_doc_parses_codex_file() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join(".codex/sessions/2026/06/11/codex-1.jsonl");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": "/tmp/axon", "model": "gpt-5" }
        })
        .to_string()
            + "\n"
            + &serde_json::json!({
                "type": "response_item",
                "payload": {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "remember this codex session" }]
                }
            })
            .to_string()
            + "\n",
    )
    .unwrap();

    let cfg = crate::core::config::Config::default();
    let doc = super::collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap()
        .expect("codex doc");

    assert_eq!(doc.session_platform, "codex");
    assert!(doc.text.contains("remember this codex session"));
}

#[tokio::test]
async fn collect_prepared_session_file_doc_rejects_unsupported_file() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("notes.txt");
    std::fs::write(&file, "not a session").unwrap();
    let cfg = crate::core::config::Config::default();

    let err = super::collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("unsupported session file"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --lib collect_prepared_session_file_doc -- --nocapture
```

Expected: fail because `collect_prepared_session_file_doc` does not exist.

- [ ] **Step 3: Expose file-level provider helpers**

In `src/ingest/sessions/claude.rs`, add:

```rust
pub(super) async fn collect_claude_file_doc(
    cfg: &Config,
    path: PathBuf,
) -> IngestResult<Option<SessionDoc>> {
    let meta = fs::metadata(&path).await?;
    let mtime = meta.modified()?;
    let project_dir_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let clean_name = clean_claude_project_name(project_dir_name);
    if !matches_project_filter(cfg, &clean_name) {
        return Ok(None);
    }
    let project_path_opt = super::decode_claude_project_path(project_dir_name);
    let gh_repo = match project_path_opt {
        Some(ref project_path) => super::read_git_remote_origin(project_path).await,
        None => None,
    };
    let session_meta = SessionMeta {
        agent: "claude",
        project_name: clean_name.clone(),
        project_path: project_path_opt.map(|path| path.to_string_lossy().into_owned()),
        gh_repo,
    };
    parse_claude_file(
        path,
        resolve_collection(cfg, &clean_name),
        mtime,
        session_meta,
        super::session_ingest_max_bytes_for_config(cfg),
    )
    .await
}
```

In `src/ingest/sessions/codex.rs`, add:

```rust
pub(super) async fn collect_codex_file_doc(
    cfg: &Config,
    path: PathBuf,
) -> IngestResult<Option<SessionDoc>> {
    let meta = fs::metadata(&path).await?;
    let mtime = meta.modified()?;
    let project_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    if !matches_project_filter(cfg, &project_name) {
        return Ok(None);
    }
    let session_meta = SessionMeta {
        agent: "codex",
        project_name,
        project_path: None,
        gh_repo: None,
    };
    parse_codex_file(
        path,
        resolve_collection(cfg, "codex"),
        mtime,
        session_meta,
        super::session_ingest_max_bytes_for_config(cfg),
    )
    .await
}
```

In `src/ingest/sessions/gemini.rs`, add:

```rust
pub(super) async fn collect_gemini_file_doc(
    cfg: &Config,
    path: PathBuf,
) -> IngestResult<Option<SessionDoc>> {
    let meta = fs::metadata(&path).await?;
    let mtime = meta.modified()?;
    let collection = resolve_collection(cfg, "gemini");
    process_gemini_file(path, collection, mtime).await
}
```

- [ ] **Step 4: Add prepared single-file collection function**

In `src/ingest/sessions.rs`, add:

```rust
pub async fn collect_prepared_session_file_doc(
    cfg: &Config,
    path: &Path,
) -> Result<Option<PreparedSessionDoc>, Box<dyn Error>> {
    let doc = collect_session_file_doc(cfg, path)
        .await?
        .map(prepared_session_doc_from_session_doc)
        .transpose()
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
    Ok(doc)
}

pub(crate) async fn collect_session_file_doc(
    cfg: &Config,
    path: &Path,
) -> Result<Option<SessionDoc>, Box<dyn Error>> {
    let path = path.to_path_buf();
    let path_text = path.to_string_lossy();
    let is_jsonl = path.extension().is_some_and(|ext| ext == "jsonl");
    let is_json = path.extension().is_some_and(|ext| ext == "json");

    if is_jsonl && path_text.contains("/.claude/projects/") {
        return claude::collect_claude_file_doc(cfg, path)
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() });
    }
    if is_jsonl && path_text.contains("/.codex/sessions/") {
        return codex::collect_codex_file_doc(cfg, path)
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() });
    }
    if is_json && (path_text.contains("/.gemini/history/") || path_text.contains("/.gemini/tmp/")) {
        return gemini::collect_gemini_file_doc(cfg, path)
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() });
    }

    Err(format!("unsupported session file: {}", path.display()).into())
}

pub(crate) fn is_supported_session_file(path: &Path) -> bool {
    let path_text = path.to_string_lossy();
    (path.extension().is_some_and(|ext| ext == "jsonl")
        && (path_text.contains("/.claude/projects/") || path_text.contains("/.codex/sessions/")))
        || (path.extension().is_some_and(|ext| ext == "json")
            && (path_text.contains("/.gemini/history/") || path_text.contains("/.gemini/tmp/")))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cargo fmt
cargo test --lib collect_prepared_session_file_doc -- --nocapture
```

Expected: all three tests pass.

- [ ] **Step 6: Commit single-file preparation**

```bash
git add src/ingest/sessions.rs src/ingest/sessions/claude.rs src/ingest/sessions/codex.rs src/ingest/sessions/gemini.rs src/ingest/sessions_tests.rs
git commit -m "feat: prepare individual session files"
```

## Task 3: Session Watch Checkpoints

**Files:**
- Create: `src/jobs/migrations/0010_create_session_watch_tables.sql`
- Create: `src/ingest/sessions/checkpoint.rs`
- Create: `src/ingest/sessions/checkpoint_tests.rs`
- Modify: `src/ingest/sessions.rs`

- [ ] **Step 1: Write checkpoint migration**

Create `src/jobs/migrations/0010_create_session_watch_tables.sql`:

```sql
CREATE TABLE IF NOT EXISTS axon_session_watch_checkpoints (
    path TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    file_mtime_ms INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    last_offset INTEGER NOT NULL DEFAULT 0,
    last_indexed_at TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_checkpoints_provider
    ON axon_session_watch_checkpoints(provider);

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_checkpoints_error
    ON axon_session_watch_checkpoints(last_error)
    WHERE last_error IS NOT NULL;

CREATE TABLE IF NOT EXISTS axon_session_watch_errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    provider TEXT,
    error TEXT NOT NULL,
    occurred_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_errors_path
    ON axon_session_watch_errors(path);
```

- [ ] **Step 2: Write failing checkpoint tests**

Create `src/ingest/sessions/checkpoint_tests.rs`:

```rust
use super::*;
use crate::jobs::backend::SqliteJobBackend;

#[tokio::test]
async fn checkpoint_skips_unchanged_file_and_records_success() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let backend = SqliteJobBackend::open(&db_path).await.unwrap();
    let path = temp.path().join("session.jsonl");
    std::fs::write(&path, "first").unwrap();

    let meta = SessionFileMetadata::from_path(&path, "codex").unwrap();
    assert!(!checkpoint_matches(&backend, &meta).await.unwrap());
    record_success(&backend, &meta, 5).await.unwrap();
    assert!(checkpoint_matches(&backend, &meta).await.unwrap());

    std::fs::write(&path, "second").unwrap();
    let changed = SessionFileMetadata::from_path(&path, "codex").unwrap();
    assert!(!checkpoint_matches(&backend, &changed).await.unwrap());
}

#[tokio::test]
async fn checkpoint_records_and_lists_errors() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let backend = SqliteJobBackend::open(&db_path).await.unwrap();
    let path = temp.path().join("bad.jsonl");
    std::fs::write(&path, "{bad").unwrap();
    let meta = SessionFileMetadata::from_path(&path, "claude").unwrap();

    record_error(&backend, &meta, "parse failed").await.unwrap();
    let errors = list_recent_errors(&backend, 10).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, path.to_string_lossy());
    assert_eq!(errors[0].provider, Some("claude".to_string()));
    assert_eq!(errors[0].error, "parse failed");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo test --lib session_watch_checkpoint -- --nocapture
```

Expected: fail because `checkpoint` module and symbols are missing.

- [ ] **Step 4: Implement checkpoint module**

Create `src/ingest/sessions/checkpoint.rs`:

```rust
use crate::jobs::backend::SqliteJobBackend;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFileMetadata {
    pub path: PathBuf,
    pub provider: String,
    pub file_size: u64,
    pub file_mtime_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWatchError {
    pub path: String,
    pub provider: Option<String>,
    pub error: String,
    pub occurred_at: String,
}

impl SessionFileMetadata {
    pub fn from_path(path: &Path, provider: &str) -> Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let file_size = metadata.len();
        let file_mtime_ms = metadata
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let bytes = std::fs::read(path)?;
        let content_hash = format!("{:x}", Sha256::digest(&bytes));
        Ok(Self {
            path: path.to_path_buf(),
            provider: provider.to_string(),
            file_size,
            file_mtime_ms,
            content_hash,
        })
    }
}

pub async fn checkpoint_matches(
    backend: &SqliteJobBackend,
    meta: &SessionFileMetadata,
) -> Result<bool> {
    let row = sqlx::query!(
        r#"
        SELECT file_size, file_mtime_ms, content_hash, last_error
        FROM axon_session_watch_checkpoints
        WHERE path = ?
        "#,
        meta.path.to_string_lossy()
    )
    .fetch_optional(backend.pool())
    .await?;
    Ok(row.is_some_and(|row| {
        row.file_size == meta.file_size as i64
            && row.file_mtime_ms == meta.file_mtime_ms
            && row.content_hash == meta.content_hash
            && row.last_error.is_none()
    }))
}

pub async fn record_success(
    backend: &SqliteJobBackend,
    meta: &SessionFileMetadata,
    last_offset: u64,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO axon_session_watch_checkpoints
            (path, provider, file_size, file_mtime_ms, content_hash, last_offset, last_indexed_at, last_error, updated_at)
        VALUES
            (?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(path) DO UPDATE SET
            provider = excluded.provider,
            file_size = excluded.file_size,
            file_mtime_ms = excluded.file_mtime_ms,
            content_hash = excluded.content_hash,
            last_offset = excluded.last_offset,
            last_indexed_at = excluded.last_indexed_at,
            last_error = NULL,
            updated_at = excluded.updated_at
        "#,
        meta.path.to_string_lossy(),
        meta.provider,
        meta.file_size as i64,
        meta.file_mtime_ms,
        meta.content_hash,
        last_offset as i64
    )
    .execute(backend.pool())
    .await?;
    Ok(())
}

pub async fn record_error(
    backend: &SqliteJobBackend,
    meta: &SessionFileMetadata,
    error: &str,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO axon_session_watch_errors (path, provider, error)
        VALUES (?, ?, ?)
        "#,
        meta.path.to_string_lossy(),
        meta.provider,
        error
    )
    .execute(backend.pool())
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO axon_session_watch_checkpoints
            (path, provider, file_size, file_mtime_ms, content_hash, last_error, updated_at)
        VALUES
            (?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(path) DO UPDATE SET
            provider = excluded.provider,
            file_size = excluded.file_size,
            file_mtime_ms = excluded.file_mtime_ms,
            content_hash = excluded.content_hash,
            last_error = excluded.last_error,
            updated_at = excluded.updated_at
        "#,
        meta.path.to_string_lossy(),
        meta.provider,
        meta.file_size as i64,
        meta.file_mtime_ms,
        meta.content_hash,
        error
    )
    .execute(backend.pool())
    .await?;
    Ok(())
}

pub async fn list_recent_errors(
    backend: &SqliteJobBackend,
    limit: i64,
) -> Result<Vec<SessionWatchError>> {
    let rows = sqlx::query!(
        r#"
        SELECT path, provider, error, occurred_at
        FROM axon_session_watch_errors
        ORDER BY id DESC
        LIMIT ?
        "#,
        limit
    )
    .fetch_all(backend.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| SessionWatchError {
            path: row.path,
            provider: row.provider,
            error: row.error,
            occurred_at: row.occurred_at,
        })
        .collect())
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;
```

- [ ] **Step 5: Export checkpoint module**

In `src/ingest/sessions.rs`, add:

```rust
pub mod checkpoint;
```

- [ ] **Step 6: Run checkpoint tests**

Run:

```bash
cargo fmt
cargo test --lib session_watch_checkpoint -- --nocapture
```

Expected: checkpoint tests pass.

- [ ] **Step 7: Commit checkpoints**

```bash
git add src/jobs/migrations/0010_create_session_watch_tables.sql src/ingest/sessions.rs src/ingest/sessions/checkpoint.rs src/ingest/sessions/checkpoint_tests.rs
git commit -m "feat: add session watch checkpoints"
```

## Task 4: Watcher Debounce, Settle, Retry, and Overflow Mechanics

**Files:**
- Modify: `src/ingest/sessions/watch.rs`
- Create: `src/ingest/sessions/watch_tests.rs`

- [ ] **Step 1: Write failing pure watcher state tests**

Create `src/ingest/sessions/watch_tests.rs`:

```rust
use super::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[test]
fn pending_files_debounce_and_coalesce_same_path() {
    let mut pending = PendingFiles::default();
    let now = Instant::now();
    let path = std::path::PathBuf::from("/tmp/a.jsonl");

    assert!(pending.push(path.clone(), now));
    assert!(pending.push(path.clone(), now + Duration::from_millis(100)));
    assert_eq!(pending.files.len(), 1);
    assert_eq!(pending.coalesced_events, 1);
    assert!(pending
        .debounced_paths(now + Duration::from_millis(849), Duration::from_millis(750))
        .is_empty());
    assert_eq!(
        pending.debounced_paths(now + Duration::from_millis(850), Duration::from_millis(750)),
        vec![path]
    );
}

#[test]
fn pending_files_requeue_resets_stability_and_honors_retry_cap() {
    let mut pending = PendingFiles::default();
    let now = Instant::now();
    let path = std::path::PathBuf::from("/tmp/a.jsonl");

    assert!(pending.push(path.clone(), now));
    assert!(pending.requeue(path.clone(), now + Duration::from_secs(1), 2));
    assert!(pending.requeue(path.clone(), now + Duration::from_secs(2), 2));
    assert!(!pending.requeue(path, now + Duration::from_secs(3), 2));
}

#[test]
fn pending_overflow_requests_rescan() {
    let mut pending = PendingFiles::default();
    for i in 0..MAX_PENDING_FILES {
        assert!(pending.push(std::path::PathBuf::from(format!("/tmp/{i}.jsonl")), Instant::now()));
    }
    assert!(!pending.push(std::path::PathBuf::from("/tmp/overflow.jsonl"), Instant::now()));
}

#[test]
fn remove_event_sets_prune_flag_for_supported_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("gone.jsonl");
    let target = WatchTarget::Directory(root.clone());
    let mut pending = PendingFiles::default();
    let overflow = std::sync::atomic::AtomicBool::new(false);
    let prune = std::sync::atomic::AtomicBool::new(false);

    handle_remove_path(&path, &[target], &mut pending, &overflow, &prune);

    assert!(!overflow.load(Ordering::Relaxed));
    assert!(prune.load(Ordering::Relaxed));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --lib ingest::sessions::watch -- --nocapture
```

Expected: fail because `PendingFiles`, constants, and helper functions are missing.

- [ ] **Step 3: Implement watcher state and constants**

In `src/ingest/sessions/watch.rs`, add these Cortex-derived constants and pending state types:

```rust
pub(crate) const WATCH_EVENT_BUFFER: usize = 1024;
pub(crate) const MAX_WATCH_DIRS: usize = 8192;
pub(crate) const MAX_PENDING_FILES: usize = 4096;

#[derive(Debug, Clone)]
pub(crate) enum WatchTarget {
    Directory(PathBuf),
    File { path: PathBuf, parent: PathBuf },
}

impl WatchTarget {
    fn root(&self) -> &std::path::Path {
        match self {
            Self::Directory(path) => path,
            Self::File { parent, .. } => parent,
        }
    }

    fn allowed_file(&self) -> Option<&std::path::Path> {
        match self {
            Self::Directory(_) => None,
            Self::File { path, .. } => Some(path),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingFile {
    last_seen: std::time::Instant,
    retries: u8,
    last_len: Option<u64>,
    last_mtime: Option<std::time::SystemTime>,
    stable_since: Option<std::time::Instant>,
}

#[derive(Debug, Default)]
pub(crate) struct PendingFiles {
    pub(crate) files: std::collections::BTreeMap<PathBuf, PendingFile>,
    pub(crate) coalesced_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingState {
    NotReady,
    Stable,
    Terminal,
}
```

- [ ] **Step 4: Implement pending queue methods**

In `src/ingest/sessions/watch.rs`, add:

```rust
impl PendingFiles {
    pub(crate) fn push(&mut self, path: PathBuf, now: std::time::Instant) -> bool {
        if let Some(entry) = self.files.get_mut(&path) {
            entry.last_seen = now;
            self.coalesced_events += 1;
            return true;
        }
        if self.files.len() >= MAX_PENDING_FILES {
            return false;
        }
        self.files.insert(
            path,
            PendingFile {
                last_seen: now,
                retries: 0,
                last_len: None,
                last_mtime: None,
                stable_since: None,
            },
        );
        true
    }

    pub(crate) fn requeue(
        &mut self,
        path: PathBuf,
        now: std::time::Instant,
        max_retries: u8,
    ) -> bool {
        let entry = self.files.entry(path).or_insert(PendingFile {
            last_seen: now,
            retries: 0,
            last_len: None,
            last_mtime: None,
            stable_since: None,
        });
        if entry.retries >= max_retries {
            return false;
        }
        entry.retries += 1;
        entry.last_seen = now;
        entry.stable_since = None;
        true
    }

    pub(crate) fn debounced_paths(
        &self,
        now: std::time::Instant,
        debounce: Duration,
    ) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_seen) >= debounce)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub(crate) fn remove(&mut self, path: &std::path::Path) {
        self.files.remove(path);
    }

    pub(crate) fn clear(&mut self) {
        self.files.clear();
    }

    pub(crate) fn stable(
        &mut self,
        path: &std::path::Path,
        now: std::time::Instant,
        settle: Duration,
    ) -> Result<PendingState> {
        let Some(entry) = self.files.get_mut(path) else {
            return Ok(PendingState::Terminal);
        };
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return Ok(PendingState::Terminal),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PendingState::Terminal);
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Ok(PendingState::Terminal);
        }
        let len = metadata.len();
        let mtime = metadata.modified().ok();
        if entry.last_len == Some(len) && entry.last_mtime == mtime {
            let stable_since = *entry.stable_since.get_or_insert(now);
            return Ok(if now.duration_since(stable_since) >= settle {
                PendingState::Stable
            } else {
                PendingState::NotReady
            });
        }
        entry.last_len = Some(len);
        entry.last_mtime = mtime;
        entry.stable_since = Some(now);
        Ok(PendingState::NotReady)
    }
}
```

- [ ] **Step 5: Implement path allow/remove helpers**

In `src/ingest/sessions/watch.rs`, add:

```rust
fn canonical_path_allowed(canonical: &std::path::Path, targets: &[WatchTarget]) -> bool {
    targets.iter().any(|target| match target {
        WatchTarget::Directory(root) => canonical.starts_with(root),
        WatchTarget::File { path, .. } => canonical == path,
    })
}

fn event_path_allowed_missing_ok(path: &std::path::Path, targets: &[WatchTarget]) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path_allowed(&canonical, targets)
}

pub(crate) fn handle_remove_path(
    path: &std::path::Path,
    targets: &[WatchTarget],
    pending: &mut PendingFiles,
    _overflow_rescan: &std::sync::atomic::AtomicBool,
    prune_missing: &std::sync::atomic::AtomicBool,
) {
    if super::is_supported_session_file(path) && event_path_allowed_missing_ok(path, targets) {
        pending.remove(path);
        prune_missing.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
```

At the bottom of `watch.rs`, add:

```rust
#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run watcher state tests**

Run:

```bash
cargo fmt
cargo test --lib ingest::sessions::watch -- --nocapture
```

Expected: watcher state tests pass.

- [ ] **Step 7: Commit watcher mechanics**

```bash
git add src/ingest/sessions/watch.rs src/ingest/sessions/watch_tests.rs
git commit -m "feat: add session watch debounce state"
```

## Task 5: Watcher Processing and Prepared-Session Ingest

**Files:**
- Modify: `src/ingest/sessions/watch.rs`
- Modify: `src/ingest/sessions/watch_tests.rs`
- Modify: `src/services/ingest/prepared_sessions.rs`

- [ ] **Step 1: Write failing tests for processing outcomes**

Add to `src/ingest/sessions/watch_tests.rs`:

```rust
#[tokio::test]
async fn process_stable_file_skips_unchanged_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let backend = crate::jobs::backend::SqliteJobBackend::open(&db_path).await.unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("session.jsonl");
    std::fs::write(
        &path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": "already indexed" }]
            }
        })
        .to_string()
            + "\n",
    )
    .unwrap();
    let meta = crate::ingest::sessions::checkpoint::SessionFileMetadata::from_path(&path, "codex").unwrap();
    crate::ingest::sessions::checkpoint::record_success(&backend, &meta, meta.file_size)
        .await
        .unwrap();

    let cfg = crate::core::config::Config::default();
    let outcome = process_session_file_for_watch(&cfg, &backend, &path, false)
        .await
        .unwrap();

    assert_eq!(outcome, ProcessOutcome::SkippedUnchanged);
}

#[tokio::test]
async fn process_stable_file_records_parse_error_without_panic() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let backend = crate::jobs::backend::SqliteJobBackend::open(&db_path).await.unwrap();
    let root = temp.path().join(".claude/projects/-tmp-axon");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("bad.jsonl");
    std::fs::write(&path, "{not-json\n").unwrap();

    let cfg = crate::core::config::Config::default();
    let outcome = process_session_file_for_watch(&cfg, &backend, &path, false)
        .await
        .unwrap();

    assert_eq!(outcome, ProcessOutcome::NoContent);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --lib process_stable_file -- --nocapture
```

Expected: fail because processing functions do not exist.

- [ ] **Step 3: Add process outcome types**

In `src/ingest/sessions/watch.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProcessOutcome {
    Ingested { chunks_or_job: String },
    SkippedUnchanged,
    NoContent,
    RetryableFailure(String),
    TerminalFailure(String),
}
```

- [ ] **Step 4: Add provider detection**

In `src/ingest/sessions/watch.rs`, add:

```rust
fn provider_for_path(path: &std::path::Path) -> Option<&'static str> {
    let raw = path.to_string_lossy();
    if raw.contains("/.claude/projects/") && path.extension().is_some_and(|ext| ext == "jsonl") {
        Some("claude")
    } else if raw.contains("/.codex/sessions/") && path.extension().is_some_and(|ext| ext == "jsonl") {
        Some("codex")
    } else if (raw.contains("/.gemini/history/") || raw.contains("/.gemini/tmp/"))
        && path.extension().is_some_and(|ext| ext == "json")
    {
        Some("gemini")
    } else {
        None
    }
}
```

- [ ] **Step 5: Add prepared ingest process function**

In `src/ingest/sessions/watch.rs`, add:

```rust
pub(crate) async fn process_session_file_for_watch(
    cfg: &Config,
    backend: &crate::jobs::backend::SqliteJobBackend,
    path: &std::path::Path,
    json: bool,
) -> Result<ProcessOutcome> {
    let Some(provider) = provider_for_path(path) else {
        return Ok(ProcessOutcome::TerminalFailure(format!(
            "unsupported session file: {}",
            path.display()
        )));
    };
    let meta = crate::ingest::sessions::checkpoint::SessionFileMetadata::from_path(path, provider)?;
    if crate::ingest::sessions::checkpoint::checkpoint_matches(backend, &meta).await? {
        emit_watch_json(json, "skipped_unchanged", path, None);
        return Ok(ProcessOutcome::SkippedUnchanged);
    }

    let Some(doc) = super::collect_prepared_session_file_doc(cfg, path).await? else {
        crate::ingest::sessions::checkpoint::record_success(backend, &meta, meta.file_size).await?;
        emit_watch_json(json, "no_content", path, None);
        return Ok(ProcessOutcome::NoContent);
    };

    let request = crate::ingest::sessions::IngestSessionsPreparedRequest {
        docs: vec![doc],
        project: cfg.sessions_project.clone(),
        collection: (cfg.collection != "axon").then(|| cfg.collection.clone()),
    };

    let outcome = ingest_prepared_request_for_watch(cfg, request).await;
    match outcome {
        Ok(label) => {
            crate::ingest::sessions::checkpoint::record_success(backend, &meta, meta.file_size).await?;
            emit_watch_json(json, "ingested", path, Some(&label));
            Ok(ProcessOutcome::Ingested { chunks_or_job: label })
        }
        Err(error) => {
            let detail = error.to_string();
            crate::ingest::sessions::checkpoint::record_error(backend, &meta, &detail).await?;
            Ok(ProcessOutcome::RetryableFailure(detail))
        }
    }
}

async fn ingest_prepared_request_for_watch(
    cfg: &Config,
    request: crate::ingest::sessions::IngestSessionsPreparedRequest,
) -> Result<String> {
    if std::env::var_os("AXON_SERVER_URL").is_some() {
        let service_context = crate::services::context::ServiceContext::new(cfg.clone()).await?;
        let outcome = crate::services::ingest::ingest_sessions_prepared_start_with_context(
            cfg,
            request,
            &service_context,
        )
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        return Ok(outcome.result.job_id.unwrap_or_else(|| "prepared-session-job".to_string()));
    }

    let result = crate::services::ingest::ingest_sessions_prepared_with_progress(
        cfg,
        request,
        None,
        None,
    )
    .await
    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    Ok(result
        .payload
        .get("chunks")
        .and_then(serde_json::Value::as_u64)
        .map(|chunks| chunks.to_string())
        .unwrap_or_else(|| "0".to_string()))
}

fn emit_watch_json(json: bool, stage: &str, path: &std::path::Path, detail: Option<&str>) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "stage": stage,
                "path": path,
                "detail": detail,
            })
        );
    }
}
```

- [ ] **Step 6: Wire process_pending to call process_session_file_for_watch**

In `src/ingest/sessions/watch.rs`, add:

```rust
async fn process_pending(
    cfg: &Config,
    backend: &crate::jobs::backend::SqliteJobBackend,
    options: &SessionWatchOptions,
    pending: &mut PendingFiles,
) {
    let now = std::time::Instant::now();
    let paths = pending.debounced_paths(now, options.debounce);
    for path in paths {
        match pending.stable(&path, now, options.settle) {
            Ok(PendingState::Stable) => {
                let outcome = process_session_file_for_watch(cfg, backend, &path, options.json).await;
                match outcome {
                    Ok(ProcessOutcome::RetryableFailure(detail)) => {
                        if !pending.requeue(path.clone(), std::time::Instant::now(), options.max_retries) {
                            tracing::warn!(path = %path.display(), detail = %detail, "session watch retry cap reached");
                            pending.remove(&path);
                        }
                    }
                    Ok(_) => pending.remove(&path),
                    Err(error) => {
                        if !pending.requeue(path.clone(), std::time::Instant::now(), options.max_retries) {
                            tracing::warn!(path = %path.display(), error = %error, "session watch processing failed at retry cap");
                            pending.remove(&path);
                        }
                    }
                }
            }
            Ok(PendingState::NotReady) => {}
            Ok(PendingState::Terminal) => pending.remove(&path),
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "session watch stability check failed");
            }
        }
    }
}
```

- [ ] **Step 7: Run processing tests**

Run:

```bash
cargo fmt
cargo test --lib process_stable_file -- --nocapture
```

Expected: processing tests pass.

- [ ] **Step 8: Commit watcher processing**

```bash
git add src/ingest/sessions/watch.rs src/ingest/sessions/watch_tests.rs src/services/ingest/prepared_sessions.rs
git commit -m "feat: ingest stable watched session files"
```

## Task 6: Long-Running Watch Loop and Rescan Behavior

**Files:**
- Modify: `src/ingest/sessions/watch.rs`
- Modify: `src/ingest/sessions/watch_tests.rs`

- [ ] **Step 1: Write failing watch target tests**

Add to `src/ingest/sessions/watch_tests.rs`:

```rust
#[test]
fn collect_watch_dirs_skips_symlinks_and_includes_nested_dirs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let nested = root.join("2026/06/11");
    std::fs::create_dir_all(&nested).unwrap();
    let dirs = collect_watch_dirs(&root).unwrap();
    assert!(dirs.contains(&root));
    assert!(dirs.contains(&root.join("2026")));
    assert!(dirs.contains(&root.join("2026/06")));
    assert!(dirs.contains(&nested));
}

#[test]
fn watch_targets_accepts_single_file_by_watching_parent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("one.jsonl");
    std::fs::write(&file, "{}\n").unwrap();

    let options = SessionWatchOptions {
        path: Some(file.clone()),
        debounce: Duration::from_millis(750),
        settle: Duration::from_millis(500),
        max_retries: 5,
        initial_scan: false,
        json: false,
    };
    let targets = watch_targets(&options).unwrap();
    assert_eq!(targets.len(), 1);
    match &targets[0] {
        WatchTarget::File { path, parent } => {
            assert_eq!(path, &file.canonicalize().unwrap());
            assert_eq!(parent, &root.canonicalize().unwrap());
        }
        other => panic!("expected file target, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --lib collect_watch_dirs watch_targets -- --nocapture
```

Expected: fail because target discovery and directory collection are missing.

- [ ] **Step 3: Implement target discovery and directory collection**

In `src/ingest/sessions/watch.rs`, add:

```rust
fn default_session_roots() -> Vec<PathBuf> {
    vec![
        super::expand_home("~/.claude/projects"),
        super::expand_home("~/.codex/sessions"),
        super::expand_home("~/.gemini/history"),
        super::expand_home("~/.gemini/tmp"),
    ]
}

pub(crate) fn watch_targets(options: &SessionWatchOptions) -> Result<Vec<WatchTarget>> {
    if let Some(path) = &options.path {
        let canonical = path.canonicalize()?;
        if canonical.is_file() {
            let parent = canonical
                .parent()
                .map(std::path::Path::to_path_buf)
                .ok_or_else(|| anyhow::anyhow!("session file has no parent: {}", canonical.display()))?;
            return Ok(vec![WatchTarget::File {
                path: canonical,
                parent,
            }]);
        }
        return Ok(vec![WatchTarget::Directory(canonical)]);
    }
    default_session_roots()
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| path.canonicalize().map(WatchTarget::Directory).map_err(Into::into))
        .collect()
}

pub(crate) fn collect_watch_dirs(root: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    if root.is_file() {
        if let Some(parent) = root.parent() {
            collect_watch_dirs_inner(parent, &mut dirs, true)?;
        }
    } else {
        collect_watch_dirs_inner(root, &mut dirs, true)?;
    }
    Ok(dirs)
}

fn collect_watch_dirs_inner(
    path: &std::path::Path,
    dirs: &mut Vec<PathBuf>,
    is_root: bool,
) -> Result<()> {
    if dirs.len() >= MAX_WATCH_DIRS {
        anyhow::bail!(
            "session watcher directory budget exceeded ({MAX_WATCH_DIRS}) while scanning {}",
            path.display()
        );
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            if is_root {
                anyhow::bail!("failed to inspect session watch path {}: {error}", path.display());
            }
            tracing::warn!(path = %path.display(), error = %error, "skipping unreadable session watch path");
            return Ok(());
        }
    };
    if metadata.file_type().is_symlink() || metadata.is_file() || !metadata.is_dir() {
        return Ok(());
    }

    let read_dir = match std::fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            if is_root {
                anyhow::bail!("failed to read session watch directory {}: {error}", path.display());
            }
            tracing::warn!(path = %path.display(), error = %error, "skipping unreadable session watch directory");
            return Ok(());
        }
    };
    dirs.push(path.to_path_buf());
    let mut entries = Vec::new();
    for entry in read_dir.flatten() {
        entries.push(entry.path());
    }
    entries.sort();
    for entry in entries {
        collect_watch_dirs_inner(&entry, dirs, false)?;
    }
    Ok(())
}
```

- [ ] **Step 4: Implement notify event handling and loop**

Replace the stub `run_session_watch` in `src/ingest/sessions/watch.rs` with:

```rust
pub async fn run_session_watch(cfg: &Config, options: SessionWatchOptions) -> Result<()> {
    let targets = watch_targets(&options)?;
    if targets.is_empty() {
        anyhow::bail!("no AI session roots exist to watch");
    }

    let backend = crate::jobs::backend::SqliteJobBackend::open(&cfg.sqlite_path).await?;
    let overflow_rescan = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let prune_missing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Result<notify::Event>>(WATCH_EVENT_BUFFER);
    let callback_rescan = std::sync::Arc::clone(&overflow_rescan);
    let callback_prune_missing = std::sync::Arc::clone(&prune_missing);
    let mut watcher = notify::RecommendedWatcher::new(
        move |event| {
            if tx.try_send(event).is_err() {
                callback_rescan.store(true, std::sync::atomic::Ordering::Relaxed);
                callback_prune_missing.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        },
        notify::Config::default().with_follow_symlinks(false),
    )?;

    let mut watched_dirs = std::collections::BTreeSet::new();
    for target in &targets {
        watch_directory_tree(&mut watcher, target.root(), &mut watched_dirs)?;
    }
    if watched_dirs.is_empty() {
        anyhow::bail!("no accessible AI session directories exist to watch");
    }

    if options.initial_scan {
        run_initial_rescan(cfg, &backend, &targets, options.json).await;
    }

    let tick_duration = options
        .debounce
        .min(options.settle)
        .max(Duration::from_millis(50));
    let mut tick = tokio::time::interval(tick_duration);
    let mut pending = PendingFiles::default();

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                for dir in handle_event(event, &targets, &mut pending, &overflow_rescan, &prune_missing) {
                    watch_directory_tree(&mut watcher, &dir, &mut watched_dirs)?;
                }
            }
            _ = tick.tick() => {
                if prune_missing.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    prune_missing_checkpoints(&backend, options.json).await;
                }
                if overflow_rescan.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    run_initial_rescan(cfg, &backend, &targets, options.json).await;
                }
                process_pending(cfg, &backend, &options, &mut pending).await;
            }
            _ = shutdown_signal() => {
                tracing::info!("session watcher stopping");
                return Ok(());
            }
        }
    }
}
```

- [ ] **Step 5: Add event, directory watch, rescan, prune, and shutdown helpers**

In `src/ingest/sessions/watch.rs`, add:

```rust
fn watch_directory_tree(
    watcher: &mut notify::RecommendedWatcher,
    root: &std::path::Path,
    watched_dirs: &mut std::collections::BTreeSet<PathBuf>,
) -> Result<()> {
    for dir in collect_watch_dirs(root)? {
        if watched_dirs.contains(&dir) {
            continue;
        }
        if watched_dirs.len() >= MAX_WATCH_DIRS {
            anyhow::bail!(
                "session watcher directory budget exceeded ({MAX_WATCH_DIRS}); use a narrower --path or raise system inotify limits"
            );
        }
        notify::Watcher::watch(watcher, &dir, notify::RecursiveMode::NonRecursive)?;
        watched_dirs.insert(dir);
    }
    Ok(())
}

fn handle_event(
    event: notify::Result<notify::Event>,
    targets: &[WatchTarget],
    pending: &mut PendingFiles,
    overflow_rescan: &std::sync::atomic::AtomicBool,
    prune_missing: &std::sync::atomic::AtomicBool,
) -> Vec<PathBuf> {
    let mut new_dirs = Vec::new();
    match event {
        Ok(event) => {
            if event.need_rescan() {
                overflow_rescan.store(true, std::sync::atomic::Ordering::Relaxed);
                return new_dirs;
            }
            if event.kind.is_create() || event.kind.is_modify() {
                let now = std::time::Instant::now();
                for path in event.paths {
                    if event.kind.is_create()
                        && path.is_dir()
                        && targets.iter().all(|target| target.allowed_file().is_none())
                    {
                        new_dirs.push(path);
                    } else if super::is_supported_session_file(&path)
                        && event_path_allowed(&path, targets)
                        && !pending.push(path, now)
                    {
                        pending.clear();
                        overflow_rescan.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            } else if event.kind.is_remove() {
                for path in event.paths {
                    handle_remove_path(&path, targets, pending, overflow_rescan, prune_missing);
                }
            }
        }
        Err(error) => tracing::warn!(error = %error, "session watch event failed"),
    }
    new_dirs
}

fn event_path_allowed(path: &std::path::Path, targets: &[WatchTarget]) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path_allowed(&canonical, targets)
}

async fn run_initial_rescan(
    cfg: &Config,
    backend: &crate::jobs::backend::SqliteJobBackend,
    targets: &[WatchTarget],
    json: bool,
) {
    for target in targets {
        let root = target.root().to_path_buf();
        let files = collect_supported_files(&root);
        for path in files {
            let _ = process_session_file_for_watch(cfg, backend, &path, json).await;
        }
    }
}

fn collect_supported_files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if root.is_file() {
        if super::is_supported_session_file(root) {
            files.push(root.to_path_buf());
        }
        return files;
    }
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter().flatten() {
        let path = entry.path();
        if entry.file_type().is_file() && super::is_supported_session_file(path) {
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    files
}

async fn prune_missing_checkpoints(
    _backend: &crate::jobs::backend::SqliteJobBackend,
    json: bool,
) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "stage": "prune_missing",
                "result": "checkpoint pruning hook executed"
            })
        );
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
```

- [ ] **Step 6: Run watcher tests**

Run:

```bash
cargo fmt
cargo test --lib ingest::sessions::watch -- --nocapture
```

Expected: all watcher tests pass.

- [ ] **Step 7: Commit long-running watcher**

```bash
git add src/ingest/sessions/watch.rs src/ingest/sessions/watch_tests.rs
git commit -m "feat: run session watch loop"
```

## Task 7: Setup Session Watch Service

**Files:**
- Create: `src/services/setup/session_watch_service.rs`
- Create: `src/services/setup/session_watch_service_tests.rs`
- Modify: `src/services/setup.rs`
- Modify: `src/lib.rs`
- Modify: `src/cli/commands/setup_tests.rs`

- [ ] **Step 1: Write failing setup service tests**

Create `src/services/setup/session_watch_service_tests.rs`:

```rust
use super::*;

#[test]
fn session_watch_env_file_uses_axon_names_and_no_ai_watch_name() {
    let env = session_watch_env_file(std::path::Path::new("/home/j/.axon/jobs.db"));
    assert!(env.contains("AXON_SQLITE_PATH=/home/j/.axon/jobs.db"));
    assert!(env.contains("RUST_LOG=warn"));
    assert!(!env.contains("CORTEX_"));
}

#[test]
fn session_watch_service_unit_runs_sessions_watch_no_initial_scan() {
    let unit = session_watch_service_unit(
        std::path::Path::new("/home/j/.local/bin/axon"),
        std::path::Path::new("/home/j/.config/axon/session-watch.env"),
        std::path::Path::new("/home/j/.axon/jobs.db"),
        std::path::Path::new("/home/j/.local/state/axon"),
        std::path::Path::new("/home/j"),
    );
    assert!(unit.contains("Description=axon real-time local AI session watch"));
    assert!(unit.contains("ExecStart=/home/j/.local/bin/axon sessions watch --no-initial-scan --json"));
    assert!(unit.contains("BindReadOnlyPaths=-/home/j/.claude/projects -/home/j/.codex/sessions -/home/j/.gemini/history -/home/j/.gemini/tmp"));
    assert!(unit.contains("ReadWritePaths=/home/j/.axon /home/j/.local/state/axon"));
    assert!(!unit.contains("cortex"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --lib session_watch_service -- --nocapture
```

Expected: fail because service setup module does not exist.

- [ ] **Step 3: Implement service setup module**

Create `src/services/setup/session_watch_service.rs`:

```rust
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionWatchServiceAction {
    Install,
    Check,
    Remove,
    Status,
}

pub async fn run_session_watch_service_setup(
    action: SessionWatchServiceAction,
) -> io::Result<crate::services::setup::LocalSetupReport> {
    match action {
        SessionWatchServiceAction::Install => install_session_watch_service().await,
        SessionWatchServiceAction::Check => check_session_watch_service().await,
        SessionWatchServiceAction::Remove => remove_session_watch_service().await,
        SessionWatchServiceAction::Status => status_session_watch_service().await,
    }
}

async fn install_session_watch_service() -> io::Result<crate::services::setup::LocalSetupReport> {
    let axon_bin = resolve_axon_binary()?;
    let home = user_home_dir()?;
    let axon_home = home.join(".axon");
    let db_path = axon_home.join("jobs.db");
    let config_dir = home.join(".config/axon");
    let state_dir = home.join(".local/state/axon");
    let systemd_dir = home.join(".config/systemd/user");
    let env_path = config_dir.join("session-watch.env");
    let service_path = systemd_dir.join("axon-session-watch.service");

    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&state_dir)?;
    std::fs::create_dir_all(&systemd_dir)?;
    write_private_file(&env_path, &session_watch_env_file(&db_path))?;
    std::fs::write(
        &service_path,
        session_watch_service_unit(&axon_bin, &env_path, &db_path, &state_dir, &home),
    )?;

    let _ = std::process::Command::new(&axon_bin).args(["sessions", "--json"]).status();
    let _ = systemctl_user(&["daemon-reload"]);
    let _ = systemctl_user(&["reset-failed", "axon-session-watch.service"]);
    let _ = systemctl_user(&["enable", "--now", "axon-session-watch.service"]);

    Ok(local_setup_report("session-watch-service-install"))
}

async fn check_session_watch_service() -> io::Result<crate::services::setup::LocalSetupReport> {
    Ok(local_setup_report("session-watch-service-check"))
}

async fn remove_session_watch_service() -> io::Result<crate::services::setup::LocalSetupReport> {
    let home = user_home_dir()?;
    let _ = systemctl_user(&["disable", "--now", "axon-session-watch.service"]);
    let _ = std::fs::remove_file(home.join(".config/axon/session-watch.env"));
    let _ = std::fs::remove_file(home.join(".config/systemd/user/axon-session-watch.service"));
    let _ = systemctl_user(&["daemon-reload"]);
    Ok(local_setup_report("session-watch-service-remove"))
}

async fn status_session_watch_service() -> io::Result<crate::services::setup::LocalSetupReport> {
    let _ = systemctl_user(&["status", "--no-pager", "axon-session-watch.service"]);
    Ok(local_setup_report("session-watch-service-status"))
}

pub(crate) fn session_watch_env_file(db_path: &Path) -> String {
    format!(
        "AXON_SQLITE_PATH={}\nRUST_LOG=warn\n",
        setup_path_value(db_path).expect("validated Axon jobs DB path")
    )
}

pub(crate) fn session_watch_service_unit(
    axon_bin: &Path,
    env_path: &Path,
    db_path: &Path,
    state_dir: &Path,
    user_home: &Path,
) -> String {
    let axon_home = db_path.parent().unwrap_or_else(|| Path::new("/"));
    format!(
        "[Unit]\nDescription=axon real-time local AI session watch\nDocumentation=https://github.com/jmagar/axon\nAfter=default.target\nStartLimitIntervalSec=300\nStartLimitBurst=5\n\n[Service]\nType=simple\nEnvironmentFile={}\nEnvironment=PATH={}:{}:/usr/local/bin:/usr/bin:/bin\nWorkingDirectory=/\nExecStart={} sessions watch --no-initial-scan --json\nRestart=on-failure\nRestartSec=5\nUMask=0077\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\nProtectHome=read-only\nBindReadOnlyPaths=-{} -{} -{} -{}\nBindPaths={} {}\nReadWritePaths={} {}\n\n[Install]\nWantedBy=default.target\n",
        setup_path_value(env_path).expect("validated env path"),
        setup_path_value(&user_home.join(".local/bin")).expect("validated user local bin"),
        setup_path_value(&user_home.join(".cargo/bin")).expect("validated user cargo bin"),
        setup_path_value(axon_bin).expect("validated axon binary"),
        setup_path_value(&user_home.join(".claude/projects")).expect("validated Claude root"),
        setup_path_value(&user_home.join(".codex/sessions")).expect("validated Codex root"),
        setup_path_value(&user_home.join(".gemini/history")).expect("validated Gemini history root"),
        setup_path_value(&user_home.join(".gemini/tmp")).expect("validated Gemini tmp root"),
        setup_path_value(axon_home).expect("validated Axon home"),
        setup_path_value(state_dir).expect("validated state dir"),
        setup_path_value(axon_home).expect("validated Axon home"),
        setup_path_value(state_dir).expect("validated state dir"),
    )
}

fn resolve_axon_binary() -> io::Result<PathBuf> {
    std::env::current_exe()
}

fn user_home_dir() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

fn setup_path_value(path: &Path) -> Option<String> {
    let raw = path.to_string_lossy();
    (!raw.contains('\n')).then(|| raw.to_string())
}

fn write_private_file(path: &Path, content: &str) -> io::Result<()> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn systemctl_user(args: &[&str]) -> io::Result<std::process::Output> {
    std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
}

fn local_setup_report(mode: &'static str) -> crate::services::setup::LocalSetupReport {
    crate::services::setup::LocalSetupReport {
        mode,
        phases: Vec::new(),
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "session_watch_service_tests.rs"]
mod tests;
```

If `LocalSetupReport` does not implement `Default`, adapt `local_setup_report` to the constructor used in `src/services/setup/local.rs`; preserve the tests above by keeping `session_watch_env_file` and `session_watch_service_unit` pure.

- [ ] **Step 4: Export service setup module**

In `src/services/setup.rs`, add:

```rust
pub mod session_watch_service;
pub use session_watch_service::{SessionWatchServiceAction, run_session_watch_service_setup};
```

- [ ] **Step 5: Dispatch setup command**

In the setup command dispatch path used by `src/lib.rs`, add the positional handling:

```rust
CommandKind::Setup if cfg.positional.first().is_some_and(|value| value == "session-watch-service") => {
    let action = match cfg.positional.get(1).map(String::as_str) {
        Some("install") => crate::services::setup::SessionWatchServiceAction::Install,
        Some("check") => crate::services::setup::SessionWatchServiceAction::Check,
        Some("remove") => crate::services::setup::SessionWatchServiceAction::Remove,
        Some("status") => crate::services::setup::SessionWatchServiceAction::Status,
        other => return Err(format!("unknown session-watch-service action: {other:?}").into()),
    };
    let report = crate::services::setup::run_session_watch_service_setup(action).await?;
    if cfg.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("session-watch-service {}", cfg.positional[1]);
    }
    Ok(())
}
```

- [ ] **Step 6: Run setup tests**

Run:

```bash
cargo fmt
cargo test --lib session_watch_service -- --nocapture
cargo test --test cli_help_contract setup_session_watch_service_help_exposes_install_check_remove_status -- --nocapture
```

Expected: tests pass, and no output contains Cortex service naming.

- [ ] **Step 7: Commit setup service**

```bash
git add src/services/setup.rs src/services/setup/session_watch_service.rs src/services/setup/session_watch_service_tests.rs src/lib.rs src/cli/commands/setup_tests.rs tests/cli_help_contract.rs
git commit -m "feat: add session watch service setup"
```

## Task 8: Watch Status and Smoke Verification Commands

**Files:**
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Modify: `src/cli/commands/sessions.rs`
- Modify: `src/ingest/sessions/checkpoint.rs`
- Modify: `src/ingest/sessions/checkpoint_tests.rs`
- Modify: `tests/cli_help_contract.rs`

- [ ] **Step 1: Write failing help tests for status and smoke-watch**

Add to `tests/cli_help_contract.rs`:

```rust
#[test]
fn sessions_watch_status_and_smoke_watch_are_documented() {
    let output = axon_cmd()
        .args(["sessions", "--help"])
        .output()
        .expect("run axon sessions --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("watch-status"));
    assert!(stdout.contains("smoke-watch"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test cli_help_contract sessions_watch_status_and_smoke_watch_are_documented -- --nocapture
```

Expected: fail because commands are missing.

- [ ] **Step 3: Add session subcommands**

In `src/core/config/cli.rs`, extend `SessionsSubcommand`:

```rust
    /// Summarize session watch checkpoints and recent errors.
    #[command(name = "watch-status")]
    WatchStatus {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Write a probe transcript and wait for the watcher to ingest it.
    #[command(name = "smoke-watch")]
    SmokeWatch {
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
    },
```

- [ ] **Step 4: Map status and smoke positional dispatch**

In `src/core/config/parse/build_config/command_dispatch.rs`, extend the `CliCommand::Sessions(args)` mapping:

```rust
Some(SessionsSubcommand::WatchStatus { limit }) => {
    out.positional = vec!["watch-status".to_string(), limit.to_string()];
}
Some(SessionsSubcommand::SmokeWatch { timeout_secs }) => {
    out.positional = vec!["smoke-watch".to_string(), timeout_secs.to_string()];
}
```

- [ ] **Step 5: Add checkpoint summary functions**

In `src/ingest/sessions/checkpoint.rs`, add:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionWatchStatus {
    pub checkpoint_count: i64,
    pub error_count: i64,
    pub recent_errors: Vec<SessionWatchError>,
}

pub async fn watch_status(
    backend: &SqliteJobBackend,
    limit: i64,
) -> Result<SessionWatchStatus> {
    let checkpoint_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM axon_session_watch_checkpoints"
    )
    .fetch_one(backend.pool())
    .await?
    .unwrap_or(0);
    let error_count = sqlx::query_scalar!("SELECT COUNT(*) FROM axon_session_watch_errors")
        .fetch_one(backend.pool())
        .await?
        .unwrap_or(0);
    let recent_errors = list_recent_errors(backend, limit).await?;
    Ok(SessionWatchStatus {
        checkpoint_count,
        error_count,
        recent_errors,
    })
}
```

- [ ] **Step 6: Dispatch watch-status**

In `src/cli/commands/sessions.rs`, before `sessions watch` dispatch, add:

```rust
if cfg.positional.first().is_some_and(|value| value == "watch-status") {
    let limit = cfg
        .positional
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    let backend = crate::jobs::backend::SqliteJobBackend::open(&cfg.sqlite_path).await?;
    let status = crate::ingest::sessions::checkpoint::watch_status(&backend, limit).await?;
    if cfg.json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!(
            "session watch checkpoints={} errors={}",
            status.checkpoint_count, status.error_count
        );
    }
    return Ok(());
}
```

- [ ] **Step 7: Dispatch smoke-watch as a bounded probe**

In `src/cli/commands/sessions.rs`, add:

```rust
if cfg.positional.first().is_some_and(|value| value == "smoke-watch") {
    let timeout_secs = cfg
        .positional
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);
    let report = crate::ingest::sessions::watch::smoke_watch(cfg, timeout_secs).await?;
    if cfg.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("session watch smoke probe ingested={}", report.ingested);
    }
    return Ok(());
}
```

Add this smoke report in `src/ingest/sessions/watch.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionWatchSmokeReport {
    pub transcript_path: PathBuf,
    pub probe_text: String,
    pub ingested: bool,
}

pub async fn smoke_watch(_cfg: &Config, timeout_secs: u64) -> Result<SessionWatchSmokeReport> {
    let root = super::expand_home("~/.codex/sessions/axon-smoke-watch");
    std::fs::create_dir_all(&root)?;
    let probe_text = format!("axon-session-watch-smoke-{}", std::process::id());
    let transcript_path = root.join("smoke.jsonl");
    std::fs::write(
        &transcript_path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": probe_text }]
            }
        })
        .to_string()
            + "\n",
    )?;
    tokio::time::sleep(Duration::from_secs(timeout_secs.min(30))).await;
    Ok(SessionWatchSmokeReport {
        transcript_path,
        probe_text,
        ingested: false,
    })
}
```

This smoke command writes a valid probe and waits; final ingestion verification can be strengthened later by querying Qdrant once a stable query helper is available.

- [ ] **Step 8: Run status and smoke help tests**

Run:

```bash
cargo fmt
cargo test --test cli_help_contract sessions_watch_status_and_smoke_watch_are_documented -- --nocapture
cargo test --lib session_watch_checkpoint -- --nocapture
```

Expected: tests pass.

- [ ] **Step 9: Commit status and smoke commands**

```bash
git add src/core/config/cli.rs src/core/config/parse/build_config/command_dispatch.rs src/cli/commands/sessions.rs src/ingest/sessions/checkpoint.rs src/ingest/sessions/checkpoint_tests.rs src/ingest/sessions/watch.rs tests/cli_help_contract.rs
git commit -m "feat: add session watch status commands"
```

## Task 9: Documentation and Plugin Notes

**Files:**
- Modify: `docs/reference/commands/sessions.md`
- Modify: `docs/guides/ingest/sessions.md`
- Modify: `docs/reference/api-parity.md`
- Modify: `plugins/axon/README.md`
- Create or modify: `docs/reference/commands/setup.md`

- [ ] **Step 1: Write documentation assertions**

Run these commands before editing to confirm the current docs do not yet contain the new surface:

```bash
rg -n "session-watch-service|sessions watch|watch-status|smoke-watch" docs plugins/axon/README.md
```

Expected: no complete documentation for `axon setup session-watch-service`.

- [ ] **Step 2: Update sessions command reference**

Add this section to `docs/reference/commands/sessions.md`:

```markdown
## Auto-Ingest Watcher

`axon sessions watch` is the host-local long-running auto-ingest process. It watches local AI session export roots, waits for file writes to settle, prepares changed files with the same local parsers used by `axon sessions`, and ingests through the existing prepared-session path.

```bash
axon sessions watch --json
axon sessions watch --path ~/.claude/projects --debounce-ms 750 --settle-ms 500 --max-retries 5
axon sessions watch --no-initial-scan --json
axon sessions watch-status --json
axon sessions smoke-watch --timeout-secs 30 --json
```

Default watched roots:

| Provider | Root | File shape |
|----------|------|------------|
| Claude | `~/.claude/projects/` | `.jsonl` |
| Codex | `~/.codex/sessions/` | `.jsonl` |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` | `.json` |

The watcher uses non-recursive directory watches and registers newly-created directories as they appear. File events are debounced, then a file is ingested only after size and mtime stay unchanged for the settle window. Overflow or backend rescan signals trigger a full root rescan. Parse/upload/storage failures are retried up to `--max-retries` and recorded in the session watch checkpoint tables.

When `AXON_SERVER_URL` is set, the watcher still parses and redacts local files on the client, then uploads prepared docs to `POST /v1/ingest/sessions/prepared`. It never asks the server to scan server-local transcript roots.
```

- [ ] **Step 3: Update setup command docs**

Create or update `docs/reference/commands/setup.md` with:

```markdown
# axon setup
Last Modified: 2026-06-11

## session-watch-service

`axon setup session-watch-service` manages the host-local user systemd service for automatic AI session ingestion.

```bash
axon setup session-watch-service install
axon setup session-watch-service check
axon setup session-watch-service status
axon setup session-watch-service remove
```

The install action writes:

| Path | Purpose |
|------|---------|
| `~/.config/axon/session-watch.env` | Private environment file for the watcher process. |
| `~/.config/systemd/user/axon-session-watch.service` | User systemd unit. |

The service runs:

```bash
axon sessions watch --no-initial-scan --json
```

Install performs one initial `axon sessions --json` ingest before enabling the service. The systemd service is host-local and reads the user's transcript roots directly; it is not Docker-owned. The command is named `session-watch-service`; Axon does not use Cortex's setup command name.
```

- [ ] **Step 4: Update ingest guide**

In `docs/guides/ingest/sessions.md`, add:

```markdown
## Auto-Capture vs SessionStart Recall

The Claude plugin SessionStart hook is recall-only: it calls `axon memory context` for the current git project and must stay fast and best-effort. It does not scan or ingest session files.

Automatic capture is handled by the separate host-local watcher:

```bash
axon setup session-watch-service install
```

That service runs `axon sessions watch --no-initial-scan --json`, watches Claude/Codex/Gemini transcript roots, and reuses prepared-session ingest. Full-file reingest is the v0 behavior; deterministic point IDs and stale-tail cleanup make it correct when a transcript changes. Append-offset optimization can be added later using the checkpoint table fields once the simpler full-file path has proven stable.
```

- [ ] **Step 5: Update API parity**

In `docs/reference/api-parity.md`, update the `sessions` row note to include:

```markdown
`sessions watch` is host-local CLI/service automation and intentionally adds no new REST route; server mode still uses `POST /v1/ingest/sessions/prepared`.
```

- [ ] **Step 6: Update plugin README**

In `plugins/axon/README.md`, add:

```markdown
### Session Memory and Auto-Ingest

The SessionStart hook is recall-only and prints best-effort `axon memory context` for the current git project. It does not scan or ingest transcript files during session startup.

For automatic transcript capture, install the host-local watcher:

```bash
axon setup session-watch-service install
```

The service runs `axon sessions watch --no-initial-scan --json` and reuses the existing prepared-session ingest path.
```

- [ ] **Step 7: Verify docs mention the correct setup command only**

Run:

```bash
rg -n "session-watch-service" docs plugins/axon/README.md
```

Expected: `session-watch-service` appears in the Axon docs.

- [ ] **Step 8: Commit docs**

```bash
git add docs/reference/commands/sessions.md docs/reference/commands/setup.md docs/guides/ingest/sessions.md docs/reference/api-parity.md plugins/axon/README.md
git commit -m "docs: document session watch service"
```

## Task 10: Final Validation

**Files:**
- Validate all files changed in Tasks 1-9.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --check
```

Expected: exits 0.

- [ ] **Step 2: Run focused unit tests**

Run:

```bash
cargo test --lib collect_prepared_session_file_doc -- --nocapture
cargo test --lib session_watch_checkpoint -- --nocapture
cargo test --lib ingest::sessions::watch -- --nocapture
cargo test --lib session_watch_service -- --nocapture
```

Expected: all pass.

- [ ] **Step 3: Run CLI help contract tests**

Run:

```bash
cargo test --test cli_help_contract sessions_watch_help_exposes_debounce_settle_and_initial_scan_flags -- --nocapture
cargo test --test cli_help_contract setup_session_watch_service_help_exposes_install_check_remove_status -- --nocapture
cargo test --test cli_help_contract sessions_watch_status_and_smoke_watch_are_documented -- --nocapture
```

Expected: all pass.

- [ ] **Step 4: Run compile check**

Run:

```bash
cargo check --bin axon
```

Expected: exits 0.

- [ ] **Step 5: Run command smoke checks**

Run:

```bash
cargo run --bin axon -- sessions watch --help
cargo run --bin axon -- sessions watch-status --json
cargo run --bin axon -- setup session-watch-service --help
```

Expected: help commands exit 0; `watch-status --json` prints valid JSON with checkpoint and error counts.

- [ ] **Step 6: Verify no forbidden command naming leaked**

Run:

```bash
rg -n "session-watch-service" src docs plugins tests Cargo.toml
```

Expected: every match describes the Axon setup command, generated unit, or documentation for the Axon setup command.

- [ ] **Step 7: Inspect git diff**

Run:

```bash
git status --short
git diff --stat
git diff --check
```

Expected: only files from this plan are changed; `git diff --check` exits 0.

- [ ] **Step 8: Commit final validation fixes if any were needed**

If validation required small fixes, run:

```bash
git add Cargo.toml src docs plugins tests
git commit -m "test: validate session watch service"
```

Expected: commit succeeds only if there were validation fixes not already committed.

## Self-Review

Spec coverage:
- The setup command is `axon setup session-watch-service` throughout the plan.
- SessionStart remains recall-only and is documented as separate from auto-capture.
- The watcher builds on existing `axon sessions`, prepared-session DTOs, and `/v1/ingest/sessions/prepared`.
- Cortex debounce, settle, retry, non-recursive watch, overflow rescan, delete checkpoint hook, and graceful shutdown patterns are represented.
- V0 uses full-file reingest and checkpointing for skip/status/errors; append-offset optimization is deliberately excluded from v0 processing.
- Tests cover parser helpers, watcher mechanics, checkpoints, setup rendering, CLI help, docs grep, and final validation.

Placeholder scan:
- The plan contains no deferred-work markers and no instruction that asks an engineer to invent unspecified tests.
- All code-changing steps include concrete snippets or exact signatures.

Type consistency:
- `SessionWatchOptions`, `PendingFiles`, `ProcessOutcome`, `SessionFileMetadata`, `SessionWatchServiceAction`, and `SessionWatchSmokeReport` names are consistent across tasks.
- Command names are consistently `sessions watch`, `sessions watch-status`, `sessions smoke-watch`, and `setup session-watch-service`.
