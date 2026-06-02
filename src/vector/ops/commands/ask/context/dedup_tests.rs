use super::*;
use crate::vector::ops::ranking::AskCandidate;
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
