# Hybrid Search (Dense + Sparse / BM42) Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add BM42 sparse vector indexing alongside the existing dense TEI embeddings so the Qdrant `/query` RRF fusion endpoint can be used for hybrid search — rescuing keyword-precise queries that semantic search scores poorly.

**Architecture:** New collections get named `dense` + named `bm42` sparse vectors via an updated `ensure_collection`. Existing unnamed-vector collections are detected and left untouched (dense-only fallback). A new `qdrant_hybrid_search` function uses the `/query` prefetch+RRF endpoint; query/ask callers use it when the collection is in Named mode. A pure-Rust BM42 tokenizer computes sparse TF vectors client-side; Qdrant applies IDF server-side via `"modifier": "idf"`.

**Tech Stack:** Qdrant v1.13.1 (`/query` endpoint + `sparse_vectors` collection config), pure Rust hashing (no new crates), existing `reqwest` + `serde_json` + `httpmock`.

---

## Background: What Changes and Why

### Vector modes
| Mode | Collection config | Point wire format | Search endpoint |
|------|------------------|-------------------|-----------------|
| `Unnamed` (legacy) | `"vectors": {"size": N, "distance": "Cosine"}` | `"vector": [...]` | `/points/search` |
| `Named` (hybrid) | `"vectors": {"dense": {"size": N, "distance": "Cosine"}}, "sparse_vectors": {"bm42": {"modifier": "idf"}}` | `"vector": {"dense": [...], "bm42": {"indices": [...], "values": [...]}}` | `/points/query` |

Existing `cortex` collection has Unnamed mode. Detection happens at `ensure_collection` time.

### Migration story
No automatic migration of the live 2.57M-point collection. To get hybrid search:
1. In `.env`, set `AXON_COLLECTION=cortex_v2` (any new name)
2. Re-crawl/embed into the new collection
3. The new collection is created in Named mode; hybrid search activates automatically

Existing `cortex` collection continues working in dense-only mode — no interruption.

### BM42 sparse vectors
- Tokenize text to lowercase alphanumeric terms (≥3 chars, no stopwords)
- Hash each unique term: `hash(term) % 30_000` → Qdrant bucket index
- Weight: raw term frequency count (Qdrant applies IDF correction server-side via `"modifier": "idf"`)
- No new crate dependency — `std::collections::hash_map::DefaultHasher` with fixed seed

### RRF fusion
Qdrant's `/query` endpoint with `"fusion": "rrf"` merges dense and sparse ranked lists using Reciprocal Rank Fusion: `score = Σ 1/(k + rank_i)` where `k=60`. No additional scoring code needed.

---

## File Map

### New files
| File | Responsibility |
|------|---------------|
| `crates/vector/ops/sparse.rs` | `SparseVector` struct + `compute_sparse_vector(text)` + tokenizer |
| `crates/vector/ops/qdrant/hybrid.rs` | `qdrant_hybrid_search(cfg, dense, sparse, limit)` using `/query` RRF |

### Modified files
| File | What changes |
|------|-------------|
| `crates/vector/ops/tei/qdrant_store.rs` | `VectorMode` enum; replace `collection_needs_init(bool)` with `collection_init_or_cached()→VectorMode`; `ensure_collection` detects Unnamed/Named and creates Named for new collections |
| `crates/vector/ops/qdrant.rs` | Re-export `qdrant_hybrid_search` and `VectorMode` |
| `crates/vector/ops/tei.rs` | `embed_chunks_impl` + `build_batch_points` use `VectorMode`; Named mode adds sparse vectors to each point |
| `crates/vector/ops/tei/pipeline.rs` | `embed_prepared_doc` uses `collection_init_or_cached`; Named mode adds sparse vectors |
| `crates/vector/ops/commands/query.rs` | Use `qdrant_hybrid_search` when Named, fall back to `qdrant_search` when Unnamed |
| `crates/vector/ops/commands/ask/context/retrieval.rs` | Same hybrid/dense dispatch |
| `crates/core/config/types/config.rs` | Add `hybrid_search_enabled: bool`, `hybrid_search_candidates: usize` |
| `crates/core/config/types/config_impls.rs` | Add defaults |
| `crates/core/config/parse/build_config.rs` | Parse `AXON_HYBRID_SEARCH` + `AXON_HYBRID_CANDIDATES` from env |

---

## Chunk 1: Sparse Vector Computation + Collection Mode Detection

### Task 1: `sparse.rs` — BM42 sparse vector computation

**Files:**
- Create: `crates/vector/ops/sparse.rs`

- [ ] **Step 1.1: Write failing tests for `compute_sparse_vector`**

```rust
// At the bottom of sparse.rs under #[cfg(test)]

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_sparse_vector_empty_text_returns_empty() {
        let sv = compute_sparse_vector("");
        assert!(sv.indices.is_empty());
        assert!(sv.values.is_empty());
    }

    #[test]
    fn compute_sparse_vector_whitespace_only_returns_empty() {
        let sv = compute_sparse_vector("   \n\t  ");
        assert!(sv.indices.is_empty());
    }

    #[test]
    fn compute_sparse_vector_indices_and_values_same_length() {
        let sv = compute_sparse_vector("hello world rust programming");
        assert_eq!(sv.indices.len(), sv.values.len());
    }

    #[test]
    fn compute_sparse_vector_all_values_positive() {
        let sv = compute_sparse_vector("axon collection config embed");
        for &v in &sv.values {
            assert!(v > 0.0, "all TF weights must be positive, got {v}");
        }
    }

    #[test]
    fn compute_sparse_vector_all_indices_in_bucket_range() {
        let sv = compute_sparse_vector("qdrant vector sparse embedding search");
        for &idx in &sv.indices {
            assert!(
                idx < SPARSE_DIM,
                "index {idx} must be < SPARSE_DIM={SPARSE_DIM}"
            );
        }
    }

    #[test]
    fn compute_sparse_vector_repeated_term_has_higher_weight() {
        // "rust rust rust" should give the bucket for "rust" a higher weight
        // than a doc with just one "rust".
        let sv_single = compute_sparse_vector("rust language systems");
        let sv_triple = compute_sparse_vector("rust rust rust language systems");
        // Find the bucket for "rust" (consistent hash).
        let rust_idx = term_to_index("rust");
        let single_val = sv_single
            .indices
            .iter()
            .zip(&sv_single.values)
            .find(|(&i, _)| i == rust_idx)
            .map(|(_, &v)| v)
            .unwrap_or(0.0);
        let triple_val = sv_triple
            .indices
            .iter()
            .zip(&sv_triple.values)
            .find(|(&i, _)| i == rust_idx)
            .map(|(_, &v)| v)
            .unwrap_or(0.0);
        assert!(
            triple_val > single_val,
            "triple occurrence must have higher weight: {triple_val} <= {single_val}"
        );
    }

    #[test]
    fn compute_sparse_vector_stopwords_excluded() {
        // "the" and "and" are stopwords and must not appear as indices.
        let sv = compute_sparse_vector("the quick and brown fox");
        let the_idx = term_to_index("the");
        let and_idx = term_to_index("and");
        assert!(
            !sv.indices.contains(&the_idx),
            "stopword 'the' must not appear"
        );
        assert!(
            !sv.indices.contains(&and_idx),
            "stopword 'and' must not appear"
        );
    }

    #[test]
    fn compute_sparse_vector_no_duplicate_indices() {
        let sv = compute_sparse_vector("embed qdrant vector search collection cortex");
        let mut seen = std::collections::HashSet::new();
        for &idx in &sv.indices {
            assert!(seen.insert(idx), "duplicate index {idx} found in sparse vector");
        }
    }

    #[test]
    fn term_to_index_is_stable() {
        // Same term must always produce the same index across calls.
        let idx_a = term_to_index("embedding");
        let idx_b = term_to_index("embedding");
        assert_eq!(idx_a, idx_b, "term_to_index must be deterministic");
    }

    #[test]
    fn compute_sparse_vector_short_tokens_excluded() {
        // Tokens shorter than 3 chars must not appear.
        let sv = compute_sparse_vector("go is ok but rust is great");
        // "go", "is", "ok" are 2 chars or fewer.
        let go_idx = term_to_index("go");
        let is_idx = term_to_index("is");
        assert!(
            !sv.indices.contains(&go_idx),
            "short token 'go' must not appear"
        );
        assert!(
            !sv.indices.contains(&is_idx),
            "short token 'is' must not appear"
        );
    }
}
```

