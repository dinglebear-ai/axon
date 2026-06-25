//! Near-duplicate collapse for reranked ask candidates.
//!
//! A self-hosted index routinely holds the *same* document more than once: the
//! canonical docs page (`code.claude.com/docs/en/plugins`) plus one or more
//! mirrors of it committed into GitHub repos. URL-based diversity selection
//! (`select_diverse_candidates`) only dedups by exact URL, so these mirror
//! copies each occupy a separate context slot — crowding out genuinely distinct
//! sources and shrinking the effective coverage handed to the LLM.
//!
//! This pass clusters candidates by normalized-content similarity and keeps a
//! single representative per cluster. The representative is chosen by a generic
//! canonical-preference ordering (authoritative domain > docs-style path > not a
//! VCS mirror blob > shallower path > higher rerank), so the canonical page wins
//! over its mirror **without hardcoding any specific host or repo**.
//!
//! Mirror handling here is strictly a *tiebreak inside a duplicate cluster*: a
//! candidate is only ever dropped when a near-identical sibling is present. A
//! mirror that is the sole copy of its content is never penalized — the user
//! ingests source repos deliberately and those answers must survive.

use crate::ops::ranking::AskCandidate;
use spider::url::Url;
use std::collections::HashSet;

/// Width of the MinHash signature (number of independent hash permutations).
///
/// Each candidate's shingle set is reduced **once** to a fixed-width signature of
/// `MINHASH_SIGNATURE_LEN` `u64` minima. Estimated Jaccard between two candidates
/// is then the fraction of signature slots that agree — an O(MINHASH_SIGNATURE_LEN)
/// comparison instead of the old O(|A| + |B|) full-set intersection. This turns the
/// per-pair cost from "touch ~300-element HashSets" into "compare 64 u64s", while
/// the overall dedup stays O(n²) in pair *count* (n = candidates) but with a tiny,
/// constant per-pair factor. The estimator is unbiased: E[agree/len] = true Jaccard,
/// with standard error ~1/sqrt(len) ≈ 0.125 at len=64 — fine for a near-duplicate
/// filter whose threshold (0.50) sits far from the boilerplate-overlap floor.
const MINHASH_SIGNATURE_LEN: usize = 64;

/// Minimum normalized shingle Jaccard for two candidates to be treated as
/// near-duplicate copies of the same source content. Tuned conservatively:
/// mirror copies of one page share most of their prose, while distinct pages
/// that merely reuse boilerplate (nav, footers) fall well below this.
const NEAR_DUP_JACCARD_THRESHOLD: f64 = 0.50;

/// Token n-gram width used to fingerprint content. Word-level 5-grams are
/// specific enough that shared boilerplate phrases rarely collide across
/// genuinely different pages.
const SHINGLE_SIZE: usize = 5;

/// Candidates shorter than this (in normalized tokens) are never used to *drop*
/// a sibling — short stubs (nav menus, title-only chunks) overlap spuriously.
const MIN_TOKENS_FOR_DEDUP: usize = 24;

/// One collapsed near-duplicate, for diagnostics/trace surfacing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ops::commands::ask::context) struct DedupCollapse {
    pub dropped_url: String,
    pub kept_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ops::commands::ask::context) struct DedupReport {
    pub dropped: usize,
    /// A bounded sample of collapsed pairs for human/JSON explain output.
    pub samples: Vec<DedupCollapse>,
}

impl DedupReport {
    /// Human-readable one-liner for the ask `warnings` channel (also surfaced in
    /// the `--explain` JSON), or `None` when nothing was collapsed.
    pub fn warning(&self) -> Option<String> {
        if self.dropped == 0 {
            return None;
        }
        let example = self
            .samples
            .first()
            .map(|c| format!(" (e.g. {} ≈ {})", c.dropped_url, c.kept_url))
            .unwrap_or_default();
        Some(format!(
            "Collapsed {} near-duplicate source chunk(s) before context selection{example}",
            self.dropped
        ))
    }
}

