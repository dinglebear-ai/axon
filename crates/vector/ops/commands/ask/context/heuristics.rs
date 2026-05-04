#[cfg(test)]
use crate::crates::vector::ops::ranking;
#[cfg(test)]
use spider::url::Url;
#[cfg(test)]
use std::collections::HashMap;

pub(super) const SUPPLEMENTAL_CONTEXT_BUDGET_PCT: usize = 85;
pub(super) const SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE: usize = 6;
pub(super) const SUPPLEMENTAL_RELEVANCE_BONUS: f64 = 0.0;

pub(super) fn push_context_entry(
    entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    score: f64,
    entry: String,
    separator: &str,
    max_chars: usize,
) -> bool {
    let projected = if entries.is_empty() {
        entry.len()
    } else {
        *context_char_count + separator.len() + entry.len()
    };
    if projected > max_chars {
        return false;
    }
    entries.push((score, entry));
    *context_char_count = projected;
    true
}

pub(super) fn should_inject_supplemental(
    context_char_count: usize,
    max_context_chars: usize,
    full_docs_selected: usize,
    top_chunks_selected: usize,
) -> bool {
    if max_context_chars == 0 {
        return false;
    }
    let within_budget =
        context_char_count * 100 < max_context_chars * SUPPLEMENTAL_CONTEXT_BUDGET_PCT;
    let coverage_needs_backfill =
        full_docs_selected == 0 || top_chunks_selected < SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE;
    within_budget && coverage_needs_backfill
}

#[cfg(test)]
pub(super) fn query_requests_low_signal_sources(query_tokens: &[String], raw_query: &str) -> bool {
    ranking::query_wants_low_signal_sources(query_tokens, raw_query)
}

#[cfg(test)]
pub(super) fn url_matches_domain_list(url: &str, domains: &[String]) -> bool {
    if domains.is_empty() {
        return true;
    }
    let host = Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()));
    let Some(host) = host else {
        return false;
    };
    domains.iter().any(|domain| {
        let normalized = domain.trim().to_ascii_lowercase();
        !normalized.is_empty() && (host == normalized || host.ends_with(&format!(".{normalized}")))
    })
}

#[cfg(test)]
fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()))
}

#[cfg(test)]
pub(super) fn top_domains(candidates: &[ranking::AskCandidate], limit: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for candidate in candidates {
        if let Some(host) = host_from_url(&candidate.url) {
            *counts.entry(host).or_insert(0) += 1;
        }
    }
    let mut entries = counts.into_iter().collect::<Vec<_>>();
    entries.sort_by(|(domain_a, count_a), (domain_b, count_b)| {
        count_b.cmp(count_a).then_with(|| domain_a.cmp(domain_b))
    });
    entries
        .into_iter()
        .take(limit)
        .map(|(domain, count)| format!("{domain}:{count}"))
        .collect()
}

#[cfg(test)]
pub(super) fn authoritative_ratio(candidates: &[ranking::AskCandidate], domains: &[String]) -> f64 {
    if candidates.is_empty() || domains.is_empty() {
        return 0.0;
    }
    let authoritative = candidates
        .iter()
        .filter(|candidate| url_matches_domain_list(&candidate.url, domains))
        .count();
    authoritative as f64 / candidates.len() as f64
}

#[cfg(test)]
fn candidate_topical_overlap_count(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
) -> usize {
    query_tokens
        .iter()
        .filter(|token| {
            candidate.url_tokens.contains(token.as_str())
                || candidate.chunk_tokens.contains(token.as_str())
        })
        .count()
}

