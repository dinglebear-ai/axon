# Page Date Extraction + Time-Decay Ranking Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract publication dates from crawled HTML, store them in Qdrant payloads, and apply a mild time-decay penalty in ranking so older blog posts score lower than recent content.

**Architecture:** Dates are extracted from raw HTML in `collector.rs` before the HTML is discarded, stored as YAML front-matter at the top of each saved markdown file, parsed back out during the embed pipeline, and written to Qdrant point payloads. The ranking pipeline reads `published_at` from the payload and applies a capped exponential decay to `rerank_score`.

**Tech Stack:** Rust, `regex = "1"` (already in Cargo.toml), `chrono` (already in Cargo.toml), Qdrant JSON payloads, existing `crates/vector/ops/ranking/mod.rs` BM25-style reranker.

---

## Data Flow

```
collector.rs                 markdown file on disk              tei.rs                    Qdrant
────────────                 ─────────────────────              ──────                    ──────
HTML bytes
  → extract_page_dates()
  → PageDates { published_at, modified_at }
  → prepend front-matter     ---
                              published_at: 2024-01-15T10:00Z
                              modified_at:  2024-03-20T15:30Z
                              ---
                              # Page content...
                                                    → strip_front_matter()
                                                    → PreparedDoc {
                                                         published_at,
                                                         modified_at,
                                                         chunks
                                                       }
                                                                          → payload {
                                                                              published_at,
                                                                              modified_at,
                                                                              scraped_at
                                                                            }
ranking/mod.rs
  → AskCandidate { published_at }
  → rerank_score -= time_decay(published_at)
```

---

## Task 1: `PageDates` struct + `extract_page_dates()` in content.rs

**Files:**
- Modify: `crates/core/content.rs`

HTML is parsed with `regex` (already a dep). Priority order: JSON-LD → OG meta → itemprop → `<time pubdate>` → generic meta → URL pattern.

**Step 1: Write the failing tests**

Add to `crates/core/content.rs` `#[cfg(test)]` block:

```rust
#[test]
fn extract_dates_from_jsonld() {
    let html = r#"<script type="application/ld+json">{"@type":"BlogPosting","datePublished":"2024-01-15T10:00:00Z","dateModified":"2024-03-20T15:30:00Z"}</script>"#;
    let dates = extract_page_dates(html.as_bytes(), "https://example.com/blog/post");
    assert_eq!(dates.published_at.unwrap(), "2024-01-15T10:00:00Z");
    assert_eq!(dates.modified_at.unwrap(), "2024-03-20T15:30:00Z");
}

#[test]
fn extract_dates_from_og_meta() {
    let html = r#"<meta property="article:published_time" content="2024-06-01T08:00:00+00:00">"#;
    let dates = extract_page_dates(html.as_bytes(), "https://example.com/blog/post");
    assert_eq!(dates.published_at.unwrap(), "2024-06-01T08:00:00+00:00");
}

#[test]
fn extract_dates_from_url_pattern() {
    let html = b"<html><body>no meta dates</body></html>";
    let dates = extract_page_dates(html, "https://example.com/blog/2023/04/15/my-post");
    assert_eq!(dates.published_at.unwrap(), "2023-04-15T00:00:00Z");
    assert!(dates.modified_at.is_none());
}

#[test]
fn extract_dates_returns_none_when_no_dates() {
    let html = b"<html><body>no dates here</body></html>";
    let dates = extract_page_dates(html, "https://example.com/docs/intro");
    assert!(dates.published_at.is_none());
    assert!(dates.modified_at.is_none());
}
```

**Step 2: Run to confirm failure**

```bash
cargo test extract_dates -- --nocapture
```
Expected: compile error — `extract_page_dates` does not exist yet.

**Step 3: Implement `PageDates` + `extract_page_dates`**

Add to `crates/core/content.rs` (after existing imports):

