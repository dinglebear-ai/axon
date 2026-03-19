# crates/core — Shared Infrastructure
Last Modified: 2026-03-02

Foundational crate. Owns configuration parsing, the `Config` struct, HTTP client + SSRF protection, content transformation, logging, terminal UI, and health checks. Every other crate imports from here.

## Module Layout

```
core/
├── config.rs             # Re-export shim: parse_args, Config, CommandKind, all enums
├── config/
│   ├── cli.rs            # Cli { command: CliCommand, global: GlobalArgs } — clap entry struct (module root)
│   ├── types/
│   │   ├── config.rs     # Config struct — ALL runtime state (100+ fields)
│   │   ├── config_impls.rs  # Config::default() + fmt::Debug (secrets redacted)
│   │   └── enums.rs      # CommandKind, RenderMode, PerformanceProfile, ScrapeFormat, RedditSort, RedditTime
│   ├── cli/
│   │   └── global_args.rs   # All ~60 global flags (#[arg(global=true)])
│   ├── parse/
│   │   ├── build_config.rs  # into_config(): CliArgs → Config (env vars, clamps, normalization)
│   │   ├── performance.rs   # profile_settings(): PerformanceProfile → concrete concurrency values
│   │   ├── excludes.rs      # default_exclude_prefixes(): 110+ default path exclusions
│   │   ├── helpers.rs       # viewport parsing, flag helpers, env_usize_clamped, env_f64_clamped
│   │   └── docker.rs        # normalize_local_service_url(): Docker-inside vs outside detection
│   └── help.rs           # maybe_print_top_level_help_and_exit(): colored help text
├── http.rs               # Module root + re-exports: validate_url, normalize_url, fetch_html, http_client, HttpError
├── http/
│   ├── ssrf.rs           # validate_url() SSRF guard + ssrf_blacklist_patterns()
│   ├── client.rs         # HTTP_CLIENT singleton (LazyLock), http_client(), fetch_html()
│   ├── normalize.rs      # normalize_url(): prepend https:// when scheme missing
│   ├── cdp.rs            # cdp_discovery_url(): Chrome DevTools Protocol URL rewriting
│   ├── error.rs          # HttpError enum: InvalidUrl, BlockedScheme, BlockedHost, BlockedIpRange
│   └── tests.rs          # 38 tests: URL normalization (11) + SSRF validation (27)
├── content.rs            # build_transform_config(), to_markdown(), url_to_filename(), extract_*()
├── content/
│   ├── engine.rs         # ExtractWebConfig + run_extract_with_engine(): deterministic extraction + LLM fallback
│   ├── deterministic.rs  # DeterministicExtractionEngine, DeterministicParser trait, JsonLdParser, OgParser, HtmlTableParser
│   └── tests.rs          # Content transformation and extraction tests
├── logging.rs            # init_tracing(), log_info/log_warn/log_done, SizeRotatingFile
├── ui.rs                 # Spinner, primary/accent/muted(), symbol_for_status(), confirm_destructive()
└── health.rs             # redis_healthy(), BrowserDiagnosticsPattern, Chrome diagnostics env vars
```

## Config Struct (`config/types/config.rs`)

The central state object. Populated once by `into_config()` and passed as `&Config` everywhere.

**Key field groups:**

| Group | Fields |
|-------|--------|
| Command & Input | `command: CommandKind`, `start_url`, `positional: Vec<String>`, `urls_csv`, `url_glob`, `query` |
| Crawl Control | `max_pages` (0 = uncapped), `max_depth` (default 5), `include_subdomains` (default false), `exclude_path_prefix`, `delay_ms` |
| Rendering | `render_mode: RenderMode`, `chrome_remote_url`, `chrome_headless/anti_bot/intercept/stealth/bootstrap` (all default true) |
| Page Filtering | `min_markdown_chars` (default 200), `drop_thin_markdown` (default true), `respect_robots` (default false) |
| Sitemap | `discover_sitemaps` (default true), `sitemap_since_days` (0 = all), `sitemap_only` |
| Vector Store | `collection` (default "cortex"), `embed` (default true), `search_limit` (default 10) |
| Output | `output_dir` (`.cache/axon-rust/output`), `output_path`, `json_output`, `format: ScrapeFormat` |
| Performance | `performance_profile`, `batch_concurrency` (default 16), `wait` (default false), `yes` (default false) |
| Service URLs | `pg_url`, `redis_url`, `amqp_url`, `qdrant_url`, `tei_url`, `openai_*`, `tavily_api_key` |
| Queues | `crawl_queue`, `extract_queue`, `embed_queue`, `ingest_queue`, `refresh_queue` |
| RAG/Ask tuning | `ask_max_context_chars` (120k), `ask_candidate_limit` (64), `ask_chunk_limit` (10), `ask_full_docs` (4), `ask_min_relevance_score` (0.45) — all clamped |
| Ingest credentials | `github_token`, `reddit_client_id`, `reddit_client_secret` |
| Auto-switch | `auto_switch_thin_ratio` (0.60), `auto_switch_min_pages` (10) |
| Spider tuning | `url_whitelist`, `block_assets`, `max_page_bytes`, `redirect_policy_strict`, `bypass_csp`, `accept_invalid_certs`, `custom_headers` |
| Job watchdog | `watchdog_stale_timeout_secs` (300), `watchdog_confirm_secs` (60) |
| Web UI | `serve_port` (default 49000, env: `AXON_SERVE_PORT`) |