- [ ] **Step 1.2: Run tests to verify they fail**

```bash
cargo test sparse -- --nocapture 2>&1 | head -30
```
Expected: compile error (`sparse` module not declared)

- [ ] **Step 1.3: Implement `sparse.rs`**

Create `crates/vector/ops/sparse.rs`:

```rust
//! BM42-style sparse vector computation.
//!
//! Computes TF-weighted sparse vectors for Qdrant's `bm42` sparse index.
//! Qdrant applies IDF correction server-side (`"modifier": "idf"` in collection config).
//! Client-side we emit raw term frequency counts — no normalization needed here.
//!
//! # Hash stability
//! `term_to_index` uses a fixed-seed FNV-1a hash. The seed is baked into the constant
//! so the same term always maps to the same bucket index across process restarts.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::collections::HashSet;

/// Number of sparse vector buckets. Matches BERT vocabulary size for compatibility.
pub const SPARSE_DIM: u32 = 30_522;

static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "the", "and", "for", "with", "that", "this", "from", "into", "how", "what",
        "where", "when", "you", "your", "are", "can", "does", "use", "using", "used",
        "get", "set", "via", "not", "all", "any", "but", "too", "out", "our", "their",
        "them", "they", "its", "then", "than", "also", "have", "has", "had", "was",
        "were", "who", "why",
    ]
    .into_iter()
    .collect()
});

/// Represents a Qdrant sparse vector as parallel `indices` and `values` arrays.
///
/// - `indices`: unique bucket indices in range `0..SPARSE_DIM`
/// - `values`: TF weight for each index (always positive; Qdrant applies IDF)
#[derive(Debug, Clone, Default)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

impl SparseVector {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Serialize to the Qdrant wire format expected in `"vector"` and `"query"` fields.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "indices": self.indices,
            "values": self.values,
        })
    }
}

/// Map a single lowercase alphanumeric term to a Qdrant bucket index.
///
/// Uses FNV-1a with a fixed seed so the mapping is stable across runs.
/// Exposed as `pub` so tests can verify specific terms are excluded.
pub fn term_to_index(term: &str) -> u32 {
    // FNV-1a 32-bit — fixed offset basis + prime
    const FNV_OFFSET: u32 = 2_166_136_261;
    const FNV_PRIME: u32 = 16_777_619;
    let mut hash = FNV_OFFSET;
    for byte in term.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash % SPARSE_DIM
}

/// Compute a BM42-style sparse vector for `text`.
///
/// Returns a `SparseVector` with one entry per unique token bucket.
/// Short tokens (< 3 chars), stopwords, and non-alphanumeric tokens are excluded.
/// On hash collision two distinct terms map to the same bucket; their TF counts are summed.
pub fn compute_sparse_vector(text: &str) -> SparseVector {
    let mut bucket_tf: HashMap<u32, u32> = HashMap::new();
    for term in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let lower = term.to_ascii_lowercase();
        if lower.len() < 3 || STOP_WORDS.contains(lower.as_str()) {
            continue;
        }
        let idx = term_to_index(&lower);
        *bucket_tf.entry(idx).or_insert(0) += 1;
    }
    if bucket_tf.is_empty() {
        return SparseVector::default();
    }
    let mut indices = Vec::with_capacity(bucket_tf.len());
    let mut values = Vec::with_capacity(bucket_tf.len());
    for (idx, count) in bucket_tf {
        indices.push(idx);
        values.push(count as f32);  // raw TF; Qdrant applies IDF server-side
    }
    SparseVector { indices, values }
}

#[cfg(test)]
mod tests {
    use super::*;
    // (tests from Step 1.1 go here)
}
```

- [ ] **Step 1.4: Declare `sparse` module in `ops.rs`**

In `crates/vector/ops.rs` (or wherever the module root is — check with `grep -n "^mod " crates/vector/ops.rs`), add:

```rust
pub mod sparse;
```

> **Note:** The vector crate re-exports via `crates/vector/ops/commands.rs` and the crate root. Check where `pub use` statements live and add re-export for `sparse::compute_sparse_vector` and `sparse::SparseVector` if callers are outside this crate.

- [ ] **Step 1.5: Run tests to verify they pass**

```bash
cargo test sparse -- --nocapture 2>&1
```
Expected: all `sparse::tests` pass, 0 failures.

- [ ] **Step 1.6: Verify lint and format**

```bash
cargo clippy -p axon --lib 2>&1 | grep "sparse\|error" | head -20
cargo fmt --check 2>&1 | head -10
```

- [ ] **Step 1.7: Commit**

```bash
git add crates/vector/ops/sparse.rs crates/vector/ops.rs  # or lib.rs depending on module root
git commit -m "feat(vector): add BM42 sparse vector computation (FNV-1a hash, TF weights)"
```

