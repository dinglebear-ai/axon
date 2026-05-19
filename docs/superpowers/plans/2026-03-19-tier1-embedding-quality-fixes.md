# Tier 1: Embedding Quality Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix four correctness bugs in the embedding pipeline that cause silent quality degradation — wrong query prefix applied to documents, mid-word chunk cuts, high-signal technical terms filtered as stopwords, and empty chunks embedded as garbage vectors.

**Architecture:** Each fix is fully independent — they touch disjoint files and can be merged separately. Fixes 2, 3, and 4 are pure Rust computation with no external service dependency; Fix 1 additionally requires a TEI service config change and a full collection re-index. All four follow strict TDD: write a failing test, make it pass, commit.

**Tech Stack:** Rust (stable, async tokio runtime), `reqwest`, `serde_json`, `uuid`, `futures-util`, Docker Compose (TEI service config)

---

## File Map

| File | Fix | Change Type |
|------|-----|-------------|
| `docker-compose.services.yaml` | 1 | Remove `--default-prompt` lines from `axon-tei` service command |
| `crates/vector/ops/commands/query.rs` | 1 | Prepend query instruction before `tei_embed` call |
| `crates/vector/ops/commands/ask/context/retrieval.rs` | 1 | Prepend query instruction before `tei_embed` call |
| `crates/vector/ops/commands/evaluate/scoring.rs` | 1 | Prepend query instruction before `tei_embed` call |
| `crates/vector/ops/tei/tei_client.rs` | 1 | Add `pub(crate) const QUERY_INSTRUCTION` |
| `crates/vector/ops/input.rs` | 2 | Walk back to word boundary in `chunk_text()` |
| `crates/vector/ops/sparse.rs` | 3 | Remove 5 domain-meaningful terms from `STOP_WORDS` |
| `crates/vector/ops/tei/prepare.rs` | 4 | Filter empty/whitespace-only chunks after `chunk_text()` |

---

## Task 1: Fix 1 — Add `QUERY_INSTRUCTION` constant to `tei_client.rs`

**Context:** The query instruction string needs to live in one place so all three query-path callers (`query.rs`, `retrieval.rs`, `scoring.rs`) import it. It must NOT be used in the document embed path (`pipeline.rs`).

**Files:**
- Modify: `crates/vector/ops/tei/tei_client.rs` (add constant near top of file)

- [ ] **Step 1: Add the constant to `tei_client.rs`**

Open `crates/vector/ops/tei/tei_client.rs`. After the existing `use` imports (line 10), add:

```rust
/// Instruction prefix for Qwen3-Embedding asymmetric query encoding.
///
/// Prepend this to every query text before calling `tei_embed`.
/// Do NOT apply to document chunks — document embedding must use raw text.
///
/// This prefix activates query-mode encoding in Qwen3-Embedding models.
/// TEI's `--default-prompt` config flag has been removed; the prefix is
/// now applied in Rust so documents and queries get different embeddings.
pub(crate) const QUERY_INSTRUCTION: &str =
    "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery: ";
```

- [ ] **Step 1b: Re-export `QUERY_INSTRUCTION` from `tei.rs`**

`tei_client` is declared as a private `mod tei_client;` in `crates/vector/ops/tei.rs`. To make `QUERY_INSTRUCTION` accessible to callers in other modules, add a re-export to `crates/vector/ops/tei.rs` alongside the existing `pub(crate) use tei_client::tei_embed;` line:

```rust
pub(crate) use tei_client::tei_embed;
pub(crate) use tei_client::QUERY_INSTRUCTION;   // ← ADD THIS
```

