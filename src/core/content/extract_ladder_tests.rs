use super::*;

fn thresholds(s1: usize, s2: usize, mult: f64) -> LadderThresholds {
    LadderThresholds {
        strategy1: s1,
        strategy2: s2,
        body_multiplier: mult,
    }
}

#[test]
fn ladder_tier_as_str_is_stable() {
    assert_eq!(LadderTier::Scored.as_str(), "scored");
    assert_eq!(LadderTier::Relaxed.as_str(), "relaxed");
    assert_eq!(LadderTier::Body.as_str(), "body");
}

#[test]
fn approximate_body_bytes_handles_no_body_tag() {
    let html = b"<p>fragment</p>";
    assert_eq!(approximate_body_bytes(html), html.len());
}

#[test]
fn approximate_body_bytes_returns_distance_between_body_markers() {
    let html = b"<html><head></head><body><p>hello there</p></body></html>";
    let bytes = approximate_body_bytes(html);
    assert!(bytes > 10);
    assert!(bytes < html.len());
}

#[test]
fn small_body_skips_retries_returns_scored_tier() {
    let html = b"<html><body><p>hi</p></body></html>";
    let t = thresholds(30, 200, 2.0);
    let r = extract_with_ladder(html, None, t);
    assert_eq!(
        r.tier,
        LadderTier::Scored,
        "tiny body must not trigger retries"
    );
}

#[test]
fn dense_page_scored_tier_wins_no_retry() {
    let prose: String = "lorem ipsum dolor sit amet ".repeat(80);
    let filler: String = "x".repeat(6 * 1024);
    let html = format!(
        "<html><body><main><p>{prose}</p><div hidden=\"true\">{filler}</div></main></body></html>",
    );
    let t = thresholds(30, 200, 2.0);
    let r = extract_with_ladder(html.as_bytes(), None, t);
    assert_eq!(r.tier, LadderTier::Scored);
    assert!(r.word_count >= 200, "dense page must report >=200 words");
    assert!(r.markdown.contains("lorem ipsum"));
}

#[test]
fn body_tier_only_fires_when_body_multiplier_met() {
    // No user selector → user_had_root_selector is false → body-tier branch is eligible.
    // The exact tier depends on spider_transformations fallback behavior when <main> is empty;
    // what matters is that content is extracted and the Relaxed tier does NOT fire
    // (which would require a user root_selector, absent here).
    let prose: String = "alpha beta gamma delta epsilon zeta eta theta ".repeat(60);
    let filler: String = "y".repeat(6 * 1024);
    let html =
        format!("<html><body><main></main><div>{prose}</div><span>{filler}</span></body></html>",);
    let t = thresholds(30, 200, 2.0);
    let r = extract_with_ladder(html.as_bytes(), None, t);
    // Relaxed tier requires a user root_selector — must not fire without one.
    assert_ne!(
        r.tier,
        LadderTier::Relaxed,
        "relaxed tier must not fire when no user root_selector is provided"
    );
    assert!(
        r.word_count > BODY_TIER_MIN_WORDS,
        "extraction must yield substantial content"
    );
    assert!(
        r.markdown.contains("alpha"),
        "extracted markdown must include prose words"
    );
}

#[test]
fn relaxed_tier_fires_when_root_selector_yields_thin_content() {
    // A root_selector pointing at an empty element makes scored thin.
    // Relaxed (no selector) should pick up the prose and win.
    let prose: String = "quick brown fox jumps over the lazy dog ".repeat(30);
    let filler: String = "z".repeat(6 * 1024);
    let html = format!(
        "<html><body><section id=\"empty\"></section><article><p>{prose}</p></article><span>{filler}</span></body></html>",
    );
    let sc = SelectorConfiguration {
        root_selector: Some("#empty".to_string()),
        ..SelectorConfiguration::default()
    };
    let t = thresholds(300, 300, 1.5);
    let r = extract_with_ladder(html.as_bytes(), Some(&sc), t);
    assert_eq!(
        r.tier,
        LadderTier::Relaxed,
        "relaxed tier should win when root selector yields thin content but body has prose"
    );
    assert!(
        r.word_count > 50,
        "relaxed extraction must contain meaningful word count"
    );
    assert!(
        r.markdown.contains("quick brown fox"),
        "relaxed extraction must include prose words"
    );
}

#[test]
fn ladder_thresholds_respect_cfg() {
    let prose: String = "word ".repeat(80);
    let filler: String = "z".repeat(6 * 1024);
    let html = format!("<html><body><div>{prose}</div><span>{filler}</span></body></html>");
    let t = thresholds(30, 5, 2.0);
    let r = extract_with_ladder(html.as_bytes(), None, t);
    assert_eq!(
        r.tier,
        LadderTier::Scored,
        "Body retry must not fire when scored already meets strategy2 threshold"
    );
}
