# llms.txt Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Probe `/llms.txt` at a site root during crawl and `map`, parse its markdown links, and feed them into the same candidate-backfill path that sitemap discovery already uses — augmenting (never replacing) sitemap-derived URLs.

**Architecture:** Mirror the existing sitemap discovery path. A new `src/crawl/engine/llms_txt.rs` module fetches `/llms.txt` with the SSRF-guarded `build_client`, extracts links with `pulldown-cmark`, resolves them against the file's base URL with `url::Url::join`, scopes them with a helper shared from `sitemap.rs`, caps the count, and hands the resulting `Vec<String>` to the generic `append_candidate_backfill`. The crawl runner discovers sitemap + llms.txt concurrently and runs ONE merged backfill pass; `map_with_sitemap` merges llms.txt URLs into its result set.

**Tech Stack:** Rust 2024, tokio, reqwest, `pulldown-cmark` 0.13.4, `url` 2.5.8, `spider`, SQLite jobs, `httpmock` (tests). Tracks beads `axon_rust-6s51.1`–`.5`.

---

## Critical project conventions (read before starting)

- **`axon <cmd>` defaults to SERVER MODE** — it forwards to a running `axon serve` container, so a freshly-built local binary has NO effect unless you pass `--local`. This plan validates via `cargo test`, which always exercises local code. (Memory: `axon-cli-server-mode-default-routing`.)
- **No `mod.rs`.** Module root is `foo.rs` beside `foo/`. Submodule files live in `foo/bar.rs`.
- **Tests live in sidecar `_tests.rs` files**, declared with `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` in the source, using `use super::*;`.
- **Monolith policy:** changed `.rs` files ≤500 lines; functions warn at 80 / fail at 120. `**/*_tests.*` is exempt.
- **Commit cadence:** commit after each task. A reset watchdog can fire on long-uncommitted working trees — if you hit it, `git commit --no-verify` immediately after a task's writes is acceptable (memory: `axon-webclaw-extraction-patterns-2026-05-15`). Prefer passing `just verify` normally.
- Run `cargo fmt` before every commit; `cargo clippy` must be clean.

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `src/core/config/types/config.rs` | `discover_llms_txt: bool`, `max_llms_txt_urls: usize` fields | 1 |
| `src/core/config/types/config_impls.rs` | Default + Debug for the new fields | 1 |
| `src/core/config/types/subconfigs.rs` | `ScrapeConfig` fields + defaults | 1 |
| `src/core/config/types/overrides.rs` | `ConfigOverrides` fields + apply | 1 |
| `src/core/config/parse/toml_config.rs` | TOML `Option<...>` fields | 1 |
| `src/core/config/parse/build_config/config_literal.rs` | TOML→Config wiring | 1 |
| `src/crawl/engine/sitemap.rs` | extract shared `loc_in_scope`; `.md`/`.txt` passthrough; body-size cap | 2, 4, 6 |
| `src/crawl/engine/llms_txt.rs` (NEW) | fetch + parse + scope + cap + `append_llms_txt_backfill` | 3, 5 |
| `src/crawl/engine.rs` | `mod llms_txt;` + re-exports | 5 |
| `src/jobs/workers/runners/crawl.rs` | concurrent merged backfill pass | 7 |
| `src/crawl/engine/map/strategy.rs` | merge llms.txt into map discovery | 8 |
| `src/mcp/schema/requests.rs`, `src/mcp/server/common.rs`, `src/mcp/thin_client.rs`, `src/web/server/handlers/rest/types.rs`, `.../async_jobs.rs`, `.../rest/async_jobs.rs`, `src/web/panel_first_run.rs`, `src/cli/server_mode/plan.rs` | request-surface fields | 9 |
| `Cargo.toml`, `CHANGELOG.md`, `plugins/axon/.claude-plugin/plugin.json`, `README.md`, `config.example.toml`, `docs/CONFIG.md`, `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md`, `CLAUDE.md` | docs + version bump (4.15.0 → 4.16.0) | 10 |

**Execution waves (dependency-driven):** Tasks 1–6 (bead .1 + .5) first; Tasks 7, 8, 9 are independent of each other (disjoint files) and depend only on 1–6; Task 10 last.

---

### Task 1: Config fields `discover_llms_txt` + `max_llms_txt_urls`

Mirrors the existing `discover_sitemaps` / `max_sitemaps` plumbing across all config layers. Runtime default ON (`true`/`512`).

**Files:**
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/config_impls.rs`
- Modify: `src/core/config/types/subconfigs.rs`
- Modify: `src/core/config/types/overrides.rs`
- Modify: `src/core/config/parse/toml_config.rs`
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Test: `src/core/config/parse/build_config_tests.rs` (extend existing)

- [ ] **Step 1: Add the canonical Config fields**

In `src/core/config/types/config.rs`, immediately after the `max_sitemaps` field (line ~127):

```rust
    /// Probe `/llms.txt` at the site root and backfill its listed URLs after the main crawl,
    /// and merge them into `map` discovery. TOML: `scrape.discover-llms-txt`.
    pub discover_llms_txt: bool,

    /// Maximum number of URLs to take from a single `/llms.txt` after scope filtering
    /// (0 = unlimited). A flat llms.txt has no document-count bound, so this caps fan-out.
    /// TOML: `scrape.max-llms-txt-urls`.
    pub max_llms_txt_urls: usize,
```

- [ ] **Step 2: Add Default + Debug entries**

In `src/core/config/types/config_impls.rs`, in the `Default` impl after `max_sitemaps: 512,` (line ~50):

```rust
            discover_llms_txt: true,
            max_llms_txt_urls: 512,
```

And in the `Debug` impl after the `max_sitemaps` field (line ~304):

```rust
            .field("discover_llms_txt", &self.discover_llms_txt)
            .field("max_llms_txt_urls", &self.max_llms_txt_urls)