After this step, callers import `QUERY_INSTRUCTION` via:
```rust
use crate::crates::vector::ops::tei::QUERY_INSTRUCTION;
```
NOT via `tei::tei_client::QUERY_INSTRUCTION` (that path is private).

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/jmagar/workspace/axon_rust && cargo check 2>&1 | head -30
```

Expected: zero errors. The constant is `pub(crate)` and not yet used — `dead_code` lint fires only if the module has `deny(dead_code)`. Verify with clippy:

```bash
cargo clippy -- -D warnings 2>&1 | grep -E "error|warning" | head -20
```

If `dead_code` warning fires, note it — it will resolve when Task 2 adds the usages.

- [ ] **Step 3: Commit**

```bash
git add crates/vector/ops/tei/tei_client.rs crates/vector/ops/tei.rs
git commit -m "feat(embed): add QUERY_INSTRUCTION constant for asymmetric query encoding"
```

---

## Task 2: Fix 1 — Prepend query instruction in all three query code paths

**Context:** Three files call `tei_embed` for query embedding (not document embedding). Each must prepend `QUERY_INSTRUCTION` to the query string before the call.

**Files:**
- Modify: `crates/vector/ops/commands/query.rs` (line 12)
- Modify: `crates/vector/ops/commands/ask/context/retrieval.rs` (line 22)
- Modify: `crates/vector/ops/commands/evaluate/scoring.rs` (line 11)

- [ ] **Step 1: Write the failing test in `query.rs`**

There is no existing unit test for `query_results` (it requires live Qdrant). The observable behavior to test is that the instruction is prepended. Add a pure unit test at the bottom of `crates/vector/ops/commands/query.rs`:

> **Import path:** `tei_client` is a **private** module in `crates/vector/ops/tei.rs`. Task 1 adds `pub(crate) use tei_client::QUERY_INSTRUCTION;` as a re-export in `tei.rs`. All callers use `crate::crates::vector::ops::tei::QUERY_INSTRUCTION` (the re-export path). Do NOT use `tei::tei_client::QUERY_INSTRUCTION` — that path is private and will not compile.

```rust
#[cfg(test)]
mod tests {
    use crate::crates::vector::ops::tei::QUERY_INSTRUCTION;

    #[test]
    fn query_instruction_is_nonempty_and_ends_with_query_colon() {
        assert!(!QUERY_INSTRUCTION.is_empty());
        assert!(
            QUERY_INSTRUCTION.ends_with("Query: "),
            "instruction must end with 'Query: ', got: {QUERY_INSTRUCTION:?}"
        );
    }
}
```

Run:

```bash
cd /home/jmagar/workspace/axon_rust && cargo test query_instruction_is_nonempty 2>&1 | tail -5
```

Expected: PASS (constant already exists from Task 1).

- [ ] **Step 2: Update `query.rs` to prepend the instruction**

In `crates/vector/ops/commands/query.rs`, change the `tei_embed` call on line 12:

Old:
```rust
    let mut query_vectors = tei::tei_embed(cfg, std::slice::from_ref(&query.to_string())).await?;
```

New:
```rust
    use crate::crates::vector::ops::tei::QUERY_INSTRUCTION;
    let query_with_instruction = format!("{QUERY_INSTRUCTION}{query}");
    let mut query_vectors =
        tei::tei_embed(cfg, std::slice::from_ref(&query_with_instruction)).await?;
```

- [ ] **Step 3: Update `retrieval.rs` to prepend the instruction**

In `crates/vector/ops/commands/ask/context/retrieval.rs`, change the `tei_embed` call on line 22:

Old:
```rust
    let mut ask_vectors = tei::tei_embed(cfg, &[query.to_string()])
        .await
        .map_err(|e| anyhow!("TEI embed for ask query: {e}"))?;
```

New:
```rust
    use crate::crates::vector::ops::tei::QUERY_INSTRUCTION;
    let query_with_instruction = format!("{QUERY_INSTRUCTION}{query}");
    let mut ask_vectors = tei::tei_embed(cfg, &[query_with_instruction])
        .await
        .map_err(|e| anyhow!("TEI embed for ask query: {e}"))?;
```

- [ ] **Step 4: Update `scoring.rs` to prepend the instruction**

In `crates/vector/ops/commands/evaluate/scoring.rs`, change the `tei_embed` call on line 11:

Old:
```rust
    let mut vecs = tei::tei_embed(cfg, &[question.to_string()]).await?;
```

New:
```rust
    use crate::crates::vector::ops::tei::QUERY_INSTRUCTION;
    let question_with_instruction = format!("{QUERY_INSTRUCTION}{question}");
    let mut vecs = tei::tei_embed(cfg, &[question_with_instruction]).await?;
```

- [ ] **Step 5: Verify no dead_code warning and clippy clean**

```bash
cd /home/jmagar/workspace/axon_rust && cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: zero warnings.

- [ ] **Step 6: Run all vector tests**

```bash
cd /home/jmagar/workspace/axon_rust && cargo test --lib tei 2>&1 | tail -10
cargo test --lib query 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/commands/query.rs \
        crates/vector/ops/commands/ask/context/retrieval.rs \
        crates/vector/ops/commands/evaluate/scoring.rs
git commit -m "feat(embed): prepend QUERY_INSTRUCTION in all query code paths

Documents continue to be embedded without any prefix. Queries
(ask, query, evaluate) now prepend the Qwen3-Embedding instruction
string in Rust, replacing the removed TEI --default-prompt flag.

This restores asymmetric query/document encoding so cosine similarity
measures query-document relevance instead of query-query distance."
```