/// Collapse near-duplicate copies in a rerank-ordered candidate list.
///
/// `reranked` MUST be sorted by descending `rerank_score` (as produced by the
/// reranker). The returned vector preserves that ordering with non-canonical
/// duplicates removed; the report records how many were dropped and a sample.
pub(in crate::ops::commands::ask::context) fn dedup_near_duplicates(
    reranked: Vec<AskCandidate>,
    authoritative_domains: &[String],
) -> (Vec<AskCandidate>, DedupReport) {
    if reranked.len() < 2 {
        return (reranked, DedupReport::default());
    }

    let normalized_domains = normalize_domains(authoritative_domains);
    // Each fingerprint is a fixed-width MinHash signature (or `None` when the
    // chunk is too short to compare reliably — semantics unchanged). Computed
    // once per candidate; pairwise comparison is then estimated-Jaccard over the
    // signatures (see `MINHASH_SIGNATURE_LEN` and `minhash_jaccard`).
    let fingerprints: Vec<Option<MinHashSig>> = reranked
        .iter()
        .map(|c| fingerprint(&c.chunk_text))
        .collect();

    // Index of each kept representative.
    let mut representatives: Vec<usize> = Vec::new();
    let mut dropped: HashSet<usize> = HashSet::new();
    let mut report = DedupReport::default();

    for idx in 0..reranked.len() {
        let Some(idx_fp) = fingerprints[idx].as_ref() else {
            // Too short to fingerprint reliably — always keep, never collapse.
            representatives.push(idx);
            continue;
        };

        let mut matched_rep: Option<usize> = None;
        for (slot, &rep_idx) in representatives.iter().enumerate() {
            let Some(rep_fp) = fingerprints[rep_idx].as_ref() else {
                continue;
            };
            if minhash_jaccard(idx_fp, rep_fp) >= NEAR_DUP_JACCARD_THRESHOLD {
                matched_rep = Some(slot);
                break;
            }
        }

        match matched_rep {
            None => representatives.push(idx),
            Some(slot) => {
                let rep_idx = representatives[slot];
                // Decide which of the two should represent the cluster.
                if more_canonical(&reranked[idx], &reranked[rep_idx], &normalized_domains) {
                    // The new (lower-reranked) candidate is more canonical: it
                    // takes the slot, the old representative is dropped.
                    record_drop(&mut report, &reranked[rep_idx], &reranked[idx]);
                    dropped.insert(rep_idx);
                    representatives[slot] = idx;
                } else {
                    record_drop(&mut report, &reranked[idx], &reranked[rep_idx]);
                    dropped.insert(idx);
                }
            }
        }
    }

    if dropped.is_empty() {
        return (reranked, report);
    }

    let kept = reranked
        .into_iter()
        .enumerate()
        .filter_map(|(idx, c)| (!dropped.contains(&idx)).then_some(c))
        .collect();
    (kept, report)
}

fn record_drop(report: &mut DedupReport, dropped: &AskCandidate, kept: &AskCandidate) {
    report.dropped += 1;
    if report.samples.len() < 5 {
        report.samples.push(DedupCollapse {
            dropped_url: dropped.url.clone(),
            kept_url: kept.url.clone(),
        });
    }
}

/// True when `a` should be preferred over `b` as the canonical representative of
/// a near-duplicate cluster. All signals are generic — no host/repo hardcoding.
fn more_canonical(a: &AskCandidate, b: &AskCandidate, authoritative_domains: &[String]) -> bool {
    canonical_key(a, authoritative_domains) > canonical_key(b, authoritative_domains)
}

/// Comparable canonical-preference key (larger = more canonical):
/// 1. on a configured authoritative domain,
/// 2. docs-style path (`/docs/`, `/api/`, `/reference/`, `/guide`),
/// 3. NOT a VCS mirror blob (`/blob/`, `/tree/`, `/raw/`, `*usercontent*`),
/// 4. shallower path depth (negated so fewer segments sorts higher),
/// 5. higher rerank score.
fn canonical_key(
    c: &AskCandidate,
    authoritative_domains: &[String],
) -> (bool, bool, bool, i32, f64) {
    (
        host_matches_authoritative(&c.url, authoritative_domains),
        is_docs_path(&c.path),
        !is_mirror_shaped(&c.url, &c.path),
        -(path_depth(&c.path) as i32),
        c.rerank_score,
    )
}

fn is_docs_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/docs/") || p.contains("/api/") || p.contains("/reference/") || p.contains("/guide")
}

/// Generic markers that a URL is a copy of content hosted canonically elsewhere:
/// VCS web view paths (`/blob/`, `/tree/`, `/raw/`, GitLab's `/-/blob/`) and raw
/// user-content hosts. These are conventions across GitHub/GitLab/Gitea/Forgejo,
/// not specific to any one account.
///
/// Shared with `selection.rs`: a mirror-shaped URL must never win the canonical
/// full-document slot nor ride host-dominance, regardless of how many copies of
/// it the index happens to hold.
pub(in crate::ops::commands::ask::context) fn is_mirror_shaped(url: &str, path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let u = url.to_ascii_lowercase();
    p.contains("/blob/")
        || p.contains("/tree/")
        || p.contains("/raw/")
        || p.contains("/-/blob/")
        || u.contains("usercontent.")
        || u.contains("raw.githubusercontent")
}