```

- [ ] **Step 3: Add ScrapeConfig subconfig fields + defaults**

In `src/core/config/types/subconfigs.rs`, after `discover_sitemaps`/`sitemap_since_days` (line ~201):

```rust
    pub discover_llms_txt: bool,
    pub max_llms_txt_urls: usize,
```

And in its default block (line ~226):

```rust
            discover_llms_txt: true,
            max_llms_txt_urls: 512,
```

- [ ] **Step 4: Add ConfigOverrides fields + apply**

In `src/core/config/types/overrides.rs`, after `max_sitemaps` (line ~99):

```rust
    /// Override `Config::discover_llms_txt`.
    pub discover_llms_txt: Option<bool>,

    /// Override `Config::max_llms_txt_urls`.
    pub max_llms_txt_urls: Option<usize>,
```

And in the apply function after the `max_sitemaps` block (line ~213):

```rust
        if let Some(v) = overrides.discover_llms_txt {
            cfg.discover_llms_txt = v;
        }
        if let Some(v) = overrides.max_llms_txt_urls {
            cfg.max_llms_txt_urls = v;
        }
```

- [ ] **Step 5: Add TOML parse fields**

In `src/core/config/parse/toml_config.rs`, after the `max_sitemaps` field (line ~51):

```rust
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
```

Note: confirm the serde rename attribute pattern the sibling fields use (e.g. `#[serde(rename = "discover-llms-txt")]` or a container `rename_all = "kebab-case"`). Match it exactly so `scrape.discover-llms-txt` parses.

- [ ] **Step 6: Wire TOML → Config**

In `src/core/config/parse/build_config/config_literal.rs`, after the `max_sitemaps` line (line ~96):

```rust
    cfg.discover_llms_txt = scrape.discover_llms_txt.unwrap_or(true);
    cfg.max_llms_txt_urls = scrape.max_llms_txt_urls.unwrap_or(512);
```

- [ ] **Step 7: Write the failing config-parse test**

In `src/core/config/parse/build_config_tests.rs`, add a test mirroring the existing `[scrape]` TOML test at line ~201:

```rust
#[test]
fn parses_llms_txt_scrape_keys() {
    let toml = "[scrape]\ndiscover-llms-txt = false\nmax-llms-txt-urls = 42\n";
    let cfg = build_config_from_toml_str(toml).expect("config builds");
    assert!(!cfg.discover_llms_txt);
    assert_eq!(cfg.max_llms_txt_urls, 42);
}
```

(Use whatever helper the sibling test at line 201 uses to build a `Config` from a TOML string — match its name exactly.)

- [ ] **Step 8: Run test — expect compile failure first, then FAIL→PASS**

Run: `cargo test -p axon parses_llms_txt_scrape_keys -- --nocapture`
Expected after Steps 1–6: PASS. (If you wrote the test before the fields, it fails to compile — that is the "red".)

- [ ] **Step 9: Verify nothing else broke (struct-literal completeness)**

Run: `cargo test --no-run --workspace --lib --locked`
Expected: compiles. (Per CLAUDE.md, missing `Config { .. }` struct-literal updates in test helpers fail ONLY at test-compile, not `cargo check` — this catches them.)

- [ ] **Step 10: Commit**

```bash
cargo fmt
git add src/core/config
git commit -m "feat(config): add discover_llms_txt + max_llms_txt_urls"
```

---

### Task 2: Extract shared scope helper `loc_in_scope`

`sitemap_loc_in_scope` (private, `sitemap.rs:102`) becomes a `pub(crate)` helper so llms.txt reuses identical host/path/exclude scope semantics. Pure refactor — sitemap tests must stay green.

**Files:**
- Modify: `src/crawl/engine/sitemap.rs:102-131` (rename + visibility)
- Test: `src/crawl/engine/sitemap_tests.rs` (existing, callers at ~lines 9-60)

- [ ] **Step 1: Rename and expose the function**

In `src/crawl/engine/sitemap.rs`, change the signature at line 102 from:

```rust
fn sitemap_loc_in_scope(
    cfg: &Config,
    loc: &str,
    start_host: &str,
    start_path: &str,
    scoped_to_root: bool,
) -> Option<String> {
```

to:

```rust
/// Returns the canonicalized URL if `loc` is in scope for a crawl/discovery rooted at
/// `start_host`/`start_path`, else `None`. Shared by sitemap and llms.txt discovery.
/// Same-host by default; honors `cfg.include_subdomains` and `cfg.exclude_path_prefix`.
pub(crate) fn loc_in_scope(
    cfg: &Config,
    loc: &str,
    start_host: &str,
    start_path: &str,
    scoped_to_root: bool,
) -> Option<String> {
```

- [ ] **Step 2: Update the two internal callers**

In `sitemap.rs`, update the call sites (around lines 182 and 194, inside `process_sitemap_batch`) from `sitemap_loc_in_scope(...)` to `loc_in_scope(...)`. Use grep to find them:

Run: `grep -n "sitemap_loc_in_scope" src/crawl/engine/sitemap.rs`
Replace each remaining reference (including any in `sitemap_tests.rs`) with `loc_in_scope`.

- [ ] **Step 3: Run sitemap tests to verify the rename is behavior-preserving**