---

### Task 2: `qdrant_store.rs` — VectorMode detection + new collection schema

**Files:**
- Modify: `crates/vector/ops/tei/qdrant_store.rs`

The goal: `ensure_collection` returns `VectorMode` describing whether the collection uses legacy unnamed dense or new named dense+sparse vectors. Callers use this to pick the right upsert format and search path.

- [ ] **Step 2.1: Write failing tests for VectorMode detection**

Add to the `#[cfg(test)]` block in `qdrant_store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;

    // ── VectorMode cache ───────────────────────────────────────────────────

    #[test]
    fn cached_vector_mode_returns_none_for_unknown_collection() {
        // Fresh process — nothing cached.
        // Use a unique collection name to avoid state from other tests.
        let result = cached_vector_mode("test_no_such_collection_xyz_999");
        assert!(result.is_none(), "unknown collection must return None");
    }

    #[test]
    fn cache_and_retrieve_named_mode() {
        cache_vector_mode("test_cache_named", VectorMode::Named);
        assert_eq!(
            cached_vector_mode("test_cache_named"),
            Some(VectorMode::Named)
        );
    }

    #[test]
    fn cache_and_retrieve_unnamed_mode() {
        cache_vector_mode("test_cache_unnamed", VectorMode::Unnamed);
        assert_eq!(
            cached_vector_mode("test_cache_unnamed"),
            Some(VectorMode::Unnamed)
        );
    }

    // ── ensure_collection (integration — requires live Qdrant) ────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore = "integration test — requires running Qdrant; run with cargo test -- --ignored"]
    async fn ensure_collection_new_collection_returns_named_mode() -> Result<(), Box<dyn std::error::Error>> {
        use crate::crates::jobs::common::resolve_test_qdrant_url;
        let Some(qdrant_url) = resolve_test_qdrant_url() else { return Ok(()); };
        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = qdrant_url.clone();
        cfg.collection = format!("test_{}", uuid::Uuid::new_v4().simple());

        let mode = ensure_collection(&cfg, 4).await?;

        // Cleanup
        let _ = reqwest::Client::new()
            .delete(format!("{}/collections/{}", qdrant_url.trim_end_matches('/'), cfg.collection))
            .send().await;

        assert_eq!(mode, VectorMode::Named, "new collection must be Named");
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore = "integration test — requires running Qdrant; run with cargo test -- --ignored"]
    async fn ensure_collection_existing_unnamed_returns_unnamed_mode() -> Result<(), Box<dyn std::error::Error>> {
        use crate::crates::jobs::common::resolve_test_qdrant_url;
        let Some(qdrant_url) = resolve_test_qdrant_url() else { return Ok(()); };
        let client = reqwest::Client::new();
        let base = qdrant_url.trim_end_matches('/').to_string();
        let collection = format!("test_{}", uuid::Uuid::new_v4().simple());

        // Create a legacy unnamed-vector collection manually.
        client
            .put(format!("{base}/collections/{collection}"))
            .json(&serde_json::json!({"vectors": {"size": 4, "distance": "Cosine"}}))
            .send().await?.error_for_status()?;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = qdrant_url;
        cfg.collection = collection.clone();

        let mode = ensure_collection(&cfg, 4).await?;

        // Cleanup
        let _ = client.delete(format!("{base}/collections/{collection}")).send().await;

        assert_eq!(mode, VectorMode::Unnamed, "existing unnamed collection must return Unnamed");
        Ok(())
    }
}
```

- [ ] **Step 2.2: Run tests to confirm they fail**

