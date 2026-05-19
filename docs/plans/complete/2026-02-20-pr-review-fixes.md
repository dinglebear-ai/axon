# PR Review Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Address all actionable findings from the `perf/command-performance-fixes` PR review: error handling in shared worker infrastructure, HTTP security hardening, evaluate command reliability, and test coverage for new ranking logic.

**Architecture:** All worker error handling lives in `crates/jobs/worker_lane.rs` (the shared generic worker already used by batch/extract/embed). Vector/ranking tests go in `tests/vector_v2_ranking_migration.rs`. SSRF tests stay in `crates/core/http.rs`. CLAUDE.md cleanup is a doc-only change.

**Tech Stack:** Rust, Tokio, lapin (AMQP), sqlx (Postgres), reqwest, cargo test

---

## Baseline

```bash
cargo test 2>&1 | grep -c "ok$"   # expect 105
cargo check --bin axon             # expect clean
```

---

### Task 1: Fix `worker_lane.rs` — 5 error handling issues

**Files:**
- Modify: `crates/jobs/worker_lane.rs`

**The 5 issues (all in one file):**

1. **Line 98** — AMQP delivery error silently dropped:
   ```rust
   Ok(Some(Err(_))) => continue,
   ```
   Fix: log the error before continuing.

2. **Line 99** — Consumer stream death returns `Ok(())` (supervisor sees clean exit):
   ```rust
   Ok(None) => break,
   ```
   Fix: break out of loop but return `Err(...)` after the loop.

3. **Lines 111-113** — DB failure during claim treated as "already claimed":
   ```rust
   if claim_pending_by_id(&pool, wc.table, job_id)
       .await
       .unwrap_or(false)
   ```
   Fix: match on `Ok(true)` / `Ok(false)` / `Err` with a `log_warn` on `Err`.

4. **Lines 42-62** — Watchdog sweep silently swallows DB errors:
   ```rust
   if let Ok(stats) = reclaim_stale_running_jobs(...).await {
       // Err arm: nothing
   }
   ```
   Fix: add `else { log_warn(...) }`.

5. **Lines 168-170** — AMQP probe failure not logged:
   ```rust
   Err(_) => false,
   ```
   Fix: `Err(e) => { log_warn(...); false }`.

**Step 1: Make all 5 changes in `worker_lane.rs`**

Replace the full `run_amqp_lane` function body with:

```rust
async fn run_amqp_lane(
    cfg: &Config,
    pool: PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_conn, ch) = open_amqp_connection_and_channel(cfg, &wc.queue_name).await?;
    let tag = format!("{}-{lane}", wc.consumer_tag_prefix);
    let mut consumer = ch
        .basic_consume(
            &wc.queue_name,
            &tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    log_info(&format!(
        "{} worker lane={lane} listening on queue={} concurrency={}",
        wc.job_kind, wc.queue_name, wc.lane_count
    ));

    let mut stream_ended = false;
    loop {
        let timed = tokio::time::timeout(
            Duration::from_secs(STALE_SWEEP_INTERVAL_SECS),
            consumer.next(),
        )
        .await;
        let delivery = match timed {
            Ok(Some(Ok(d))) => d,
            Ok(Some(Err(e))) => {
                log_warn(&format!(
                    "{} worker lane={lane} AMQP delivery error: {e}",
                    wc.job_kind
                ));
                continue;
            }
            Ok(None) => {
                stream_ended = true;
                break;
            }
            Err(_) => {
                sweep_stale_jobs(cfg, &pool, wc, "amqp", lane).await;
                continue;
            }
        };

        let parsed = std::str::from_utf8(&delivery.data)
            .ok()
            .and_then(|s| Uuid::parse_str(s.trim()).ok());
        delivery.ack(BasicAckOptions::default()).await?;
        if let Some(job_id) = parsed {
            match claim_pending_by_id(&pool, wc.table, job_id).await {
                Ok(true) => {
                    process_fn(cfg.clone(), pool.clone(), job_id).await;
                }
                Ok(false) => {} // legitimately claimed by another lane
                Err(e) => {
                    log_warn(&format!(
                        "{} worker lane={lane} DB error claiming job {job_id} (already acked): {e}",
                        wc.job_kind
                    ));
                }
            }
        } else {
            log_warn(&format!(
                "{} worker lane={lane} malformed delivery payload (len={}), skipped after ack",
                wc.job_kind,
                delivery.data.len()
            ));
        }
    }

    if stream_ended {
        Err(format!(
            "{} worker lane={lane} AMQP consumer stream ended unexpectedly",
            wc.job_kind
        )
        .into())
    } else {
        Ok(())
    }
}
```