Run: `cargo test -p axon sitemap -- --nocapture`
Expected: PASS (same count as before the rename). If `sitemap_tests.rs` referenced `sitemap_loc_in_scope` by name, update those references too.

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add src/crawl/engine/sitemap.rs src/crawl/engine/sitemap_tests.rs
git commit -m "refactor(crawl): extract shared loc_in_scope from sitemap"
```

---

### Task 3: `src/crawl/engine/llms_txt.rs` — fetch, parse, scope, cap

The core discovery function. Fetches `/llms.txt` with the SSRF-guarded client, rejects soft-404s, strips BOM, extracts links via `pulldown-cmark`, resolves relatives, scopes, dedupes, and caps.

**Files:**
- Create: `src/crawl/engine/llms_txt.rs`
- Create: `src/crawl/engine/llms_txt_tests.rs`
- Modify: `src/crawl/engine.rs` (declare the module — re-exports come in Task 5)

- [ ] **Step 1: Declare the module**

In `src/crawl/engine.rs`, beside the existing `pub(crate) mod sitemap;` (line ~6):

```rust
pub(crate) mod llms_txt;
```

- [ ] **Step 2: Write the failing parser test**

Create `src/crawl/engine/llms_txt_tests.rs`:

```rust
use super::*;

const FIXTURE: &str = "\u{feff}# Example Docs\n\n> A short summary.\n\nSome intro prose with an inline [ignored-in-prose-too](https://example.com/intro.md) link.\n\n## Docs\n\n- [Getting Started](/docs/start.md): the basics\n- [Guide](guide.md)\n- [External](https://other.com/x.md)\n- [Email](mailto:hi@example.com)\n- [Anchor](#section)\n\n## Optional\n\n- [Extra](/docs/extra.md)\n";

#[test]
fn extracts_and_resolves_links() {
    let links = extract_llms_txt_links(FIXTURE, "https://example.com/llms.txt");
    // Relative resolved against base; mailto/anchor dropped; external kept (scope happens later).
    assert!(links.contains(&"https://example.com/docs/start.md".to_string()));
    assert!(links.contains(&"https://example.com/guide.md".to_string()));
    assert!(links.contains(&"https://other.com/x.md".to_string()));
    assert!(links.contains(&"https://example.com/docs/extra.md".to_string()));
    assert!(!links.iter().any(|u| u.starts_with("mailto:")));
    assert!(!links.iter().any(|u| u.contains("#section")));
}

