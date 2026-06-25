use super::*;
use crate::ops::ranking::AskCandidate;
use std::collections::HashSet;

fn candidate(url: &str, path: &str, text: &str, rerank: f64) -> AskCandidate {
    AskCandidate {
        score: rerank,
        url: url.to_string(),
        path: path.to_string(),
        chunk_text: text.to_string(),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score: rerank,
    }
}

/// A long, distinctive body so fingerprints clear MIN_TOKENS_FOR_DEDUP and two
/// copies share nearly all shingles.
fn plugin_body() -> String {
    "This quickstart walks you through creating a plugin with a custom skill. \
     You will create a manifest, the configuration file that defines your plugin, \
     add a skill, and test it locally with the plugin dir flag before you decide \
     to share it with other people on your team or the wider community."
        .to_string()
}

#[test]
fn collapses_mirror_keeping_canonical_docs_page() {
    let body = plugin_body();
    // Mirror reranks slightly higher, canonical slightly lower — canonical must
    // still win on the canonical-preference tiebreak.
    let reranked = vec![
        candidate(
            "https://github.com/someone/repo/blob/main/docs/plugins.md",
            "/someone/repo/blob/main/docs/plugins.md",
            &body,
            0.90,
        ),
        candidate(
            "https://code.claude.com/docs/en/plugins",
            "/docs/en/plugins",
            &body,
            0.80,
        ),
    ];
    let (kept, report) = dedup_near_duplicates(reranked, &["code.claude.com".to_string()]);
    assert_eq!(kept.len(), 1, "one mirror should be collapsed");
    assert_eq!(kept[0].url, "https://code.claude.com/docs/en/plugins");
    assert_eq!(report.dropped, 1);
    assert!(report.warning().is_some());
}

#[test]
fn distinct_pages_are_not_collapsed() {
    let a = candidate(
        "https://code.claude.com/docs/en/plugins",
        "/docs/en/plugins",
        "Plugins bundle skills agents hooks and commands into one installable unit \
         that extends Claude Code with new capabilities across your projects.",
        0.9,
    );
    let b = candidate(
        "https://code.claude.com/docs/en/hooks",
        "/docs/en/hooks",
        "Hooks run shell commands at lifecycle events letting you validate tool use \
         block dangerous operations and automate repository chores deterministically.",
        0.8,
    );
    let (kept, report) = dedup_near_duplicates(vec![a, b], &[]);
    assert_eq!(kept.len(), 2, "different content must survive");
    assert_eq!(report.dropped, 0);
}

#[test]
fn sole_mirror_is_never_dropped() {
    // No canonical sibling present — a lone GitHub copy must survive untouched.
    let only = candidate(
        "https://github.com/someone/repo/blob/main/docs/plugins.md",
        "/someone/repo/blob/main/docs/plugins.md",
        &plugin_body(),
        0.7,
    );
    let (kept, report) = dedup_near_duplicates(vec![only], &["code.claude.com".to_string()]);
    assert_eq!(kept.len(), 1);
    assert_eq!(report.dropped, 0);
}

#[test]
fn short_chunks_are_not_used_to_collapse() {
    // Two short title-only stubs that happen to overlap must not collapse.
    let a = candidate("https://a.com/x", "/x", "Plugins overview page", 0.9);
    let b = candidate("https://b.com/y", "/y", "Plugins overview page", 0.8);
    let (kept, report) = dedup_near_duplicates(vec![a, b], &[]);
    assert_eq!(kept.len(), 2);
    assert_eq!(report.dropped, 0);
}

#[test]
fn three_mirrors_collapse_to_single_canonical() {
    let body = plugin_body();
    let reranked = vec![
        candidate(
            "https://github.com/u/rust-bin/blob/main/refs/create-plugins.md",
            "/u/rust-bin/blob/main/refs/create-plugins.md",
            &body,
            0.40,
        ),
        candidate(
            "https://code.claude.com/docs/en/plugins",
            "/docs/en/plugins",
            &body,
            0.30,
        ),
        candidate(
            "https://github.com/u/agentcast/blob/main/refs/0093-plugins.md",
            "/u/agentcast/blob/main/refs/0093-plugins.md",
            &body,
            0.20,
        ),
    ];
    let (kept, report) = dedup_near_duplicates(reranked, &["code.claude.com".to_string()]);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].url, "https://code.claude.com/docs/en/plugins");
    assert_eq!(report.dropped, 2);
}

// -- MinHash estimator fidelity + scale (TEST-M1) --