```rust
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Default, Clone)]
pub struct PageDates {
    pub published_at: Option<String>,
    pub modified_at: Option<String>,
}

// Regex patterns — compiled once at startup
static JSONLD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<script[^>]+type=["']application/ld\+json["'][^>]*>([\s\S]*?)</script>"#)
        .unwrap()
});
static OG_PUBLISHED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<meta[^>]+property=["']article:published_time["'][^>]+content=["']([^"']+)["']|<meta[^>]+content=["']([^"']+)["'][^>]+property=["']article:published_time["']"#)
        .unwrap()
});
static OG_MODIFIED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<meta[^>]+property=["']article:modified_time["'][^>]+content=["']([^"']+)["']|<meta[^>]+content=["']([^"']+)["'][^>]+property=["']article:modified_time["']"#)
        .unwrap()
});
static ITEMPROP_PUBLISHED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:meta|time)[^>]+itemprop=["']datePublished["'][^>]+(?:content|datetime)=["']([^"']+)["']|<(?:meta|time)[^>]+(?:content|datetime)=["']([^"']+)["'][^>]+itemprop=["']datePublished["']"#)
        .unwrap()
});
static TIME_PUBDATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<time[^>]+pubdate[^>]+datetime=["']([^"']+)["']"#).unwrap()
});
static META_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<meta[^>]+name=["'](?:date|published_date|dc\.date|dcterms\.date)["'][^>]+content=["']([^"']+)["']|<meta[^>]+content=["']([^"']+)["'][^>]+name=["'](?:date|published_date|dc\.date|dcterms\.date)["']"#)
        .unwrap()
});
static URL_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/(\d{4})[/-](\d{1,2})[/-](\d{1,2})(?:[/-]|$)").unwrap()
});

/// Extract the first non-empty capture group from a regex match.
fn first_capture(re: &Regex, text: &str) -> Option<String> {
    re.captures(text)?
        .iter()
        .skip(1) // skip full match
        .flatten()
        .find(|m| !m.as_str().is_empty())
        .map(|m| m.as_str().to_string())
}

/// Try to extract datePublished / dateModified from a JSON-LD block.
fn parse_jsonld_dates(html: &str) -> (Option<String>, Option<String>) {
    for cap in JSONLD_RE.captures_iter(html) {
        let json_text = &cap[1];
        let published = serde_json::from_str::<serde_json::Value>(json_text)
            .ok()
            .and_then(|v| {
                v.get("datePublished")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
            });
        let modified = serde_json::from_str::<serde_json::Value>(json_text)
            .ok()
            .and_then(|v| {
                v.get("dateModified")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
            });
        if published.is_some() || modified.is_some() {
            return (published, modified);
        }
    }
    (None, None)
}

/// Try to parse a date string into a canonical UTC ISO-8601 string.
/// Accepts RFC3339, bare dates (YYYY-MM-DD), and falls back to returning as-is.
fn normalize_date(raw: &str) -> String {
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    // Try full RFC3339 / ISO-8601 with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw.trim()) {
        return dt.with_timezone(&Utc).to_rfc3339();
    }
    // Try bare date YYYY-MM-DD
    if let Ok(nd) = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") {
        return Utc
            .from_utc_datetime(&nd.and_hms_opt(0, 0, 0).unwrap())
            .to_rfc3339();
    }
    raw.trim().to_string()
}

/// Extract publication and modification dates from raw HTML bytes.
/// Checks (in priority order):
///   1. JSON-LD datePublished / dateModified
///   2. <meta property="article:published_time"> (Open Graph)
///   3. <meta itemprop="datePublished"> (Schema.org)
///   4. <time pubdate datetime="...">
///   5. <meta name="date"> / DC.date / published_date
///   6. URL path date pattern /YYYY/MM/DD/ (last resort)
pub fn extract_page_dates(html: &[u8], url: &str) -> PageDates {
    let text = String::from_utf8_lossy(html);

    // 1. JSON-LD (most structured)
    let (jsonld_pub, jsonld_mod) = parse_jsonld_dates(&text);
    if jsonld_pub.is_some() || jsonld_mod.is_some() {
        return PageDates {
            published_at: jsonld_pub.map(|s| normalize_date(&s)),
            modified_at: jsonld_mod.map(|s| normalize_date(&s)),
        };
    }

    // 2. OG article meta
    let og_pub = first_capture(&OG_PUBLISHED_RE, &text);
    let og_mod = first_capture(&OG_MODIFIED_RE, &text);
    if og_pub.is_some() || og_mod.is_some() {
        return PageDates {
            published_at: og_pub.map(|s| normalize_date(&s)),
            modified_at: og_mod.map(|s| normalize_date(&s)),
        };
    }

    // 3. Schema.org itemprop
    if let Some(v) = first_capture(&ITEMPROP_PUBLISHED_RE, &text) {
        return PageDates {
            published_at: Some(normalize_date(&v)),
            modified_at: None,
        };
    }

    // 4. <time pubdate datetime="...">
    if let Some(v) = first_capture(&TIME_PUBDATE_RE, &text) {
        return PageDates {
            published_at: Some(normalize_date(&v)),
            modified_at: None,
        };
    }

    // 5. Generic meta name
    if let Some(v) = first_capture(&META_DATE_RE, &text) {
        return PageDates {
            published_at: Some(normalize_date(&v)),
            modified_at: None,
        };
    }

    // 6. URL date pattern /YYYY/MM/DD/ or /YYYY-MM-DD- (last resort)
    if let Some(caps) = URL_DATE_RE.captures(url) {
        let y = &caps[1];
        let m = caps[2].parse::<u32>().unwrap_or(1).clamp(1, 12);
        let d = caps[3].parse::<u32>().unwrap_or(1).clamp(1, 31);
        use chrono::{NaiveDate, TimeZone, Utc};
        if let Some(nd) = NaiveDate::from_ymd_opt(y.parse().unwrap_or(2000), m, d) {
            let dt = Utc
                .from_utc_datetime(&nd.and_hms_opt(0, 0, 0).unwrap())
                .to_rfc3339();
            return PageDates {
                published_at: Some(dt),
                modified_at: None,
            };
        }
    }

    PageDates::default()
}
```