fn path_depth(path: &str) -> usize {
    path.split('/').filter(|s| !s.is_empty()).count()
}

fn host_matches_authoritative(url: &str, normalized_domains: &[String]) -> bool {
    if normalized_domains.is_empty() {
        return false;
    }
    let Some(host) = host_of(url) else {
        return false;
    };
    normalized_domains
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

fn normalize_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .map(|d| d.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect()
}

/// Fixed-width MinHash signature for one candidate's shingle set.
type MinHashSig = [u64; MINHASH_SIGNATURE_LEN];

/// Build a MinHash fingerprint, or `None` if the text is too short to compare
/// reliably (in which case the candidate is never used to drop a sibling).
///
/// The signature is the elementwise minimum, over all shingle hashes, of
/// `MINHASH_SIGNATURE_LEN` independent hash permutations. Each permutation is a
/// cheap reversible scramble of the base FNV-1a shingle hash mixed with a
/// per-slot seed (XOR-shift / multiply), so each slot draws its minimum from a
/// different random ordering of the same shingle set. Two candidates sharing a
/// fraction `J` of their shingles agree on each signature slot with probability
/// `J` (the MinHash property), so the count of agreeing slots over the signature
/// length is an unbiased estimator of the true shingle Jaccard.
fn fingerprint(text: &str) -> Option<MinHashSig> {
    let tokens = normalize_tokens(text);
    if tokens.len() < MIN_TOKENS_FOR_DEDUP {
        return None;
    }

    // Collect distinct shingle hashes first; an empty set is "too short" per the
    // original semantics (e.g. fewer than SHINGLE_SIZE tokens after normalize).
    let mut shingles: HashSet<u64> = HashSet::new();
    for window in tokens.windows(SHINGLE_SIZE) {
        shingles.insert(hash_shingle(window));
    }
    if shingles.is_empty() {
        return None;
    }

    let mut sig = [u64::MAX; MINHASH_SIGNATURE_LEN];
    for &h in &shingles {
        for (slot, sig_min) in sig.iter_mut().enumerate() {
            let permuted = permute_hash(h, slot as u64);
            if permuted < *sig_min {
                *sig_min = permuted;
            }
        }
    }
    Some(sig)
}

/// One of `MINHASH_SIGNATURE_LEN` independent hash permutations of a shingle
/// hash. Mixes in a per-slot seed then runs a SplitMix64-style finalizer so each
/// slot induces a different (but deterministic) ordering over the shingle set.
fn permute_hash(h: u64, seed: u64) -> u64 {
    // Distinct, well-spread per-slot seed (odd multiplier keeps it a bijection).
    let mut x = h ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // SplitMix64 finalizer — strong avalanche, cheap (no division/branches).
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn normalize_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

/// FNV-1a over the joined n-gram. Cheap, allocation-light, and good enough for
/// set membership — collisions only risk an occasional false-positive collapse,
/// which the conservative threshold already guards against.
fn hash_shingle(window: &[String]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for token in window {
        for byte in token.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x00000100000001B3);
        }
        // Word boundary marker so "ab cd" and "abcd" don't collide.
        hash ^= 0x20;
        hash = hash.wrapping_mul(0x00000100000001B3);
    }
    hash
}

/// Estimated shingle Jaccard between two MinHash signatures.
///
/// The fraction of agreeing signature slots is an unbiased estimator of the true
/// Jaccard similarity of the underlying shingle sets (standard error ~1/sqrt(len)).
/// O(MINHASH_SIGNATURE_LEN) — a fixed 64-element scan — versus the old
/// O(|A| + |B|) full-set intersection over ~300-element HashSets. The comparison
/// target (`NEAR_DUP_JACCARD_THRESHOLD = 0.50`) is unchanged: this estimates the
/// same quantity the old exact `jaccard` computed, just with bounded per-pair cost.
fn minhash_jaccard(a: &MinHashSig, b: &MinHashSig) -> f64 {
    let agree = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    agree as f64 / MINHASH_SIGNATURE_LEN as f64
}

#[cfg(test)]
#[path = "dedup_tests.rs"]
mod tests;
