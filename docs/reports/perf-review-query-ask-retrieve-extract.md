# Performance Review: query · ask · retrieve · extract

**Date:** 2026-02-19
**Scope:** `crates/vector/ops.rs`, `crates/cli/commands/extract.rs`, `crates/core/content.rs`, `crates/extract/remote_extract.rs`

---

## Top 5 Highest-Impact Fixes

1. **LLM streaming in `ask`** — time-to-first-token goes from minutes to milliseconds
2. **Parallel LLM fallback in `extract`** — decouples crawl throughput from LLM latency
3. **`select_best_preview_item` O(N²) → O(N)** in `query` — pre-score before `max_by`
4. **Parallel URL variant lookup in `retrieve`** — eliminates sequential dead round-trips
5. **Delete `remote_extract.rs`** + unify the two identical tokenizer functions — removes active maintenance hazard

---

## `query`

### [High] `select_best_preview_item` — O(N log N) calls to `meaningful_snippet`

**Location:** `crates/vector/ops.rs:568–603`

The `preview_score` closure is defined inside `max_by`, so it fires **twice per comparison** (once for `a`, once for `b`). `preview_score` calls `meaningful_snippet`, which allocates a full sentence list and a stop-word `HashSet` on every invocation. For a URL group with N chunks, this is `2*(N-1)` full sentence-parsing passes. The winner's snippet is then recomputed a second time in the display loop at line 1029.

**Fix:** Pre-score every item into `(f64, &QueryCandidate)` pairs before calling `max_by`, reducing snippet computations from O(N log N) to O(N). Return the pre-computed snippet alongside the winner to eliminate the display-loop recomputation.

```rust
fn select_best_preview_item(items: &[QueryCandidate], query: &str) -> Option<(QueryCandidate, String)> {
    let terms = tokenize_query(query);
    items
        .iter()
        .map(|item| {
            let snippet = meaningful_snippet(&item.chunk_text, query);
            let snippet_lc = snippet.to_ascii_lowercase();
            let term_hits = terms.iter().filter(|t| snippet_lc.contains(t.as_str())).count();
            let lexical = if terms.is_empty() { 0.0 } else { term_hits as f64 / terms.len() as f64 };
            let header_penalty = if item.chunk_header.as_ref().map(|h| h.len() <= 60).unwrap_or(false) { 0.05 } else { 0.0 };
            let score = item.score + lexical * 0.35 - header_penalty;
            (score, item, snippet)
        })
        .max_by(|(sa, ..), (sb, ..)| sa.partial_cmp(sb).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, item, snippet)| (item.clone(), snippet))
}
```

---

### [High] `tokenize_query_terms` and `tokenize_query` are identical functions

**Location:** `crates/vector/ops.rs:416–427` and `crates/vector/ops.rs:491–502`

Both functions have byte-for-byte identical implementations — same stop-word list, same logic, same return type. Each call rebuilds a `HashSet<&str>` from the stop list from scratch. A divergence between the two (e.g. updating one stop list but not the other) would silently produce inconsistent ranking across `query` and `ask`.

**Fix:** Delete `tokenize_query_terms`, rename all call sites to `tokenize_query`. Hoist the stop-word set to a `LazyLock` so it's built once per process:

```rust
static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    ["the", "and", "for", "with", "that", "this", "from", "into",
     "how", "what", "where", "when", "you", "your", "are", "can",
     "does", "create", "make"]
        .into_iter().collect()
});

fn tokenize_query(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOP_WORDS.contains(*t))
        .map(str::to_string)
        .collect()
}
```

---

### [Medium] 10× over-fetch from Qdrant with client-side grouping

**Location:** `crates/vector/ops.rs:988`

```rust
let fetch_limit = (requested_limit.saturating_mul(10)).clamp(requested_limit, 1000);
```

For `limit=10` this fetches 100 points; for `limit=100` it fetches 1000. Each point carries a `chunk_text` payload of up to 2000 chars. At limit=100, that's potentially 2–5 MB of JSON deserialized and then mostly discarded after client-side grouping.

**Fix:** Use Qdrant's native `/points/search/groups` endpoint (`group_by: "url"`, `group_size: 3`). The server does the grouping and sends only `limit × 3` results instead of `limit × 10`.

---

### [Medium] Display loop recomputes `meaningful_snippet` for the already-selected winner

**Location:** `crates/vector/ops.rs:1029`