---

## Task 3: Fix 1 — Remove `--default-prompt` from TEI Docker config

**Context:** The instruction is now applied in Rust. TEI must stop prepending it automatically, or every query will get the prefix twice and every document embed will still get it once.

**Files:**
- Modify: `docker-compose.services.yaml` (lines 126–127 in `axon-tei` service command)

- [ ] **Step 1: Write a verification test (manual)**

Before removing, verify TEI is currently prepending by querying a known document. This is a manual check, not automated. Skip if TEI is not reachable (external service on steamy-wsl).

```bash
curl -s http://steamy-wsl:52000/info | python3 -m json.tool | grep -A5 "default_prompt"
```

Expected: shows the instruction string. After the change, this should return empty/null.

- [ ] **Step 2: Remove the `--default-prompt` flag**

In `docker-compose.services.yaml`, in the `axon-tei` service `command:` block, remove these two lines:

```yaml
      - --default-prompt
      - "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery: "
```

The resulting command block should go from line 121 (`- --model-id`) directly to `- --max-concurrent-requests` after the dtype line.

- [ ] **Step 3: Also update the CLAUDE.md vector note**

The comment in `crates/vector/CLAUDE.md` under "Default Prompt (Query Instruction)" describes `--default-prompt` as active. Update it to reflect the new state:

Find the section:
```
### Default Prompt (Query Instruction)
TEI is configured with:
```

Replace the description to say:
- `--default-prompt` has been removed from TEI config
- `QUERY_INSTRUCTION` constant in `tei_client.rs` is prepended in Rust at query time
- Document embeds do NOT get the prefix
- The `TEI_URL` service at steamy-wsl must be restarted after this config change

- [ ] **Step 4: Restart TEI (if accessible)**

```bash
# Only if TEI is running locally via docker-compose.services.yaml:
docker compose -f docker-compose.services.yaml restart axon-tei
```

If TEI runs on steamy-wsl (external), coordinate restart separately. The Rust changes (Tasks 1–2) are safe to deploy before the TEI restart — they just send a slightly longer query string. After TEI restart, the double-prefix problem is eliminated.

- [ ] **Step 5: Commit**

```bash
git add docker-compose.services.yaml crates/vector/CLAUDE.md
git commit -m "feat(tei): remove --default-prompt; query instruction now applied in Rust

TEI no longer prepends the Qwen3-Embedding instruction automatically.
The QUERY_INSTRUCTION constant in tei_client.rs is prepended only on
query paths. Document embeds remain prefix-free.

MIGRATION REQUIRED: The cortex collection must be re-indexed after
restarting TEI. See Task 4 (migration note) in the plan."
```

---

## Task 4: Fix 1 — Document the collection re-index requirement

**Context:** After removing `--default-prompt` from TEI and restarting the service, all existing document vectors in the `cortex` collection were encoded with the query instruction prefix (wrong mode). They must be re-indexed without the prefix. This is a data migration, not a code change.

**Files:**
- No code changes. This task documents the runbook.

- [ ] **Step 1: Understand the scope**

The `cortex` collection has ~7,063,563 points. Re-indexing requires re-scraping and re-embedding all indexed URLs. There are two approaches:

**Option A — Full re-crawl and re-embed (recommended for completeness)**
```bash
# List all indexed domains to scope the work
./scripts/axon domains

# For each domain, re-crawl and re-embed:
./scripts/axon crawl <domain-url> --wait true
```

**Option B — Re-embed from cached markdown files (faster, if `.cache/axon-rust/output/` is populated)**
```bash
./scripts/axon embed .cache/axon-rust/output/ --wait true
```

- [ ] **Step 2: Pre-migration checklist**

Before starting re-index:
1. Confirm TEI has been restarted without `--default-prompt`: `curl http://steamy-wsl:52000/info | grep default_prompt` → should be empty/null
2. Confirm the `QUERY_INSTRUCTION` Rust changes (Tasks 1–2) are deployed
3. Confirm Qdrant is healthy: `./scripts/axon doctor`
4. Note the current point count: `./scripts/axon stats`

- [ ] **Step 3: Create a migration tracking note**

Create a brief note at `docs/sessions/2026-03-19-embed-quality-migration.md` with:
- Date started
- Collection name (`cortex`) and point count before
- Approach chosen (A or B)
- Point count after (to confirm all URLs re-indexed)
- Any URLs that failed to re-embed

