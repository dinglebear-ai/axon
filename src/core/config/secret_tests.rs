use super::*;

#[test]
fn secret_debug_is_redacted() {
    let s = Secret::new("my-api-key".to_string());
    assert_eq!(format!("{s:?}"), "[REDACTED]");
}

#[test]
fn secret_display_is_redacted() {
    let s = Secret::new("my-api-key".to_string());
    assert_eq!(format!("{s}"), "[REDACTED]");
}

#[test]
fn secret_expose_returns_inner() {
    let s = Secret::new("my-api-key".to_string());
    assert_eq!(s.expose(), "my-api-key");
}

#[test]
fn secret_into_inner() {
    let s = Secret::new("my-api-key".to_string());
    assert_eq!(s.into_inner(), "my-api-key");
}

#[test]
fn secret_as_str() {
    let s = Secret::new("my-api-key".to_string());
    assert_eq!(s.as_str(), "my-api-key");
}

#[test]
fn secret_default_string_is_empty() {
    let s: Secret<String> = Secret::default();
    assert_eq!(s.expose(), "");
}

#[test]
fn secret_equality_compares_inner() {
    let a = Secret::new("key".to_string());
    let b = Secret::new("key".to_string());
    let c = Secret::new("other".to_string());
    assert_eq!(a, b);
    assert_ne!(a, c);
}
