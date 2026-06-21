# src/cli — Command Orchestration Layer
Last Modified: 2026-05-09

Translates parsed `Config` state into command execution. Delegates all business logic to `src/jobs`, `src/crawl`, `src/vector`, and `src/ingest`. This crate owns routing, output formatting, and job lifecycle UX — not business logic.

## Module Layout

```
cli/
├── commands.rs                   # Module declarations + pub use exports (NOT dispatch)
└── commands/
    ├── common.rs                 # Tiny stub — most shared helpers live in common_urls.rs / common_jobs.rs
    ├── common_urls.rs            # truncate_chars, parse_urls, expand_url_glob_seed, start_url_from_cfg
    ├── common_jobs.rs            # handle_job_status/cancel/errors/list/cleanup/clear/recover renderers
    ├── job_contracts.rs          # + job_contracts/{record,responses,summary}.rs — stable JSON output types
    ├── ingest_common.rs          # Shared ingest subcommand routing + enqueue helpers
    ├── ask.rs                    # RAG ask command via services layer
    ├── completions.rs            # Shell completion script generation
    ├── crawl.rs                  # Crawl entry: sync/async dispatch + URL validation
    ├── debug.rs                  # doctor + LLM-assisted troubleshooting
    ├── dedupe.rs                 # Dedupe command entry point
    ├── doctor.rs                 # Service connectivity diagnostics
    ├── domains.rs                # Indexed-domain summaries
    ├── embed.rs                  # Embed files/dirs/URLs into Qdrant
    ├── evaluate.rs               # RAG evaluation command
    ├── extract.rs                # LLM-powered structured data extraction
    ├── ingest.rs                 # Unified ingest: classify_target → enqueue or run_ingest_sync
    ├── map.rs                    # Discover all URLs without scraping
    ├── mcp.rs                    # MCP server entry point (HTTP / stdio)
    ├── migrate.rs                # Collection migration entry (unnamed → named-mode upgrade)
    ├── probe.rs                  # HTTP probing utilities used by doctor command
    ├── query.rs                  # Semantic/vector query command
    ├── research.rs               # SearXNG/Tavily research + LLM synthesis
    ├── retrieve.rs               # Retrieve stored document chunks
    ├── scrape.rs                 # Scrape URLs to markdown/html/json
    ├── screenshot.rs             # Screenshot entry: URL loop, Chrome requirement check
    ├── search.rs                 # Web search via SearXNG/Tavily
    ├── serve.rs                  # unified Axum HTTP server entry point
    ├── services_migration_tests.rs # Migration tests for the services-layer refactor
    ├── sessions.rs               # Ingest AI session exports (Claude/Codex/Gemini)
    ├── setup.rs                  # First-run / interactive config setup
    ├── sources.rs                # Indexed source listing
    ├── stats.rs                  # Qdrant/stats command entry point
    ├── status.rs                 # System/job status entry point
    ├── summarize.rs              # Scrape URL context + configured LLM summary via services layer
    ├── suggest.rs                # Suggested crawl target discovery
    ├── watch.rs                  # Watch definition and run management
    ├── crawl/
    │   ├── subcommands.rs        # Job lifecycle routing: status/cancel/errors/list/cleanup/clear/worker/recover/audit/diff
    │   ├── runtime.rs            # Thin shim — delegates to src/crawl/engine::resolve_cdp_ws_url
    │   ├── sync_crawl.rs         # Thin shim — sync-crawl logic lives in src/services/crawl_sync.rs
    │   ├── runtime_migration_tests.rs
    │   ├── sync_backfill_migration_tests.rs
    │   ├── sync_crawl_migration_tests.rs
    │   └── audit.rs              # Thin shim — delegates to src/services/crawl/audit
    ├── screenshot/
    │   ├── screenshot_migration_tests.rs
    │   └── util.rs               # Filename generation, require_chrome()
    ├── scrape/
    │   ├── scrape_migration_tests.rs
    │   └── tests.rs               # Scrape unit tests
    ├── status/
    │   ├── failure_summary.rs    # Recent failure summary rendering
    │   ├── metrics.rs            # Shared status formatting helpers
    │   └── metrics/format.rs
    ├── doctor/
    │   └── render.rs             # Doctor report rendering (human + JSON)
    └── map/
        ├── map_migration_tests.rs
        └── map_sitemap_tests.rs
```