After `select_best_preview_item` identifies and scores a winner, the display loop calls `meaningful_snippet` again on the same chunk. Fixed for free by returning the snippet from `select_best_preview_item` (see fix above).

---

### [Medium] `canonical_url_key` runs a full URL parse on every Qdrant hit

**Location:** `crates/vector/ops.rs:429–441`, called at line 999

Runs the full WHATWG URL parser + `to_string()` re-serialization on every hit (up to 1000). Stored chunk URLs are already normalized and rarely carry `?` or `#`.

**Fix:** Fast-path for the common case:

```rust
fn canonical_url_key(raw: &str) -> String {
    let s = raw.trim_end_matches('/');
    if !s.contains('?') && !s.contains('#') {
        return normalize_url(s);
    }
    // slow path: full parse to strip query/fragment
    let normalized = normalize_url(raw);
    let Ok(mut parsed) = Url::parse(&normalized) else {
        return normalized.trim_end_matches('/').to_string();
    };
    parsed.set_fragment(None);
    parsed.set_query(None);
    let mut out = parsed.to_string();
    if out.ends_with('/') { out.pop(); }
    out
}
```

---

### [Low] `is_probable_header` allocates `Vec<&str>` to count words

**Location:** `crates/vector/ops.rs:513–519`

```rust
// before
let words: Vec<&str> = sentence.split_whitespace().collect();
if words.len() <= 5 && sentence.len() <= 60 { ... }

// after
sentence.len() <= 60 && sentence.split_whitespace().count() <= 5
```

---

### [Low] `sentence_candidates` allocates a full String copy via `replace`

**Location:** `crates/vector/ops.rs:504–511`

`text.replace('\n', ". ")` allocates a copy of the entire chunk before splitting. Splitting on `['.', '!', '?', '\n']` directly avoids the allocation and allows returning `impl Iterator<Item = &str>`.

---

## `ask`

### [High] No SSE streaming — full LLM response buffered before any output

**Location:** `crates/vector/ops.rs:1839–1844`

The request has no `"stream": true`. `response.json().await?` blocks until the entire response body arrives. With 120 KB of context and a local Ollama instance generating at 15 tok/s, a 2000-token answer takes ~130 seconds with zero output visible to the user.

**Fix:** Add `"stream": true` to the request body and process `response.bytes_stream()` line-by-line, printing each `delta.content` token immediately. `reqwest 0.12` (already in `Cargo.toml`) supports this with no new dependencies. Buffer all deltas for `--json` mode.

---

### [High] `cfg: &Config` captured by copy in `async move` closures

**Location:** `crates/vector/ops.rs:1695–1702`

```rust
stream::iter(top_full_docs.iter().enumerate().map(|(idx, doc)| async move {
    let points = qdrant_retrieve_by_url(cfg, &url, Some(doc_chunk_limit)).await;
    ...
}))
```

`cfg` is `&Config` captured by copy into each `async move` closure. This compiles today because the futures are polled inline, but it would fail a `'static` bound if any future is ever `tokio::spawn`ed.

**Fix:** Wrap `Config` in `Arc<Config>`, or clone only the needed fields (`qdrant_url`, `collection`) into the closure.

---

### [Medium] Stop-word `HashSet` rebuilt on every tokenizer call; 64+ calls during candidate building

**Location:** `crates/vector/ops.rs:491–502`, called via `tokenize_text_set` at line 606 for each of 64 candidates

Same root cause as the `query` duplicate-tokenizer finding. Apply the same `LazyLock` fix.

---

### [Medium] `rerank_ask_candidates` re-tokenizes the query internally

**Location:** `crates/vector/ops.rs:622`

The query is tokenized again inside `rerank_ask_candidates` even though the caller has already tokenized it (implicitly, through candidate construction). Change the signature to accept `&[String]` pre-tokenized tokens so the caller computes them once and passes them through to `rerank_ask_candidates`, `meaningful_snippet`, and `select_best_preview_item`.

---

### [Medium] `select_diverse_candidates` deep-clones `AskCandidate` including `HashSet<String>` fields; called 3×

**Location:** `crates/vector/ops.rs:663–707`, called at lines 1665, 1666, 1731

`AskCandidate` carries `url_tokens: HashSet<String>` and `chunk_tokens: HashSet<String>`. Each clone deep-copies both sets. The function is called three times on overlapping slices of the same `reranked` vec.