#[test]
fn rejects_soft_404_html() {
    // text without a leading '# ' H1 is not a valid llms.txt
    assert!(!looks_like_llms_txt("<!DOCTYPE html><html>not found</html>"));
    assert!(looks_like_llms_txt("# Title\n\n> x"));
    // BOM-prefixed still recognized
    assert!(looks_like_llms_txt("\u{feff}# Title"));
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p axon llms_txt -- --nocapture`
Expected: FAIL to compile ("cannot find function `extract_llms_txt_links`").

- [ ] **Step 4: Write the module with parse + scope + discovery**

Create `src/crawl/engine/llms_txt.rs`:

```rust
use super::sitemap::fetch_text_with_retry;
use super::{BackfillStats, append_candidate_backfill, loc_in_scope};
use crate::core::config::Config;
use crate::core::http::build_client;
use crate::core::logging::log_info;
use crate::crawl::engine::CrawlSummary;
use pulldown_cmark::{Event, Parser, Tag};
use spider::url::Url;
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;

fn request_timeout_secs(cfg: &Config) -> u64 {
    cfg.request_timeout_ms.unwrap_or(30_000).div_ceil(1000).max(1)
}

/// Strip a leading UTF-8 BOM and check for a markdown H1 — a cheap soft-404 guard.
/// Many CMS hosts serve an HTML "not found" page at /llms.txt with HTTP 200.
pub(crate) fn looks_like_llms_txt(body: &str) -> bool {
    let s = body.strip_prefix('\u{feff}').unwrap_or(body).trim_start();
    s.starts_with("# ") || s.starts_with("#\t")
}

/// Extract every markdown hyperlink destination, resolve relatives against `base_url`,
/// drop non-fetchable schemes, and strip fragments. Returns absolute http(s) URLs.
pub(crate) fn extract_llms_txt_links(body: &str, base_url: &str) -> Vec<String> {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let Ok(base) = Url::parse(base_url) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for event in Parser::new(body) {
        let Event::Start(Tag::Link { dest_url, .. }) = event else {
            continue;
        };
        let dest = dest_url.trim();
        // Skip fragments and non-fetchable schemes before resolution.
        if dest.is_empty()
            || dest.starts_with('#')
            || dest.starts_with("mailto:")
            || dest.starts_with("tel:")
            || dest.starts_with("javascript:")
            || dest.starts_with("data:")
        {
            continue;
        }
        // base.join resolves relative, absolute-path, protocol-relative, and absolute URLs.
        let Ok(mut resolved) = base.join(dest) else {
            continue;
        };
        if resolved.scheme() != "http" && resolved.scheme() != "https" {
            continue;
        }
        resolved.set_fragment(None);
        out.push(resolved.to_string());
    }
    out
}

/// Probe `/llms.txt` at the site root, parse links, scope + dedupe + cap them.
pub async fn discover_llms_txt_urls(
    cfg: &Config,
    start_url: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let parsed = Url::parse(start_url)
        .map_err(|e| format!("invalid start URL for llms.txt discovery {start_url}: {e}"))?;
    let scheme = parsed.scheme();
    let bare_host = parsed
        .host_str()
        .ok_or_else(|| format!("missing host in llms.txt start URL {start_url}"))?
        .to_string();
    let host = match parsed.port() {
        Some(port) => format!("{bare_host}:{port}"),
        None => bare_host.clone(),
    };
    let llms_url = format!("{scheme}://{host}/llms.txt");

    // SSRF-guarded client (redirect revalidation + DNS-rebind guard live here).
    let client = build_client(request_timeout_secs(cfg), None)
        .map_err(|e| format!("failed to build HTTP client for llms.txt discovery: {e}"))?;

    let Some(body) = fetch_text_with_retry(&client, &llms_url, cfg.fetch_retries, cfg.retry_backoff_ms).await
    else {
        return Ok(Vec::new());
    };
    if !looks_like_llms_txt(&body) {
        log_info(&format!("command=llms_txt no_valid_file url={llms_url}"));
        return Ok(Vec::new());
    }

    // Scope: mirror sitemap's scoped_to_root derivation from the start path.
    let start_path = parsed.path().trim_end_matches('/').to_string();
    let segment_count = start_path.split('/').filter(|s| !s.is_empty()).count();
    let scoped_to_root = start_path.is_empty() || segment_count <= 1;

    let mut seen = HashSet::new();
    let mut urls: Vec<String> = extract_llms_txt_links(&body, &llms_url)
        .into_iter()
        .filter_map(|loc| loc_in_scope(cfg, &loc, &bare_host, &start_path, scoped_to_root))
        .filter(|u| seen.insert(u.clone()))
        .collect();

    // Mandatory fan-out cap (0 = unlimited).
    if cfg.max_llms_txt_urls != 0 && urls.len() > cfg.max_llms_txt_urls {
        urls.truncate(cfg.max_llms_txt_urls);
    }
    urls.sort();
    log_info(&format!("command=llms_txt discovered_urls={} url={llms_url}", urls.len()));
    Ok(urls)
}

#[cfg(test)]
#[path = "llms_txt_tests.rs"]
mod tests;
```

Note: `loc_in_scope` already calls `canonicalize_url_for_dedupe` internally and returns the canonical form, so the `seen` set dedupes canonical URLs. Confirm `append_candidate_backfill` and `BackfillStats` are re-exportable from `super` (they live in `sitemap.rs`, re-exported by `engine.rs` — Task 5 adds the llms.txt re-exports; the `use super::{...}` here resolves through `engine.rs`'s existing `pub(crate) use sitemap::...`). If a path doesn't resolve, import directly from `super::sitemap::{append_candidate_backfill, ...}`.

- [ ] **Step 5: Run the parser tests to verify they pass**

Run: `cargo test -p axon llms_txt -- --nocapture`
Expected: `extracts_and_resolves_links` and `rejects_soft_404_html` PASS.

- [ ] **Step 6: Add scope + cap tests**

Append to `src/crawl/engine/llms_txt_tests.rs`:

```rust
fn cfg_for(host_include_subdomains: bool, max: usize) -> crate::core::config::Config {
    let mut c = crate::core::config::Config::default();
    c.include_subdomains = host_include_subdomains;
    c.max_llms_txt_urls = max;
    c
}

#[test]
fn scope_drops_offhost_and_caps() {
    let cfg = cfg_for(false, 1);
    // Two same-host links + one off-host; cap=1 keeps only one same-host after sort.
    let body = "# T\n\n## S\n- [a](/a.md)\n- [b](/b.md)\n- [ext](https://other.com/c.md)\n";
    // discover_llms_txt_urls needs network; test the pure pieces instead:
    let links = extract_llms_txt_links(body, "https://example.com/llms.txt");
    let scoped: Vec<String> = links
        .into_iter()
        .filter_map(|l| loc_in_scope(&cfg, &l, "example.com", "", true))
        .collect();
    assert!(scoped.iter().all(|u| u.contains("example.com")));
    assert_eq!(scoped.len(), 2, "off-host dropped, two same-host kept");
}
```

Run: `cargo test -p axon llms_txt -- --nocapture`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt
git add src/crawl/engine/llms_txt.rs src/crawl/engine/llms_txt_tests.rs src/crawl/engine.rs
git commit -m "feat(crawl): llms.txt discovery — parse, resolve, scope, cap"
```

---

### Task 4: `.md`/`.txt` passthrough in `fetch_and_convert_backfill_url`

**Why:** Confirmed by tracing — `fetch_and_convert_backfill_url` (`sitemap.rs:371`) runs `to_markdown(main_content: true)` unconditionally, which strips a raw `.md` body (no `<main>`/`<article>`) below `min_markdown_chars` and drops it as thin. llms.txt overwhelmingly links raw `.md`, so without this the feature discards most of its value.

**Files:**
- Modify: `src/crawl/engine/sitemap.rs:371-388` (`fetch_and_convert_backfill_url`)
- Test: `src/crawl/engine/sitemap_tests.rs`

- [ ] **Step 1: Write the failing fidelity test**

In `src/crawl/engine/sitemap_tests.rs`, add a test for the passthrough decision. Since `fetch_and_convert_backfill_url` is private and fetches over HTTP, test the decision helper directly (introduced in Step 3):

```rust
#[test]
fn markdown_url_uses_passthrough() {
    assert!(is_already_markdown("https://x.com/docs/api.md"));
    assert!(is_already_markdown("https://x.com/llms.txt"));
    assert!(is_already_markdown("https://x.com/a/b.MD")); // case-insensitive
    assert!(!is_already_markdown("https://x.com/docs/page"));
    assert!(!is_already_markdown("https://x.com/index.html"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axon markdown_url_uses_passthrough -- --nocapture`
Expected: FAIL to compile ("cannot find function `is_already_markdown`").

- [ ] **Step 3: Add the helper and branch in `sitemap.rs`**

Add this helper near `fetch_and_convert_backfill_url` in `src/crawl/engine/sitemap.rs`:

```rust
/// Raw markdown/text targets (e.g. llms.txt-listed `.md` docs) must skip the HTML→markdown
/// transform — `to_markdown(main_content:true)` would strip them to nothing and drop them as thin.
pub(crate) fn is_already_markdown(url: &str) -> bool {
    // Compare only the path, ignoring query/fragment.
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt")
}
```

Then change the body of `fetch_and_convert_backfill_url` (the `to_markdown` line at ~383) from:

```rust
    let trimmed = to_markdown(&html, selector_config.as_ref());
```

to:

```rust
    let trimmed = if is_already_markdown(&url) {
        // Already markdown/plaintext — pass through verbatim, do not run the HTML transform.
        html.trim().to_string()
    } else {
        to_markdown(&html, selector_config.as_ref())
    };
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p axon markdown_url_uses_passthrough -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Run the full sitemap suite (no regression)**

Run: `cargo test -p axon sitemap -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/crawl/engine/sitemap.rs src/crawl/engine/sitemap_tests.rs
git commit -m "feat(crawl): pass through raw .md/.txt targets in backfill"
```

---

### Task 5: `append_llms_txt_backfill` wrapper + engine re-exports

Wraps discovery + `append_candidate_backfill`, mirroring `append_sitemap_backfill` (`sitemap.rs:556`).

**Files:**
- Modify: `src/crawl/engine/llms_txt.rs` (add the wrapper)
- Modify: `src/crawl/engine.rs` (re-export)

- [ ] **Step 1: Add the wrapper to `llms_txt.rs`**

Append to `src/crawl/engine/llms_txt.rs` (before the `#[cfg(test)]` block):

```rust
/// Discover llms.txt URLs, fetch new ones, convert to markdown, and append to the manifest.
/// Mirrors `append_sitemap_backfill`. Updates `summary.markdown_files`/`thin_pages`.
pub async fn append_llms_txt_backfill(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
    seen_urls: &HashSet<String>,
    summary: &mut CrawlSummary,
) -> Result<BackfillStats, Box<dyn Error>> {
    let urls = discover_llms_txt_urls(cfg, start_url).await?;
    if urls.is_empty() {
        return Ok(BackfillStats::default());
    }
    let discovered = urls.len();
    let (mut stats, _) = append_candidate_backfill(cfg, output_dir, seen_urls, urls, summary).await?;
    stats.discovered_urls = discovered;
    log_info(&format!("llms_txt backfill_complete urls_added={}", stats.written));
    Ok(stats)
}
```

- [ ] **Step 2: Re-export from `engine.rs`**

In `src/crawl/engine.rs`, beside the sitemap re-exports (lines ~36-38):

```rust
pub use llms_txt::append_llms_txt_backfill;
pub(crate) use llms_txt::discover_llms_txt_urls;
```

- [ ] **Step 3: Verify the crate builds and llms.txt tests pass**

Run: `cargo build --bin axon`
Expected: clean build.
Run: `cargo test -p axon llms_txt -- --nocapture`
Expected: PASS.

- [ ] **Step 4: Clippy gate**

Run: `cargo clippy -p axon --lib 2>&1 | tail -5`
Expected: no warnings on the new module.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src/crawl/engine/llms_txt.rs src/crawl/engine.rs
git commit -m "feat(crawl): append_llms_txt_backfill wrapper + re-exports"
```

---

### Task 6: Response body-size cap in `fetch_text_with_retry` (bead .5)

Shared hardening: a multi-GB `/llms.txt` or `/sitemap.xml` must not OOM the worker. Default cap 512 KB.

**Files:**
- Modify: `src/crawl/engine/sitemap.rs:46-77` (`fetch_text_with_retry`)
- Test: `src/crawl/engine/sitemap_tests.rs`

- [ ] **Step 1: Write the failing cap test (httpmock)**

In `src/crawl/engine/sitemap_tests.rs` (httpmock is already used in this crate's tests — match the existing import style):

```rust
#[tokio::test]
async fn fetch_text_rejects_oversized_body() {
    let server = httpmock::MockServer::start();
    let big = "x".repeat(600 * 1024); // 600 KB > 512 KB cap
    let m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/big.txt");
        then.status(200).body(&big);
    });
    crate::core::http::set_allow_loopback(true);
    let client = crate::core::http::build_client(5, None).unwrap();
    let url = server.url("/big.txt");
    let got = fetch_text_with_retry(&client, &url, 0, 0).await;
    crate::core::http::set_allow_loopback(false);
    m.assert();
    assert!(got.is_none(), "oversized body must be rejected, not buffered");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axon fetch_text_rejects_oversized_body -- --nocapture`
Expected: FAIL (current code buffers the full body and returns `Some`).

- [ ] **Step 3: Add a streaming size cap**

In `src/crawl/engine/sitemap.rs`, add a const near the top of the module:

```rust
/// Max bytes read from a discovery document (/llms.txt, /sitemap.xml). Guards against
/// OOM from a malicious/misconfigured host. 512 KB comfortably exceeds real llms.txt
/// (≤~100 KB in practice) and typical sitemaps.
const DISCOVERY_MAX_BODY_BYTES: u64 = 512 * 1024;
```

In `fetch_text_with_retry`, replace the success branch `return resp.text().await.ok();` (line ~61) with a Content-Length fast-reject + streamed read:

```rust
                if status.is_success() {
                    if resp
                        .content_length()
                        .is_some_and(|len| len > DISCOVERY_MAX_BODY_BYTES)
                    {
                        return None;
                    }
                    let mut collected: Vec<u8> = Vec::new();
                    let mut stream = resp;
                    loop {
                        match stream.chunk().await {
                            Ok(Some(chunk)) => {
                                if collected.len() as u64 + chunk.len() as u64
                                    > DISCOVERY_MAX_BODY_BYTES
                                {
                                    return None; // exceeded cap mid-stream
                                }
                                collected.extend_from_slice(&chunk);
                            }
                            Ok(None) => break,
                            Err(_) => return None,
                        }
                    }
                    return String::from_utf8(collected).ok();
                }
