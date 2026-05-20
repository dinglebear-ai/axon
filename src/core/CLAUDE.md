# src/core — Shared Infrastructure
Last Modified: 2026-05-16

Foundational crate. Owns configuration parsing, the `Config` struct, HTTP client + SSRF protection, content transformation, logging, terminal UI, and health checks. Every other crate imports from here.

## Module Layout

```
core/
├── config.rs             # Module root: re-exports parse_args, Config, CommandKind, all enums
├── config/
│   ├── cli.rs            # Cli { command, global } — clap entry struct
│   ├── cli/
│   │   └── global_args.rs   # All global flags (#[arg(global=true)])
│   ├── help.rs           # maybe_print_top_level_help_and_exit(): colored help text
│   ├── parse.rs          # Module root for the parse subtree
│   ├── parse/
│   │   ├── build_config.rs  # into_config(): CliArgs -> Config (env vars, clamps, normalization)
│   │   ├── performance.rs   # profile_settings(): PerformanceProfile → concrete concurrency values
│   │   ├── excludes.rs      # default_exclude_prefixes(): default path exclusions
│   │   ├── helpers.rs       # viewport parsing, flag helpers, env_usize_clamped, env_f64_clamped
│   │   ├── docker.rs        # normalize_local_service_url(): Docker-inside vs outside detection
│   │   └── toml_config.rs   # Optional ~/.axon/config.toml loader + merge
│   ├── secret.rs         # Secret-handling helpers used during parse
│   ├── validation.rs     # Validation helpers used during parse
│   ├── types.rs          # Module root for the types subtree
│   └── types/
│       ├── config.rs        # Config struct — top-level runtime state
│       ├── config_impls.rs  # Config::default(), Config::default_minimal(), fmt::Debug (secrets redacted)
│       ├── enums.rs         # CommandKind, RenderMode, PerformanceProfile, ScrapeFormat, RedditSort, RedditTime
│       ├── subconfigs.rs    # Sub-config structs (legacy infra URLs live here, not on Config directly)
│       └── overrides.rs     # CLI/env override application
├── http.rs               # Module root + re-exports
├── http/
│   ├── ssrf.rs           # validate_url() SSRF guard + ssrf_blacklist_patterns() + SsrfBlockingResolver
│   ├── client.rs         # HTTP_CLIENT singleton (LazyLock), http_client(), fetch_html()
│   ├── normalize.rs      # normalize_url(): prepend https:// when scheme missing
│   ├── cdp.rs            # cdp_discovery_url(): Chrome DevTools Protocol URL rewriting
│   ├── error.rs          # HttpError enum
│   ├── headers.rs        # Custom-header parsing helpers
│   ├── tests.rs          # URL normalization + SSRF validation tests
│   └── proptest_tests.rs # Property-based URL/SSRF tests
├── content.rs            # build_transform_config(), to_markdown(), url_to_filename(), extract_*()
├── content/
│   ├── engine.rs                # ExtractWebConfig + run_extract_with_engine(): deterministic extraction + LLM fallback
│   ├── engine_tests.rs          # sidecar tests for engine.rs
│   ├── engine/
│   │   └── chrome.rs                # Chrome-backed extraction helpers
│   ├── deterministic.rs         # DeterministicExtractionEngine + parsers (JsonLd / OG / HtmlTable)
│   ├── deterministic_tests.rs   # sidecar tests for deterministic.rs
│   ├── extract_ladder.rs        # DOM retry ladder — re-extract thin pages with successively richer parsers before Chrome fallback (jh32)
│   ├── extract_ladder_tests.rs  # sidecar tests for extract_ladder.rs
│   ├── extraction.rs            # Top-level extraction orchestration helpers
│   ├── filename.rs              # url_to_filename() — URL → safe output path
│   ├── markdown.rs              # to_markdown() and markdown transformation helpers
│   ├── url_parsing.rs           # URL parse + normalize helpers used by extraction
│   ├── url_parsing_tests.rs     # sidecar tests for url_parsing.rs
│   └── tests.rs                 # Content transformation + extraction tests (legacy inline coverage)
├── health.rs             # browser_diagnostics_pattern() + Chrome diagnostics env vars
├── health/
│   └── doctor.rs         # probe_tei_info, probe_openai, build_browser_runtime
│   └── doctor/
│       └── sqlite.rs        # SQLite-runtime doctor probe orchestration
├── logging.rs            # init_tracing(), log_info/log_warn/log_done
├── logging/
│   └── size_rotating.rs  # SizeRotatingFile: byte-budget rotation writer
├── paths.rs              # Path helpers (data dir, output dir, cache dir)
└── ui.rs                 # Spinner, primary/accent/muted(), symbol_for_status(), confirm_destructive()
```

## Config Struct (`config/types/config.rs`)