**Step 4: Run tests**

```bash
cargo test extract_dates -- --nocapture
```
Expected: 4 PASS.

**Step 5: Commit**

```bash
git add crates/core/content.rs
git commit -m "feat(content): add extract_page_dates() with JSON-LD, OG, itemprop, time, URL fallback"
```

---

## Task 2: Write front-matter into markdown files in collector.rs

**Files:**
- Modify: `crates/crawl/engine/collector.rs`

The raw HTML bytes (`page.get_html_bytes_u8()`) are available before they're transformed. Extract dates there, then prepend front-matter to the saved file. The content hash is computed from `trimmed` (markdown, no front-matter) so hash stability is preserved.

**Step 1: Write a failing test**

Add to `crates/crawl/engine/tests.rs` (or a new `collector_test` block):

```rust
#[test]
fn front_matter_format_roundtrip() {
    // Test that build_front_matter and strip_front_matter are inverses
    use crate::crates::crawl::engine::collector::{build_front_matter, strip_front_matter};
    let fm = build_front_matter("2024-01-15T10:00:00Z", Some("2024-03-20T15:30:00Z"));
    let content = format!("{fm}# Hello\nworld");
    let (dates, body) = strip_front_matter(&content);
    assert_eq!(dates.published_at.unwrap(), "2024-01-15T10:00:00Z");
    assert_eq!(dates.modified_at.unwrap(), "2024-03-20T15:30:00Z");
    assert_eq!(body.trim(), "# Hello\nworld");
}
```

**Step 2: Run to confirm failure**

```bash
cargo test front_matter_format_roundtrip -- --nocapture
```
Expected: compile error — functions don't exist yet.

**Step 3: Add `build_front_matter` helper and wire into `collect_crawl_pages`**

In `crates/crawl/engine/collector.rs`, add these helpers near the top:

```rust
use crate::crates::core::content::extract_page_dates;

/// Build a YAML front-matter block for date metadata.
/// Only included when at least one date is present.
pub(crate) fn build_front_matter(published_at: &str, modified_at: Option<&str>) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("published_at: {published_at}\n"));
    if let Some(m) = modified_at {
        fm.push_str(&format!("modified_at: {m}\n"));
    }
    fm.push_str("---\n");
    fm
}

/// Strip YAML front-matter from markdown content.
/// Returns extracted key-value pairs and the body after the closing `---`.
pub(crate) fn strip_front_matter(content: &str) -> (FrontMatterDates, &str) {
    if !content.starts_with("---\n") {
        return (FrontMatterDates::default(), content);
    }
    let rest = &content[4..];
    let Some(end_pos) = rest.find("\n---\n") else {
        return (FrontMatterDates::default(), content);
    };
    let fm_body = &rest[..end_pos];
    let body = &rest[end_pos + 5..]; // skip "\n---\n"
    let mut dates = FrontMatterDates::default();
    for line in fm_body.lines() {
        if let Some(v) = line.strip_prefix("published_at: ") {
            dates.published_at = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("modified_at: ") {
            dates.modified_at = Some(v.to_string());
        }
    }
    (dates, body)
}

#[derive(Debug, Default, Clone)]
pub(crate) struct FrontMatterDates {
    pub published_at: Option<String>,
    pub modified_at: Option<String>,
}
```