**Fix:** Return `Vec<usize>` indices into the original slice instead of owned clones. Callers already have access to `reranked` and can index into it directly.

---

### [Low] `tokenize_path_set` calls `Url::parse` once per candidate (64×)

**Location:** `crates/vector/ops.rs:609–619`, called at line 1645

Parses the full URL to extract the path on every candidate. Extract and store the path segment at candidate-build time instead.

---

### [Low] 8 `env::var` syscalls on every `ask` invocation

**Location:** `crates/vector/ops.rs:1601–1629`

`AXON_ASK_MAX_CONTEXT_CHARS`, `AXON_ASK_CANDIDATE_LIMIT`, `AXON_ASK_CHUNK_LIMIT`, `AXON_ASK_FULL_DOCS`, `AXON_ASK_BACKFILL_CHUNKS`, `AXON_ASK_DOC_FETCH_CONCURRENCY`, `AXON_ASK_DOC_CHUNK_LIMIT`, `AXON_ASK_MIN_RELEVANCE_SCORE` — all read via `env::var` on every invocation. Move them into `Config` (parsed once at startup via clap's `env` feature).

---

## `retrieve`

### [High] Sequential URL variant fallback — up to 4 serial Qdrant round-trips

**Location:** `crates/vector/ops.rs:1070–1075`

```rust
for candidate in url_lookup_candidates(target) {  // up to 4 variants
    points = qdrant_retrieve_by_url(cfg, &candidate, None).await?;
    if !points.is_empty() { break; }
}
```

If the matching variant is #3, two full empty scroll requests have already been awaited. `FuturesUnordered` is already imported.

**Fix:**

```rust
let mut futs: FuturesUnordered<_> = url_lookup_candidates(target)
    .into_iter()
    .map(|c| async move { qdrant_retrieve_by_url(cfg, &c, Some(500)).await })
    .collect();

let mut points = Vec::new();
while let Some(result) = futs.next().await {
    let pts = result?;
    if !pts.is_empty() { points = pts; break; }
}
// dropping futs cancels remaining in-flight futures
```

---

### [High] `max_points: None` — unbounded chunk materialization

**Location:** `crates/vector/ops.rs:1071`

`run_retrieve_native` passes `None` for `max_points`, so `qdrant_retrieve_by_url` fetches every chunk with no ceiling. A document with 500 × 2000-char chunks allocates ~1 MB before sorting. The `ask` command correctly passes `Some(doc_chunk_limit)`.

**Fix:** Pass `Some(500)` (or an env-configurable constant) — consistent with how `ask` uses this function.

---

### [Medium] `offset.clone()` per scroll page

**Location:** `crates/vector/ops.rs:321` (and `qdrant_scroll_pages:201`)

`serde_json::Value` is heap-allocated. The offset is cloned then immediately consumed into the request body. Use `offset.take()` instead — the value is overwritten from the response on the next line anyway.

---

### [Medium] Sort+concat logic duplicated vs `render_full_doc_from_points`

**Location:** `crates/vector/ops.rs:1081–1090` vs `709–721`

Identical algorithm (sort by `chunk_index`, skip empties, concatenate with `\n`) written twice. Future changes to rendering (e.g. adding chunk headers) must be applied in two places.

**Fix:**
```rust
let chunk_count = points.len();
let out = render_full_doc_from_points(points);
```

---

### [Low] `String::new()` with no capacity for chunk concatenation

**Location:** `crates/vector/ops.rs:1083`, `render_full_doc_from_points:711`

With `points.len()` known before the loop, pre-allocate:

```rust
String::with_capacity(points.len() * 2048)
```

---

## `extract`

### [High] Serial URL loop — N URLs processed one after another

**Location:** `crates/cli/commands/extract.rs:254–294`

```rust
for url in &urls {
    let run = run_extract_with_engine(url, ...).await?;
    ...
}
```

Each URL's full crawl+extract pipeline runs to completion before the next begins. These are fully independent.

**Fix:** Replace with `FuturesUnordered`:

```rust
let mut futs: FuturesUnordered<_> = urls.iter().map(|url| {
    run_extract_with_engine(url, &prompt, cfg.max_pages, ..., Arc::clone(&engine))
}).collect();

while let Some(run) = futs.next().await {
    let run = run?;
    // accumulate stats
}
```

---

### [High] LLM fallback blocks the channel consumer — crawler back-pressures

**Location:** `crates/core/content.rs:573–617`

The `while let Ok(page) = rx.recv().await` loop awaits `extract_items_fallback` inline. During a 2–10s LLM call, no pages are processed and the spider.rs channel (capacity 16) fills up, stalling the crawler entirely. Throughput collapses to one page per LLM round-trip.

**Fix:** Spawn LLM calls into a `JoinSet` with a `Semaphore` so the channel is drained continuously:

```rust
let sem = Arc::new(Semaphore::new(4)); // max 4 concurrent LLM calls
let mut join_set: JoinSet<Option<FallbackResponse>> = JoinSet::new();

while let Ok(page) = rx.recv().await {
    let deterministic = engine.extract(&page_url, &html);
    if !deterministic.items.is_empty() { /* accumulate */ continue; }
    if has_fallback {
        let permit = sem.clone().acquire_owned().await.unwrap();
        join_set.spawn(async move {
            let _permit = permit;
            extract_items_fallback(...).await.ok()
        });
    }
}
// drain remaining LLM tasks
while let Some(result) = join_set.join_next().await { /* accumulate */ }
```

---

### [High] Raw HTML sent to LLM instead of cleaned markdown

**Location:** `crates/core/content.rs:462, 477`

```rust
let trimmed_html: String = html.chars().take(20_000).collect();
```

Raw HTML at 20k chars is 30–70% token-waste compared to the same content as readability-processed markdown. Mid-tag truncation also produces malformed input that degrades LLM output quality. `to_markdown()` already exists in the same file.

**Fix:**
```rust
let markdown = to_markdown(html);
let trimmed: String = markdown.chars().take(12_000).collect();
// Use `trimmed` in the user message instead of `trimmed_html`
```

---

### [Medium] Dedup via `serde_json::to_string` allocates and retains full JSON strings

**Location:** `crates/core/content.rs:254–259`

```rust
if let Ok(key) = serde_json::to_string(&item) {
    if seen.insert(key) { ... }
}
```

`seen: HashSet<String>` retains the full serialized JSON of every seen item for the lifetime of the crawl. For 50 items averaging 400 bytes each, that's 20 KB held in the set.

**Fix:** Hash to `u64` and store only 8 bytes per item:

```rust
let mut seen: HashSet<u64> = HashSet::new();

fn item_hash(item: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher, DefaultHasher};
    let mut h = DefaultHasher::new();
    item.to_string().hash(&mut h);
    h.finish()
}

if seen.insert(item_hash(&item)) {
    all_items.push(item);
}
```

---

### [Medium] `remote_extract.rs` is entirely dead code

**Location:** `crates/extract/remote_extract.rs` (146 lines)

`run_remote_extract` has zero call sites across the entire workspace (verified). The file also contains a stripped-down duplicate of `extract_items_fallback` (without token tracking) and `collect_items` (duplicate of `flatten_results`). The compiler doesn't warn because the function is `pub`.

**Fix:** Delete the file. Remove `pub mod remote_extract;` from `crates/extract/mod.rs`.

---

### [Medium] `website.get_all_links_visited().await.len()` clones the full visited URL set for a count

**Location:** `crates/core/content.rs:626`

An async lock + collection clone just to call `.len()`.

**Fix:** Increment a `pages_visited` counter inside the channel task and return it alongside the results tuple. Remove the `get_all_links_visited` call entirely.

---

### [Low] `extract_attr` calls `to_ascii_lowercase()` 4× per call

**Location:** `crates/core/content.rs:419`

`tag.to_ascii_lowercase()` is called once per loop iteration (4 times) on the same unchanged string. Hoist it before the loop.

---

### [Low] `HtmlTableParser` allocates a 500-char String per table as a "preview"

**Location:** `crates/core/content.rs:400`

```rust
// before
"html_preview": table_html.chars().take(500).collect::<String>(),

// after — zero-copy &str slice to a UTF-8 boundary
let preview_end = table_html.char_indices().nth(500).map(|(i,_)| i).unwrap_or(table_html.len());
"html_preview": &table_html[..preview_end],
```

---

## Issue Count by Command

| Command | High | Medium | Low | Total |
|---------|------|--------|-----|-------|
| `query` | 2 | 3 | 2 | 7 |
| `ask` | 2 | 3 | 2 | 7 |
| `retrieve` | 2 | 2 | 1 | 5 |
| `extract` | 3 | 3 | 2 | 8 |
| **Total** | **9** | **11** | **7** | **27** |