The central state object. Populated once by `into_config()` and passed as `&Config` everywhere.

**Key field groups:**

| Group | Fields |
|-------|--------|
| Command & Input | `command: CommandKind`, `start_url`, `positional: Vec<String>`, `urls_csv`, `url_glob`, `query` |
| Crawl Control | `max_pages` (0 = uncapped), `max_depth` (default 10), `include_subdomains` (default false), `exclude_path_prefix`, `delay_ms` |
| Rendering | `render_mode: RenderMode`, `chrome_remote_url`, Chrome bootstrap timing, Chrome network-idle/screenshot settings |
| Page Filtering | `min_markdown_chars` (default 200), `drop_thin_markdown` (default true), `respect_robots` (default false) |
| Sitemap | `discover_sitemaps` (default true), `sitemap_since_days` (0 = all), `sitemap_only` |
| Vector Store | `collection` (default "axon"), `embed` (default true), `search_limit` (default 10) |
| Output | `output_dir` (`.cache/axon-rust/output`), `output_path`, `json_output`, `format: ScrapeFormat` |
| Performance | `performance_profile`, `batch_concurrency` (default 16), `wait` (default false), `yes` (default false) |
| Service URLs | `qdrant_url`, `tei_url`, `tavily_api_key` |
| RAG/Ask tuning | `ask_max_context_chars` (300k), `ask_candidate_limit` (250), `ask_chunk_limit` (20), `ask_full_docs` (6), `ask_min_relevance_score` (0.45) — all clamped |
| Ingest credentials | `github_token`, `reddit_client_id`, `reddit_client_secret` |
| Auto-switch | `auto_switch_thin_ratio` (0.60), `auto_switch_min_pages` (10) |
| Spider tuning | `url_whitelist`, `block_assets`, `max_page_bytes`, `redirect_policy_strict`, `bypass_csp`, `accept_invalid_certs`, `custom_headers` |
| Job watchdog | `watchdog_stale_timeout_secs` (300), `watchdog_confirm_secs` (60) |
| HTTP server | `mcp_http_host` / `mcp_http_port` (default `127.0.0.1:8001`) |
| Job runtime | SQLite-backed in-process jobs. `sqlite_path: PathBuf` defaults to `$AXON_DATA_DIR/jobs.db` → `~/.axon/jobs.db`. `axon_data_base_dir()` defaults to `~/.axon` — flat layout, no nested `axon/` subdir |

**Debug redacts secrets:** `Config`'s `fmt::Debug` redacts credential fields (`github_token`, `reddit_client_id`, `reddit_client_secret`, `tavily_api_key`) with `[REDACTED]`. Sub-configs in `src/core/config/types/subconfigs.rs` redact their own legacy `pg_url`/`redis_url`/`amqp_url` fields independently.

## CommandKind Enum (`config/types/enums.rs`)

36 variants (verify against `src/core/config/types/enums.rs:5-40`):
`Scrape`, `Crawl`, `Watch`, `Map`, `Endpoints`, `Extract`, `Search`, `Embed`, `Debug`, `Doctor`, `Query`, `Retrieve`, `Ask`, `Summarize`, `Evaluate`, `Train`, `Suggest`, `Sources`, `Domains`, `Stats`, `Status`, `Dedupe`, `Ingest`, `Sessions`, `Research`, `Screenshot`, `Completions`, `Mcp`, `Serve`, `Preflight`, `Smoke`, `Stack`, `Setup`, `Migrate`, `Config`, `Sync`.

The legacy `Refresh`, `Github`, `Reddit`, `Youtube` variants were removed: GitHub/Reddit/YouTube are now subtypes routed through `CommandKind::Ingest` and the auto-classifier in `src/ingest/classify.rs`.

Other enums: `RenderMode` (Http/Chrome/AutoSwitch), `ScrapeFormat` (Markdown/Html/RawHtml/Json), `PerformanceProfile` (HighStable/Extreme/Balanced/Max), `RedditSort` (Hot/Top/New/Rising), `RedditTime` (Hour/Day/Week/Month/Year/All)

## `into_config()` — CLI → Config Translation (`config/parse/build_config.rs`)

Translates `clap` output into the runtime `Config` struct:
1. Extracts command-specific args (ask_diagnostics, github_include_source (default: true, disabled by `--no-source`), reddit_*, sessions_*, serve_port)
2. Maps `CliCommand` → `(CommandKind, Vec<String> positional)`
3. Normalizes service URLs via `normalize_local_service_url()` (Docker detection)
4. Applies `profile_settings()` for performance defaults
5. Clamps all Ask parameters to their defined ranges
7. Parses viewport string ("1920x1080") into width/height
8. Normalizes exclude-path-prefixes via `default_exclude_prefixes()` + user overrides