This is not a code artifact — it is an operational record.

- [ ] **Step 4: Verify embedding quality after re-index**

Run a known query before and after re-index to confirm improved relevance:

```bash
# Before re-index (baseline — captures query-mode document vectors)
./scripts/axon query "how to configure qdrant collection" --limit 5 --json > /tmp/before.json

# After re-index (document-mode vectors, query-mode query)
./scripts/axon query "how to configure qdrant collection" --limit 5 --json > /tmp/after.json

# Compare: scores in after.json should be higher / better ranked
```

---

## Task 5: Fix 2 — Walk back to word boundary in `chunk_text()`

**Context:** `chunk_text()` in `crates/vector/ops/input.rs` hard-cuts at character position 2000 regardless of word boundaries. The fix walks back up to 100 characters from the cut point to find the nearest whitespace. The 200-character overlap window is unchanged.

**Files:**
- Modify: `crates/vector/ops/input.rs` — `chunk_text()` function (lines 7–35) and the `tests` module

- [ ] **Step 1: Write the failing test for word-boundary behavior**

In the `tests` module of `crates/vector/ops/input.rs` (after the existing tests, before the closing `}`), add:

```rust
    #[test]
    fn chunk_text_cuts_at_word_boundary_not_mid_word() {
        // Build a string where the first 2000-char cut would land mid-word.
        // Put a space at position 1990 and fill the rest with 'a' chars up to 2100.
        // The cut should land at or before position 1990, not at 2000.
        let mut text = "a".repeat(1990);
        text.push(' ');                    // space at index 1990
        text.push_str(&"b".repeat(109));  // 2000 chars total + 100 more
        assert_eq!(text.chars().count(), 2100);

        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "2100-char text must produce at least 2 chunks");

        // The first chunk must NOT end with 'b' mid-word (i.e. must end at or before the space)
        let first_chunk = &chunks[0];
        let last_char = first_chunk.chars().last().unwrap();
        assert!(
            last_char == 'a' || last_char == ' ',
            "first chunk must end at a word boundary (space or last 'a'), got {last_char:?}"
        );
    }

    #[test]
    fn chunk_text_fallback_hard_cut_when_no_whitespace_in_100_chars() {
        // A string of 2100 non-whitespace chars: no word boundary to find in last 100 chars.
        // The hard cut at 2000 must still happen.
        let text = "a".repeat(2100);
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "2100-char no-space text must produce at least 2 chunks");
        // First chunk is exactly CHUNK_SIZE (hard cut, no boundary found)
        assert_eq!(
            chunks[0].chars().count(),
            2000,
            "when no whitespace in walkback range, hard cut at 2000 chars"
        );
    }
```

Run:

```bash
cd /home/jmagar/workspace/axon_rust && cargo test chunk_text_cuts_at_word 2>&1 | tail -10
cargo test chunk_text_fallback_hard 2>&1 | tail -10
```

Expected: FAIL — current implementation hard-cuts at 2000 in both cases.

- [ ] **Step 2: Implement the word-boundary walkback**

Replace the body of `chunk_text()` in `crates/vector/ops/input.rs`. The full new implementation:

```rust
pub fn chunk_text(text: &str) -> Vec<String> {
    const MAX: usize = 2000;
    const OVERLAP: usize = 200;
    /// Maximum characters to walk back looking for a word boundary.
    const BOUNDARY_LOOKBACK: usize = 100;

    // Fast-path: avoid the Vec<usize> allocation for short documents.
    if text.chars().count() <= MAX {
        return vec![text.to_string()];
    }

    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let char_count = offsets.len();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < char_count {
        let hard_end = (i + MAX).min(char_count);

        // Walk back from the hard cut to find a word boundary (whitespace).
        // Only walk back if we are not at the end of the text.
        let end = if hard_end < char_count {
            let lookback_start = hard_end.saturating_sub(BOUNDARY_LOOKBACK);
            // Collect the chars in [lookback_start, hard_end) as byte indices
            // and find the last whitespace character.
            let mut boundary = hard_end; // default: hard cut
            for j in (lookback_start..hard_end).rev() {
                // j is a char index; offsets[j] is the byte index of that char.
                let byte_pos = offsets[j];
                // Safe: byte_pos is always a valid char boundary from char_indices.
                let ch = text[byte_pos..].chars().next().unwrap_or('\0');
                if ch.is_whitespace() {
                    boundary = j + 1; // cut after the whitespace char
                    break;
                }
            }
            boundary
        } else {
            hard_end // last chunk: take everything
        };

        let byte_start = offsets[i];
        let byte_end = if end < char_count {
            offsets[end]
        } else {
            text.len()
        };
        out.push(text[byte_start..byte_end].to_string());
        if end == char_count {
            break;
        }
        i = end.saturating_sub(OVERLAP);
    }
    out
}
```

