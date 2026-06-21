use super::*;

#[test]
fn debug_redacts_the_value() {
    let s = Secret::from("super-secret-token".to_string());
    let rendered = format!("{s:?}");
    assert!(!rendered.contains("super-secret-token"));
    assert!(rendered.contains("redacted"));
}

#[test]
fn expose_returns_the_inner_value() {
    assert_eq!(Secret::from("abc".to_string()).expose(), "abc");
}

#[test]
fn serde_is_transparent() {
    let s: Secret = serde_json::from_str("\"tok\"").unwrap();
    assert_eq!(s.expose(), "tok");
    assert_eq!(serde_json::to_string(&s).unwrap(), "\"tok\"");
}
