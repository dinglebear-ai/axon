use super::*;

#[test]
fn chat_message_accepts_query_without_positional_text() {
    let mut cfg = Config::test_default();
    cfg.query = Some("query-only prompt".to_string());
    assert_eq!(chat_message(&cfg).unwrap(), "query-only prompt");
}

#[test]
fn chat_message_prefers_positional_text() {
    let mut cfg = Config::test_default();
    cfg.query = Some("fallback".to_string());
    cfg.positional = vec!["positional".to_string(), "prompt".to_string()];
    assert_eq!(chat_message(&cfg).unwrap(), "positional prompt");
}

#[test]
fn chat_message_rejects_empty_input() {
    let cfg = Config::test_default();
    assert!(
        chat_message(&cfg)
            .unwrap_err()
            .to_string()
            .contains("MESSAGE")
    );
}