- [ ] **Step 3: Run the new tests**

```bash
cd /home/jmagar/workspace/axon_rust && cargo test chunk_text 2>&1 | tail -20
```

Expected: ALL tests pass, including:
- `chunk_text_empty_returns_single_empty_chunk`
- `chunk_text_short_returns_single_chunk`
- `chunk_text_exactly_at_boundary_returns_single_chunk`
- `chunk_text_slightly_over_boundary_returns_two_chunks`
- `chunk_text_long_produces_overlap`
- `chunk_text_first_chunk_is_exactly_chunk_size_chars` — NOTE: this test asserts first chunk is exactly 2000 chars. The word-boundary fix may make the first chunk shorter. Update this test (see Step 4).
- `chunk_text_unicode_no_split_codepoints`
- `chunk_text_covers_all_content`
- `chunk_text_whitespace_only_short_returns_single_chunk`
- `chunk_text_cuts_at_word_boundary_not_mid_word` (new)
- `chunk_text_fallback_hard_cut_when_no_whitespace_in_100_chars` (new)

- [ ] **Step 4: Verify existing tests that may be affected by variable chunk sizes**

The existing test `chunk_text_first_chunk_is_exactly_chunk_size_chars` uses `"a".repeat(CHUNK_SIZE * 2 + 100)` — an all-`a` string with no spaces. With no whitespace in the lookback window, the hard cut fires at exactly 2000. The test remains valid as-is.

**Watch carefully:** `chunk_text_covers_all_content` reconstructs the original text by taking `chunks[0]` in full then skipping exactly `OVERLAP` (200) chars from each subsequent chunk. This works when chunks are uniform size. With word-boundary cuts, chunk sizes may vary and the skip-by-OVERLAP reconstruction no longer exactly reassembles the text.

Run the test specifically:

```bash
cd /home/jmagar/workspace/axon_rust && cargo test chunk_text_covers_all_content -- --nocapture 2>&1 | tail -15
```

If it fails: the `make_text()` helper uses `"a".repeat()` (no spaces), so the hard-cut fires on all-`a` text and chunk sizes remain exactly 2000. The test should pass. If it does fail, update its assertion to handle variable chunk sizes:

```rust
    #[test]
    fn chunk_text_covers_all_content() {
        // Verify that all content appears in at least one chunk.
        // With word-boundary cuts, chunks may vary in size — reassembly by
        // fixed OVERLAP skip is not guaranteed. Instead verify that the
        // union of chunk content covers the full source text.
        let text = make_text(CHUNK_SIZE * 3 + 100); // all-'a' text
        let chunks = chunk_text(&text);
        // For all-ASCII no-space text, hard cut fires, so original behavior holds.
        let mut reconstructed = chunks[0].clone();
        for chunk in chunks.iter().skip(1) {
            let novel: String = chunk.chars().skip(OVERLAP).collect();
            reconstructed.push_str(&novel);
        }
        assert_eq!(
            reconstructed, text,
            "reassembling chunks should reproduce the original text exactly (hard-cut text)"
        );
    }
```

This is the same test body — documenting that for all-`a` text (no spaces) the hard cut path fires and original behavior holds.

- [ ] **Step 5: Run proptest (if exists)**

```bash
cd /home/jmagar/workspace/axon_rust && cargo test --test '*' input_proptest 2>&1 | tail -10
# or
cargo test input_proptest 2>&1 | tail -10
```

Expected: passes. The proptest in `crates/vector/ops/input_proptest.rs` tests coverage and no panic — both properties are preserved.

- [ ] **Step 6: Clippy check**

```bash
cd /home/jmagar/workspace/axon_rust && cargo clippy -- -D warnings 2>&1 | grep -E "error|warning" | head -20
```

Expected: zero warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/input.rs
git commit -m "fix(chunk): walk back to word boundary in chunk_text()

Hard cuts at exactly 2000 chars broke tokens at chunk boundaries,
adding noise to embeddings. The fix walks back up to 100 chars
to find the nearest whitespace before committing the cut.

