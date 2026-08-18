use super::{extract_progress_mode, extract_provenance_message};
use crate::commands::wait_progress::ProgressMode;
use axon_core::config::{CommandKind, Config};

#[test]
fn extract_wait_reuses_shared_renderer_and_json_stays_silent() {
    let mut cfg = Config {
        command: CommandKind::Extract,
        wait: true,
        ..Config::default()
    };
    assert_eq!(extract_progress_mode(&cfg, true), ProgressMode::Interactive);
    cfg.json_output = true;
    assert_eq!(extract_progress_mode(&cfg, true), ProgressMode::Silent);
}

#[test]
fn provenance_message_reports_deterministic_only_without_fallback() {
    let summary = serde_json::json!({
        "deterministic_pages": 2,
        "llm_fallback_pages": 0,
        "parser_hits": {
            "json-ld": 1,
            "open-graph": 1
        }
    });

    let message = extract_provenance_message(&summary).expect("message");

    assert!(message.contains("2 page(s) handled by json-ld, open-graph"));
    assert!(message.contains("LLM fallback was not used"));
}

#[test]
fn provenance_message_reports_mixed_parser_and_fallback_use() {
    let summary = serde_json::json!({
        "deterministic_pages": 1,
        "llm_fallback_pages": 3,
        "parser_hits": {
            "html-table": 1
        }
    });

    let message = extract_provenance_message(&summary).expect("message");

    assert!(message.contains("1 page(s) handled by html-table"));
    assert!(message.contains("LLM fallback ran for 3 page(s)"));
}