/// Build a body of `n` distinct word tokens with a deterministic per-seed prefix
/// so different seeds share no incidental shingles.
fn token_body(seed: usize, n: usize) -> String {
    (0..n)
        .map(|i| format!("s{seed}word{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Two bodies sharing ~70% of their token sequence (a long common run) must
/// estimate above the 0.50 threshold and collapse. Guards against signature-length
/// shrink or a broken permutation bijection — those would deflate the estimate.
#[test]
fn high_overlap_pair_collapses() {
    // 50 shared tokens give 46 shared 5-gram shingles. Each doc adds ~20 unique
    // tokens (a divergent tail), so shared/union of shingles is well above 0.50.
    let shared: Vec<String> = (0..50).map(|i| format!("shared{i}")).collect();
    let mut a = shared.clone();
    a.extend((0..20).map(|i| format!("atail{i}")));
    let mut b = shared.clone();
    b.extend((0..20).map(|i| format!("btail{i}")));
    let body_a = a.join(" ");
    let body_b = b.join(" ");

    let reranked = vec![
        candidate("https://x.com/a", "/a", &body_a, 0.9),
        candidate("https://x.com/b", "/b", &body_b, 0.8),
    ];
    let (kept, report) = dedup_near_duplicates(reranked, &[]);
    assert_eq!(
        kept.len(),
        1,
        "~70%-overlap bodies must collapse (estimate >= 0.50)"
    );
    assert_eq!(report.dropped, 1);
}

/// Two bodies sharing only ~30% of their shingles must estimate below the 0.50
/// threshold and survive as distinct sources.
#[test]
fn low_overlap_pair_survives() {
    // ~20 shared tokens, ~50 unique tokens each → shared shingles are a small
    // fraction of the union, well below 0.50.
    let shared: Vec<String> = (0..20).map(|i| format!("common{i}")).collect();
    let mut a = shared.clone();
    a.extend((0..50).map(|i| format!("auniq{i}")));
    let mut b = shared.clone();
    b.extend((0..50).map(|i| format!("buniq{i}")));
    let body_a = a.join(" ");
    let body_b = b.join(" ");

    let reranked = vec![
        candidate("https://x.com/a", "/a", &body_a, 0.9),
        candidate("https://x.com/b", "/b", &body_b, 0.8),
    ];
    let (kept, report) = dedup_near_duplicates(reranked, &[]);
    assert_eq!(
        kept.len(),
        2,
        "~30%-overlap bodies must NOT collapse (estimate < 0.50)"
    );
    assert_eq!(report.dropped, 0);
}

/// 10 distinct bodies, each with 5 near-identical copies = 50 candidates. The pass
/// must collapse each cluster to exactly its first-seen representative: 10 kept,
/// 40 dropped. Scale guard against the O(n^2) pairwise estimator degrading.
#[test]
fn fifty_candidates_collapse_to_ten_distinct() {
    let mut reranked = Vec::new();
    for body_seed in 0..10usize {
        // 60 distinct tokens per body — well above MIN_TOKENS_FOR_DEDUP and long
        // enough that copies share essentially all shingles.
        let body = token_body(body_seed, 60);
        for copy in 0..5usize {
            // Distinct URLs so URL-dedup never fires — only content dedup can.
            // Descending rerank so ordering is well-defined.
            let rerank = 1.0 - (body_seed * 5 + copy) as f64 * 0.001;
            reranked.push(candidate(
                &format!("https://host{body_seed}.example/copy{copy}"),
                &format!("/host{body_seed}/copy{copy}"),
                &body,
                rerank,
            ));
        }
    }
    assert_eq!(reranked.len(), 50);

    let (kept, report) = dedup_near_duplicates(reranked, &[]);
    assert_eq!(
        kept.len(),
        10,
        "10 distinct bodies x 5 copies must collapse to 10 representatives"
    );
    assert_eq!(
        report.dropped, 40,
        "the 40 duplicate copies must be dropped"
    );
}

#[test]
fn shallower_path_wins_when_no_authority_or_docs_signal() {
    let body = plugin_body();
    // Neither is on an authoritative domain nor docs-shaped nor a mirror blob;
    // the shallower path is treated as the more canonical representative.
    let reranked = vec![
        candidate("https://x.com/a/b/c/deep", "/a/b/c/deep", &body, 0.8),
        candidate("https://x.com/shallow", "/shallow", &body, 0.7),
    ];
    let (kept, _) = dedup_near_duplicates(reranked, &[]);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].url, "https://x.com/shallow");
}
