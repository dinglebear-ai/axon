use super::*;

#[test]
fn ask_hybrid_candidates_default_is_150() {
    let cfg = Config::default();
    assert_eq!(
        cfg.ask_hybrid_candidates, 150,
        "ask_hybrid_candidates should preserve a wider recall window for ask"
    );
}

#[test]
fn config_debug_redacts_server_url_credentials() {
    let cfg = Config {
        server_url: Some(reqwest::Url::parse("https://user:secret@example.com/v1").unwrap()),
        ..Config::default()
    };
    let dbg = format!("{cfg:?}");
    assert!(dbg.contains("[REDACTED]"));
    assert!(!dbg.contains("user:secret"));
    assert!(!dbg.contains("secret@example.com"));
}
