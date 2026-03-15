# crates/cli — Command Orchestration Layer
Last Modified: 2026-03-02

Translates parsed `Config` state into command execution. Delegates all business logic to `crates/jobs`, `crates/crawl`, `crates/vector`, and `crates/ingest`. This crate owns routing, output formatting, and job lifecycle UX — not business logic.

## Module Layout

```
cli/
├── commands.rs                   # Module declarations + pub use exports (NOT dispatch)
└── commands/
    ├── common.rs                 # Shared URL parsing, job output, handle_job_* helpers
    ├── job_contracts.rs          # Stable JSON output types for all job commands
    ├── ingest_common.rs          # Shared ingest subcommand routing + enqueue helpers
    ├── probe.rs                  # HTTP probing utilities used by doctor command
    │
    ├── scrape.rs                 # Scrape URLs to markdown/html/json
    ├── map.rs                    # Discover all URLs without scraping
    ├── crawl.rs                  # Crawl entry: sync/async dispatch + URL validation
    ├── crawl/
    │   ├── subcommands.rs        # Job lifecycle routing: status/cancel/errors/list/cleanup/clear/worker/recover/audit/diff
    │   ├── runtime.rs            # Chrome bootstrap: CDP discovery, WS URL pre-resolution
    │   ├── sync_crawl.rs         # Sync crawl: 24h cache, sitemap-only mode, HTTP→Chrome fallback
    │   └── audit/                # crawl audit + crawl diff: snapshot generation and comparison
    │       ├── audit.rs          # Entry point + snapshot/diff dispatch
    │       ├── audit_diff.rs     # Diff computation (added/removed/changed URLs)
    │       ├── manifest_audit.rs # Snapshot persistence to disk
    │       └── sitemap.rs        # Sitemap + robots.txt URL discovery (adapter over engine)
    ├── refresh.rs                # Refresh command entry point
    ├── refresh/
    │   ├── resolve.rs            # URL resolution from manifest or CLI args
    │   └── schedule.rs           # Scheduled refresh job management
    ├── extract.rs                # LLM-powered structured data extraction
    ├── embed.rs                  # Embed files/dirs/URLs into Qdrant
    ├── search.rs                 # Web search via Tavily API
    ├── research.rs               # Tavily AI research + LLM synthesis
    ├── screenshot.rs             # Screenshot entry: URL loop, Chrome requirement check
    ├── screenshot/
    │   ├── screenshot_migration_tests.rs  # Migration tests for screenshot command refactor
    │   └── util.rs               # Filename generation, require_chrome()
    ├── sessions.rs               # Ingest AI session exports (Claude/Codex/Gemini)
    ├── ingest.rs                 # Unified ingest: classify_target → enqueue or run_ingest_sync
    ├── status/
    │   ├── metrics.rs            # Postgres metrics: job counts, rates, stale jobs
    │   └── presentation.rs       # Status output rendering (JSON + human text)
    ├── doctor.rs                 # Service connectivity diagnostics
    ├── doctor/
    │   └── render.rs             # Doctor report rendering (human + JSON)
    ├── debug.rs                  # doctor + LLM-assisted troubleshooting
    ├── mcp.rs                    # MCP HTTP server entry point
    └── serve.rs                  # axum web UI + WebSocket server entry point
```

## Dispatch

`commands.rs` declares modules and exports — it is **not** the dispatch layer. The actual match lives in `lib.rs`:

```rust
match cfg.command {
    CommandKind::Crawl => run_crawl(cfg).await?,
    CommandKind::Ask   => run_ask(cfg).await?,   // delegates to crates/services::query
    // ...
}
```

All command handlers share the same signature:
```rust
pub async fn run_<command>(cfg: &Config) -> Result<(), Box<dyn Error>>
```

## Critical Pattern: `maybe_handle_subcommand()`

Commands with job lifecycle operations (crawl, extract, embed, ingest, refresh) use this pattern:

```rust
pub async fn run_crawl(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if subcommands::maybe_handle_subcommand(cfg).await? {
        return Ok(());   // subcommand handled — exit
    }
    // ... normal URL-based logic continues
}
```

`maybe_handle_subcommand()` inspects `cfg.positional.first()`:
- Matches `"status"`, `"cancel"`, `"errors"`, `"list"`, `"cleanup"`, `"clear"`, `"worker"`, `"recover"`, `"audit"`, `"diff"` → executes, returns `Ok(true)`
- Anything else → returns `Ok(false)` (caller proceeds)

**Gotcha:** If a user tries to crawl a URL whose path happens to match a subcommand name (e.g., `axon crawl https://example.com/status`), it will be intercepted as a subcommand. This is a known, accepted limitation.

## Critical Pattern: `start_url_from_cfg()`

**Never** blindly use `cfg.positional[0]` as a URL. Use `start_url_from_cfg(cfg)` from `common.rs`:

```rust
pub fn start_url_from_cfg(cfg: &Config) -> String
```

This function guards against subcommand names leaking into URL extraction. It returns `cfg.positional[0]` only if it is NOT a known subcommand token. Otherwise falls back to `cfg.start_url`.

## `commands/common.rs` — Shared Helpers

