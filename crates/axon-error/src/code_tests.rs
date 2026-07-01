use super::*;
use crate::severity::ErrorSeverity;

#[test]
fn category_is_prefix_before_first_dot() {
    assert_eq!(
        ErrorCode::from("provider.unavailable").category(),
        "provider"
    );
    assert_eq!(
        ErrorCode::from("source.acquire.fetch_failed").category(),
        "source"
    );
    assert_eq!(ErrorCode::from("standalone").category(), "standalone");
}

#[test]
fn code_serializes_transparently() {
    let code = ErrorCode::from("provider.unavailable");
    assert_eq!(serde_json::to_value(&code).unwrap(), "provider.unavailable");
    let back: ErrorCode = serde_json::from_value(serde_json::json!("x.y")).unwrap();
    assert_eq!(back, ErrorCode::from("x.y"));
}

#[test]
fn classify_maps_codes_to_severity_and_retry() {
    // Not retryable: command/action/route parse/validation.
    for code in ["command.unknown", "action.removed", "route.not_found"] {
        assert_eq!(
            ErrorCode::from(code).classify(),
            (ErrorSeverity::Failed, false),
            "{code}"
        );
    }

    // Retryable transient families.
    for code in [
        "source.acquire.fetch_failed",
        "ledger.transaction",
        "embedding.batch_failed",
        "vector.upsert_failed",
        "artifact.write_failed",
        "provider.unavailable",
        "prune.cleanup_failed",
    ] {
        assert_eq!(
            ErrorCode::from(code).classify(),
            (ErrorSeverity::Failed, true),
            "{code}"
        );
    }

    // Unsupported scope: not retryable.
    assert_eq!(
        ErrorCode::from("source.scope.unsupported").classify(),
        (ErrorSeverity::Failed, false)
    );

    // Parser/graph degrade, not retryable.
    assert_eq!(
        ErrorCode::from("parser.fallback").classify(),
        (ErrorSeverity::Degraded, false)
    );
    assert_eq!(
        ErrorCode::from("graph.merge_conflict").classify(),
        (ErrorSeverity::Degraded, false)
    );

    // Redaction: fatal, not retryable.
    assert_eq!(
        ErrorCode::from("redaction.leak_detected").classify(),
        (ErrorSeverity::Fatal, false)
    );
}
