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

use crate::vector::ops::ranking::AskCandidate;
use spider::url::Url;
use std::collections::HashSet;

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
pub(in crate::vector::ops::commands::ask::context) struct DedupCollapse {
    pub dropped_url: String,
    pub kept_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::vector::ops::commands::ask::context) struct DedupReport {
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
pub(in crate::vector::ops::commands::ask::context) fn dedup_near_duplicates(
    reranked: Vec<AskCandidate>,
    authoritative_domains: &[String],
) -> (Vec<AskCandidate>, DedupReport) {
    if reranked.len() < 2 {
        return (reranked, DedupReport::default());
    }

    let normalized_domains = normalize_domains(authoritative_domains);
    let fingerprints: Vec<Option<HashSet<u64>>> = reranked
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
            if jaccard(idx_fp, rep_fp) >= NEAR_DUP_JACCARD_THRESHOLD {
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
pub(in crate::vector::ops::commands::ask::context) fn is_mirror_shaped(
    url: &str,
    path: &str,
) -> bool {
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

/// Build a shingle fingerprint, or `None` if the text is too short to compare
/// reliably (in which case the candidate is never used to drop a sibling).
fn fingerprint(text: &str) -> Option<HashSet<u64>> {
    let tokens = normalize_tokens(text);
    if tokens.len() < MIN_TOKENS_FOR_DEDUP {
        return None;
    }
    let mut shingles = HashSet::new();
    for window in tokens.windows(SHINGLE_SIZE) {
        shingles.insert(hash_shingle(window));
    }
    if shingles.is_empty() {
        return None;
    }
    Some(shingles)
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

fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.iter().filter(|s| b.contains(*s)).count();
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

#[cfg(test)]
#[path = "dedup_tests.rs"]
mod tests;