In `collect_crawl_pages`, just before `tokio::fs::write(&path, trimmed.as_bytes())` (line ~145), add:

```rust
// Extract publication dates from raw HTML before we discard it.
let page_dates = extract_page_dates(page.get_html_bytes_u8(), &url);
let file_content = if let Some(pub_at) = &page_dates.published_at {
    let fm = build_front_matter(pub_at, page_dates.modified_at.as_deref());
    format!("{fm}{trimmed}")
} else {
    trimmed.to_string()
};
tokio::fs::write(&path, file_content.as_bytes())
    .await
    .map_err(|e| format!("write failed: {e}"))?;
```

Replace the existing `tokio::fs::write(&path, trimmed.as_bytes())` line with the above.

**Step 4: Run tests**

```bash
cargo test front_matter -- --nocapture
cargo check
```
Expected: PASS, no compile errors.

**Step 5: Commit**

```bash
git add crates/crawl/engine/collector.rs
git commit -m "feat(crawl): extract page dates from HTML and prepend as front-matter to markdown files"
```

---

## Task 3: Add date fields to `PreparedDoc` and strip front-matter in `prepare_embed_docs`

**Files:**
- Modify: `crates/vector/ops/tei.rs`

`PreparedDoc` (line 53) needs `published_at`/`modified_at` fields. `prepare_embed_docs` (line 380) must strip front-matter before chunking and populate those fields.

**Step 1: Write a failing test**

Add to the `#[cfg(test)]` block in `tei.rs`:

```rust
#[test]
fn prepare_doc_strips_front_matter_and_captures_dates() {
    // Write a temp markdown file with front-matter
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("page.md");
    std::fs::write(
        &file_path,
        "---\npublished_at: 2024-01-15T10:00:00Z\n---\n# Hello\nsome content here for chunking",
    ).unwrap();
    // URL file alongside it (read_inputs convention)
    let url_path = dir.path().join("page.md.url");
    std::fs::write(&url_path, "https://example.com/blog/hello").unwrap();

    let docs = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(prepare_embed_docs(dir.path().to_str().unwrap(), &[]))
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].published_at.as_deref(), Some("2024-01-15T10:00:00Z"));
    assert!(docs[0].modified_at.is_none());
    // Chunks should NOT contain the front-matter
    assert!(docs[0].chunks.iter().all(|c| !c.contains("published_at:")));
}
```

**Step 2: Run to confirm failure**

```bash
cargo test prepare_doc_strips_front_matter -- --nocapture
```
Expected: compile error — `PreparedDoc` missing fields.

**Step 3: Update `PreparedDoc` and `prepare_embed_docs`**

Update `PreparedDoc` (line 53):

```rust
struct PreparedDoc {
    url: String,
    domain: String,
    chunks: Vec<String>,
    published_at: Option<String>,
    modified_at: Option<String>,
}
```

In `prepare_embed_docs` (line 408), replace the `prepared.push(PreparedDoc { url, domain, chunks })` block with:

```rust
// Strip YAML front-matter (written by collector) before chunking.
use crate::crates::crawl::engine::collector::strip_front_matter;
let (fm_dates, body) = strip_front_matter(&raw);
let chunks = input::chunk_text(body.trim());
let domain = Url::parse(&url)
    .ok()
    .and_then(|u| u.host_str().map(|s| s.to_string()))
    .unwrap_or_else(|| "unknown".to_string());
prepared.push(PreparedDoc {
    url,
    domain,
    chunks,
    published_at: fm_dates.published_at,
    modified_at: fm_dates.modified_at,
});
```

**Step 4: Run tests**