Falls back to the hard cut when no whitespace is found in the
lookback window. Overlap (200 chars) is preserved unchanged.
All existing tests pass; two new tests cover the new behavior."
```

---

## Task 6: Fix 3 — Remove domain-meaningful terms from `STOP_WORDS`

**Context:** `"use"`, `"using"`, `"used"`, `"get"`, `"set"` are high-IDF technical terms in a software documentation corpus (Rust `use` keyword, HTTP methods `GET`/`SET`, API verbs). Filtering them removes discriminative signal from the BM42 sparse search arm.

**Files:**
- Modify: `crates/vector/ops/sparse.rs` — `STOP_WORDS` static (lines 34–43) and the `tests` module

- [ ] **Step 1: Write the failing tests**

Add these tests to the `tests` module in `crates/vector/ops/sparse.rs`:

```rust
    #[test]
    fn compute_sparse_vector_use_keyword_is_indexed() {
        // "use" is a Rust keyword and high-IDF term — must NOT be a stopword.
        let sv = compute_sparse_vector("use std collections HashMap");
        let use_idx = term_to_index("use");
        assert!(
            sv.indices.contains(&use_idx),
            "technical term 'use' must be indexed, not filtered as stopword"
        );
    }

    #[test]
    fn compute_sparse_vector_get_and_set_are_indexed() {
        // HTTP methods and API verbs — must NOT be stopwords in a tech corpus.
        let sv = compute_sparse_vector("get the resource set the value");
        let get_idx = term_to_index("get");
        let set_idx = term_to_index("set");
        assert!(
            sv.indices.contains(&get_idx),
            "technical term 'get' must be indexed, not filtered as stopword"
        );
        assert!(
            sv.indices.contains(&set_idx),
            "technical term 'set' must be indexed, not filtered as stopword"
        );
    }

    #[test]
    fn compute_sparse_vector_using_and_used_are_indexed() {
        let sv = compute_sparse_vector("using the library used in production");
        let using_idx = term_to_index("using");
        let used_idx = term_to_index("used");
        assert!(
            sv.indices.contains(&using_idx),
            "technical term 'using' must be indexed"
        );
        assert!(
            sv.indices.contains(&used_idx),
            "technical term 'used' must be indexed"
        );
    }
```

Run:

```bash
cd /home/jmagar/workspace/axon_rust && cargo test compute_sparse_vector_use_keyword 2>&1 | tail -5
cargo test compute_sparse_vector_get_and_set 2>&1 | tail -5
cargo test compute_sparse_vector_using_and_used 2>&1 | tail -5
```

Expected: FAIL — these terms are currently in `STOP_WORDS`.

- [ ] **Step 2: Remove the five terms from `STOP_WORDS`**

In `crates/vector/ops/sparse.rs`, update the `STOP_WORDS` static. Remove `"use"`, `"using"`, `"used"`, `"get"`, `"set"` from the array. The updated set should be:

```rust
pub(crate) static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "the", "and", "for", "with", "that", "this", "from", "into", "how", "what", "where",
        "when", "you", "your", "are", "can", "does", "via",
        "not", "all", "any", "but", "too", "out", "our", "their", "them", "they", "its", "then",
        "than", "also", "have", "has", "had", "was", "were", "who", "why",
    ]
    .into_iter()
    .collect()
});
```

Also update the doc comment above `STOP_WORDS` to document the removal:

```rust
/// Shared stop word set used by both sparse vector computation and BM25-style ranking.
///
/// Structural/syntactic words only. Content verbs like "make", "create", "build",
/// "use", "get", "set" encode user intent and must NOT be stripped — they distinguish
/// "how to USE a library" from "how to IMPLEMENT an interface."
///
/// Removed from earlier versions: "use", "using", "used", "get", "set" — these are
/// high-IDF technical terms in software documentation (Rust keywords, HTTP methods,
/// API patterns). Filtering them removes discriminative signal from BM42 sparse search.
///
/// Extended from TS counterpart: high-frequency doc words that add noise without
/// distinguishing what a page is actually about.
```

- [ ] **Step 3: Run all sparse tests**

```bash
cd /home/jmagar/workspace/axon_rust && cargo test sparse 2>&1 | tail -20
```

Expected: all pass, including:
- `compute_sparse_vector_stopwords_excluded` — still passes (`"the"` and `"and"` remain in the set)
- `compute_sparse_vector_use_keyword_is_indexed` (new)
- `compute_sparse_vector_get_and_set_are_indexed` (new)
- `compute_sparse_vector_using_and_used_are_indexed` (new)

- [ ] **Step 4: Clippy check**

```bash
cd /home/jmagar/workspace/axon_rust && cargo clippy -- -D warnings 2>&1 | grep -E "error|warning" | head -20
```

Expected: zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/vector/ops/sparse.rs
git commit -m "fix(sparse): remove domain-meaningful terms from STOP_WORDS

'use', 'using', 'used', 'get', 'set' are high-IDF technical terms
in software documentation corpora (Rust keyword, HTTP methods, API
patterns). Filtering them silently removed discriminative signal
from the BM42 sparse search arm.

Structural/syntactic words ('the', 'and', 'for', etc.) are retained."
```