```bash
cargo test qdrant_store -- --nocapture 2>&1 | grep -E "FAILED|error" | head -10
```
Expected: compile error (VectorMode doesn't exist yet).

- [ ] **Step 2.3: Implement `VectorMode` and updated `qdrant_store.rs`**

Replace the contents of `crates/vector/ops/tei/qdrant_store.rs` with:

```rust
use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_base};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Mutex, OnceLock};

/// Describes how a Qdrant collection's vectors are configured.
///
/// - `Unnamed`: legacy single unnamed dense vector (`"vectors": {"size": N}`)
///   — hybrid search is disabled, `/points/search` is used.
/// - `Named`: named `dense` + named `bm42` sparse vectors
///   — hybrid search is enabled, `/points/query` with RRF is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorMode {
    Unnamed,
    Named,
}

static COLLECTION_MODES: OnceLock<Mutex<HashMap<String, VectorMode>>> = OnceLock::new();

/// Return the cached `VectorMode` for `name`, or `None` if not yet initialized.
pub(super) fn cached_vector_mode(name: &str) -> Option<VectorMode> {
    COLLECTION_MODES
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|map| map.get(name).copied())
}

/// Store `mode` in the collection-mode cache for `name`.
pub(super) fn cache_vector_mode(name: &str, mode: VectorMode) {
    let map = COLLECTION_MODES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut m) = map.lock() {
        m.insert(name.to_owned(), mode);
    }
}

/// Return the `VectorMode` for `cfg.collection`, initializing the Qdrant collection
/// if this is the first call for that collection in this process.
///
/// Subsequent calls return the cached mode without hitting Qdrant.
pub(super) async fn collection_init_or_cached(
    cfg: &Config,
    dim: usize,
) -> Result<VectorMode, Box<dyn Error>> {
    if let Some(mode) = cached_vector_mode(&cfg.collection) {
        return Ok(mode);
    }
    let mode = ensure_collection(cfg, dim).await?;
    cache_vector_mode(&cfg.collection, mode);
    Ok(mode)
}

/// Return the `VectorMode` for `cfg.collection` by inspecting the live Qdrant schema.
///
/// Used by search-only paths (query/ask) where `collection_init_or_cached` may not
/// have been called yet. Checks cache first; falls back to a GET if not cached.
pub(crate) async fn get_or_fetch_vector_mode(cfg: &Config) -> Result<VectorMode, Box<dyn Error>> {
    if let Some(mode) = cached_vector_mode(&cfg.collection) {
        return Ok(mode);
    }
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        // Collection may not exist yet; default to Unnamed (dense-only fallback).
        return Ok(VectorMode::Unnamed);
    }
    let body: serde_json::Value = resp.json().await?;
    let mode = detect_vector_mode(&body);
    cache_vector_mode(&cfg.collection, mode);
    Ok(mode)
}

/// Infer `VectorMode` from a Qdrant collection GET response body.
fn detect_vector_mode(body: &serde_json::Value) -> VectorMode {
    // Named dense shows up at /result/config/params/vectors/dense
    // Unnamed dense shows up at /result/config/params/vectors/size (flat object)
    if body
        .pointer("/result/config/params/vectors/dense")
        .is_some()
    {
        VectorMode::Named
    } else {
        VectorMode::Unnamed
    }
}

async fn ensure_payload_indexes(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let index_url = format!(
        "{}/collections/{}/index?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
    for field in &["url", "domain"] {
        client
            .put(&index_url)
            .json(&serde_json::json!({
                "field_name": field,
                "field_schema": "keyword"
            }))
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

/// Ensure the collection exists and is configured with the right vector schema.
///
/// Returns the `VectorMode` that describes the collection after this call.
///
/// | Prior state | Action | Returns |
/// |-------------|--------|---------|
/// | Does not exist | Create with named `dense` + `bm42` sparse | `Named` |
/// | Exists, named `dense` | Ensure sparse; PATCH to add `bm42` if missing | `Named` |
/// | Exists, unnamed dense | Log warning; leave unchanged | `Unnamed` |
pub(super) async fn ensure_collection(cfg: &Config, dim: usize) -> Result<VectorMode, Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);

    let get_resp = client.get(&url).send().await?;
    if get_resp.status().is_success() {
        let body: serde_json::Value = get_resp.json().await?;
        let mode = detect_vector_mode(&body);
        match mode {
            VectorMode::Named => {
                // Named dense exists — ensure sparse is configured.
                let has_sparse = body
                    .pointer("/result/config/params/sparse_vectors/bm42")
                    .is_some();
                if !has_sparse {
                    patch_add_sparse(cfg).await?;
                }
            }
            VectorMode::Unnamed => {
                log_warn(&format!(
                    "collection '{}' uses legacy unnamed dense vectors; \
                     hybrid search is disabled for this collection. \
                     To enable, set AXON_COLLECTION to a new name and re-index.",
                    cfg.collection
                ));
            }
        }
        log_debug(&format!(
            "qdrant collection_exists collection={} mode={:?}",
            cfg.collection, mode
        ));
        ensure_payload_indexes(cfg).await?;
        return Ok(mode);
    }

    // Collection does not exist — create with named dense + BM42 sparse.
    let create = serde_json::json!({
        "vectors": {
            "dense": {"size": dim, "distance": "Cosine"}
        },
        "sparse_vectors": {
            "bm42": {"modifier": "idf"}
        }
    });
    let resp = client.put(&url).json(&create).send().await?;
    if resp.status() != StatusCode::CONFLICT {
        resp.error_for_status()?;
    }
    log_info(&format!(
        "qdrant collection_created collection={} mode=Named",
        cfg.collection
    ));
    ensure_payload_indexes(cfg).await?;
    Ok(VectorMode::Named)
}

/// PATCH an existing Named collection to add the `bm42` sparse vector config.
/// This is idempotent — Qdrant accepts the patch even if `bm42` is already present.
async fn patch_add_sparse(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    client
        .patch(&url)
        .json(&serde_json::json!({
            "sparse_vectors": {
                "bm42": {"modifier": "idf"}
            }
        }))
        .send()
        .await?
        .error_for_status()?;
    log_info(&format!(
        "qdrant collection_patched_sparse collection={}",
        cfg.collection
    ));
    Ok(())
}

pub(super) async fn qdrant_upsert(
    cfg: &Config,
    points: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    if points.is_empty() {
        return Ok(());
    }
    let client = http_client()?;
    let upsert_batch_size = env_usize_clamped("AXON_QDRANT_UPSERT_BATCH_SIZE", 256, 1, 4096);
    let url = format!(
        "{}/collections/{}/points?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
    log_debug(&format!(
        "qdrant upsert_start point_count={} collection={}",
        points.len(),
        cfg.collection
    ));
    for batch in points.chunks(upsert_batch_size) {
        client
            .put(&url)
            .json(&serde_json::json!({"points": batch}))
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;

    // ... (tests from Step 2.1 go here)
}
```

> **Wire format note:** The Qdrant PATCH endpoint for collections is `PATCH /collections/{name}` with body `{"sparse_vectors": {"bm42": {"modifier": "idf"}}}`. Verify against your live Qdrant v1.13.1 during integration testing with `cargo test -- --ignored`.

- [ ] **Step 2.4: Run unit tests**

```bash
cargo test qdrant_store::tests -- --nocapture 2>&1
```
Expected: `cached_vector_mode_returns_none`, `cache_and_retrieve_named_mode`, `cache_and_retrieve_unnamed_mode` all pass.

- [ ] **Step 2.5: Run ignored integration tests (requires live Qdrant)**

```bash
cargo test qdrant_store -- --ignored --nocapture 2>&1
```
Expected: both integration tests pass.

- [ ] **Step 2.6: Verify no regressions in qdrant tests**

```bash
cargo test qdrant -- --nocapture 2>&1 | tail -5
```
Expected: `test result: ok.` with 0 failures.

- [ ] **Step 2.7: Lint and format**

```bash
cargo clippy -p axon --lib 2>&1 | grep error | head -10
cargo fmt --check 2>&1 | head -5
```

- [ ] **Step 2.8: Commit**

```bash
git add crates/vector/ops/tei/qdrant_store.rs
git commit -m "feat(vector): add VectorMode detection — new collections use named dense+BM42 sparse"
```

---

## Chunk 2: Upsert Path — Include Sparse Vectors

### Task 3: `tei.rs` — embed paths include sparse vectors for Named collections

**Files:**
- Modify: `crates/vector/ops/tei.rs`

Both `embed_chunks_impl` (used by `embed_text_with_metadata`, `embed_code_with_metadata`) and `build_batch_points` (used by `embed_documents_batch`) need to emit the right point format based on `VectorMode`.

- [ ] **Step 3.1: Write failing tests for Named vs Unnamed point format**

Add to `crates/vector/ops/tei/tests.rs` (file already exists; add to it):

```rust
// In tei/tests.rs — add these test functions

#[test]
fn named_mode_point_has_dense_and_bm42_in_vector_field() {
    use crate::crates::vector::ops::tei::qdrant_store::VectorMode;
    use crate::crates::vector::ops::tei::build_point_for_test;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let chunk = "embed axon qdrant vector search collection";
    let point = build_point_for_test(dense, chunk, "https://ex.com/a", 0, VectorMode::Named);

    // Named mode: vector must be an object with "dense" and "bm42" keys.
    let vector_obj = point["vector"].as_object().expect("vector must be an object for Named mode");
    assert!(vector_obj.contains_key("dense"), "Named point must have 'dense' key");
    assert!(vector_obj.contains_key("bm42"), "Named point must have 'bm42' key");

    // bm42 must have indices and values.
    let bm42 = &vector_obj["bm42"];
    assert!(bm42["indices"].is_array(), "bm42.indices must be an array");
    assert!(bm42["values"].is_array(), "bm42.values must be an array");
    assert_eq!(
        bm42["indices"].as_array().unwrap().len(),
        bm42["values"].as_array().unwrap().len(),
        "bm42 indices and values must have the same length"
    );
}

#[test]
fn unnamed_mode_point_has_flat_array_vector() {
    use crate::crates::vector::ops::tei::qdrant_store::VectorMode;
    use crate::crates::vector::ops::tei::build_point_for_test;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let chunk = "embed qdrant vector";
    let point = build_point_for_test(dense, chunk, "https://ex.com/b", 0, VectorMode::Unnamed);

    // Unnamed mode: vector must be a flat array, not an object.
    assert!(
        point["vector"].is_array(),
        "Unnamed point must have a flat array vector"
    );
}

#[test]
fn named_mode_bm42_values_non_empty_for_content_chunk() {
    use crate::crates::vector::ops::tei::qdrant_store::VectorMode;
    use crate::crates::vector::ops::tei::build_point_for_test;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let chunk = "axon hybrid search qdrant embedding pipeline";
    let point = build_point_for_test(dense, chunk, "https://ex.com/c", 0, VectorMode::Named);

    let bm42 = &point["vector"]["bm42"];
    let values = bm42["values"].as_array().unwrap();
    assert!(!values.is_empty(), "non-empty chunk must produce non-empty sparse vector");
}
```

> `build_point_for_test` is a test-only helper you'll expose with `#[cfg(test)] pub(crate)` from `tei.rs`.

- [ ] **Step 3.2: Run to confirm failure**

```bash
cargo test tei::tests -- --nocapture 2>&1 | grep -E "FAILED|error" | head -5
```
Expected: compile error (function doesn't exist yet).

- [ ] **Step 3.3: Add `build_point` helper to `tei.rs`**

Add a private `build_point` function that returns the right JSON structure for a single chunk, and a `#[cfg(test)] pub(crate)` wrapper for testing:

```rust
// In crates/vector/ops/tei.rs

use crate::crates::vector::ops::sparse;
use crate::crates::vector::ops::tei::qdrant_store::VectorMode;

fn build_point(
    point_id: uuid::Uuid,
    vecv: Vec<f32>,
    chunk: &str,
    payload: serde_json::Value,
    mode: VectorMode,
) -> serde_json::Value {
    match mode {
        VectorMode::Named => {
            let sparse = sparse::compute_sparse_vector(chunk);
            serde_json::json!({
                "id": point_id.to_string(),
                "vector": {
                    "dense": vecv,
                    "bm42": sparse.to_json(),
                },
                "payload": payload,
            })
        }
        VectorMode::Unnamed => serde_json::json!({
            "id": point_id.to_string(),
            "vector": vecv,
            "payload": payload,
        }),
    }
}

/// Test-only helper: build one point JSON for the given mode without touching Qdrant.
#[cfg(test)]
pub(crate) fn build_point_for_test(
    dense: Vec<f32>,
    chunk: &str,
    url: &str,
    chunk_index: usize,
    mode: VectorMode,
) -> serde_json::Value {
    let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("{url}:{chunk_index}").as_bytes());
    let payload = serde_json::json!({
        "url": url,
        "chunk_index": chunk_index,
        "chunk_text": chunk,
    });
    build_point(id, dense, chunk, payload, mode)
}
```

- [ ] **Step 3.4: Update `embed_chunks_impl` to use `collection_init_or_cached` + `build_point`**

In `embed_chunks_impl`, replace:
```rust
if qdrant_store::collection_needs_init(&cfg.collection) {
    qdrant_store::ensure_collection(cfg, dim).await?;
}
// ...
points.push(serde_json::json!({
    "id": point_id.to_string(),
    "vector": vecv,
    "payload": payload,
}));
```

With:
```rust
let mode = qdrant_store::collection_init_or_cached(cfg, dim).await?;
// ...
points.push(build_point(point_id, vecv, &chunk, payload, mode));
```

- [ ] **Step 3.5: Update `build_batch_points` to accept `VectorMode`**

Change the function signature to accept `mode: VectorMode` and use `build_point` for each chunk:

```rust
fn build_batch_points(
    prepared: &[PreparedBatchDocument],
    vectors: Vec<Vec<f32>>,
    mode: VectorMode,   // ← add this parameter
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    // ...
    // Replace the json! literal for each point with:
    points.push(build_point(point_id, vector, chunk, payload, mode));
    // ...
}
```

Update the call site in `embed_documents_batch`:
```rust
let mode = qdrant_store::collection_init_or_cached(cfg, dim).await?;
// Remove the old `collection_needs_init` + `ensure_collection` block.
let points = build_batch_points(&prepared, vectors, mode)?;
```

- [ ] **Step 3.6: Run all tei tests**

```bash
cargo test tei -- --nocapture 2>&1 | tail -10
```
Expected: all tests pass including the new Named/Unnamed point format tests.

- [ ] **Step 3.7: Lint and format**

```bash
cargo clippy -p axon --lib 2>&1 | grep "error\|warning.*tei" | head -10
cargo fmt --check 2>&1 | head -5
```

- [ ] **Step 3.8: Commit**

```bash
git add crates/vector/ops/tei.rs crates/vector/ops/tei/tests.rs
git commit -m "feat(vector): include BM42 sparse vectors in document upsert (Named collections)"
```

---

### Task 4: `tei/pipeline.rs` — embed pipeline includes sparse vectors

**Files:**
- Modify: `crates/vector/ops/tei/pipeline.rs`

`embed_prepared_doc` in `pipeline.rs` builds its own points independently of `embed_chunks_impl`. It needs the same `build_point` + `collection_init_or_cached` treatment.

- [ ] **Step 4.1: Write failing test**

Add to an appropriate test location (e.g., an inline test module at the bottom of `pipeline.rs` or a new `pipeline_tests.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::vector::ops::tei::qdrant_store::VectorMode;

    #[test]
    fn embed_prepared_doc_builds_named_points_for_named_mode() {
        use crate::crates::vector::ops::tei::build_point_for_test;
        // Verify that build_point is called with Named mode when VectorMode::Named.
        // We can test this through build_point_for_test since embed_prepared_doc
        // delegates to the same helper.
        let point = build_point_for_test(
            vec![0.1f32, 0.2, 0.3],
            "pipeline test chunk with content",
            "https://pipeline.example/doc",
            0,
            VectorMode::Named,
        );
        assert!(
            point["vector"].is_object(),
            "Named pipeline point must have object vector"
        );
        assert!(point["vector"]["dense"].is_array());
        assert!(point["vector"]["bm42"]["indices"].is_array());
    }
}
```

- [ ] **Step 4.2: Update `embed_prepared_doc` in `pipeline.rs`**

Replace the `collection_needs_init` / `ensure_collection` call and point-building loop:

```rust
// Old:
let dim = vectors[0].len();
let timestamp = Utc::now().to_rfc3339();
let mut points = Vec::with_capacity(vectors.len());
for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors.into_iter()).enumerate() {
    let point_id = Uuid::new_v5(...);
    points.push(serde_json::json!({
        "id": point_id.to_string(),
        "vector": vecv,
        "payload": { ... }
    }));
}
```

```rust
// New:
use crate::crates::vector::ops::tei::{build_point, qdrant_store::VectorMode};

let dim = vectors[0].len();
let mode = qdrant_store::collection_init_or_cached(cfg, dim).await?;
let timestamp = Utc::now().to_rfc3339();
let mut points = Vec::with_capacity(vectors.len());
for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors.into_iter()).enumerate() {
    let point_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{}:{}", doc.url, idx).as_bytes(),
    );
    let payload = serde_json::json!({
        "url": doc.url,
        "domain": doc.domain,
        "source_command": "embed",
        "content_type": "markdown",
        "chunk_index": idx,
        "chunk_text": chunk,
        "scraped_at": timestamp,
    });
    points.push(build_point(point_id, vecv, &chunk, payload, mode));
}
```

Also remove the old `collection_needs_init` check in `run_embed_pipeline` that surrounds `ensure_collection`:

```rust
// Old (in run_embed_pipeline):
match collection_dim {
    None => {
        if qdrant_store::collection_needs_init(&cfg.collection) {
            qdrant_store::ensure_collection(cfg, dim).await?;
        }
        collection_dim = Some(dim);
    }
    // ...
}

// New: collection_init_or_cached in embed_prepared_doc handles this.
// Keep the dimension mismatch check:
match collection_dim {
    None => { collection_dim = Some(dim); }
    Some(existing) if existing != dim => {
        return Err(format!("TEI dimension mismatch: expected {}, got {}", existing, dim).into());
    }
    _ => {}
}
```

- [ ] **Step 4.3: Run all tests**

```bash
cargo test -- --nocapture 2>&1 | tail -10
```
Expected: 0 failures.

- [ ] **Step 4.4: Commit**

```bash
git add crates/vector/ops/tei/pipeline.rs
git commit -m "feat(vector): update embed pipeline to include sparse vectors for Named collections"
```

---

## Chunk 3: Search Path + Config

### Task 5: `qdrant/hybrid.rs` — `/query` endpoint with RRF fusion

**Files:**
- Create: `crates/vector/ops/qdrant/hybrid.rs`
- Modify: `crates/vector/ops/qdrant.rs` (add `mod hybrid; pub use hybrid::qdrant_hybrid_search;`)

- [ ] **Step 5.1: Write failing tests for `qdrant_hybrid_search`**

```rust
// In crates/vector/ops/qdrant/hybrid.rs — at the bottom under #[cfg(test)]

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    fn make_search_response(hits: Vec<(&str, f64)>) -> serde_json::Value {
        let result: Vec<serde_json::Value> = hits
            .iter()
            .map(|(url, score)| {
                serde_json::json!({
                    "id": "test-id",
                    "score": score,
                    "payload": {"url": url, "chunk_text": "test chunk text"}
                })
            })
            .collect();
        serde_json::json!({"result": result})
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_sends_prefetch_rrf_query() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path_contains("/points/query")
                    .json_body_partial(r#"{"query":{"fusion":"rrf"}}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = crate::crates::vector::ops::sparse::compute_sparse_vector("hybrid search test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5).await;

        mock.assert_async().await;
        assert!(result.is_ok(), "hybrid search must succeed: {:?}", result.err());
        let hits = result.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].payload.url, "https://example.com/a");
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_request_includes_both_prefetch_arms() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path_contains("/points/query")
                    // Both prefetch arms must be present
                    .json_body_partial(r#"{"prefetch":[{"using":"dense"},{"using":"bm42"}]}"#);
                then.status(200)
                    .json_body(make_search_response(vec![]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2];
        let sparse = crate::crates::vector::ops::sparse::SparseVector {
            indices: vec![100, 200],
            values: vec![1.0, 2.0],
        };
        let _ = qdrant_hybrid_search(&cfg, &dense, &sparse, 10).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_propagates_qdrant_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path_contains("/points/query");
                then.status(500).body("internal server error");
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_hybrid_search(
            &cfg,
            &[0.1f32],
            &crate::crates::vector::ops::sparse::SparseVector::default(),
            5,
        )
        .await;
        assert!(result.is_err(), "HTTP 500 must propagate as Err");
    }
}
```

- [ ] **Step 5.2: Run to confirm failure**

```bash
cargo test hybrid -- --nocapture 2>&1 | grep -E "FAILED|error" | head -5
```
Expected: compile error (module doesn't exist).

- [ ] **Step 5.3: Implement `qdrant/hybrid.rs`**

```rust
//! Hybrid search via Qdrant `/query` endpoint with RRF fusion.
//!
//! Sends two prefetch arms (dense + BM42 sparse) and fuses with Reciprocal Rank Fusion.
//! Only called for collections in `VectorMode::Named` (named dense + sparse vectors).

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_warn};
use crate::crates::vector::ops::qdrant::types::{QdrantSearchHit, QdrantSearchResponse};
use crate::crates::vector::ops::qdrant::utils::qdrant_base;
use crate::crates::vector::ops::sparse::SparseVector;
use anyhow::{Result, anyhow};
use std::time::Instant;

/// Perform hybrid search using dense + BM42 sparse prefetch with RRF fusion.
///
/// `limit` is the final number of results after fusion. Each prefetch arm fetches
/// `cfg.hybrid_search_candidates` candidates. Requires a Named-mode collection.
pub(crate) async fn qdrant_hybrid_search(
    cfg: &Config,
    dense_vector: &[f32],
    sparse_vector: &SparseVector,
    limit: usize,
) -> Result<Vec<QdrantSearchHit>> {
    let client = http_client().map_err(|e| anyhow!(e.to_string()))?;
    let url = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );

    let prefetch_limit = cfg.hybrid_search_candidates.max(limit);
    let body = serde_json::json!({
        "prefetch": [
            {
                "query": dense_vector,
                "using": "dense",
                "limit": prefetch_limit
            },
            {
                "query": sparse_vector.to_json(),
                "using": "bm42",
                "limit": prefetch_limit
            }
        ],
        "query": {"fusion": "rrf"},
        "limit": limit,
        "with_payload": true,
        "with_vector": false
    });

    let search_start = Instant::now();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_hybrid_search transport_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_hybrid_search status_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?;

    let parsed: QdrantSearchResponse = resp.json().await.map_err(|e| anyhow!(e.to_string()))?;
    log_debug(&format!(
        "qdrant hybrid_search hits={} collection={}",
        parsed.result.len(),
        cfg.collection
    ));
    Ok(parsed.result)
}

#[cfg(test)]
mod tests {
    // tests from Step 5.1 go here
}
```

- [ ] **Step 5.4: Register the module in `qdrant.rs`**

Add to `crates/vector/ops/qdrant.rs`:

```rust
mod hybrid;
pub(crate) use hybrid::qdrant_hybrid_search;
```

- [ ] **Step 5.5: Run tests**

```bash
cargo test hybrid -- --nocapture 2>&1 | tail -10
```
Expected: all 3 httpmock tests pass.

- [ ] **Step 5.6: Commit**

```bash
git add crates/vector/ops/qdrant/hybrid.rs crates/vector/ops/qdrant.rs
git commit -m "feat(vector): add qdrant_hybrid_search using /query RRF fusion endpoint"
```

---

### Task 6: Config — `hybrid_search_enabled` + `hybrid_search_candidates`

**Files:**
- Modify: `crates/core/config/types/config.rs`
- Modify: `crates/core/config/types/config_impls.rs`
- Modify: `crates/core/config/parse/build_config.rs`

- [ ] **Step 6.1: Write failing test for new config defaults**

In `crates/core/config/types/types.rs` (wherever config default tests live — use grep to find the `config_default_ask_settings` test):

```rust
#[test]
fn config_default_hybrid_search_settings() {
    let cfg = Config::default();
    assert!(cfg.hybrid_search_enabled, "hybrid search must default to enabled");
    assert_eq!(cfg.hybrid_search_candidates, 100, "hybrid candidates default must be 100");
}
```

- [ ] **Step 6.2: Add fields to `config.rs`**

Find the `ask_candidate_limit` field in `crates/core/config/types/config.rs` and add nearby (keeping the ask-config group together):

```rust
    /// Enable hybrid search (dense + BM42 sparse + RRF) for Named-mode collections.
    /// Env: `AXON_HYBRID_SEARCH` (true/false/1/0). Default: true.
    pub hybrid_search_enabled: bool,

    /// Candidates fetched per prefetch arm (dense + sparse) before RRF fusion.
    /// Env: `AXON_HYBRID_CANDIDATES` (clamped 10–500). Default: 100.
    pub hybrid_search_candidates: usize,
```

- [ ] **Step 6.3: Add defaults to `config_impls.rs`**

In the `Config { .. }` struct literal in `config_impls.rs`:

```rust
hybrid_search_enabled: true,
hybrid_search_candidates: 100,
```

- [ ] **Step 6.4: Parse from env in `build_config.rs`**

```rust
hybrid_search_enabled: env_bool("AXON_HYBRID_SEARCH", true),
hybrid_search_candidates: performance::env_usize_clamped("AXON_HYBRID_CANDIDATES", 100, 10, 500),
```

Where `env_bool` is the local function already used in `pipeline.rs`:
```rust
fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}
```

> If `env_bool` is not already in `build_config.rs`, add it. It already exists in `pipeline.rs` — consider whether to share it from a common location or duplicate. For now, duplicate in `build_config.rs` to avoid cross-module coupling.

- [ ] **Step 6.5: Update any Config struct literals in test helpers**

Per the codebase pattern: search for inline `Config { .. }` literals in test helpers:

```bash
grep -rn "make_test_config\|test_config\|Config {" crates/cli/commands/research.rs crates/cli/commands/search.rs crates/jobs/common/ 2>/dev/null | grep -v "\.json\|\/\/" | head -20
```

Add `hybrid_search_enabled: true, hybrid_search_candidates: 100,` to any literal that doesn't use `..Config::default()`.

- [ ] **Step 6.6: Run config tests**

```bash
cargo test config_default_hybrid -- --nocapture 2>&1
```
Expected: `config_default_hybrid_search_settings` passes.

- [ ] **Step 6.7: Verify full test suite**

```bash
cargo test --lib 2>&1 | tail -5
```
Expected: `test result: ok.` with 0 failures.

- [ ] **Step 6.8: Commit**

```bash
git add crates/core/config/types/config.rs crates/core/config/types/config_impls.rs crates/core/config/parse/build_config.rs
git commit -m "feat(config): add hybrid_search_enabled and hybrid_search_candidates env vars"
```

---

### Task 7: Wire Hybrid Search into `query.rs` and `retrieval.rs`

**Files:**
- Modify: `crates/vector/ops/commands/query.rs`
- Modify: `crates/vector/ops/commands/ask/context/retrieval.rs`

Both callers need to:
1. Compute a sparse vector for the query text
2. Check `cfg.hybrid_search_enabled` AND the collection's VectorMode
3. Use `qdrant_hybrid_search` when both are true; fall back to `qdrant_search`

- [ ] **Step 7.1: Write failing tests**

Add to `crates/vector/ops/commands/query.rs` (inline `#[cfg(test)]` block at bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    fn mock_tei_response(server: &MockServer, dim: usize) {
        server.mock(|when, then| {
            when.method(POST).path("/embed");
            then.status(200)
                .json_body(serde_json::json!([vec![0.1f32; dim]]));
        });
    }

    fn mock_qdrant_query_response(server: &MockServer) {
        server.mock(|when, then| {
            when.method(POST).path_contains("/points/query");
            then.status(200).json_body(serde_json::json!({
                "result": [{
                    "id": "test-id",
                    "score": 0.9,
                    "payload": {
                        "url": "https://docs.example.com/page",
                        "chunk_text": "axon hybrid search result content",
                        "chunk_index": 0
                    }
                }]
            }));
        });
    }

    fn mock_qdrant_search_response(server: &MockServer) {
        server.mock(|when, then| {
            when.method(POST).path_contains("/points/search");
            then.status(200).json_body(serde_json::json!({
                "result": [{
                    "id": "test-id",
                    "score": 0.85,
                    "payload": {
                        "url": "https://docs.example.com/page",
                        "chunk_text": "axon dense search result content",
                        "chunk_index": 0
                    }
                }]
            }));
        });
    }

    #[tokio::test]
    async fn query_results_uses_hybrid_search_when_named_collection() {
        let qdrant_server = MockServer::start_async().await;
        let tei_server = MockServer::start_async().await;
        mock_tei_response(&tei_server, 4);
        // Return Named mode for collection GET
        qdrant_server.mock(|when, then| {
            when.method(GET).path_contains("/collections/");
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "config": {
                        "params": {
                            "vectors": {"dense": {"size": 4, "distance": "Cosine"}}
                        }
                    }
                }
            }));
        });
        mock_qdrant_query_response(&qdrant_server);

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = qdrant_server.base_url();
        cfg.tei_url = tei_server.base_url();
        cfg.hybrid_search_enabled = true;

        let result = query_results(&cfg, "axon search query", 5, 0).await;
        assert!(result.is_ok(), "query_results must succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn query_results_falls_back_to_dense_when_hybrid_disabled() {
        let qdrant_server = MockServer::start_async().await;
        let tei_server = MockServer::start_async().await;
        mock_tei_response(&tei_server, 4);
        mock_qdrant_search_response(&qdrant_server);

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = qdrant_server.base_url();
        cfg.tei_url = tei_server.base_url();
        cfg.hybrid_search_enabled = false;  // explicitly disabled

        let result = query_results(&cfg, "dense only query", 5, 0).await;
        assert!(result.is_ok(), "dense fallback must succeed: {:?}", result.err());
    }
}
```

- [ ] **Step 7.2: Run tests to confirm failure**

```bash
cargo test query::tests -- --nocapture 2>&1 | grep -E "error|FAILED" | head -5
```
Expected: compile error (cfg field doesn't exist yet — or test body issues).

- [ ] **Step 7.3: Update `query_results` in `query.rs`**

Replace the `qdrant::qdrant_search` call with a hybrid-aware dispatch:

```rust
use crate::crates::vector::ops::sparse;
use crate::crates::vector::ops::tei::qdrant_store;

pub async fn query_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let mut query_vectors = tei::tei_embed(cfg, std::slice::from_ref(&query.to_string())).await?;
    if query_vectors.is_empty() {
        return Err("TEI returned no vector for query".into());
    }
    let vector = query_vectors.remove(0);

    let fetch_limit = ((limit + offset).max(1) * 8).max(limit + offset).min(500);
    let hits = if cfg.hybrid_search_enabled {
        let mode = qdrant_store::get_or_fetch_vector_mode(cfg)
            .await
            .unwrap_or(qdrant::VectorMode::Unnamed);  // NOTE: VectorMode needs re-export
        if mode == qdrant::VectorMode::Named {
            let sparse_vec = sparse::compute_sparse_vector(query);
            qdrant::qdrant_hybrid_search(cfg, &vector, &sparse_vec, fetch_limit)
                .await
                .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?
        } else {
            qdrant::qdrant_search(cfg, &vector, fetch_limit).await?
        }
    } else {
        qdrant::qdrant_search(cfg, &vector, fetch_limit).await?
    };

    // ... rest of function unchanged (ranking, diversity selection, output) ...
}
```

> **Re-export note:** `VectorMode` needs to be accessible from the `qdrant` module. Add to `crates/vector/ops/qdrant.rs`:
> ```rust
> pub(crate) use tei::qdrant_store::VectorMode;
> // Or re-export directly:
> pub(crate) use super::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
> ```
> Alternatively, call `qdrant_store::get_or_fetch_vector_mode` directly since it's already `pub(crate)`.

- [ ] **Step 7.4: Update `retrieve_ask_candidates` in `retrieval.rs`**

Same hybrid dispatch pattern, but using `cfg.ask_candidate_limit` as the limit for the hybrid search:

```rust
use crate::crates::vector::ops::sparse;
use crate::crates::vector::ops::tei::qdrant_store;