Replace the `sweep_stale_jobs` function body with:

```rust
async fn sweep_stale_jobs(
    cfg: &Config,
    pool: &PgPool,
    wc: &WorkerConfig,
    source: &str,
    lane: usize,
) {
    match reclaim_stale_running_jobs(
        pool,
        wc.table,
        wc.job_kind,
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
        source,
    )
    .await
    {
        Ok(stats) => {
            if stats.stale_candidates > 0 || stats.reclaimed_jobs > 0 {
                log_info(&format!(
                    "watchdog {} sweep lane={} candidates={} marked={} reclaimed={}",
                    wc.job_kind,
                    lane,
                    stats.stale_candidates,
                    stats.marked_candidates,
                    stats.reclaimed_jobs
                ));
            }
        }
        Err(e) => {
            log_warn(&format!(
                "watchdog {} sweep failed (lane={lane}): {e}",
                wc.job_kind
            ));
        }
    }
}
```

Replace the AMQP probe block in `run_job_worker` with:

```rust
    let amqp_available = match open_amqp_connection_and_channel(cfg, &wc.queue_name).await {
        Ok((conn, ch)) => {
            let _ = ch.close(0, "probe").await;
            let _ = conn.close(200, "probe").await;
            true
        }
        Err(e) => {
            log_warn(&format!(
                "{} worker: AMQP probe failed ({}), falling back to polling: {e}",
                wc.job_kind, wc.queue_name
            ));
            false
        }
    };
```

**Step 2: Run tests**

```bash
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```
Expected: same pass count (105), 0 failed.

**Step 3: Commit**

```bash
git add crates/jobs/worker_lane.rs
git commit -m "fix: harden worker_lane AMQP error handling — log delivery errors, signal stream death, log probe failures"
```

---

### Task 2: Fix `ensure_collection` — swallowed Qdrant HTTP errors

**Files:**
- Modify: `crates/vector/ops/tei.rs:80`

**Current (line 80):**
```rust
let _ = client.put(url).json(&create).send().await?;
```

**Fix:** The `?` only catches transport errors; HTTP 4xx/5xx are silently ignored. `.error_for_status()?` surfaces them.

**Step 1: Make the fix**

Replace line 80:
```rust
    let _ = client.put(url).json(&create).send().await?;
```
with:
```rust
    client
        .put(url)
        .json(&create)
        .send()
        .await?
        .error_for_status()?;
```

**Step 2: Run tests**

```bash
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```
Expected: same pass count, 0 failed.

**Step 3: Commit**

```bash
git add crates/vector/ops/tei.rs
git commit -m "fix: ensure_collection checks HTTP status — surfaces Qdrant 4xx/5xx instead of silently succeeding"
```

---

### Task 3: Fix `evaluate.rs` — two error handling issues

**Files:**
- Modify: `crates/vector/ops/commands/evaluate.rs`

**Issue A (line ~100):** `build_judge_reference` error is completely dropped (`|_|`).
**Issue B (line ~344):** Non-streaming LLM fallback failure → `unwrap_or_default()` → silent empty string.

**Step 1: Fix `build_judge_reference` error logging**

Find:
```rust
    let (judge_reference, ref_chunk_count) = build_judge_reference(cfg, &query)
        .await
        .unwrap_or_else(|_| (NO_REFERENCE.to_string(), 0));
```