```bash
cargo test prepare_doc -- --nocapture
```
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/vector/ops/tei.rs
git commit -m "feat(vector): strip front-matter in prepare_embed_docs and populate PreparedDoc dates"
```

---

## Task 4: Write `published_at` and `modified_at` into Qdrant payload

**Files:**
- Modify: `crates/vector/ops/tei.rs` (lines 266–278, `embed_prepared_doc` payload block)
- Modify: `crates/vector/ops/qdrant/types.rs`

**Step 1: Write a failing test**

Add to `tei.rs` tests:

```rust
#[tokio::test]
async fn embed_prepared_doc_includes_dates_in_payload() {
    // This test requires httpmock — see existing TEI tests for setup pattern.
    // Verify that when PreparedDoc has published_at set, the Qdrant upsert payload
    // contains "published_at" and "modified_at" fields.
    // (Mirror the pattern of existing tei_embed_* httpmock tests)
}
```

Note: the existing httpmock tests in `tei.rs` show the pattern. Implement similarly — mock TEI embed endpoint + mock Qdrant upsert, assert payload JSON contains `published_at`.

**Step 2: Update the payload in `embed_prepared_doc`**

In `embed_prepared_doc` (lines 266–278), update the payload block:

```rust
let mut payload = serde_json::json!({
    "url": doc.url,
    "domain": doc.domain,
    "source_command": "embed",
    "content_type": "markdown",
    "chunk_index": idx,
    "chunk_text": chunk,
    "scraped_at": timestamp,
});
if let Some(pub_at) = &doc.published_at {
    payload["published_at"] = serde_json::Value::String(pub_at.clone());
}
if let Some(mod_at) = &doc.modified_at {
    payload["modified_at"] = serde_json::Value::String(mod_at.clone());
}
points.push(serde_json::json!({
    "id": point_id.to_string(),
    "vector": vecv,
    "payload": payload,
}));
```

**Step 3: Add to `QdrantPayload`**

In `crates/vector/ops/qdrant/types.rs` (struct at line 3):

```rust
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct QdrantPayload {
    pub url: String,
    pub chunk_text: String,
    pub text: String,
    pub chunk_index: Option<i64>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub modified_at: Option<String>,
}
```

`#[serde(default)]` ensures existing points without these fields deserialize cleanly.

**Step 4: Run tests + check**

```bash
cargo test tei -- --nocapture
cargo check
```
Expected: all pass, no compile errors.

**Step 5: Commit**

```bash
git add crates/vector/ops/tei.rs crates/vector/ops/qdrant/types.rs
git commit -m "feat(vector): write published_at/modified_at into Qdrant point payload"
```

---

## Task 5: Time-decay ranking in `ranking/mod.rs`

**Files:**
- Modify: `crates/vector/ops/ranking/mod.rs`
- Modify: `crates/vector/ops/commands/ask/context.rs` (populate `AskCandidate.published_at`)

**Decay formula:** Mild, capped at 0.10. No penalty for content < 90 days old.

```
age_days = days since published_at (or scraped_at fallback)
if age_days > 90:
    decay = min((age_days - 90) / 365.0 * 0.04, 0.10)
    rerank_score -= decay
```

This means:
- < 90 days: no penalty
- 1 year old: -0.04
- 2 years old: -0.08
- 3+ years old: -0.10 (capped)

**Step 1: Write failing tests**

Add to `crates/vector/ops/ranking/mod.rs` `#[cfg(test)]` block:

```rust
#[test]
fn time_decay_no_penalty_for_fresh_content() {
    use chrono::Utc;
    let recent = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
    let decay = compute_time_decay(Some(&recent));
    assert_eq!(decay, 0.0, "content under 90 days old should have zero penalty");
}

#[test]
fn time_decay_capped_at_max() {
    let old = "2010-01-01T00:00:00Z";
    let decay = compute_time_decay(Some(old));
    assert!((decay - 0.10).abs() < 0.001, "very old content capped at 0.10");
}

#[test]
fn time_decay_none_returns_zero() {
    assert_eq!(compute_time_decay(None), 0.0);
}

#[test]
fn rerank_applies_decay_to_old_blog_post() {
    use std::collections::HashSet;
    let mut candidate = AskCandidate {
        score: 0.80,
        url: "https://example.com/blog/2020/old-post".to_string(),
        path: "/blog/2020/old-post".to_string(),
        chunk_text: "old content".to_string(),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score: 0.80,
        published_at: Some("2020-01-01T00:00:00Z".to_string()),
    };
    apply_time_decay(&mut candidate);
    assert!(candidate.rerank_score < 0.80, "old post should be penalized");
}
```