> There is **no `commands/graph.rs`**, no `Graph` `CommandKind` variant, and no graph ask flag. Graph retrieval is not part of the production CLI, MCP, or `/v1/ask` request contract.

## Dispatch

`commands.rs` declares modules and exports — it is **not** the dispatch layer. The actual match lives in `lib.rs::run_once()` (see `lib.rs` lines 34–61 for the full 28-arm match). Excerpt:

```rust
match cfg.command {
    CommandKind::Scrape    => run_scrape(cfg).await?,
    CommandKind::Crawl     => run_crawl(cfg, service_context).await?,
    CommandKind::Watch     => run_watch(cfg, service_context).await?,
    CommandKind::Extract   => run_extract(cfg, service_context).await?,
    CommandKind::Embed     => run_embed(cfg, service_context).await?,
    CommandKind::Ask       => run_ask(cfg).await?,
    CommandKind::Summarize => run_summarize(cfg).await?,
    CommandKind::Status    => run_status(cfg, service_context).await?,
    CommandKind::Ingest    => run_ingest(cfg, service_context).await?,
    CommandKind::Sessions  => run_sessions(cfg, service_context).await?,
    // ... 19 more arms
}
```

Two handler signatures coexist:
```rust
pub async fn run_<command>(cfg: &Config) -> Result<(), Box<dyn Error>>
pub async fn run_<command>(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>>
```

Handlers that touch the job runtime (anything that enqueues/inspects jobs) take `&ServiceContext`; pure-CLI handlers take `&Config` only.

## Critical Pattern: `maybe_handle_subcommand()`

Commands with job lifecycle operations (crawl, extract, embed, ingest) use this pattern:

```rust
pub async fn run_crawl(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if subcommands::maybe_handle_subcommand(cfg).await? {
        return Ok(());   // subcommand handled — exit
    }
    // ... normal URL-based logic continues
}
```

`maybe_handle_subcommand()` inspects `cfg.positional.first()`:
- Crawl matches `"status"`, `"cancel"`, `"errors"`, `"list"`, `"cleanup"`, `"clear"`, `"worker"`, `"recover"`, `"audit"`, `"diff"` → executes, returns `Ok(true)`
- Anything else → returns `Ok(false)` (caller proceeds)

**Gotcha:** If a user tries to crawl a URL whose path happens to match a subcommand name (e.g., `axon crawl https://example.com/status`), it will be intercepted as a subcommand. This is a known, accepted limitation.

## Critical Pattern: `start_url_from_cfg()`

**Never** blindly use `cfg.positional[0]` as a URL. Use `start_url_from_cfg(cfg)` from `commands/common_urls.rs`:

```rust
pub fn start_url_from_cfg(cfg: &Config) -> String
```

This function guards against subcommand names leaking into URL extraction. It returns `cfg.positional[0]` only if it is NOT a known subcommand token. Otherwise falls back to `cfg.start_url`.

Current guard list:
- Crawl: `"status"`, `"cancel"`, `"errors"`, `"list"`, `"cleanup"`, `"clear"`, `"worker"`, `"recover"`, `"audit"`, `"diff"`
- Extract/Embed: `"status"`, `"cancel"`, `"errors"`, `"list"`, `"cleanup"`, `"clear"`, `"worker"`, `"recover"`

`"doctor"` is not part of the `start_url_from_cfg()` guard list.

## Shared Helpers — `common_urls.rs` and `common_jobs.rs`