let hits = if cfg.hybrid_search_enabled {
    let mode = qdrant_store::get_or_fetch_vector_mode(cfg)
        .await
        .unwrap_or(qdrant::VectorMode::Unnamed);
    if mode == qdrant::VectorMode::Named {
        let sparse_vec = sparse::compute_sparse_vector(query);
        qdrant::qdrant_hybrid_search(cfg, &vecq, &sparse_vec, cfg.ask_candidate_limit)
            .await
            .map_err(|e| anyhow!(e.to_string()))?
    } else {
        qdrant::qdrant_search(cfg, &vecq, cfg.ask_candidate_limit)
            .await
            .map_err(|e| anyhow!(e.to_string()))?
    }
} else {
    qdrant::qdrant_search(cfg, &vecq, cfg.ask_candidate_limit)
        .await
        .map_err(|e| anyhow!(e.to_string()))?
};
```

- [ ] **Step 7.5: Run all tests**

```bash
cargo test --lib 2>&1 | tail -5
```
Expected: `test result: ok.` with 0 failures.

- [ ] **Step 7.6: Verify with `just verify`**

```bash
just verify 2>&1 | tail -10
```
Expected: all gates pass (fmt-check, clippy, check, test).

- [ ] **Step 7.7: Final commit**

```bash
git add crates/vector/ops/commands/query.rs crates/vector/ops/commands/ask/context/retrieval.rs
git commit -m "feat(vector): wire hybrid search into query and ask — uses /query RRF for Named collections"
```

---

## Summary of Changes

After all tasks complete:

| What changed | Effect |
|---|---|
| `sparse.rs` | BM42 TF sparse vectors for any text — no new dependencies |
| `qdrant_store.rs` | New collections created with named `dense` + `bm42` sparse; existing unnamed collections detected and preserved |
| `tei.rs` + `pipeline.rs` | Documents indexed in Named collections get dense+sparse vectors; Unnamed collections unchanged |
| `qdrant/hybrid.rs` | `/query` RRF endpoint used for Named collections |
| `query.rs` + `retrieval.rs` | Hybrid search for Named, dense fallback for Unnamed |
| Config | `AXON_HYBRID_SEARCH=true` (default), `AXON_HYBRID_CANDIDATES=100` |

**To test end-to-end:**
```bash
# 1. In .env: set a new collection name
echo "AXON_COLLECTION=cortex_hybrid" >> .env

# 2. Embed a few pages into the new Named collection
./scripts/axon embed https://docs.example.com --wait true

# 3. Query — should now use /query RRF endpoint
./scripts/axon query "AXON_COLLECTION env var" --collection cortex_hybrid

# 4. Ask — same
./scripts/axon ask "how do I configure the qdrant collection name?" --collection cortex_hybrid
```

**Rollback:** Set `AXON_HYBRID_SEARCH=false` in `.env`. All search paths revert to dense-only `/points/search`. Existing Named collections remain usable — hybrid just won't be activated.