Replace with:
```rust
    let (judge_reference, ref_chunk_count) = build_judge_reference(cfg, &query)
        .await
        .unwrap_or_else(|e| {
            log_warn(&format!("evaluate: judge reference retrieval failed (proceeding without grounding): {e}"));
            (NO_REFERENCE.to_string(), 0)
        });
```

**Step 2: Fix double-LLM failure → propagate error**

Find the fallback block (in `run_analysis` or similar):
```rust
            let fallback = judge_llm_non_streaming(
                ...
            )
            .await
            .unwrap_or_default();
            if !cfg.json_output {
                print!("{fallback}");
            }
            fallback
```

Replace `unwrap_or_default()` chain with proper error propagation. The function `run_analysis` returns `(String, u128)` — change to return a `Result` or map the error to a warning + sentinel:

```rust
            match judge_llm_non_streaming(
                cfg, client, query, rag_answer, baseline_answer,
                judge_reference, rag_sources_list, ref_quality_note,
                rag_elapsed_ms, baseline_elapsed_ms, source_count, context_chars,
            )
            .await
            {
                Ok(fallback) => {
                    if !cfg.json_output {
                        print!("{fallback}");
                    }
                    fallback
                }
                Err(e2) => {
                    log_warn(&format!("evaluate: both streaming and non-streaming judge failed: {e2}"));
                    String::from("(judge unavailable — both streaming and non-streaming LLM calls failed)")
                }
            }
```

**Step 3: Run tests**

```bash
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```

**Step 4: Commit**

```bash
git add crates/vector/ops/commands/evaluate.rs
git commit -m "fix: evaluate command logs judge reference failure and surfaces double-LLM failure instead of returning empty string"
```

---

### Task 4: Fix SSRF `localhost?`/`localhost#` bypass + add tests

**Files:**
- Modify: `crates/core/http.rs`

**Current (lines 129-130):**
```rust
        r"^https?://localhost[/:]".to_string(),
        r"^https?://localhost$".to_string(),
```

These miss `http://localhost?admin=true` and `http://localhost#config`.

**Step 1: Write the failing tests first**

In `http.rs`, find the existing `#[cfg(test)] mod tests` block and add:

```rust
        #[test]
        fn test_ssrf_blacklist_blocks_localhost_with_query() {
            use super::ssrf_blacklist_patterns;
            let patterns = ssrf_blacklist_patterns();
            let url = "http://localhost?admin=true";
            let blocked = patterns.iter().any(|p| {
                regex::Regex::new(p).unwrap().is_match(url)
            });
            assert!(blocked, "localhost with query string should be blocked by blacklist");
        }

        #[test]
        fn test_ssrf_blacklist_blocks_localhost_with_fragment() {
            use super::ssrf_blacklist_patterns;
            let patterns = ssrf_blacklist_patterns();
            let url = "https://localhost#secret";
            let blocked = patterns.iter().any(|p| {
                regex::Regex::new(p).unwrap().is_match(url)
            });
            assert!(blocked, "localhost with fragment should be blocked by blacklist");
        }
```

**Step 2: Run tests to confirm they fail**