`commands/common.rs` is a small stub. The shared helpers were split into two files. Use the table to find the right module.

### `commands/common_urls.rs` — URL & string helpers

| Function | Purpose |
|----------|---------|
| `truncate_chars(s, n)` | UTF-8-safe truncation at char boundary (no mid-codepoint panic) |
| `parse_urls(cfg)` | Collects URLs from `urls_csv`, `url_glob`, and `positional`; expands `{a,b}` and `{1..10}` brace syntax; dedupes; normalizes |
| `expand_url_glob_seed(seed)` | Expands a single URL glob string into `Vec<String>` (capped at depth 10 and 10,000 total outputs) |
| `start_url_from_cfg(cfg)` | Subcommand-aware URL extraction — always use this, never raw `positional[0]` |

### `commands/common_jobs.rs` — Job lifecycle renderers

| Function | Purpose |
|----------|---------|
| `handle_job_status(cfg, job, id, cmd)` | Renders job status (JSON or human) |
| `handle_job_cancel(cfg, id, canceled, cmd)` | Renders cancel result |
| `handle_job_errors(cfg, job, id, cmd)` | Renders job error text |
| `handle_job_list(cfg, jobs, cmd)` | Renders job list (truncated IDs, status symbols) |
| `handle_job_cleanup(cfg, removed, cmd)` | Renders cleanup count |
| `handle_job_clear(cfg, removed, cmd)` | Renders clear count + queue purge message |
| `handle_job_recover(cfg, reclaimed, cmd)` | Renders stale job reclaim count |

All `handle_job_*` functions accept `T: JobStatus + Serialize` — new job types must implement both.

### `src/core/ui.rs` — UI helpers

`confirm_destructive(cfg, prompt)` lives in `src/core/ui.rs` (not in the CLI common modules). It returns `Ok(true)` if `cfg.yes` is set OR if stdout is not a TTY.

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

Human output uses `primary()`, `accent()`, `muted()`, `symbol_for_status()`, `status_text()` from `src/core/ui`.

## Confirmation Prompts

For destructive operations (clear, delete), always use:

```rust
if !confirm_destructive(cfg, "This will delete all jobs. Continue?")? {
    return Ok(());
}
```

`confirm_destructive()` returns `Ok(true)` if `cfg.yes` is set OR if stdout is not a TTY. Never gate on `cfg.yes` directly — this function handles both cases.

## `crawl/runtime.rs` — Chrome Bootstrap (thin shim)

`src/cli/commands/crawl/runtime.rs` is a small (≈1 KB) shim that delegates to the shared engine resolver `src/crawl/engine::resolve_cdp_ws_url`. All real resolution logic (Docker host rewrite, `/json/version` discovery, `ws://` shortcut, retries) lives in the crawl engine. Always call the bootstrap entry point once before Chrome-mode crawls so each worker doesn't probe independently.

## `crawl/sync_crawl.rs` — Synchronous Crawl (thin shim)

`src/cli/commands/crawl/sync_crawl.rs` is a thin shim. The synchronous crawl logic — 24-hour disk cache, sitemap-only mode, HTTP→Chrome fallback, sitemap backfill — lives in `src/services/crawl_sync.rs`. The shim exists so the CLI handler can stay tiny and the same logic can be reused by other entry points.

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
3. Add `CommandKind::<Name>` variant to `src/core/config/types/enums.rs`
4. Add field(s) to `Config` in `src/core/config/types/config.rs` and `Config::default()` in `config_impls.rs`
5. Add flag(s) to `GlobalArgs` or a new command-specific `Args` struct in `config/cli/`
6. Add the parse logic to `config/parse/build_config.rs`
7. Add match arm to `lib.rs::run_once()`
8. No test-helper updates needed — helpers build on `Config::test_default()` (spreads `..Default::default()`), so a new `Config` field only needs `Config::default()` in `config_impls.rs`
