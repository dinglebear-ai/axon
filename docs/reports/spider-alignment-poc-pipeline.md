# Spider Alignment: POC and Content Pipeline Analysis

**Report date:** 2026-02-19
**Analyst:** poc-pipeline-analyst (Haiku 4.5)
**Scope:** spider_to_axon_poc.rs (1938 lines), change_detection.rs, content_pipeline.rs, transform_markdown.rs, css_scrape.rs, download.rs, serde.rs vs. axon_rust production code

---

## Table of Contents

1. [spider_to_axon_poc.rs — Full Walkthrough](#1-spider_to_axon_pocrs--full-walkthrough)
2. [Gap Analysis](#2-gap-analysis)
3. [change_detection.rs — What It Demonstrates](#3-change_detectionrs--what-it-demonstrates)
4. [content_pipeline.rs — Transformation Stages](#4-content_pipeliners--transformation-stages)
5. [css_scrape.rs — CSS Selector Support](#5-css_scraprs--css-selector-support)
6. [transform_markdown.rs — Markdown Patterns](#6-transform_markdownrs--markdown-patterns)
7. [download.rs — Binary/Media Download Patterns](#7-downloadrs--binarymedia-download-patterns)
8. [serde.rs — Serialization Patterns](#8-serdrs--serialization-patterns)
9. [Priority Implementation List](#9-priority-implementation-list)

---

## 1. spider_to_axon_poc.rs — Full Walkthrough

### 1.1 Overview

The POC is a 1938-line standalone Rust binary that demonstrates the intended integration between spider.rs and an external "axon" (previously a TypeScript CLI). It is **the reference design** for what axon_rust should be — a self-contained Rust crawler that writes markdown, then embeds via axon's CLI. In the current axon_rust, the embedding and crawl are already native; the POC shows the crawl patterns we should match.

### 1.2 Enums and Configuration (lines 35–691)

**RenderMode (lines 35–59):**
```rust
enum RenderMode { Http, Chrome, AutoSwitch }
```
Matches axon_rust's `RenderMode` in `crates/core/config.rs`. Identical semantics.

**RunMode (lines 61–75):**
```rust
enum RunMode { Ingest, Map }
```
axon_rust has separate `crawl` and `map` subcommands. The POC's `RunMode` is a flag to the same binary. **Gap:** axon_rust has no `--mode` flag to switch between ingest and map within one invocation. Minor gap.

**PerformanceProfile (lines 77–95):**
Identical to axon_rust's performance profile enum.

**ChromeFallback (lines 97–113):**
```rust
enum ChromeFallback { Auto, Always, Never }
```
**Gap:** axon_rust does NOT have a `--chrome-fallback` flag. The current `should_fallback_to_chrome()` in `engine.rs` is always `Auto` behavior — there is no `Never` or `Always` override. This is a real feature gap.

**Config struct (lines 115–150):**
Notable fields axon_rust is MISSING or differs on:
- `english_only: bool` — path-prefix based i18n filtering. axon_rust has `exclude_path_prefix` but no `english_only` flag.
- `chrome_connection_url: Option<String>` — axon_rust has this via `--chrome-connection-url`.
- `chrome_bootstrap_local: bool` — **axon_rust does NOT auto-launch Chrome**. The POC manages a local Chrome process.
- `chrome_debug_port: u16` — axon_rust has no local Chrome bootstrap.
- `chrome_binary: Option<String>` — same as above.
- `skip_embed: bool` — axon_rust: no equivalent CLI flag (embedding is always attempted if TEI is configured).
- `exclude_path_prefix: Vec<String>` — axon_rust has this.
- `shared_queue: bool` — axon_rust has `--shared-queue`.

**Default exclude prefixes (lines 300–330):**
The POC hard-codes 28 i18n path prefixes (`/fr`, `/de`, `/es`, etc.) as defaults. **axon_rust has no default exclude list** — users must specify via `--exclude-path-prefix`. This means axon_rust crawls non-English paths by default, which bloats the index.

### 1.3 URL Utilities (lines 693–777)

**`is_excluded_url_path()` (lines 693–702):**
```rust
fn is_excluded_url_path(url: &str, excludes: &[String]) -> bool {
    let path = Url::parse(url)
        .ok()
        .map(|u| u.path().to_string())
        .unwrap_or_else(|| "/".to_string());
    excludes.iter().any(|prefix| path.starts_with(prefix))
}
```
The POC uses simple `starts_with()`. **axon_rust's `engine.rs` uses a more correct boundary-aware check (`is_path_prefix_excluded`)** that ensures `/fr` doesn't match `/french`. axon_rust is strictly better here.

**`sanitize_stem()` (lines 709–724):**
Uses lowercase alphanumeric with `-` replacement and collapse of consecutive dashes. Also trims leading/trailing dashes. axon_rust's `url_to_filename()` in `content.rs` is simpler — it doesn't collapse consecutive dashes and doesn't trim them. Minor difference.

**`url_to_filename()` (lines 762–777):**
```rust
fn url_to_filename(url: &str, idx: u32) -> String {
    // uses sanitize_stem + DefaultHasher for uniqueness
    format!("{:04}-{stem}-{hash:016x}.md", idx)
}
```
The POC appends a 16-hex-char hash suffix for guaranteed uniqueness. **axon_rust's `url_to_filename()` in `content.rs:51-73` does NOT include a hash.** This means two URLs that produce the same stem (e.g., `/products/1` and `/products-1`) would collide in the output directory. The POC's approach is strictly safer.

### 1.4 TransformConfig (lines 787–807)

```rust
fn build_transform_config() -> TransformConfig {
    let mut config = TransformConfig::default();
    config.return_format = ReturnFormat::Markdown;
    config.readability = true;
    config.clean_html = true;
    config.main_content = true;
    config.filter_images = true;
    config.filter_svg = true;
    config
}

fn build_transform_fallback_config() -> TransformConfig {
    let mut config = TransformConfig::default();
    config.return_format = ReturnFormat::Markdown;
    config.readability = false;     // ← key difference
    config.clean_html = true;
    config.main_content = false;    // ← key difference
    config.filter_images = true;
    config.filter_svg = true;
    config
}
```

The POC maintains TWO transform configs: primary (readability=true, main_content=true) and fallback (readability=false, main_content=false). When primary produces a thin result, it retries with the fallback to recover more content from pages where readability extraction fails. **axon_rust has only ONE transform config in `content.rs:11-20`** — identical to the primary. The fallback config and dual-pass retry logic are missing.

**This is a high-impact gap**: pages that produce thin markdown due to aggressive readability filtering would be silently dropped in axon_rust. The POC would salvage them.

### 1.5 Dual-Pass Transform Retry (lines 1361–1396)

```rust
let markdown = transform_content_input(input, &transform_config);
let mut markdown_owned = markdown.trim().to_string();
let mut markdown_chars = markdown_owned.chars().count();
if markdown_chars < min_chars && page.get_html_bytes_u8().len() > 4000 {
    // Retry with fallback config
    let fallback_markdown = transform_content_input(fallback_input, &transform_fallback_config);
    let fallback_trimmed = fallback_markdown.trim().to_string();
    let fallback_chars = fallback_trimmed.chars().count();
    if fallback_chars > markdown_chars {
        markdown_owned = fallback_trimmed;
        markdown_chars = fallback_chars;
    }
}
```

Conditions for fallback: markdown is thin AND original HTML is substantial (> 4000 bytes). The fallback only wins if it produces MORE content. axon_rust's `to_markdown()` in `content.rs:22-34` is a single-pass with no fallback. **This is the most impactful missing pattern.**

### 1.6 JSONL Manifest (lines 1292–1294, 1415–1424)

```rust
let manifest_path = output_dir.join("manifest.jsonl");
let manifest_file = File::create(&manifest_path)?;
let mut manifest_writer = BufWriter::new(manifest_file);
// ...
let record = serde_json::json!({
    "url": url,
    "file_path": file_path.to_string_lossy(),
    "markdown_chars": markdown_chars,
    "crawl_mode": summary.crawl_mode,
});
writeln!(manifest_writer, "{}", record)?;
```

The POC writes a `manifest.jsonl` alongside the markdown files, one JSON record per page, tracking URL, file path, char count, and crawl mode. **axon_rust writes markdown files but no manifest.** The manifest enables post-hoc analysis, filtering, and re-embedding without recrawling.

### 1.7 Chrome Runtime Management (lines 987–1060)

```rust
async fn ensure_chrome_runtime(cfg: &Config) -> Result<Option<LocalChromeGuard>, Box<dyn Error>> {
    let endpoint = /* ... */;
    if chrome_endpoint_ready(endpoint, Duration::from_secs(2)).await {
        return Ok(None);
    }
    if !cfg.chrome_bootstrap_local {
        return Err(/* ... */);
    }
    let chrome_binary = detect_chrome_binary(cfg).ok_or_else(|| /* ... */)?;
    // Launch headless Chrome with temp profile dir
    let mut child = Command::new(chrome_binary)
        .arg("--headless")
        .arg("--no-sandbox")
        // ...
        .spawn()?;
    // Wait up to 10 seconds for CDP ready
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if chrome_endpoint_ready(endpoint, Duration::from_secs(2)).await {
            return Ok(Some(LocalChromeGuard { child: Some(child), profile_dir }));
        }
    }
}
```

`LocalChromeGuard` implements `Drop` to kill the process and clean up the temp profile dir. **axon_rust does not manage Chrome lifecycle at all.** If Chrome isn't already running, Chrome-mode crawls silently fail or hang. This is a significant operational gap.

### 1.8 Sitemap Crawler (lines 1076–1228)

The POC's `crawl_sitemap_urls()` is more sophisticated than axon_rust's:
- Parses `robots.txt` first to discover sitemap URLs, then processes sitemaps
- Handles both sitemap index files and leaf sitemaps
- Uses `JoinSet` for concurrent robots.txt and sitemap fetching
- Tracks `discovered_hosts` for subdomain sitemaps
- Enqueues standard URLs: `/sitemap.xml`, `/sitemap_index.xml`, `/sitemap-index.xml` per host

axon_rust's `crawl_sitemap_urls()` in `engine.rs` is functionally similar but lacks:
- `robots.txt` parsing for sitemap directives
- Multi-host subdomain sitemap discovery
- Explicit `discovered_hosts` tracking

### 1.9 Website Configuration (lines 1230–1280)

```rust
fn configure_website(cfg: &Config, crawl_mode: RenderMode) -> Result<Website, Box<dyn Error>> {
    // ...
    if !cfg.exclude_path_prefix.is_empty() {
        let blacklist_patterns: Vec<spider::compact_str::CompactString> =
            build_exclude_blacklist_patterns(&cfg.start_url, &cfg.exclude_path_prefix)
                .into_iter()
                .map(Into::into)
                .collect();
        website.with_blacklist_url(Some(blacklist_patterns));
    }
    // ...
}
```

The POC passes blacklist URL patterns directly to `spider::Website` via `with_blacklist_url()`. **axon_rust does NOT use `with_blacklist_url()`** — it only filters in the subscriber loop after pages are already fetched. The POC approach prevents fetching excluded pages at all, saving bandwidth and time.

### 1.10 `crawl_and_write_markdown()` (lines 1282–1468)

Key patterns:
- Resets output directory on each run (clean slate)
- Creates both `markdown/` subdir and `manifest.jsonl`
- Uses `website.subscribe(4096)` with 4096-buffer broadcast
- Runs subscriber in a `tokio::spawn` task concurrently with crawl
- Handles `Lagged` broadcast errors gracefully (continues with warning)
- Uses `summary.total_pages_seen % progress_interval` for throttled progress
- After crawl: calls `website.unsubscribe()` then joins the task

axon_rust's `engine.rs` `run_crawl_once()` has similar structure but doesn't handle `Lagged` errors explicitly.

### 1.11 Backfill (lines 1589–1673)

```rust
async fn backfill_sitemap_markdown(
    cfg: &Config,
    output_dir: &Path,
    existing_urls: &HashSet<String>,
    mut next_index: u32,
    sitemap_urls: Vec<String>,
) -> Result<(u32, u32), Box<dyn Error>> {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(worker_limit));
    let mut join_set = tokio::task::JoinSet::new();
    for url in sitemap_urls {
        if existing_urls.contains(&url) || is_excluded_url_path(&url, &cfg.exclude_path_prefix) {
            skipped_existing += 1;
            continue;
        }
        // spawn with semaphore permit, fetch + transform
    }
    // Collect results, sort by URL, write deterministically
    worker_results.sort_by(|a, b| a.0.cmp(&b.0));
```

Results are **sorted by URL before writing** — deterministic output ordering. axon_rust's `append_sitemap_backfill()` in `engine.rs` has this same sort. Pattern is matched.

### 1.12 Chrome Fallback Logic (lines 1675–1701)

```rust
fn should_fallback_to_chrome(summary: &CrawlSummary, max_pages: u32) -> bool {
    if summary.markdown_files_written == 0 { return true; }
    let thin_ratio = summary.thin_ratio();
    let very_low_coverage = summary.markdown_files_written < (max_pages / 10).max(10);
    thin_ratio > 0.60 || very_low_coverage
}

fn should_run_chrome_fallback(cfg, summary, max_pages, initial_mode) -> bool {
    if matches!(initial_mode, RenderMode::Chrome) { return false; }
    match cfg.chrome_fallback {
        ChromeFallback::Never => false,
        ChromeFallback::Always => true,
        ChromeFallback::Auto => should_fallback_to_chrome(summary, max_pages),
    }
}
```

The POC has `ChromeFallback::Never/Always/Auto` wrapping the thin-ratio check. axon_rust's `should_fallback_to_chrome()` in `engine.rs` is identical to `ChromeFallback::Auto` but without the Never/Always override.

### 1.13 `run_axon_embed()` (lines 1703–1739)

The POC invokes the OLD TypeScript axon CLI as a subprocess:
```rust
fn run_axon_embed(cfg: &Config, markdown_dir: &Path) -> Result<(), Box<dyn Error>> {
    let command = format!(
        "{} embed {} --collection {}",
        base,
        quote_shell(&absolute_markdown_dir.to_string_lossy()),
        quote_shell(&cfg.collection)
    );
    let status = Command::new("sh").arg("-lc").arg(&command)
        .current_dir(&cfg.axon_dir).status()?;
}
```

This is the **integration bridge** the POC was designed to test. In axon_rust, this is replaced by native Rust embedding — the `embed` command directly calls TEI and Qdrant. This subprocess approach is obsolete in axon_rust.

### 1.14 Main Flow (lines 1762–1938)

The `main()` demonstrates two parallel tasks:
```rust
let (first_output_res, sitemap_res) = tokio::join!(
    crawl_and_write_markdown(&cfg, selected_mode, &first_output_dir),
    crawl_sitemap_urls(&cfg)
);
```

Crawling and sitemap discovery run concurrently. axon_rust does the same in `engine.rs`. Pattern is matched.

---

## 2. Gap Analysis

| Pattern | In POC | In axon_rust | Gap Level |
|---------|--------|--------------|-----------|
| Dual-pass transform (primary + fallback config) | Yes (lines 1361–1396) | No — single pass only | HIGH |
| Hash suffix in filenames for uniqueness | Yes (line 776) | No — stem only, can collide | MEDIUM |
| JSONL manifest alongside markdown | Yes (lines 1292, 1415–1424) | No | MEDIUM |
| Default i18n path exclude list | Yes (28 prefixes) | No — empty default | MEDIUM |
| `with_blacklist_url()` passed to spider | Yes (lines 1255–1260) | No — only post-fetch filtering | MEDIUM |
| `--chrome-fallback never/always/auto` flag | Yes | No — always Auto | LOW |
| Local Chrome auto-launch + lifecycle guard | Yes (lines 987–1060) | No | LOW |
| `robots.txt` sitemap directive parsing | Yes (lines 1134–1142) | No | LOW |
| Multi-host subdomain sitemap discovery | Yes (lines 1204–1207) | No | LOW |
| `Lagged` broadcast error handling | Yes (lines 1322–1329) | Partial | LOW |
| `skip_embed` flag | Yes | No | LOW |
| Concurrent crawl + sitemap (tokio::join!) | Yes | Yes | MATCHED |
| Chrome fallback thin-ratio logic | Yes | Yes | MATCHED |
| Sitemap backfill with semaphore | Yes | Yes | MATCHED |
| URL path exclusion filtering | Yes | Yes (more correct) | MATCHED |
| Performance profiles | Yes | Yes | MATCHED |
| TransformConfig (primary) | Yes | Yes | MATCHED |

---

## 3. change_detection.rs — What It Demonstrates

### Pattern: AI-Powered Snapshot Diffing

`change_detection.rs` (390 lines) shows a monitoring/change-detection workflow:

1. **Crawl two snapshots** of two different pages using Spider + `RemoteMultimodalConfigs`
2. **Extract structured data** (book titles, prices) via LLM from each crawl
3. **Compare snapshots** via a second LLM call that returns a structured `ChangeReport`
4. **Persist** snapshots and report as JSON to disk

```rust
// Lines 54-70: RemoteMultimodalConfigs setup
fn extraction_config(api_url: &str, model: &str, api_key: &str) -> RemoteMultimodalConfigs {
    let mut mm = RemoteMultimodalConfigs::new(api_url, model);
    mm.cfg.extra_ai_data = true;
    mm.cfg.include_html = true;
    mm.cfg.include_title = true;
    mm.cfg.include_url = true;
    mm.cfg.max_rounds = 1;
    mm.cfg.request_json_object = true;
    mm.cfg.extraction_prompt = Some("...".to_string());
    mm
}

// Lines 86-88: Consuming per-page AI data
if let Some(ref ai_data) = page.extra_remote_multimodal_data {
    for result in ai_data.iter() {
        let content = &result.content_output;
```

**Key API:** `page.extra_remote_multimodal_data` — this is spider's native per-page LLM integration via `RemoteMultimodalConfigs`. The LLM is called as part of the crawl, with the full page HTML submitted automatically.

### Does axon_rust support this?

**No.** axon_rust's `extract` command uses its own HTTP client to call the OpenAI API directly (`extract_items_fallback()` in `content.rs:393-458`). It does not use `RemoteMultimodalConfigs`.

The spider native integration is superior for extraction use cases because:
- LLM is called concurrently per-page during the crawl
- No separate HTTP client needed — spider handles auth and retry
- Token usage is tracked in `result.usage`
- Multi-round extraction (`max_rounds`) is supported natively

**To implement this pattern in axon_rust:** Add `with_remote_multimodal()` support to the extract command's website configuration. This would replace the current "crawl then extract serially" pattern with "extract during crawl."

---

## 4. content_pipeline.rs — Transformation Stages

### Pattern: SEO-Enriched Content Metadata Pipeline

`content_pipeline.rs` (289 lines) demonstrates a pipeline that:
1. Crawls pages with spider
2. Applies an LLM prompt to each page: "Transform this book page into SEO-optimized content metadata"
3. Extracts typed fields: `title`, `summary`, `tags`, `meta_description`, `link_anchors`
4. Tracks token usage per page and aggregates totals
5. Writes per-page JSON files to `./storage/content_pipeline/`

```rust
// Lines 29-30: ContentMetadata struct
struct ContentMetadata {
    url: String,
    title: String,
    summary: String,
    tags: Vec<String>,
    meta_description: String,
    link_anchors: Vec<String>,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}
```

### What axon_rust skips or simplifies:

1. **No SEO field extraction.** axon_rust's extract pipeline produces arbitrary JSON from LLM responses but does not have a built-in "SEO metadata" extraction schema.
2. **No per-page token accounting.** axon_rust's `ExtractionMetrics` in `content.rs:134-143` does track `prompt_tokens`, `completion_tokens`, `total_tokens` — but only in aggregate across a job. No per-page breakdown.
3. **No per-page JSON file output.** axon_rust's extract command writes results to the DB (`result_json` column) or stdout, not individual JSON files.
4. **Token cost estimation.** axon_rust has `estimate_llm_cost_usd()` in `content.rs:460-477`. This is MATCHED.

### Key API difference:

content_pipeline.rs uses `RemoteMultimodalConfigs` (native spider LLM), whereas axon_rust uses a separate HTTP client to call the API post-crawl. The spider-native approach is more efficient.

---

## 5. css_scrape.rs — CSS Selector Support

### Pattern: Streaming CSS Map Extraction

```rust
// Lines 22-27
let map = QueryCSSMap::from([("list", QueryCSSSelectSet::from(["ul", "ol"]))]);
let data = css_query_select_map_streamed(&res.get_html(), &build_selectors(map)).await;
```

`css_scrape.rs` uses `spider_utils::css_query_select_map_streamed()` — a streaming CSS selector engine from the `spider_utils` crate. The user defines named selector groups (`"list" -> ["ul", "ol"]`), and gets back structured data matching those selectors.

### Does axon_rust support CSS selection?

**No.** axon_rust has no CSS selector support at all. The extraction pipeline uses:
- Pattern 1: Deterministic parsers (JSON-LD, OpenGraph, HTML tables) — string search based
- Pattern 2: LLM fallback — sends full HTML to LLM

Adding `spider_utils::css_query_select_map_streamed` would enable structured scraping without LLM costs for pages with predictable DOM structure (product listings, tables, navigation).

**Crate to add:** `spider_utils` from the spider workspace.

---

## 6. transform_markdown.rs — Markdown Patterns

`transform_markdown.rs` is **fully commented out** (lines 1–49). The code is dead/example-only. It demonstrates the older `transform_content(&res, &conf, &None, &None, &None)` API which takes a `&Page` reference rather than `TransformInput`.

The newer API (`transform_content_input`) used in both the POC and axon_rust is the correct one.

**Notable:** The transform_markdown example uses `tokio::io::AsyncWriteExt` to write directly to stdout — a pattern axon_rust does not use (it writes to files or returns strings).

**Gap:** None. The file is commented out and documents a deprecated API. axon_rust is already using the correct `transform_content_input` API.

---

## 7. download.rs — Binary/Media Download Patterns

### Pattern: Raw HTML/Binary Download to Files

```rust
// Lines 29-54
website.scrape().await;
for page in website.get_pages().unwrap().iter() {
    let download_file = percent_encode(page.get_url().as_bytes(), NON_ALPHANUMERIC).to_string();
    // ...
    if let Some(b) = page.get_bytes() {
        file.write_all(b).unwrap_or_default();
    }
}
```

`download.rs` uses `website.scrape().await` (not `crawl().await`) — this fetches pages but does not follow links. Uses `page.get_bytes()` for raw binary content, not `page.get_html()`. Files are percent-encoded URL strings.

### vs. axon_rust batch:

axon_rust's `batch` command fetches URLs individually via `reqwest` HTTP client, not via `spider::Website::scrape()`. Spider's `scrape()` method batches concurrently with spider's internal concurrency controls, which may be more efficient.

**Key gaps:**
1. **`page.get_bytes()`** — binary content download. axon_rust only handles text (markdown). No binary file download support.
2. **`website.scrape()` for batch** — axon_rust's batch uses manual `JoinSet` + `reqwest`. Using spider's `scrape()` could simplify the batch implementation and inherit spider's retry/timeout/concurrency logic.
3. **Percent-encoded filenames** — `spider::percent_encoding` is available. axon_rust's `url_to_filename()` uses alphanumeric replacement instead.

---

## 8. serde.rs — Serialization Patterns

### Pattern: FlexBuffers Serialization

```rust
// Lines 15-21
use spider::serde::ser::Serialize;
let mut s = flexbuffers::FlexbufferSerializer::new();
links.serialize(&mut s).unwrap();
println!("{:?}", s)
```

`serde.rs` demonstrates serializing `get_all_links_visited()` (a `HashSet<CompactString>`) to FlexBuffers format — a binary, schemaless serialization format (Facebook's FlatBuffers variant).

**Does axon_rust use this?** No. axon_rust uses `serde_json` exclusively. FlexBuffers would be relevant if axon_rust needed to persist crawl state or link sets in binary format for speed/size benefits.

**Gap level:** LOW — the use case is niche. JSON is appropriate for axon_rust's current needs (DB columns, output files). FlexBuffers is an optimization for high-throughput link set persistence.

---

## 9. Priority Implementation List

Ordered by impact on crawl quality and data completeness:

### Priority 1 (HIGH): Dual-Pass Transform Fallback

**What:** Add `build_transform_fallback_config()` to `content.rs` and apply dual-pass logic in `engine.rs`'s page processing loop.

**Why:** Pages that fail readability extraction (SPA shells, heavily JS-rendered layouts, pages with atypical structure) are silently dropped. The fallback config (readability=false, main_content=false) often recovers usable content.

**Where to add:**
- `crates/core/content.rs`: add `build_transform_fallback_config()` (lines 787–807 of POC)
- `crates/crawl/engine.rs`: wrap `to_markdown()` calls with the dual-pass pattern (lines 1361–1396 of POC)

**Effort:** ~2 hours.

### Priority 2 (MEDIUM): Hash Suffix in `url_to_filename()`

**What:** Append a DefaultHasher-based hex suffix to filenames.

**Why:** Two URLs with identical path segments (after sanitization) produce the same filename, causing silent overwrites. The POC generates `{:04}-{stem}-{hash:016x}.md`.

**Where:** `crates/core/content.rs:51-73` (`url_to_filename()`).

**Effort:** ~30 minutes.

### Priority 3 (MEDIUM): JSONL Manifest Output

**What:** Alongside the `markdown/` directory, write a `manifest.jsonl` tracking URL, file path, char count, and crawl mode for each page written.

**Why:** Enables post-crawl analysis, filtering, selective re-embedding, and debugging without re-crawling. Useful for `--mode map` output as well.

**Where:** `crates/crawl/engine.rs` — in the crawl subscriber task, after successful markdown write.

**Effort:** ~2 hours.

### Priority 4 (MEDIUM): Default i18n Exclude Path Prefixes

**What:** When `--exclude-path-prefix` is not specified, default to the 28 i18n path prefixes from the POC (lines 300–330).

**Why:** Without exclusion, axon_rust crawls `/fr/`, `/de/`, `/es/`, etc. by default, polluting the vector index with non-English content and wasting capacity.

**Where:** `crates/core/config.rs` — in the `Config` default or `parse_args()`.

**Effort:** ~30 minutes.

### Priority 5 (MEDIUM): `with_blacklist_url()` in Website Configuration

**What:** Pass exclude path prefixes as regex blacklist patterns to `spider::Website::with_blacklist_url()` so excluded URLs are never fetched.

**Why:** Currently, axon_rust fetches excluded pages and filters them in the subscriber. The spider-native approach prevents fetching entirely — saving bandwidth and time.

**Where:** `crates/crawl/engine.rs` — `configure_website()` or equivalent.

**Effort:** ~1 hour (includes building the regex patterns as in POC lines 740–760).

### Priority 6 (LOW): `--chrome-fallback never/always/auto` Flag

**What:** Add a `chrome_fallback` field to `Config` and a CLI flag.

**Why:** Users may want to force Chrome for all sites (JS-heavy apps) or never use Chrome (Chrome not available). Currently no override.

**Where:** `crates/core/config.rs` + `crates/crawl/engine.rs:should_fallback_to_chrome`.

**Effort:** ~1 hour.

### Priority 7 (LOW): `RemoteMultimodalConfigs` for Extract Command

**What:** Use `spider::features::automation::RemoteMultimodalConfigs` in the extract command's website configuration instead of (or in addition to) the current separate HTTP client approach.

**Why:** Native spider LLM integration runs extraction concurrently per-page during crawl. Eliminates the current "crawl then extract" serial pattern.

**Where:** `crates/cli/commands/extract.rs` + `crates/core/content.rs:run_extract_with_engine`.

**Effort:** ~1 day.

### Priority 8 (LOW): CSS Selector Extraction via `spider_utils`

**What:** Add `spider_utils` dependency and expose `css_query_select_map_streamed()` in the extract command.

**Why:** Enables structured scraping without LLM for pages with predictable DOM structure.

**Effort:** ~1 day (including CLI design for selector map input).

### Priority 9 (LOW): robots.txt Sitemap Directive Parsing

**What:** Parse `robots.txt` before sitemap discovery to find `Sitemap:` directives.

**Why:** Many sites announce sitemaps in robots.txt. The POC finds these; axon_rust only tries standard paths.

**Where:** `crates/crawl/engine.rs:crawl_sitemap_urls`.

**Effort:** ~2 hours.

### Priority 10 (LOW): Local Chrome Bootstrap

**What:** Add `detect_chrome_binary()`, `chrome_endpoint_ready()`, and `LocalChromeGuard` to axon_rust.

**Why:** When Chrome mode is requested but no CDP endpoint is running, axon_rust currently fails silently. The POC auto-starts Chrome and cleans up on exit.

**Where:** New module `crates/core/chrome.rs` or additions to `crates/crawl/engine.rs`.

**Effort:** ~4 hours.

---

## Conclusion

The POC (`spider_to_axon_poc.rs`) is an extremely high-fidelity reference design. axon_rust implements the core patterns correctly — the overall architecture, performance profiles, auto-switch logic, sitemap backfill, and concurrent crawl+sitemap are all present and working.

The most impactful gaps are:
1. **Dual-pass transform fallback** — recovers content that primary transform drops
2. **Hash suffix in filenames** — prevents silent file collisions
3. **JSONL manifest** — enables post-hoc analysis without recrawling
4. **Default i18n excludes** — prevents index pollution from non-English paths
5. **`with_blacklist_url()`** — avoids fetching excluded pages at all

These five items represent roughly a day of work and would close the most meaningful quality gaps between the POC's design intent and axon_rust's production behavior.