```

Note: `Response::chunk()` consumes the response by `&mut`, so bind it mutably (`let mut stream = resp;`). If the existing code shape makes `resp` already-owned, drop the rebinding.

- [ ] **Step 4: Run the cap test + full suite**

Run: `cargo test -p axon fetch_text_rejects_oversized_body -- --nocapture`
Expected: PASS.
Run: `cargo test -p axon sitemap llms_txt -- --nocapture`
Expected: PASS (under-cap bodies still returned intact).

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src/crawl/engine/sitemap.rs src/crawl/engine/sitemap_tests.rs
git commit -m "feat(crawl): cap discovery-doc body size in fetch_text_with_retry"
```

---

### Task 7: Crawl runner — concurrent merged backfill pass (bead .2)

Replace the single sitemap backfill phase with one pass that discovers sitemap + llms.txt concurrently, unions/dedupes the candidates, and runs ONE `append_candidate_backfill`. This overrides any "sequential after sitemap" idea — it overlaps discovery, avoids a second manifest read, and prevents cross-source double-fetch.

**Files:**
- Modify: `src/jobs/workers/runners/crawl.rs` (`maybe_append_sitemap_backfill` → unified merged backfill; call site at ~line 88)
- Test: add to the crawl runner's sidecar test file (find it: `ls src/jobs/workers/runners/crawl_tests.rs 2>/dev/null` or the inline path declared in `crawl.rs`)