## `Config::default_minimal()`

`Config::default_minimal()` (in `config_impls.rs`) is the test convenience constructor for the current SQLite runtime. It fills service URLs with dummy values and applies default minimal tuning for tests that need a `Config` without real service credentials.

## CRITICAL: Adding a Field to `Config`

When adding a **non-`Option`** field:
1. Add the field to `Config` in `config/types/config.rs`
2. Add a default in `Config::default()` in `config_impls.rs`
3. **Update inline struct literals** in:
   - `src/cli/commands/research.rs` (`make_test_config()`)
   - `src/cli/commands/search.rs` (`make_test_config()`)
   - Any `make_test_config()` in `src/jobs/common/`
4. The compiler only catches missing struct literal fields at **test build time**, not `cargo check`.

## Docker URL Rewriting (`config/parse/docker.rs`)

`normalize_local_service_url(url: String) -> String`:
- Checks if `/.dockerenv` exists
- **Inside Docker:** returns URL unchanged (container DNS resolves service hostnames)
- **Outside Docker:** rewrites container hostnames to `127.0.0.1` with mapped ports:

| Container hostname | Rewrites to |
|--------------------|-------------|
| `axon-qdrant` | `127.0.0.1:53333` |
| `axon-tei` | `127.0.0.1:52000` |
| `axon-ollama` | `127.0.0.1:11434` |
| `axon-chrome` | `127.0.0.1:6000` |

`.env` can safely use container-internal DNS — the CLI rewrites transparently.

## SSRF Protection (`http/ssrf.rs`)

**Primary function:** `pub fn validate_url(url: &str) -> Result<(), HttpError>`

Blocks:
- Non-http/https schemes
- Loopback: 127.0.0.0/8, ::1
- Link-local: 169.254.0.0/16, fe80::/10
- RFC-1918 private: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
- IPv6 unique-local: fc00::/7
- TLDs: `.internal`, `.local`
- Hostnames: `localhost`, `*.localhost`

**IPv6 implementation gotcha:** Use `host_str()` + `host.parse::<IpAddr>()` directly. Do **NOT** match on `spider::url::Host::Ipv4` / `spider::url::Host::Ipv6` enum variants — that pattern fails silently for IPv6 addresses. This was a confirmed production bug.

**DNS rebinding TOCTOU — MITIGATED (v0.32.4):** `validate_url()` checks the hostname at parse time (literal IPs, TLDs, `localhost`). The connect-time TOCTOU window is closed by `SsrfBlockingResolver` in `http/ssrf.rs`, which is wired into the reqwest client via `ClientBuilder::dns_resolver()`. The resolver calls `check_ip()` on every OS-resolved IP at the moment reqwest dials — even a TTL-0 record that flips to `127.0.0.1` after `validate_url()` is caught. Test builds skip the resolver (so httpmock servers on `127.0.0.1` work); the `ALLOW_LOOPBACK` thread-local still guards `validate_url()` in tests.

**Secondary defense:** `ssrf_blacklist_patterns()` returns 12 regex patterns passed to `spider.rs` `with_blacklist_url()` — applied to every discovered URL during crawl, not just seed URLs.

## HTTP Client Singleton (`http/client.rs`)

```rust
pub static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>>
```

- 30-second timeout, initialized once
- **Always use `http_client()`** — never `reqwest::Client::new()` per call. New clients per call exhaust sockets and bypass connection pooling.

```rust
let client = http_client()?;   // correct
```

## Content Transformation (`content.rs`)

### `build_transform_config()` — DO NOT CHANGE THESE TWO SETTINGS

```rust
readability: false   // DO NOT set to true — see below
clean_html: false    // DO NOT set to true — see below
main_content: true   // correct setting for structural extraction
```

**`readability: false`:** Mozilla Readability scores VitePress/sidebar doc layouts as low-quality (no `<article>` structure) and strips them to just the title. Before this fix: 97% thin rate on doc sites. `main_content: true` extracts `<main>`/`<article>`/`role=main` without the scoring penalty.

**`clean_html: false`:** The `clean_html` CSS selector `[class*='ad']` matches Tailwind's `shadow-*` classes (sh**ad**ow contains "ad"). This silently wipes all shadow-styled elements from Tailwind CSS sites (react.dev, shadcn.com, etc.), leaving only the title. `html2md` ignores `<script>`/`<style>` natively, so `clean_html` provides no benefit and causes this destructive side effect.

Both are the result of confirmed production regressions. Do not "improve" them.

### Other Content Functions

