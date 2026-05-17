use super::*;
use std::ffi::OsString;

#[test]
fn allowlist_excludes_common_secret_keys() {
    let denied = [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "AXON_MCP_HTTP_TOKEN",
        "TAVILY_API_KEY",
        "GITHUB_TOKEN",
        "BEADS_DOLT_PASSWORD",
        "CLAUDECODE",
    ];
    for key in denied {
        assert!(!allowed_env_keys().contains(&key));
    }
}

#[test]
fn capture_keeps_only_allowed_keys() {
    let captured = capture_allowed_env(&[
        ("PATH", "/usr/bin"),
        ("OPENAI_API_KEY", "secret"),
        ("GEMINI_API_KEY", "gemini"),
        ("GOOGLE_CLOUD_LOCATION", "us-central1"),
    ]);
    assert_eq!(
        captured,
        vec![
            ("PATH", OsString::from("/usr/bin")),
            ("GOOGLE_CLOUD_LOCATION", OsString::from("us-central1")),
            ("GEMINI_API_KEY", OsString::from("gemini")),
        ]
    );
}