| Function | Purpose |
|----------|---------|
| `truncate_chars(s, n)` | UTF-8-safe truncation at char boundary (no mid-codepoint panic) |
| `parse_urls(cfg)` | Collects URLs from `urls_csv`, `url_glob`, and `positional`; expands `{a,b}` and `{1..10}` brace syntax; dedupes; normalizes |
| `expand_url_glob_seed(seed)` | Expands single URL glob string into `Vec<String>` (capped at depth 10) |
| `start_url_from_cfg(cfg)` | Subcommand-aware URL extraction — always use this, never raw `positional[0]` |
| `handle_job_status(cfg, job, id, cmd)` | Renders job status (JSON or human) |
| `handle_job_cancel(cfg, id, canceled, cmd)` | Renders cancel result |
| `handle_job_errors(cfg, job, id, cmd)` | Renders job error text |
| `handle_job_list(cfg, jobs, cmd)` | Renders job list (truncated IDs, status symbols) |
| `handle_job_cleanup(cfg, removed, cmd)` | Renders cleanup count |
| `handle_job_clear(cfg, removed, cmd)` | Renders clear count + queue purge message |
| `handle_job_recover(cfg, reclaimed, cmd)` | Renders stale job reclaim count |

All `handle_job_*` functions accept `T: JobStatus + Serialize` — new job types must implement both.

## `commands/job_contracts.rs` — Stable Output Types

Defines the stable JSON API shapes for `--json` output across all job commands:

| Type | Used by |
|------|---------|
| `JobStatusResponse` | `crawl status`, `extract status`, `ingest status` — unified schema with optional `url`/`source_type`/`target` |
| `JobCancelResponse` | All cancel operations |
| `JobErrorsResponse` | All errors queries |
| `JobSummaryEntry` | All list operations |

**Do not change field names** — these are the externally stable JSON contract. Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields.

## `commands/ingest_common.rs` — Shared Ingest Helpers

| Function | Purpose |
|----------|---------|
| `maybe_handle_ingest_subcommand(cfg, cmd)` | Routes ingest subcommands (same pattern as crawl): status, cancel, errors, list, cleanup, clear, worker |
| `parse_ingest_job_id(cfg, cmd, action)` | Parses `cfg.positional[1]` as UUID; descriptive error if missing |
| `enqueue_ingest_job(cfg, source)` | Enqueues job, prints job ID (JSON or human) |
| `print_ingest_sync_result(cfg, cmd, chunks, target)` | Prints sync completion summary |

## Subcommand Arg Indexing

When a subcommand takes an argument (e.g., `crawl status <job-id>`):
- `cfg.positional[0]` = subcommand name (`"status"`)
- `cfg.positional[1]` = the argument (`"<uuid>"`)

Always use `.get(1)` — never `.first()` — when extracting subcommand arguments.

## Output Pattern

Every command branches on `cfg.json_output`:

```rust
if cfg.json_output {
    println!("{}", serde_json::to_string_pretty(&data)?);
} else {
    println!("{} {}", primary("Label:"), accent(&value));
}
```

JSON output is always **pretty-printed** (`to_string_pretty`). Use types from `job_contracts.rs` for job responses; use `serde_json::json!()` for simple ad-hoc responses.

Human output uses `primary()`, `accent()`, `muted()`, `symbol_for_status()`, `status_text()` from `crates/core/ui`.

## Confirmation Prompts

For destructive operations (clear, delete), always use:

```rust
if !confirm_destructive(cfg, "This will delete all jobs. Continue?")? {
    return Ok(());
}
```

`confirm_destructive()` returns `Ok(true)` if `cfg.yes` is set OR if stdout is not a TTY. Never gate on `cfg.yes` directly — this function handles both cases.

## `crawl/runtime.rs` — Chrome Bootstrap

Pre-resolves the CDP WebSocket URL before starting the crawl by delegating to the shared engine resolver (`crates/crawl/engine::resolve_cdp_ws_url`). The CLI runtime no longer owns a CDP probe implementation — all resolution logic (Docker host rewrite, `/json/version` discovery, ws:// shortcut) lives in the crawl engine.

The bootstrap function retries resolution with configurable backoff (`chrome_bootstrap_retries`) and passes the resolved URL into the crawl config to avoid a redundant `/json/version` fetch mid-crawl.

Always call `bootstrap_chrome_runtime(cfg)` before Chrome-mode crawls; do not let each worker probe independently.

## `crawl/sync_crawl.rs` — Synchronous Crawl

- Checks 24-hour disk cache before crawling; returns cached result if hit
- Supports sitemap-only mode (`--sitemap-only`) — skips main crawl, backfills from sitemap
- Calls `should_fallback_to_chrome()` after HTTP crawl and retries with Chrome if thin rate is too high
- Sitemap backfill delegates to `crawl::engine::append_sitemap_backfill()` — no CLI-owned fetch loop

## Testing

```bash
cargo test cli              # all CLI tests
cargo test truncate_chars   # UTF-8 truncation (3 tests)
cargo test job_contracts    # JSON output contract tests (12 tests)
cargo test url_glob         # brace expansion tests
```

Tests are in `common.rs` (pure functions) and `job_contracts.rs` (serialization). No integration tests — command handlers are orchestration and require services.

## Adding a New Command

1. Create `commands/<name>.rs` with `pub async fn run_<name>(cfg: &Config) -> Result<(), Box<dyn Error>>`
2. Add `pub mod <name>;` and `pub use <name>::run_<name>;` to `commands.rs`
3. Add `CommandKind::<Name>` variant to `crates/core/config/types/enums.rs`
4. Add field(s) to `Config` in `crates/core/config/types/config.rs` and `Config::default()` in `config_impls.rs`
5. Add flag(s) to `GlobalArgs` or a new command-specific `Args` struct in `config/cli/`
6. Add the parse logic to `config/parse/build_config.rs`
7. Add match arm to `lib.rs::run_once()`
8. **Update inline `Config { ... }` literals** in `crates/cli/commands/research.rs`, `search.rs`, and any `make_test_config()` helpers — compiler only catches this at test build time
