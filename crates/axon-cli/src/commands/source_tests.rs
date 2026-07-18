use super::*;

use axon_api::source::SourceItemKey;

fn warning(message: &str, severity: Severity) -> SourceWarning {
    SourceWarning {
        code: "test.warning".to_string(),
        severity,
        message: message.to_string(),
        source_item_key: Some(SourceItemKey::from("item")),
        retryable: false,
    }
}

#[test]
fn grouped_warnings_collapses_repeats_in_first_seen_order() {
    let warnings = vec![
        warning("requested parser is not registered: web", Severity::Warning),
        warning(
            "pre-chunk redaction pass scrubbed sensitive values before chunking",
            Severity::Warning,
        ),
        warning("requested parser is not registered: web", Severity::Warning),
        warning("one-off", Severity::Warning),
        warning("requested parser is not registered: web", Severity::Warning),
    ];

    assert_eq!(
        grouped_warnings(&warnings),
        vec![
            ("Warning:", "requested parser is not registered: web", 3),
            (
                "Warning:",
                "pre-chunk redaction pass scrubbed sensitive values before chunking",
                1
            ),
            ("Warning:", "one-off", 1),
        ]
    );
}

#[test]
fn grouped_warnings_labels_informational_severities_as_notes() {
    let warnings = vec![
        warning("hint fell back", Severity::Info),
        warning("degraded", Severity::Warning),
        warning("hint fell back", Severity::Info),
        warning("verbose detail", Severity::Debug),
    ];

    assert_eq!(
        grouped_warnings(&warnings),
        vec![
            ("Note:", "hint fell back", 2),
            ("Warning:", "degraded", 1),
            ("Note:", "verbose detail", 1),
        ]
    );
}

#[test]
fn sanitize_terminal_text_strips_control_characters() {
    assert_eq!(
        sanitize_terminal_text("clean \x1b[31mred\x1b[0m\r\nline\t!"),
        "clean [31mred[0mline!"
    );
    assert_eq!(sanitize_terminal_text("untouched"), "untouched");
}
