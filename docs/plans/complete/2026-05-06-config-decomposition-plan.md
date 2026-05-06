# Config Decomposition Plan

**Tracking issue:** A-H-01 (Config god-object decomposition), A-M-02 (Cargo workspace)
**Status:** Phase 1 (TOML loading) implemented in v0.36 — full subconfig migration pending
**Last updated:** 2026-05-04

> **Phase 1 complete (v0.36):** `crates/core/config/parse/toml_config.rs` implements
> `TomlConfig` with `[search]`, `[ask]`, `[tei]`, `[workers]` sections. 6 Config fields
> wired with CLI > env > TOML > default priority. See `config.example.toml` for all
> supported keys. Remaining fields (hnsw_ef, collection, TEI/worker fields) require
> either Config type changes or subsystem-level TOML reads — tracked as follow-up beads.

---

## Table of Contents

1. [Current State](#current-state)
2. [Target: Sub-Config Structs](#target-sub-config-structs)
3. [Migration Path](#migration-path)
4. [Migration Checklist](#migration-checklist)
5. [Cargo Workspace Migration](#cargo-workspace-migration)

---

## Current State

`crates/core/config/types/config.rs` contains a single `Config` struct with 90+ fields spanning:

- Crawl behavior (max_pages, max_depth, include_subdomains, thin-page filtering, ...)
- Chrome/browser settings (14 fields)
- Ask/RAG pipeline tuning (12 fields)
- Service connection URLs (9 fields including secrets)
- AMQP queue names (5 fields)
- Ingest credentials and options (10 fields)
- Performance tuning (concurrency, timeouts, retries)
- Job scheduling/watchdog
- Output/format flags

This is a god-object. It works, but every new field requires updating:
1. `config.rs` (field definition)
2. `config_impls.rs` (`Default` impl)
3. `into_config()` in `parse/build_config.rs`
4. `global_args.rs` (CLI flag)
5. `make_test_config()` helpers in `crates/cli/commands/research.rs`, `search.rs`, and `crates/jobs/common/`

The compiler catches missing struct literal fields **only at test build time**, not at `cargo check` time.

---

## Target: Sub-Config Structs

Sub-config struct definitions are scaffolded in `crates/core/config/types/subconfigs.rs`.

| Struct | Fields | Migration priority |
|--------|--------|-------------------|
| `ServiceUrls` | pg_url, redis_url, amqp_url, qdrant_url, tei_url, openai_*, tavily_api_key | 4 (after Secret<T>) |
| `IngestConfig` | github_token, reddit_*, youtube_max_retries | 1 (fewest dependencies) |
| `AskConfig` | ask_max_context_chars, ask_candidate_limit, ask_chunk_limit, ask_full_docs, ask_backfill_chunks, ask_doc_*, ask_min_relevance_score, ask_authoritative_* | 2 |
| `ChromeConfig` | chrome_remote_url, chrome_proxy, chrome_user_agent, chrome_headless/anti_bot/intercept/stealth/bootstrap, chrome_network_idle_timeout_secs, chrome_wait_for_selector, chrome_screenshot, screenshot_full_page, viewport_*, bypass_csp, accept_invalid_certs | 3 |
| `CrawlConfig` | max_pages, max_depth, include_subdomains, exclude_path_prefix, respect_robots, min_markdown_chars, drop_thin_markdown, discover_sitemaps, sitemap_since_days, sitemap_only, delay_ms, url_whitelist, block_assets, max_page_bytes, redirect_policy_strict, custom_headers, auto_switch_* | 5 |
| `QueueConfig` | shared_queue, crawl_queue, refresh_queue, extract_queue, embed_queue, ingest_queue | 6 |

---

## Migration Path

### Phase 0 (DONE): Scaffold
- Sub-config structs defined in `subconfigs.rs` — no behavioral change
- `ConfigOverrides` added for per-request field overrides (MCP/CLI)
- `Secret<T>` wrapper type added (not yet used on Config fields)
- `Config::test_default()` added to prevent struct-literal fragility in tests

### Phase 1: Migrate `IngestConfig` (lowest risk)
1. Add `ingest: IngestConfig` field to `Config`
2. Remove individual `github_token`, `reddit_*` fields from `Config`
3. Update `Config::default()` — `ingest: IngestConfig::default()`
4. Update `into_config()` to populate `cfg.ingest.*`
5. Update all call sites: `cfg.github_token` → `cfg.ingest.github_token`
6. Update `Config::test_default()` — set `ingest` fields as needed
7. Run `cargo test` to verify

Call sites to update (search for `cfg.github_token`, `cfg.reddit_*`):
- `crates/ingest/github.rs`
- `crates/ingest/reddit.rs`
- `crates/cli/commands/refresh/github.rs`
- `crates/jobs/ingest/ops.rs`

### Phase 2: Migrate `AskConfig`
Follow the same pattern. Call sites are concentrated in:
- `crates/vector/ops/commands/ask/`
- `crates/vector/ops/commands/evaluate.rs`
- `crates/vector/ops/commands/query.rs`

### Phase 3: Migrate `ChromeConfig`
Call sites are concentrated in:
- `crates/crawl/engine.rs`
- `crates/core/config/parse/build_config.rs`

### Phase 4: Migrate `ServiceUrls` (requires Secret<T> first)
Before migrating service URLs, wrap the secret fields using `Secret<T>`:
- `openai_api_key` → `Secret<String>`
- `tavily_api_key` → `Secret<String>`
- `pg_url`, `redis_url`, `amqp_url` — consider `Secret<String>` (high-value targets for log scraping)

All call sites that access these fields must be updated to use `.expose()`.

### Phase 5 & 6: `CrawlConfig`, `QueueConfig`
These have the most call sites. Migrate last.

### Validation (each phase)
```bash
cargo check          # catches missing fields at compile time
cargo test --lib     # catches struct literal test helpers
cargo clippy         # catches unused field warnings
```

---

## Migration Checklist

- [x] Scaffold sub-config struct definitions (`subconfigs.rs`)
- [x] Add `ConfigOverrides` + `Config::apply_overrides()` (`overrides.rs`)
- [x] Add `Secret<T>` wrapper type (`secret.rs`)
- [x] Add `Config::test_default()` (`config_impls.rs`)
- [x] Add `// TODO(A-H-01)` comment to `config.rs` struct file
- [ ] Phase 1: Migrate `IngestConfig` fields
- [ ] Phase 2: Migrate `AskConfig` fields
- [ ] Phase 3: Migrate `ChromeConfig` fields
- [ ] Phase 4: Apply `Secret<T>` to secret fields in `ServiceUrls`
- [ ] Phase 4: Migrate `ServiceUrls` fields
- [ ] Phase 5: Migrate `CrawlConfig` fields
- [ ] Phase 6: Migrate `QueueConfig` fields
- [ ] Remove `make_test_config()` struct literals from research.rs, search.rs, jobs/common/ — replace with `Config::test_default()`

---

## Cargo Workspace Migration

**Tracking issue:** A-M-02

### Current Structure

Single flat Cargo.toml with all crates as internal modules:
```
axon_rust/
├── Cargo.toml          ← single package
├── lib.rs              ← crate root
├── main.rs             ← binary entry
└── crates/
    ├── core/           ← config, http, content, logging, ui, health
    ├── crawl/          ← spider.rs crawl engine
    ├── jobs/           ← AMQP workers
    ├── vector/         ← TEI + Qdrant ops
    ├── mcp/            ← MCP server
    ├── web.rs          ← axum web UI
    └── ingest/         ← github/reddit/youtube ingest
```

### Target: Cargo Workspace

```
axon_rust/
├── Cargo.toml              ← workspace root (members = [...])
├── crates/
│   ├── axon-core/          ← config, http, content, logging, ui, health
│   ├── axon-crawl/         ← spider.rs engine (heavy dep, feature-gated)
│   ├── axon-jobs/          ← AMQP workers (sqlx, lapin)
│   ├── axon-vector/        ← TEI + Qdrant ops (reqwest)
│   ├── axon-mcp/           ← MCP server (rmcp)
│   ├── axon-web/           ← axum web UI (bollard, WebSocket)
│   └── axon-ingest/        ← ingest handlers (octocrab, reddit)
└── apps/
    └── axon/               ← binary crate (thin, ties everything together)
```

### Feature Flags for Heavy Dependencies

Isolate compile-heavy dependencies behind features:

| Dep | Feature | Crate |
|-----|---------|-------|
| spider | `crawl` | axon-crawl |
| bollard | `web` | axon-web |
| octocrab | `ingest-github` | axon-ingest |
| rmcp | `mcp` | axon-mcp |

### Migration Steps

1. Create `Cargo.toml` workspace root (no-op initially, single member)
2. Extract `axon-core` first (no cross-crate deps)
3. Extract `axon-jobs` next (depends only on axon-core)
4. Extract `axon-vector` (depends on axon-core)
5. Extract `axon-crawl` (depends on axon-core, optional axon-jobs)
6. Extract `axon-ingest` (depends on axon-core, axon-vector, axon-jobs)
7. Extract `axon-mcp` (depends on axon-core + all ops crates)
8. Extract `axon-web` (depends on axon-core, websocket, bollard)
9. Create thin `apps/axon` binary that ties everything together
10. Verify `cargo build --release -p axon` produces the same binary

### Key Constraint

`spider.rs` futures are `!Send` — this limits parallelism options in the crawl crate. The workspace split does not change this; `axon-crawl` must remain single-threaded for the core crawl loop.