- [ ] **Step 1: Read the current backfill function + call site**

Run: `sed -n '160,215p' src/jobs/workers/runners/crawl.rs`
Confirm the exact `maybe_append_sitemap_backfill` signature, the cancellation `select!` block (lines ~192-199), and how its return string feeds `build_crawl_result_json` (call site ~line 88, result wiring ~line 121).

- [ ] **Step 2: Add the merged candidate discovery + single backfill**

Rename/extend `maybe_append_sitemap_backfill` to a unified function (keep the name if you prefer minimal call-site churn, but have it cover both sources). Core logic, preserving the existing cancellation `select!` and warn-and-continue policy:

```rust
// Discover both sources concurrently (each gated on its flag), union + dedupe, single pass.
let (sitemap_res, llms_res) = tokio::join!(
    async {
        if effective_cfg.discover_sitemaps {
            crate::crawl::engine::discover_sitemap_urls(effective_cfg, url)
                .await
                .map(|d| d.urls)
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    },
    async {
        if effective_cfg.discover_llms_txt {
            crate::crawl::engine::discover_llms_txt_urls(effective_cfg, url)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    },
);

// Union + canonical dedupe.
let mut seen_merge = std::collections::HashSet::new();
let mut merged: Vec<String> = sitemap_res
    .into_iter()
    .chain(llms_res)
    .filter_map(|u| crate::crawl::engine::canonicalize_url_for_dedupe(&u))
    .filter(|u| seen_merge.insert(u.clone()))
    .collect();

// Combined fan-out cap: bound total backfill volume regardless of per-source caps.
let combined_cap = effective_cfg
    .max_sitemaps
    .max(effective_cfg.max_llms_txt_urls);
if combined_cap != 0 && merged.len() > combined_cap {
    merged.truncate(combined_cap);
}

if merged.is_empty() {
    return Ok(None);
}
// Single fetch/convert/manifest pass over the merged candidate set.
crate::crawl::engine::append_candidate_backfill(
    effective_cfg, job_output_dir, seen_urls, merged, summary,
)
.await
.map_err(|e| e.to_string())
```