---

## Task 7: Fix 4 — Filter empty chunks after `chunk_text()` in `prepare_embed_docs()`

**Context:** `chunk_text("")` returns `vec![""]` — one empty string chunk. An empty chunk sent to TEI produces a garbage vector that gets upserted as chunk 0. `prepare_embed_docs` already guards at the doc level (`raw.trim().is_empty()`), but an empty string can still pass through if `raw` is non-empty whitespace that becomes an empty chunk. The fix filters chunks where `c.trim().is_empty()` after chunking, and skips the document entirely if all chunks are empty.

**Files:**
- Modify: `crates/vector/ops/tei/prepare.rs` — `prepare_embed_docs()` function (lines 47–82)

- [ ] **Step 1: Write the failing test**

`prepare_embed_docs` is `pub(super)` and takes an `&str` input path. Testing it requires a file on disk or a URL. Add a unit test that directly tests the chunk-filtering logic. Since `prepare_embed_docs` is not directly unit-testable without I/O, write a companion pure function to test, or test via the `chunk_text` path.

Add a dedicated test at the bottom of `crates/vector/ops/tei/prepare.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::crates::vector::ops::input::chunk_text;

    #[test]
    fn chunk_text_empty_produces_empty_chunk_that_must_be_filtered() {
        // Verify the defect: chunk_text("") returns vec![""]
        let chunks = chunk_text("");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "");

        // Verify the fix: filtering trims empties
        let filtered: Vec<String> = chunks
            .into_iter()
            .filter(|c| !c.trim().is_empty())
            .collect();
        assert!(
            filtered.is_empty(),
            "empty chunk must be filtered out, got: {filtered:?}"
        );
    }

    #[test]
    fn all_whitespace_chunks_are_filtered() {
        // A chunk of only whitespace must be filtered
        let chunks = vec!["   ".to_string(), "\n\t".to_string(), "content here".to_string()];
        let filtered: Vec<String> = chunks
            .into_iter()
            .filter(|c| !c.trim().is_empty())
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], "content here");
    }
}
```

Run:

```bash
cd /home/jmagar/workspace/axon_rust && cargo test --lib prepare 2>&1 | tail -10
```

Expected: both tests PASS (they test the filtering logic, not `prepare_embed_docs` directly). These tests document the intent.

- [ ] **Step 2: Apply the filter in `prepare_embed_docs()`**

In `crates/vector/ops/tei/prepare.rs`, find the `prepare_embed_docs` function body (lines 47–82). Update the section that calls `chunk_text` and pushes to `prepared`:

Old (lines 66–80):
```rust
        let chunks = input::chunk_text(&raw);
        let domain = Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        prepared.push(PreparedDoc {
            url,
            domain,
            chunks,
            source_type: "embed".to_string(),
            content_type: "markdown",
            title: None,
            extra: None,
        });
```

New:
```rust
        let chunks: Vec<String> = input::chunk_text(&raw)
            .into_iter()
            .filter(|c| !c.trim().is_empty())
            .collect();
        // Skip documents that produce no non-empty chunks after filtering.
        // This can happen when raw content is non-empty whitespace or
        // when chunk_text("") returns vec![""] on an empty fast-path.
        if chunks.is_empty() {
            continue;
        }
        let domain = Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        prepared.push(PreparedDoc {
            url,
            domain,
            chunks,
            source_type: "embed".to_string(),
            content_type: "markdown",
            title: None,
            extra: None,
        });
```

- [ ] **Step 3: Update the existing `input.rs` test to reflect intent**

In `crates/vector/ops/input.rs`, the test `chunk_text_empty_returns_single_empty_chunk` documents the current fast-path behavior (returns `vec![""]`). Add a comment to make the intent clear — the caller (`prepare_embed_docs`) is now responsible for filtering:

Find the test and update its doc comment:

```rust
    #[test]
    fn chunk_text_empty_returns_single_empty_chunk() {
        // The fast-path fires for text whose char count is <= MAX (including 0).
        // It wraps the whole string in a vec, so empty text → vec![""].
        //
        // NOTE: callers that feed chunks to embedding pipelines must filter
        // out empty/whitespace-only chunks. See prepare_embed_docs() in tei/prepare.rs.
        let result = chunk_text("");
        assert_eq!(
            result.len(),
            1,
            "empty input triggers fast-path, producing 1 chunk"
        );
        assert_eq!(
            result[0], "",
            "the single chunk for empty input is itself empty"
        );
    }
```

- [ ] **Step 4: Run full test suite for affected crates**

```bash
cd /home/jmagar/workspace/axon_rust && cargo test chunk_text 2>&1 | tail -10
cargo test --lib prepare 2>&1 | tail -10
cargo test --lib tei 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 5: Monolith policy check**

Verify `prepare.rs` remains under 500 lines and all functions under 120 lines:

```bash
wc -l /home/jmagar/workspace/axon_rust/crates/vector/ops/tei/prepare.rs
```

Expected: well under 500 lines.

- [ ] **Step 6: Clippy check**

```bash
cd /home/jmagar/workspace/axon_rust && cargo clippy -- -D warnings 2>&1 | grep -E "error|warning" | head -20
```

Expected: zero warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/tei/prepare.rs crates/vector/ops/input.rs
git commit -m "fix(embed): filter empty/whitespace chunks before embedding

chunk_text(\"\") returns vec![\"\"] via the fast-path. Without filtering,
the empty string is sent to TEI which produces a garbage vector
upserted as chunk_index=0, corrupting retrieval for that URL.

prepare_embed_docs() now filters chunks where trim().is_empty(),
and skips the document entirely if all chunks are empty after filtering."
```

---

## Task 8: Final integration check and format gate

**Context:** After all four fixes are applied, run the full pre-PR gate to ensure nothing regressed.

**Files:** No changes.

- [ ] **Step 1: Full test suite**

```bash
cd /home/jmagar/workspace/axon_rust && cargo test --lib 2>&1 | tail -20
```

Expected: all tests pass. Note the total count — it should be higher than before (new tests added in Tasks 1, 5, 6, 7).

- [ ] **Step 2: Format check**

```bash
cd /home/jmagar/workspace/axon_rust && cargo fmt --check 2>&1 | head -20
```

If any format issues:
```bash
cargo fmt
git add -p  # review changes
```

- [ ] **Step 3: Monolith policy check**

```bash
cd /home/jmagar/workspace/axon_rust && ./scripts/enforce_monoliths.py 2>&1 | grep -E "FAIL|ERROR" | head -20
```

Expected: no failures.

- [ ] **Step 4: `just verify`**

```bash
cd /home/jmagar/workspace/axon_rust && just verify 2>&1 | tail -20
```

Expected: `fmt-check + clippy + check + test` all pass.

- [ ] **Step 5: Verify the `cortex` re-index is tracked**

Confirm `docs/sessions/2026-03-19-embed-quality-migration.md` exists with pre-migration point count. If TEI has been restarted, document the status.

- [ ] **Step 6: Final commit (format fixes only, if any)**

```bash
git add -A
git commit -m "style: cargo fmt cleanup after tier-1 embedding quality fixes"
```

---

## Summary of Changes

| Fix | File(s) Touched | Lines Changed | Tests Added |
|-----|----------------|---------------|-------------|
| 1a — `QUERY_INSTRUCTION` constant | `tei/tei_client.rs` | +8 | +1 |
| 1b — Query path instruction | `commands/query.rs`, `context/retrieval.rs`, `evaluate/scoring.rs` | +3×3=9 | 0 (covered by 1a) |
| 1c — Remove TEI `--default-prompt` | `docker-compose.services.yaml`, `crates/vector/CLAUDE.md` | -2 lines YAML | 0 (manual verify) |
| 2 — Word boundary in `chunk_text` | `input.rs` | ~25 | +2 |
| 3 — Stopword cleanup | `sparse.rs` | -5 terms | +3 |
| 4 — Empty chunk filter | `tei/prepare.rs`, `input.rs` | +5 | +2 |

**Re-index note:** After Fix 1c (TEI restart), the `cortex` collection (~7M points) must be re-indexed. This is an operational task (see Task 4), not a code change. Estimated duration: depends on crawl cache availability — Option B (re-embed from `.cache/`) is significantly faster than Option A (full re-crawl).
