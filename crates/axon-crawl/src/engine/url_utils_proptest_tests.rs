//! Property-based tests for `is_junk_discovered_url`.
//!
//! The hand-written tests in `engine/tests.rs` cover specific examples.
//! These properties verify the heuristics hold across adversarially generated
//! inputs — length boundaries, encoded-character combinations, and
//! clean-URL guarantees.

use super::is_junk_discovered_url;
use proptest::prelude::*;

// ── Length heuristic ─────────────────────────────────────────────────────────

proptest! {
    /// Any URL longer than 2048 characters must always be detected as junk,
    /// regardless of content.
    #[test]
    fn junk_url_any_url_over_2048_is_always_junk(
        // Generate a suffix that, together with the prefix, pushes total length > 2048.
        suffix in "[a-zA-Z0-9/]{2020,2048}",
    ) {
        let url = format!("https://example.com/{suffix}");
        // Sanity: ensure the generated URL is actually over the limit.
        prop_assume!(url.len() > 2048);
        prop_assert!(
            is_junk_discovered_url(&url),
            "URL longer than 2048 bytes must be junk: len={}", url.len()
        );
    }
}

proptest! {
    /// A URL at or under 2048 characters that is otherwise clean must not be
    /// flagged as junk by the length heuristic alone.
    #[test]
    fn junk_url_short_clean_url_not_junk(
        // Only alphanumeric + safe path characters — no encoded sequences.
        path in "[a-zA-Z0-9/_-]{1,100}",
    ) {
        let url = format!("https://example.com/{path}");
        prop_assume!(url.len() <= 2048);
        // Only assert no panic and that the result is deterministic; but a
        // clean ASCII path should never trigger any heuristic.
        prop_assert!(
            !is_junk_discovered_url(&url),
            "clean short URL must not be junk: {url}"
        );
    }
}

// ── Encoded HTML tags ────────────────────────────────────────────────────────

proptest! {
    /// Any URL whose path contains %3C (<) must be junk.
    #[test]
    fn junk_url_any_path_with_encoded_lt_is_junk(
        prefix in "[a-z0-9/]{0,30}",
        suffix in "[a-z0-9/]{0,30}",
        variant in prop::sample::select(vec!["%3C", "%3c"]),
    ) {
        let url = format!("https://example.com/{prefix}{variant}{suffix}");
        prop_assert!(
            is_junk_discovered_url(&url),
            "path with {variant} (encoded <) must be junk: {url}"
        );
    }
}

proptest! {
    /// Any URL whose path contains %3E (>) must be junk.
    #[test]
    fn junk_url_any_path_with_encoded_gt_is_junk(
        prefix in "[a-z0-9/]{0,30}",
        suffix in "[a-z0-9/]{0,30}",
        variant in prop::sample::select(vec!["%3E", "%3e"]),
    ) {
        let url = format!("https://example.com/{prefix}{variant}{suffix}");
        prop_assert!(
            is_junk_discovered_url(&url),
            "path with {variant} (encoded >) must be junk: {url}"
        );
    }
}

// ── Template literal heuristic ───────────────────────────────────────────────

proptest! {
    /// Any URL whose path contains %7B ({) must be junk.
    #[test]
    fn junk_url_any_path_with_encoded_open_brace_is_junk(
        prefix in "[a-z0-9/]{0,30}",
        suffix in "[a-z0-9/]{0,30}",
        variant in prop::sample::select(vec!["%7B", "%7b"]),
    ) {
        let url = format!("https://example.com/{prefix}{variant}{suffix}");
        prop_assert!(
            is_junk_discovered_url(&url),
            "path with {variant} (encoded {{) must be junk: {url}"
        );
    }
}

proptest! {
    /// Any URL whose path contains %7D (}) must be junk.
    #[test]
    fn junk_url_any_path_with_encoded_close_brace_is_junk(
        prefix in "[a-z0-9/]{0,30}",
        suffix in "[a-z0-9/]{0,30}",
        variant in prop::sample::select(vec!["%7D", "%7d"]),
    ) {
        let url = format!("https://example.com/{prefix}{variant}{suffix}");
        prop_assert!(
            is_junk_discovered_url(&url),
            "path with {variant} (encoded }}) must be junk: {url}"
        );
    }
}

// ── Encoded-space heuristic ──────────────────────────────────────────────────

proptest! {
    /// Any URL path with 3 or more %20 sequences must be junk.
    #[test]
    fn junk_url_three_or_more_encoded_spaces_in_path_is_junk(
        extra in 0u8..=10,
    ) {
        // Build a URL with exactly (3 + extra) %20 sequences in the path.
        let spaces: String = "%20word".repeat(3 + extra as usize);
        let url = format!("https://example.com/some{spaces}/end");
        prop_assert!(
            is_junk_discovered_url(&url),
            "path with 3+ %20 sequences must be junk: {url}"
        );
    }
}

proptest! {
    /// URLs with 0, 1, or 2 encoded spaces in the path must not be flagged
    /// by the space heuristic alone (assuming no other junk signals).
    #[test]
    fn junk_url_one_or_two_encoded_spaces_not_junk_by_space_heuristic(
        count in 0u8..=2,
        word in "[a-zA-Z]{3,10}",
    ) {
        // Build a clean path with exactly `count` %20 sequences.
        let spaces: String = (0..count).map(|_| format!("%20{word}")).collect();
        let url = format!("https://example.com/{word}{spaces}");
        prop_assert!(
            !is_junk_discovered_url(&url),
            "path with <=2 clean %20 sequences must not be junk: {url}"
        );
    }
}

// ── Query-string isolation ────────────────────────────────────────────────────

proptest! {
    /// Junk patterns in the query string must NOT trigger the filter.
    /// The function only checks the URL path portion.
    #[test]
    fn junk_url_encoded_html_in_query_string_not_junk(
        key in "[a-z]{1,10}",
        junk in prop::sample::select(vec!["%3C", "%3E", "%7B", "%7D"]),
    ) {
        let url = format!("https://example.com/clean-path?{key}={junk}value");
        prop_assert!(
            !is_junk_discovered_url(&url),
            "junk pattern in query string must NOT trigger filter: {url}"
        );
    }
}

// ── No-panic guarantee ────────────────────────────────────────────────────────

proptest! {
    /// `is_junk_discovered_url` must never panic on arbitrary byte strings,
    /// including empty strings, control characters, and non-UTF-8-looking sequences.
    #[test]
    fn junk_url_never_panics_on_arbitrary_printable_strings(
        s in "\\PC*",
    ) {
        // We only care that this does not panic.
        let _ = is_junk_discovered_url(&s);
    }
}

proptest! {
    /// Result is deterministic: calling twice with the same input gives the same output.
    #[test]
    fn junk_url_is_deterministic(
        s in "\\PC{0,200}",
    ) {
        let a = is_junk_discovered_url(&s);
        let b = is_junk_discovered_url(&s);
        prop_assert_eq!(a, b, "is_junk_discovered_url must be deterministic");
    }
}