#[cfg(test)]
pub(super) fn candidate_has_topical_overlap(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
) -> bool {
    if query_tokens.is_empty() {
        return true;
    }
    let overlap = candidate_topical_overlap_count(candidate, query_tokens);
    let coverage = overlap as f64 / query_tokens.len() as f64;
    match query_tokens.len() {
        0 => true,
        1 | 2 => overlap >= 1,
        3 | 4 => overlap >= 1 || coverage >= 0.5,
        _ => overlap >= 2 && coverage >= 0.34,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::vector::ops::ranking;
    use std::collections::HashSet;

    fn make_candidate(
        url: &str,
        url_tokens: &[&str],
        chunk_tokens: &[&str],
    ) -> ranking::AskCandidate {
        ranking::AskCandidate {
            score: 0.5,
            url: url.to_string(),
            path: String::new(),
            chunk_text: String::new(),
            url_tokens: url_tokens
                .iter()
                .map(|s| s.to_string())
                .collect::<HashSet<_>>(),
            chunk_tokens: chunk_tokens
                .iter()
                .map(|s| s.to_string())
                .collect::<HashSet<_>>(),
            rerank_score: 0.0,
        }
    }

    // ── push_context_entry ────────────────────────────────────────────────────

    #[test]
    fn push_context_entry_first_entry_no_separator_overhead() {
        let mut entries: Vec<(f64, String)> = Vec::new();
        let mut count: usize = 0;
        let entry = "hello world".to_string(); // len = 11
        let result = push_context_entry(&mut entries, &mut count, 1.0, entry, "\n\n", 100);
        assert!(result, "first entry should be accepted");
        assert_eq!(count, 11, "count should equal entry length (no separator)");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn push_context_entry_second_entry_within_budget() {
        let mut entries: Vec<(f64, String)> = vec![(1.0, "aaaa".to_string())]; // first entry, len=4
        let mut count: usize = 4;
        let sep = "\n\n"; // len=2
        let entry = "bbbbb".to_string(); // len=5  => projected = 4+2+5 = 11
        let result = push_context_entry(&mut entries, &mut count, 0.5, entry, sep, 20);
        assert!(result, "second entry within budget should be accepted");
        assert_eq!(count, 11);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn push_context_entry_rejected_when_over_budget() {
        let mut entries: Vec<(f64, String)> = vec![(1.0, "aaaa".to_string())]; // len=4
        let mut count: usize = 4;
        let sep = "\n\n"; // len=2
        // projected = 4+2+5 = 11, max=10 => reject
        let entry = "bbbbb".to_string();
        let result = push_context_entry(&mut entries, &mut count, 0.5, entry, sep, 10);
        assert!(!result, "entry over budget should be rejected");
        assert_eq!(count, 4, "count must be unchanged");
        assert_eq!(entries.len(), 1, "entries must be unchanged");
    }

    #[test]
    fn push_context_entry_exactly_at_boundary_accepted() {
        let mut entries: Vec<(f64, String)> = vec![(1.0, "aaaa".to_string())]; // len=4
        let mut count: usize = 4;
        let sep = "\n\n"; // len=2
        // projected = 4+2+5 = 11 == max=11 => accepted (projected <= max)
        let entry = "bbbbb".to_string();
        let result = push_context_entry(&mut entries, &mut count, 0.5, entry, sep, 11);
        assert!(
            result,
            "entry exactly at max_chars boundary should be accepted"
        );
        assert_eq!(count, 11);
    }

    // ── should_inject_supplemental ────────────────────────────────────────────

    #[test]
    fn should_inject_supplemental_false_when_max_chars_zero() {
        assert!(!should_inject_supplemental(0, 0, 0, 0));
        assert!(!should_inject_supplemental(100, 0, 0, 0));
    }

    #[test]
    fn should_inject_supplemental_true_within_budget_no_full_docs() {
        // within_budget: 0 * 100 = 0 < 1000 * 85 = 85000
        // coverage_needs_backfill: full_docs==0
        assert!(should_inject_supplemental(0, 1000, 0, 10));
    }

    #[test]
    fn should_inject_supplemental_true_within_budget_low_top_chunks() {
        // full_docs > 0 but top_chunks < SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE (6)
        // within_budget: 100 * 100 = 10_000 < 10_000 * 85 = 850_000
        assert!(should_inject_supplemental(
            100,
            10_000,
            1,
            SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE - 1
        ));
    }

    #[test]
    fn should_inject_supplemental_false_over_budget() {
        // context_char_count * 100 >= max_context_chars * 85
        // 850 * 100 = 85_000 >= 1000 * 85 = 85_000 => NOT within budget
        assert!(!should_inject_supplemental(850, 1000, 0, 0));
    }

    #[test]
    fn should_inject_supplemental_false_no_backfill_needed() {
        // full_docs > 0 AND top_chunks >= SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE
        // within_budget true but coverage_needs_backfill is false
        assert!(!should_inject_supplemental(
            0,
            1000,
            1,
            SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE
        ));
    }

    // ── query_requests_low_signal_sources ────────────────────────────────────

    #[test]
    fn query_requests_low_signal_raw_query_docs_sessions() {
        let tokens: Vec<String> = vec![];
        assert!(query_requests_low_signal_sources(
            &tokens,
            "show me docs/sessions from last week"
        ));
    }

    #[test]
    fn query_requests_low_signal_token_session() {
        let tokens = vec!["session".to_string()];
        assert!(query_requests_low_signal_sources(&tokens, "my query"));
    }

    #[test]
    fn query_requests_low_signal_token_logs() {
        let tokens = vec!["logs".to_string()];
        assert!(query_requests_low_signal_sources(&tokens, "show logs"));
    }

    #[test]
    fn query_requests_low_signal_token_history() {
        let tokens = vec!["history".to_string()];
        assert!(query_requests_low_signal_sources(&tokens, "query history"));
    }

    #[test]
    fn query_requests_low_signal_false_for_normal_query() {
        let tokens = vec!["rust".to_string(), "async".to_string(), "tokio".to_string()];
        assert!(!query_requests_low_signal_sources(
            &tokens,
            "how does tokio async runtime work"
        ));
    }

    // ── is_low_signal_url (via ranking) ──────────────────────────────────────

    #[test]
    fn is_low_signal_source_url_docs_sessions_path() {
        assert!(ranking::is_low_signal_url(
            "https://example.com/docs/sessions/2026-03-01.md"
        ));
    }

    #[test]
    fn is_low_signal_source_url_cache_path() {
        assert!(ranking::is_low_signal_url(
            "https://example.com/.cache/axon/something"
        ));
    }

    #[test]
    fn is_low_signal_source_url_local_log_file() {
        assert!(ranking::is_low_signal_url("/var/logs/app.log"));
    }

    #[test]
    fn is_low_signal_source_url_web_url_with_logs_segment_is_not_low_signal() {
        // is_web_url=true so the /logs/ guard is skipped
        assert!(!ranking::is_low_signal_url("https://example.com/logs/"));
    }

    #[test]
    fn is_low_signal_source_url_normal_docs_url() {
        assert!(!ranking::is_low_signal_url(
            "https://docs.example.com/guide/getting-started"
        ));
    }

    // ── url_matches_domain_list ───────────────────────────────────────────────

    #[test]
    fn url_matches_domain_list_empty_domains_permissive() {
        assert!(url_matches_domain_list("https://example.com/page", &[]));
    }

    #[test]
    fn url_matches_domain_list_exact_domain_match() {
        let domains = vec!["example.com".to_string()];
        assert!(url_matches_domain_list(
            "https://example.com/page",
            &domains
        ));
    }

    #[test]
    fn url_matches_domain_list_subdomain_matches_parent() {
        let domains = vec!["example.com".to_string()];
        assert!(url_matches_domain_list(
            "https://sub.example.com/page",
            &domains
        ));
    }

    #[test]
    fn url_matches_domain_list_different_domain_no_match() {
        let domains = vec!["example.com".to_string()];
        assert!(!url_matches_domain_list("https://other.com/page", &domains));
    }

    #[test]
    fn url_matches_domain_list_non_url_string_with_domains_returns_false() {
        let domains = vec!["example.com".to_string()];
        assert!(!url_matches_domain_list("not-a-url", &domains));
    }

    // ── top_domains ───────────────────────────────────────────────────────────

    #[test]
    fn top_domains_empty_candidates_returns_empty() {
        let result = top_domains(&[], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn top_domains_returns_domain_colon_count_format() {
        let candidates = vec![
            make_candidate("https://example.com/a", &[], &[]),
            make_candidate("https://example.com/b", &[], &[]),
        ];
        let result = top_domains(&candidates, 10);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "example.com:2");
    }

    #[test]
    fn top_domains_sorted_by_count_descending() {
        let candidates = vec![
            make_candidate("https://alpha.com/a", &[], &[]),
            make_candidate("https://beta.com/a", &[], &[]),
            make_candidate("https://beta.com/b", &[], &[]),
            make_candidate("https://beta.com/c", &[], &[]),
        ];
        let result = top_domains(&candidates, 10);
        // beta.com has 3, alpha.com has 1 => beta first
        assert_eq!(result[0], "beta.com:3");
        assert_eq!(result[1], "alpha.com:1");
    }

    #[test]
    fn top_domains_respects_limit() {
        let candidates = vec![
            make_candidate("https://a.com/x", &[], &[]),
            make_candidate("https://b.com/x", &[], &[]),
            make_candidate("https://c.com/x", &[], &[]),
        ];
        let result = top_domains(&candidates, 2);
        assert_eq!(result.len(), 2);
    }

    // ── authoritative_ratio ───────────────────────────────────────────────────

    #[test]
    fn authoritative_ratio_empty_candidates_returns_zero() {
        let domains = vec!["example.com".to_string()];
        assert_eq!(authoritative_ratio(&[], &domains), 0.0);
    }

    #[test]
    fn authoritative_ratio_empty_domains_returns_zero() {
        let candidates = vec![make_candidate("https://example.com/a", &[], &[])];
        assert_eq!(authoritative_ratio(&candidates, &[]), 0.0);
    }

    #[test]
    fn authoritative_ratio_all_authoritative_returns_one() {
        let candidates = vec![
            make_candidate("https://example.com/a", &[], &[]),
            make_candidate("https://example.com/b", &[], &[]),
        ];
        let domains = vec!["example.com".to_string()];
        let ratio = authoritative_ratio(&candidates, &domains);
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn authoritative_ratio_half_authoritative_returns_half() {
        let candidates = vec![
            make_candidate("https://example.com/a", &[], &[]),
            make_candidate("https://other.com/b", &[], &[]),
        ];
        let domains = vec!["example.com".to_string()];
        let ratio = authoritative_ratio(&candidates, &domains);
        assert!((ratio - 0.5).abs() < f64::EPSILON);
    }

    // ── candidate_has_topical_overlap ─────────────────────────────────────────

    #[test]
    fn candidate_has_topical_overlap_empty_tokens_permissive() {
        let candidate = make_candidate("https://example.com", &[], &[]);
        assert!(candidate_has_topical_overlap(&candidate, &[]));
    }

    #[test]
    fn candidate_has_topical_overlap_one_token_match() {
        // 1-2 tokens: overlap >= 1
        let candidate = make_candidate("https://example.com", &["rust"], &[]);
        let tokens = vec!["rust".to_string()];
        assert!(candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_one_token_no_match() {
        let candidate = make_candidate("https://example.com", &[], &[]);
        let tokens = vec!["rust".to_string()];
        assert!(!candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_three_tokens_single_match_passes() {
        // 3-4 tokens: overlap >= 1 OR coverage >= 0.5
        // overlap = 1 >= 1 => true (relaxed from old threshold of 2)
        let candidate = make_candidate("https://example.com", &["async"], &[]);
        let tokens = vec!["async".to_string(), "rust".to_string(), "tokio".to_string()];
        assert!(candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_three_tokens_two_matches_passes() {
        // 3 tokens, overlap=2 >= 2 => true
        let candidate = make_candidate("https://example.com", &["async", "rust"], &[]);
        let tokens = vec!["async".to_string(), "rust".to_string(), "tokio".to_string()];
        assert!(candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_four_tokens_coverage_threshold_passes() {
        // 4 tokens, overlap=2 => coverage=2/4=0.5 >= 0.5 => true
        let candidate = make_candidate("https://example.com", &["async", "rust"], &[]);
        let tokens = vec![
            "async".to_string(),
            "rust".to_string(),
            "tokio".to_string(),
            "future".to_string(),
        ];
        assert!(candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_five_tokens_passes_both_conditions() {
        // 5+ tokens: overlap >= 2 AND coverage >= 0.34
        // overlap=2, coverage=2/5=0.4 >= 0.34 => true
        let candidate = make_candidate("https://example.com", &["async", "rust"], &[]);
        let tokens = vec![
            "async".to_string(),
            "rust".to_string(),
            "tokio".to_string(),
            "future".to_string(),
            "spawn".to_string(),
        ];
        assert!(candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_five_tokens_overlap_one_fails() {
        // 5+ tokens: overlap=1 < 2 => false even if coverage would pass
        let candidate = make_candidate("https://example.com", &["async"], &[]);
        let tokens = vec![
            "async".to_string(),
            "rust".to_string(),
            "tokio".to_string(),
            "future".to_string(),
            "spawn".to_string(),
        ];
        assert!(!candidate_has_topical_overlap(&candidate, &tokens));
    }

    #[test]
    fn candidate_has_topical_overlap_chunk_tokens_count_toward_overlap() {
        // url_tokens empty, chunk_tokens has the match
        let candidate = make_candidate("https://example.com", &[], &["rust"]);
        let tokens = vec!["rust".to_string()];
        assert!(candidate_has_topical_overlap(&candidate, &tokens));
    }
}