**Step 2: Run to confirm failure**

```bash
cargo test time_decay -- --nocapture
```
Expected: compile errors.

**Step 3: Add `published_at` to `AskCandidate` and implement decay functions**

Update `AskCandidate` (line 28):

```rust
#[derive(Debug, Clone)]
pub struct AskCandidate {
    pub score: f64,
    pub url: String,
    pub path: String,
    pub chunk_text: String,
    pub url_tokens: HashSet<String>,
    pub chunk_tokens: HashSet<String>,
    pub rerank_score: f64,
    pub published_at: Option<String>,
}
```

Add these functions after `AskCandidate`:

```rust
/// Compute the time decay penalty (0.0–0.10) for content based on its age.
/// No penalty for content under 90 days old; linearly increasing up to 0.10 cap.
pub fn compute_time_decay(published_at: Option<&str>) -> f64 {
    use chrono::{DateTime, Utc};
    let Some(raw) = published_at else { return 0.0 };
    let Ok(dt) = DateTime::parse_from_rfc3339(raw.trim()) else { return 0.0 };
    let age_days = (Utc::now() - dt.with_timezone(&Utc)).num_days().max(0);
    if age_days <= 90 {
        return 0.0;
    }
    ((age_days - 90) as f64 / 365.0 * 0.04_f64).min(0.10)
}

/// Apply time decay to a candidate's rerank_score in place.
pub fn apply_time_decay(candidate: &mut AskCandidate) {
    let decay = compute_time_decay(candidate.published_at.as_deref());
    candidate.rerank_score -= decay;
}
```

In `rerank_candidates` (line ~117), add before the sort:

```rust
for c in &mut candidates {
    apply_time_decay(c);
}
```

**Step 4: Wire `published_at` in `ask/context.rs`**

In `crates/vector/ops/commands/ask/context.rs` (around line 99 where `AskCandidate` is constructed), add:

```rust
let published_at = hit.payload.published_at.clone();
// ...existing fields...
AskCandidate {
    score,
    url,
    path,
    chunk_text,
    url_tokens,
    chunk_tokens,
    rerank_score: hit.score as f64,
    published_at,
}
```

**Step 5: Run tests**

```bash
cargo test time_decay -- --nocapture
cargo test ranking -- --nocapture
```
Expected: all PASS.

**Step 6: Commit**

```bash
git add crates/vector/ops/ranking/mod.rs crates/vector/ops/commands/ask/context.rs
git commit -m "feat(ranking): add time-decay penalty for older content (max 0.10, no penalty < 90 days)"
```

---

## Task 6: Full verification

**Step 1: Run full test suite**

```bash
cargo test
```
Expected: all existing tests pass + new tests pass.

**Step 2: Lint**

```bash
cargo clippy
cargo fmt --check
```
Expected: clean.

**Step 3: Monolith check**

```bash
./scripts/axon doctor  # or just: python3 scripts/enforce_monoliths.py
```
Expected: no violations. `content.rs` is under the 500-line limit (check with `wc -l crates/core/content.rs`).

**Step 4: Final commit if anything was auto-fixed**

```bash
git add -p
git commit -m "chore: lint fixes"
```

---

## Notes for Implementer

- `page.get_html_bytes_u8()` returns `&[u8]` — pass directly to `extract_page_dates`
- The content hash in `ManifestEntry` is computed from `trimmed` (markdown, no front-matter) so incremental re-crawl logic is not affected
- `#[serde(default)]` on new `QdrantPayload` fields is critical — millions of existing Qdrant points don't have these fields and will deserialize as `None`
- The decay is applied only in the `ask` command's reranker. The `query` command returns raw Qdrant scores — consider wiring it there too in a follow-up
- URL date pattern extraction fires for paths like `/2024/01/15/` and `/2024-01-15-` — it does NOT fire for `/blog/` alone (no date in path), which is correct behavior