Notes:
- `canonicalize_url_for_dedupe` and `append_candidate_backfill` need to be re-exported from `crate::crawl::engine` (confirm with `grep -n "canonicalize_url_for_dedupe\|append_candidate_backfill" src/crawl/engine.rs`; add `pub(crate) use sitemap::canonicalize_url_for_dedupe;` if absent).
- Keep this whole block inside the existing cancellation `select!` so a cancel still requests spider shutdown.
- The combined cap uses `max(...)` of the two per-source caps as the simplest bound that adds no new config field (per the bead's discretion note). If you prefer an explicit `max_backfill_candidates` field, coordinate with bead .5's config edits.

- [ ] **Step 3: Write a runner test (sidecar)**

Add a test asserting that with both a sitemap fixture and an llms.txt fixture served by an httpmock server, the manifest gains the llms.txt-listed URLs, and a URL present in both is written once. Mirror the existing crawl-runner test setup (find a sibling test for backfill and copy its scaffolding). If a full runner test is too heavyweight, at minimum unit-test the union+cap helper by extracting the merge into a small `fn merge_candidates(sitemap: Vec<String>, llms: Vec<String>, cap: usize) -> Vec<String>` and test that:

```rust
#[test]
fn merge_candidates_unions_dedupes_and_caps() {
    let s = vec!["https://x.com/a".to_string(), "https://x.com/b".to_string()];
    let l = vec!["https://x.com/b".to_string(), "https://x.com/c".to_string()];
    let out = merge_candidates(s, l, 0);
    assert_eq!(out.len(), 3, "b deduped");
    let capped = merge_candidates(
        vec!["https://x.com/a".into()],
        vec!["https://x.com/b".into(), "https://x.com/c".into()],
        2,
    );
    assert_eq!(capped.len(), 2);
}
```

- [ ] **Step 4: Run runner + map tests**

Run: `cargo test -p axon crawl -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src/jobs/workers/runners/crawl.rs src/jobs/workers/runners/crawl_tests.rs
git commit -m "feat(crawl): merge sitemap+llms.txt into one backfill pass"
```

---

### Task 8: Map discovery — merge llms.txt into `map_with_sitemap` (bead .2)

`map_with_sitemap` early-returns sitemap-only when a sitemap exists (`strategy.rs:211-219`). Merge llms.txt URLs into that success branch and add a sitemap-empty branch so a curated llms.txt isn't lost.

**Files:**
- Modify: `src/crawl/engine/map/strategy.rs:161-260`
- Test: `src/cli/commands/map/map_sitemap_tests.rs` (mirror the skip test at line 658) or `strategy`'s sidecar

- [ ] **Step 1: Read the current function**

Run: `sed -n '161,260p' src/crawl/engine/map/strategy.rs`
Identify the `tokio::join!` (line ~164), the sitemap-success early return (lines ~209-219), and the fallback branches (~230, ~246).

- [ ] **Step 2: Add a concurrent llms.txt discovery arm (warn-and-continue)**

Extend the existing `tokio::join!` to add a third arm that returns `Vec<String>` and never propagates errors:

```rust
let (seed_result, sitemap_result, llms_result) = tokio::join!(
    /* existing seed arm */,
    async {
        if cfg.discover_sitemaps {
            discover_sitemap_urls(cfg, start_url).await
        } else {
            Ok(SitemapDiscovery::default())
        }
    },
    async {
        if cfg.discover_llms_txt {
            crate::crawl::engine::discover_llms_txt_urls(cfg, start_url)
                .await
                .unwrap_or_default() // warn-and-continue: never fail the map call
        } else {
            Vec::new()
        }
    },
);
let llms_urls: Vec<String> = llms_result;
```

(Match the exact existing arm expressions when you edit — do not remove the seed/sitemap arms.)

- [ ] **Step 3: Merge into the sitemap-success branch BEFORE its early return**

In the `if sitemap_discovery.parsed_sitemap_documents > 0 {` block (line ~209), where it currently merges only sitemap URLs (line ~210) and returns with `map_source: "sitemap"`:

```rust
if sitemap_discovery.parsed_sitemap_documents > 0 {
    let mut combined = sitemap_discovery.urls;
    combined.extend(llms_urls.iter().cloned());
    let urls = merge_map_candidate_urls(Vec::new(), combined, &scope, true);
    let map_source = if llms_urls.is_empty() { "sitemap" } else { "sitemap+llms" };
    // ... return MapResult { ..., map_source: map_source.to_string(), ... }
}
```

`merge_map_candidate_urls` canonicalizes/dedupes, so a URL in both sources collapses to one.

- [ ] **Step 4: Add a sitemap-empty / llms.txt-present branch**

Immediately after the sitemap-success block and before the crawl/structure fallback (line ~230), add:

```rust
if !llms_urls.is_empty() {
    let urls = merge_map_candidate_urls(Vec::new(), llms_urls, &scope, true);
    if !urls.is_empty() {
        // return MapResult { urls, map_source: "llms".to_string(), ... }
        // mirror the field shape of the sitemap-success MapResult return.
    }
}
```

- [ ] **Step 5: Write the failing map test**

In `src/cli/commands/map/map_sitemap_tests.rs`, add (mirroring the existing httpmock map tests):

```rust
#[tokio::test]
async fn map_unions_sitemap_and_llms_txt_deduped() {
    // Serve a sitemap with /a, /b and an llms.txt linking /b, /c.
    // Assert MapResult.urls == {/a,/b,/c} (b once) and map_source == "sitemap+llms".
    // ... (set up httpmock + loopback flag like the sibling sitemap map tests)
}

#[tokio::test]
async fn map_skips_llms_txt_when_disabled() {
    // cfg.discover_llms_txt = false → no GET to /llms.txt (assert mock NOT hit).
}
```

- [ ] **Step 6: Run map tests**

Run: `cargo test -p axon map -- --nocapture`
Expected: PASS. The dedup test guards the early-return-drop regression.

- [ ] **Step 7: Commit**

```bash
cargo fmt
git add src/crawl/engine/map/strategy.rs src/cli/commands/map/map_sitemap_tests.rs
git commit -m "feat(map): merge llms.txt URLs into sitemap discovery"
```

---

### Task 9: Request surfaces — MCP + REST + web + first-run (bead .3)

Expose `discover_llms_txt` + `max_llms_txt_urls` everywhere `discover_sitemaps`/`max_sitemaps` appear. Pure plumbing.

**Files:**
- Modify: `src/mcp/schema/requests.rs` (~line 28)
- Modify: `src/mcp/server/common.rs` (~line 293)
- Modify: `src/mcp/thin_client.rs` (~line 91)
- Modify: `src/web/server/handlers/rest/types.rs` (~lines 103, 105)
- Modify: `src/web/server/handlers/async_jobs.rs` (~lines 106, 108)
- Modify: `src/web/server/handlers/rest/async_jobs.rs` (~lines 71, 74)
- Modify: `src/web/panel_first_run.rs` (~line 40)
- Modify: `src/cli/server_mode/plan.rs` (~lines 308, 309)

- [ ] **Step 1: Enumerate the exact sibling lines to mirror**

Run: `grep -rn "discover_sitemaps\|max_sitemaps" src/mcp src/web src/cli/server_mode/plan.rs | grep -v _tests`
Each hit gets a `discover_llms_txt` / `max_llms_txt_urls` sibling on the adjacent line.

- [ ] **Step 2: Add request struct fields (MCP schema)**

In `src/mcp/schema/requests.rs` after the `discover_sitemaps`/`sitemap_since_days` fields (line ~28):

```rust
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
```

Match the serde attributes (rename/skip_serializing_if) of the sibling fields exactly.

- [ ] **Step 3: Map fields into cfg at every surface**

Apply the parallel edit at each location found in Step 1. Examples:

`src/mcp/server/common.rs` (~line 293):
```rust
        discover_llms_txt: req.discover_llms_txt,
        max_llms_txt_urls: req.max_llms_txt_urls,
```

`src/web/server/handlers/rest/async_jobs.rs` (~line 71):
```rust
    if let Some(v) = req.discover_llms_txt {
        cfg.discover_llms_txt = v;
    }
    if let Some(v) = req.max_llms_txt_urls {
        cfg.max_llms_txt_urls = v;
    }
```

`src/web/panel_first_run.rs` (~line 40) — conservative UI default (intentionally `false`, matching `discover_sitemaps: Some(false)`):
```rust
        discover_llms_txt: Some(false),
```

`src/cli/server_mode/plan.rs` (~line 308):
```rust
            discover_llms_txt: Some(cfg.discover_llms_txt),
            max_llms_txt_urls: Some(cfg.max_llms_txt_urls),
```

`src/mcp/thin_client.rs` (~line 91): add `"discover_llms_txt"` and `"max_llms_txt_urls"` to the forwarded field-name list.

- [ ] **Step 4: Write the failing serde round-trip + mapping test**

In the MCP requests sidecar test (or wherever request mapping is tested), add:

```rust
#[test]
fn llms_txt_request_fields_roundtrip_and_map() {
    let json = r#"{"url":"https://x.com","discover_llms_txt":false,"max_llms_txt_urls":50}"#;
    let req: ScrapeRequest = serde_json::from_str(json).unwrap(); // use the actual request type
    assert_eq!(req.discover_llms_txt, Some(false));
    assert_eq!(req.max_llms_txt_urls, Some(50));
    // serialize back and confirm snake_case wire names (guards silent no-op from a casing mismatch)
    let out = serde_json::to_string(&req).unwrap();
    assert!(out.contains("discover_llms_txt"));
    assert!(out.contains("max_llms_txt_urls"));
}
```

- [ ] **Step 5: Run + verify parity**

Run: `cargo test -p axon mcp web -- --nocapture`
Expected: PASS.
Run: `grep -rn "discover_sitemaps" src/mcp src/web src/cli/server_mode/plan.rs | grep -v _tests | wc -l` and the same for `discover_llms_txt` — counts should match.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/mcp src/web src/cli/server_mode/plan.rs
git commit -m "feat(api): expose discover_llms_txt + max_llms_txt_urls on request surfaces"
```

---

### Task 10: Docs + version bump (bead .4)

`feat` → minor bump: **4.15.0 → 4.16.0** across all version-bearing files. Sole owner of these files.

**Files:**
- Modify: `Cargo.toml` (`version`)
- Modify: `plugins/axon/.claude-plugin/plugin.json` (`version`) — and any other version-bearing plugin.json
- Modify: `README.md` (version reference)
- Modify: `CHANGELOG.md` (new entry)
- Modify: `config.example.toml` (new keys)
- Modify: `docs/CONFIG.md` (new env/TOML keys)
- Modify: `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md` (new request fields)
- Modify: `CLAUDE.md` (gotcha note near "Sitemap backfill")

- [ ] **Step 1: Confirm current version**

Run: `grep -m1 '^version' Cargo.toml && grep -n '"version"' plugins/axon/.claude-plugin/plugin.json`
Expected: `4.15.0`.

- [ ] **Step 2: Bump all version-bearing files to 4.16.0**

Edit `Cargo.toml` `[package] version = "4.16.0"`, the plugin.json `"version": "4.16.0"`, and the README version reference.

- [ ] **Step 3: Add CHANGELOG entry**

Add under a new `## [4.16.0]` heading:

```markdown
### Added
- `llms.txt` probing: crawl and `map` now fetch `/llms.txt` at the site root, parse its markdown links, and merge them into the sitemap-backfill candidate set (config: `scrape.discover-llms-txt`, default on; `scrape.max-llms-txt-urls`, default 512). Raw `.md`/`.txt` targets pass through without the HTML transform. `fetch_text_with_retry` now caps discovery-document body size (512 KB) for both sitemap and llms.txt.
```

- [ ] **Step 4: Document the config keys**

In `config.example.toml` and `docs/CONFIG.md`, add `scrape.discover-llms-txt` and `scrape.max-llms-txt-urls` next to the sitemap keys, with the defaults and a one-line description. In `docs/MCP.md` / `docs/MCP-TOOL-SCHEMA.md`, add the `discover_llms_txt` / `max_llms_txt_urls` request fields next to `discover_sitemaps`.

- [ ] **Step 5: Add the CLAUDE.md gotcha**

Near the "Sitemap backfill" note, add:

```markdown
### llms.txt probe
After a crawl (and during `map`), if `cfg.discover_llms_txt` (default true; first-run panel default false), axon fetches `/llms.txt` at the site root, parses its markdown links, scopes them like sitemap URLs, caps to `max_llms_txt_urls` (512), and merges them into the same backfill candidate set as sitemap discovery. Raw `.md`/`.txt` targets skip the HTML→markdown transform (else they'd be dropped as thin). `llms-full.txt` is intentionally NOT parsed (it is a content dump, not a link index).
```

- [ ] **Step 6: Verify versions match + build**

Run: `grep -rn "4.16.0" Cargo.toml plugins/axon/.claude-plugin/plugin.json README.md CHANGELOG.md`
Expected: all present and identical.
Run: `cargo build --bin axon`
Expected: clean (validates the Cargo.toml edit).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml plugins README.md CHANGELOG.md config.example.toml docs CLAUDE.md
git commit -m "docs: document llms.txt probe + bump to 4.16.0"
```

---

## Final verification (after all tasks)

- [ ] Run the pre-PR gate: `just verify` (fmt-check + clippy + check + test)
- [ ] Confirm `cargo test -p axon llms_txt sitemap crawl map mcp web` all pass
- [ ] Manual smoke (local binary — note `--local`): `./target/debug/axon map https://docs.firecrawl.dev --local --json` and confirm llms.txt-listed URLs appear in the result
- [ ] `bd close axon_rust-6s51.1 axon_rust-6s51.2 axon_rust-6s51.3 axon_rust-6s51.4 axon_rust-6s51.5` as each completes; close the epic when all children are done
- [ ] `bd doctor` (the session had `git add failed` export warnings), then `git push`

## Out of scope (do NOT build)
- `llms-full.txt` parsing (content dump, not an index).
- Recursive nested llms.txt discovery (Cloudflare root → per-product) — deferred to `axon_rust-y35u`.
- A CLI clap flag for `--discover-llms-txt` (mirrors `discover_sitemaps`, which is TOML/override/request-only).
- `llms_txt_since_*` date filter (llms.txt has no `<lastmod>`).