```bash
cargo test test_ssrf_blacklist_blocks_localhost -- --nocapture
```
Expected: FAIL (patterns don't match `localhost?` or `localhost#`).

**Step 3: Fix the patterns**

Replace:
```rust
        r"^https?://localhost[/:]".to_string(),
        r"^https?://localhost$".to_string(),
```
with:
```rust
        r"^https?://localhost([^a-zA-Z0-9]|$)".to_string(),
```

**Step 4: Run tests to confirm they pass**

```bash
cargo test test_ssrf_blacklist -- --nocapture
cargo test test_validate_url -- --nocapture
```
Expected: all pass including existing localhost test.

**Step 5: Commit**

```bash
git add crates/core/http.rs
git commit -m "fix: SSRF blacklist covers localhost?query and localhost#fragment variants; add regression tests"
```

---

### Task 5: Fix `extract_meta_description` — unsafe byte indexing

**Files:**
- Modify: `crates/core/content.rs`

**Current (lines ~117-119):**
```rust
    let content_idx = lower[idx..].find("content=\"")? + idx + "content=\"".len();
    let rest = &head[content_idx..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
```

`rest[..end]` indexes `head` at a byte offset derived from `lower`. If non-ASCII bytes appear between the `name=` and `content=` attributes, the offset could land on a non-char-boundary. Use `.get()`.

**Step 1: Make the fix**

Replace the four lines with:
```rust
    let content_idx = lower[idx..].find("content=\"")? + idx + "content=\"".len();
    let rest = head.get(content_idx..)?;
    let end = rest.find('"')?;
    Some(rest.get(..end)?.to_string())
```

**Step 2: Run tests**

```bash
cargo test test_extract_meta -- --nocapture
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```

**Step 3: Commit**

```bash
git add crates/core/content.rs
git commit -m "fix: extract_meta_description uses .get() for byte-offset slicing to prevent panic on non-ASCII HTML attributes"
```

---

### Task 6: Add test coverage for new ranking logic

**Files:**
- Modify: `tests/vector_v2_ranking_migration.rs`

Add tests for the four gaps identified in the review.

**Step 1: Write failing tests**

Add to `tests/vector_v2_ranking_migration.rs`:

```rust
// ── phrase_boost path ──────────────────────────────────────────────────────

#[test]
fn rerank_boosts_candidate_with_consecutive_query_phrase() {
    use axon::crates::vector::ops::ranking::{rerank_ask_candidates, AskCandidate};
    // Candidate A: phrase "install package" appears consecutively in chunk
    // Candidate B: slightly higher base score, phrase absent
    let a = AskCandidate {
        url: "https://docs.example.com/install".to_string(),
        path: "/install".to_string(),
        chunk_text: "You can install the package by running npm install.".to_string(),
        score: 0.50,
        chunk_index: 0,
        rerank_score: 0.0,
        payload: Default::default(),
    };
    let b = AskCandidate {
        url: "https://docs.example.com/overview".to_string(),
        path: "/overview".to_string(),
        chunk_text: "An overview of the library and its capabilities.".to_string(),
        score: 0.54,
        chunk_index: 0,
        rerank_score: 0.0,
        payload: Default::default(),
    };
    let candidates = vec![b.clone(), a.clone()];
    let tokens = axon::crates::vector::ops::ranking::tokenize_query("install package");
    let reranked = rerank_ask_candidates(&candidates, &tokens);
    // A should rank first due to phrase_boost even though B has higher vector score
    assert_eq!(
        reranked[0].url, a.url,
        "phrase match should outrank higher-score candidate without phrase"
    );
}

// ── stop-word preservation regression ─────────────────────────────────────

#[test]
fn tokenize_query_preserves_intent_verbs_create_and_make() {
    use axon::crates::vector::ops::ranking::tokenize_query;
    let tokens = tokenize_query("how to create a new component");
    assert!(
        tokens.contains(&"create".to_string()),
        "'create' must be preserved — it encodes user intent"
    );
    assert!(
        tokens.contains(&"component".to_string()),
        "'component' must be preserved"
    );
    assert!(
        !tokens.contains(&"how".to_string()),
        "'how' is a stop word and must be dropped"
    );

    let tokens2 = tokenize_query("make a widget");
    assert!(
        tokens2.contains(&"make".to_string()),
        "'make' must be preserved — it encodes user intent"
    );
}

// ── docs_boost path-vs-url boundary ───────────────────────────────────────

#[test]
fn rerank_docs_boost_uses_path_not_url_domain() {
    use axon::crates::vector::ops::ranking::{rerank_ask_candidates, AskCandidate};
    // URL domain contains "docs" but path does NOT — should NOT get docs_boost
    let no_boost = AskCandidate {
        url: "https://my-docs-host.com/blog/post".to_string(),
        path: "/blog/post".to_string(),
        chunk_text: "some content without docs in path".to_string(),
        score: 0.60,
        chunk_index: 0,
        rerank_score: 0.0,
        payload: Default::default(),
    };
    // Path contains "/docs/" — should get docs_boost
    let with_boost = AskCandidate {
        url: "https://example.com/docs/guide".to_string(),
        path: "/docs/guide".to_string(),
        chunk_text: "documentation content".to_string(),
        score: 0.55,
        chunk_index: 0,
        rerank_score: 0.0,
        payload: Default::default(),
    };
    let candidates = vec![no_boost.clone(), with_boost.clone()];
    let tokens = axon::crates::vector::ops::ranking::tokenize_query("docs guide");
    let reranked = rerank_ask_candidates(&candidates, &tokens);
    // with_boost should win despite lower vector score due to docs_boost
    assert_eq!(
        reranked[0].url, with_boost.url,
        "docs_boost must fire on path, not URL domain"
    );
}

// ── get_meaningful_snippet edge cases ──────────────────────────────────────

#[test]
fn get_meaningful_snippet_returns_non_empty_for_relevant_content() {
    use axon::crates::vector::ops::ranking::{get_meaningful_snippet, tokenize_query};
    let text = "Tokio is an async runtime for Rust. \
                It provides async I/O, timers, and task scheduling. \
                You can install tokio by adding it to Cargo.toml.";
    let tokens = tokenize_query("tokio async runtime");
    let snippet = get_meaningful_snippet(text, &tokens);
    assert!(!snippet.is_empty(), "should return a snippet for relevant content");
    assert!(snippet.len() <= 800, "snippet should be reasonably sized");
}

#[test]
fn get_meaningful_snippet_handles_empty_input() {
    use axon::crates::vector::ops::ranking::{get_meaningful_snippet, tokenize_query};
    let snippet = get_meaningful_snippet("", &tokenize_query("anything"));
    // Should not panic; empty or minimal output is acceptable
    assert!(snippet.len() < 100);
}

#[test]
fn get_meaningful_snippet_handles_no_query_tokens() {
    use axon::crates::vector::ops::ranking::get_meaningful_snippet;
    let text = "First sentence. Second sentence. Third sentence. Fourth sentence. Fifth sentence.";
    let snippet = get_meaningful_snippet(text, &[]);
    // With no query tokens, should fall back to first few sentences
    assert!(!snippet.is_empty(), "should return first sentences when no tokens provided");
}
```

**Step 2: Run tests to check which ones fail**

```bash
cargo test --test vector_v2_ranking_migration -- --nocapture 2>&1 | grep -E "FAILED|ok"
```

Note: Some tests may require `AskCandidate` and `rerank_ask_candidates` / `get_meaningful_snippet` to be `pub`. Check visibility and add `pub` where needed in `ranking.rs`.

**Step 3: Adjust visibility in `ranking.rs` if needed**

If `AskCandidate`, `rerank_ask_candidates`, `get_meaningful_snippet`, or `tokenize_query` are private, add `pub` before them. They're already tested indirectly in the existing migration tests, so making them `pub(crate)` or `pub` is safe.

**Step 4: Run until all pass**

```bash
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```
Expected: 105 + N new tests all passing.

**Step 5: Commit**

```bash
git add tests/vector_v2_ranking_migration.rs crates/vector/ops/ranking.rs
git commit -m "test: add ranking coverage — phrase_boost, docs_boost path/url boundary, stop-word preservation, snippet edge cases"
```

---

### Task 7: Remove dead `crates/extract` module

**Files:**
- Modify: `crates/mod.rs` — remove `pub mod extract;`
- Delete: `crates/extract/mod.rs`
- Delete: `crates/extract/` directory

**Step 1: Verify nothing imports from `crates::extract`**

```bash
grep -rn "crates::extract\b" crates/ --include="*.rs"
```
Expected: 0 results (cli/commands/extract.rs imports from jobs/core, not from crates::extract).

**Step 2: Remove the declaration**

In `crates/mod.rs`, delete the line:
```rust
pub mod extract;
```

**Step 3: Delete the empty file and directory**

```bash
rm crates/extract/mod.rs && rmdir crates/extract
```

**Step 4: Verify compilation**

```bash
cargo check --bin axon
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```
Expected: same pass count, clean compile.

**Step 5: Commit**

```bash
git add crates/mod.rs
git commit -m "chore: remove empty crates/extract module — LLM extraction lives in vector/ops/commands"
```

---

### Task 8: Update CLAUDE.md — remove stale file references

**Files:**
- Modify: `CLAUDE.md`

The architecture diagram in CLAUDE.md still references four files deleted in this PR.

**Step 1: Find and fix the stale references**

In the `## Architecture` section of CLAUDE.md, find the jobs section:
```
│   ├── crawl_jobs.rs, crawl_jobs_dispatch.rs
```
Replace with:
```
│   ├── crawl_jobs/    # V2 crawl pipeline (config, manifest, processor, repo, sitemap, watchdog, worker, runtime)
```

Find:
```
│   └── remote_extract.rs  # LLM extraction via OpenAI-compatible API
```
Replace with:
```
│   └── mod.rs          # (placeholder; LLM extraction is in vector/ops/commands/)
```

Wait — actually Task 7 removes the extract module entirely. So update the architecture diagram to remove the `extract` entry from `crates/`:
```
│   ├── extract/        # (removed — LLM extraction is in vector/ops/commands/)
```
becomes simply omitted.

Find the vector section:
```
│   ├── mod.rs, ops.rs, ops_dispatch.rs
│   │   # ops.rs: tei_embed(), qdrant_upsert(), qdrant_search(), run_query_native(), run_ask_native()
```
Replace with:
```
│   ├── mod.rs, ops_dispatch.rs
│   │   # ops_dispatch.rs: re-exports all v2 ops (embed, query, retrieve, ask, evaluate, suggest, sources, domains, stats, dedupe)
```

Also fix in the Gotchas section:
```
`tei_embed()` in `vector/ops.rs` auto-splits batches on HTTP 413
```
→
```
`tei_embed()` in `vector/ops/tei.rs` auto-splits batches on HTTP 413
```

Fix `config.rs` / `content.rs` descriptions in the architecture to reflect they are now module directories with submodules.

**Step 2: Verify**

```bash
grep -n "crawl_jobs\.rs\|crawl_jobs_dispatch\|remote_extract\.rs\|ops\.rs\b" CLAUDE.md
```
Expected: 0 results.

**Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md architecture diagram — remove deleted files, fix module paths"
```

---

### Task 9: Final verification

**Step 1: Full test run**

```bash
cargo test 2>&1 | grep -E "FAILED|ok\. [0-9]+ passed"
```
Expected: all tests pass, count >= 115 (105 original + new ranking tests).

**Step 2: Clippy clean**

```bash
cargo clippy --all-targets -- -D warnings
```
Expected: 0 warnings.

**Step 3: Format check**

```bash
cargo fmt --check
```
Expected: clean.

**Step 4: Confirm no stale references**

```bash
grep -rn "crawl_jobs_dispatch\|remote_extract\b\|ops_no_legacy\|vector/ops\.rs" . --include="*.rs" --include="*.md" | grep -v ".git"
```
Expected: 0 results.

**Step 5: Final commit summary**

```bash
git log --oneline -9
```

---

## Summary of Changes

| Task | File(s) | Type |
|------|---------|------|
| 1 | `crates/jobs/worker_lane.rs` | Fix — 5 error handling issues |
| 2 | `crates/vector/ops/tei.rs` | Fix — ensure_collection HTTP errors |
| 3 | `crates/vector/ops/commands/evaluate.rs` | Fix — 2 evaluate error handling |
| 4 | `crates/core/http.rs` | Fix + Test — SSRF localhost bypass |
| 5 | `crates/core/content.rs` | Fix — safe byte indexing |
| 6 | `tests/vector_v2_ranking_migration.rs` | Test — ranking logic coverage |
| 7 | `crates/mod.rs`, `crates/extract/` | Chore — remove dead module |
| 8 | `CLAUDE.md` | Docs — remove stale references |
| 9 | — | Verify — final checks |
