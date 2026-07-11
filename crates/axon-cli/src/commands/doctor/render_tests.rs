use super::*;
use serde_json::json;

// gitleaks:allow -- synthetic test fixtures, not real credentials
const FAKE_OPENAI_KEY: &str = "sk-abcdefghijklmnopqrstuvwxyz012345";
const FAKE_GOOGLE_KEY: &str = "AIzaSyDaGmWKa4JsXZ-HjGw7ISLn_3namBGewQe";

#[test]
fn report_text_redacts_openai_style_key_in_detail_field() {
    let report = json!({
        "services": {
            "openai": {
                "detail": format!("connection failed: invalid key {FAKE_OPENAI_KEY}")
            }
        }
    });

    let text = report_text(&report, &["services", "openai", "detail"], "");

    assert!(
        !text.contains(FAKE_OPENAI_KEY),
        "token leaked into rendered doctor output: {text}"
    );
    assert!(
        text.contains("[REDACTED]"),
        "expected redaction marker: {text}"
    );
}

#[test]
fn report_text_redacts_google_api_key_in_gemini_detail() {
    let report = json!({
        "services": {
            "gemini_headless": {
                "detail": format!("auth error, key {FAKE_GOOGLE_KEY} rejected"),
            }
        }
    });

    let text = report_text(&report, &["services", "gemini_headless", "detail"], "");

    assert!(
        !text.contains(FAKE_GOOGLE_KEY),
        "google api key leaked into rendered doctor output: {text}"
    );
    assert!(
        text.contains("[REDACTED]"),
        "expected redaction marker: {text}"
    );
}

#[test]
fn report_text_leaves_non_secret_text_untouched() {
    let report = json!({
        "services": {
            "tei": {
                "model": "Qwen3-Embedding-0.6B",
                "detail": "reachable"
            }
        }
    });

    assert_eq!(
        report_text(&report, &["services", "tei", "model"], ""),
        "Qwen3-Embedding-0.6B"
    );
    assert_eq!(
        report_text(&report, &["services", "tei", "detail"], ""),
        "reachable"
    );
}

#[test]
fn report_text_redacts_bearer_authorization_header_in_detail() {
    let report = json!({
        "services": {
            "qdrant": {
                "detail": "request failed: Authorization: Bearer sekrit-testing-token-value-123456"
            }
        }
    });

    let text = report_text(&report, &["services", "qdrant", "detail"], "");

    assert!(!text.contains("sekrit-testing-token-value-123456"));
    assert!(text.contains("[REDACTED]"));
}
