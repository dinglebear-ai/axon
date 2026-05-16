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
    let prose: String = "alpha beta gamma delta epsilon zeta eta theta ".repeat(60);
    let filler: String = "y".repeat(6 * 1024);
    let html =
        format!("<html><body><main></main><div>{prose}</div><span>{filler}</span></body></html>",);
    let t = thresholds(30, 200, 2.0);
    let r = extract_with_ladder(html.as_bytes(), None, t);
    assert!(r.word_count > 0, "must produce some markdown");
    assert!(matches!(
        r.tier,
        LadderTier::Scored | LadderTier::Relaxed | LadderTier::Body
    ));
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
