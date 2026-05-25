use super::*;

#[test]
fn hyperlink_emits_osc8_when_forced() {
    let out = hyperlink_for_test("https://example.com", "click me", true);
    assert!(out.starts_with("\x1b]8;;https://example.com\x1b\\"));
    assert!(out.ends_with("\x1b]8;;\x1b\\"));
    assert!(out.contains("click me"));
}

#[test]
fn hyperlink_returns_plain_text_when_unsupported() {
    let out = hyperlink_for_test("https://example.com", "click me", false);
    assert_eq!(out, "click me");
}

#[test]
fn hyperlink_empty_label_falls_back_to_url() {
    let out = hyperlink_for_test("https://example.com", "", true);
    assert!(out.contains("https://example.com"));
}

#[test]
fn hyperlink_empty_label_unsupported_returns_url() {
    let out = hyperlink_for_test("https://example.com", "", false);
    assert_eq!(out, "https://example.com");
}