**Debug redacts secrets:** `Config`'s `fmt::Debug` replaces `pg_url`, `redis_url`, `amqp_url`, `github_token`, `reddit_client_id`, `reddit_client_secret`, `openai_api_key`, `tavily_api_key` with `[REDACTED]`.

## CommandKind Enum (`config/types/enums.rs`)

28 variants: `Scrape`, `Crawl`, `Refresh`, `Map`, `Extract`, `Search`, `Embed`, `Debug`, `Doctor`, `Query`, `Retrieve`, `Ask`, `Evaluate`, `Suggest`, `Sources`, `Domains`, `Stats`, `Status`, `Dedupe`, `Github`, `Ingest`, `Reddit`, `Youtube`, `Sessions`, `Research`, `Screenshot`, `Mcp`, `Serve`

Other enums: `RenderMode` (Http/Chrome/AutoSwitch), `ScrapeFormat` (Markdown/Html/RawHtml/Json), `PerformanceProfile` (HighStable/Extreme/Balanced/Max), `RedditSort` (Hot/Top/New/Rising), `RedditTime` (Hour/Day/Week/Month/Year/All)

## `into_config()` — CLI → Config Translation (`config/parse/build_config.rs`)

Translates `clap` output into the runtime `Config` struct:
1. Extracts command-specific args (ask_diagnostics, github_include_source (default: true, disabled by `--no-source`), reddit_*, sessions_*, serve_port)
2. Maps `CliCommand` → `(CommandKind, Vec<String> positional)`
3. Normalizes service URLs via `normalize_local_service_url()` (Docker detection)
4. Applies `profile_settings()` for performance defaults
5. Clamps all Ask parameters to their defined ranges
6. Parses viewport string ("1920x1080") into width/height
7. Normalizes exclude-path-prefixes via `default_exclude_prefixes()` + user overrides

## CRITICAL: Adding a Field to `Config`

When adding a **non-`Option`** field:
1. Add the field to `Config` in `config/types/config.rs`
2. Add a default in `Config::default()` in `config_impls.rs`
3. **Update inline struct literals** in:
   - `crates/cli/commands/research.rs` (`make_test_config()`)
   - `crates/cli/commands/search.rs` (`make_test_config()`)
   - Any `make_test_config()` in `crates/jobs/common/`
4. The compiler only catches missing struct literal fields at **test build time**, not `cargo check`.

## Docker URL Rewriting (`config/parse/docker.rs`)

`normalize_local_service_url(url: String) -> String`:
- Checks if `/.dockerenv` exists
- **Inside Docker:** returns URL unchanged (container DNS resolves `axon-postgres`, etc.)
- **Outside Docker:** rewrites container hostnames to `127.0.0.1` with mapped ports:

| Container hostname | Rewrites to |
|--------------------|-------------|
| `axon-postgres` | `127.0.0.1:53432` |
| `axon-redis` | `127.0.0.1:53379` |
| `axon-rabbitmq` | `127.0.0.1:45535` |
| `axon-qdrant` | `127.0.0.1:53333` |
| `axon-tei` | `127.0.0.1:52000` |
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

**TOCTOU residual risk:** `validate_url()` checks IP at parse time; `reqwest` resolves DNS independently at connect time. TTL-0 DNS rebinding can bypass validation. Full mitigation requires connection pinning (not currently implemented). Acceptable risk for internal tooling.

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
- **File:** `AXON_LOG_FILE` (default: `logs/axon.log`), `INFO` level, JSON format
- **Rotation:** `AXON_LOG_MAX_BYTES` (default 10 MB), `AXON_LOG_MAX_FILES` (default 3 files)
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

## Health Checks (`health.rs`)

```rust
// Service health probes (used by doctor command)
redis_healthy(redis_url: &str) -> bool   // 5-second PING timeout
```

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