| Function | Purpose |
|----------|---------|
| `to_markdown(html)` | HTML → markdown via spider_transformations |
| `url_to_domain(url)` | Extract domain; replace `[`, `]`, `:` with `_` for use as identifiers |
| `redact_url(url)` | Replace username:password in URL with `***` |
| `url_to_filename(url, idx)` | Filesystem-safe filename: `{idx:04d}-{host}{path}.md` (max 80 chars) |
| `extract_meta_description(html)` | Parse `<meta name="description">` (scans only first 8 KB) |
| `extract_links(html, limit)` | Extract http/https hrefs up to limit |
| `extract_loc_values(xml)` | Extract `<loc>` from sitemap XML (case-insensitive) |

### Deterministic Extraction (`content/deterministic.rs`)

`DeterministicExtractionEngine` runs registered parsers against HTML pages:
- `JsonLdParser` — extracts JSON-LD structures
- `OpenGraphParser` — extracts OG metadata
- `HtmlTableParser` — extracts HTML tables
- Results deduplicated by content hash
- Falls back to LLM extraction when deterministic parsers find nothing

## Logging (`logging.rs`)

**Initialize once at startup:**
```rust
init_tracing()  // call in main.rs before anything else
```

**Use these functions in library code (never `println!`):**
```rust
log_info("message")   // → tracing::info!
log_warn("message")   // → tracing::warn!
log_done("message")   // → tracing::info! with status = "done"
```

**Log targets:**
- **Console:** stderr, `WARN` level (override with `RUST_LOG`)
- **File:** `<AXON_LOG_DIR>/<AXON_LOG_FILE>` — defaults to `$AXON_DATA_DIR/logs/axon.log` (i.e. `~/.axon/logs/axon.log`; falls back to `./logs/axon.log` if no data dir is resolvable), `INFO` level, JSON format. `AXON_LOG_FILE` is a bare filename, **not** a path.
- **Rotation:** size-based via `SizeRotatingFile` (`logging/size_rotating.rs`). When the active file exceeds `AXON_LOG_MAX_BYTES` (default 10 MiB / `10485760`), archives shift `<file>.{N-1} → <file>.N` from the top down and a fresh `<file>` is opened. `AXON_LOG_MAX_FILES` (default `3`) caps the number of archives. `max_bytes=0` disables rotation; `max_files=0` truncates without keeping any archive.
- `tracing-appender::non_blocking` serialises writes through one worker thread; the returned `WorkerGuard` MUST be held for the process lifetime (returned by `init_tracing()`).
- CDP noise suppressed: `chromiumoxide::conn::raw_ws::parse_errors=off`

## Terminal UI (`ui.rs`)

```rust
// Spinner
let sp = Spinner::new("Crawling...");
sp.finish("Done");

// Colors
primary("text")     // peach/salmon, bold
accent("text")      // light blue
muted("text")       // dim
subtle("text")      // soft blue

// Status
symbol_for_status("completed")  // ✓ (green)
symbol_for_status("failed")     // ✗ (red)
symbol_for_status("running")    // ◐ (yellow)
symbol_for_status("pending")    // • (cyan)
status_text("completed")        // colored word

// Destructive confirmation (respects --yes and non-TTY)
if !confirm_destructive(cfg, "Delete all jobs?")? { return Ok(()); }
```

**Do not use `println!` for colored output** — use these functions so output is consistent with the rest of the CLI.

## Health Checks (`health.rs` + `health/doctor*`)

`health.rs` itself exports `browser_diagnostics_pattern()` plus the Chrome diagnostics env wiring. Active service probes live under `health/doctor.rs` (e.g. `probe_tei_info`, `probe_openai`, `build_browser_runtime`) and `health/doctor/sqlite.rs` (current SQLite-runtime orchestration). There is no `redis_healthy()` — Redis was removed with the legacy queue runtime.

**Chrome diagnostics (env-controlled):**
- `AXON_CHROME_DIAGNOSTICS=1` — enable screenshot/event capture
- `AXON_CHROME_DIAGNOSTICS_SCREENSHOT=1` — override per-signal
- `AXON_CHROME_DIAGNOSTICS_DIR` — output dir (default: `.cache/chrome-diagnostics`)

## Default URL Path Exclusions (`config/parse/excludes.rs`)

`default_exclude_prefixes()` returns 110+ exclusions by category: auth paths, legal, framework internals (`_next/`, `_astro/`), syndication, marketing, user-generated, locale prefixes (27 languages).

**Key behavior:** `/ja` blocks both `/ja/docs` **and** `/ja-jp/docs` — `/` and `-` are word boundaries in the matcher. Disable all exclusions with `--exclude-path-prefix none`.

## Testing

```bash
cargo test http          # 38 tests: URL normalization + SSRF validation (no services needed)
cargo test content       # content transformation + extraction tests
cargo test health        # 4 tests: flag parsing, defaults (no services needed)
cargo test excludes      # 8 tests: path exclusion normalization
```

All core tests are pure logic — no external services required.
