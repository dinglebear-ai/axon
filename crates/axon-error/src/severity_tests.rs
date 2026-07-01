use super::*;

#[test]
fn severity_json_names_are_snake_case() {
    let cases = [
        (ErrorSeverity::Info, "info"),
        (ErrorSeverity::Warning, "warning"),
        (ErrorSeverity::Degraded, "degraded"),
        (ErrorSeverity::Failed, "failed"),
        (ErrorSeverity::Fatal, "fatal"),
    ];
    for (severity, name) in cases {
        assert_eq!(serde_json::to_value(severity).unwrap(), name);
    }
}

#[test]
fn is_terminal_matches_contract() {
    assert!(!ErrorSeverity::Info.is_terminal());
    assert!(!ErrorSeverity::Warning.is_terminal());
    assert!(!ErrorSeverity::Degraded.is_terminal());
    assert!(ErrorSeverity::Failed.is_terminal());
    assert!(ErrorSeverity::Fatal.is_terminal());
}
