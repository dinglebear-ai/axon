use super::*;

#[test]
fn readyz_response_includes_sqlite_dependency() {
    let (status, body) = readiness_response(false, true, true);

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!body.ok);
    assert_eq!(body.sqlite, "not_ready");
    assert_eq!(body.qdrant, "ready");
    assert_eq!(body.tei, "ready");
}

#[test]
fn readyz_response_is_ok_only_when_all_dependencies_are_ready() {
    let (status, body) = readiness_response(true, true, true);

    assert_eq!(status, StatusCode::OK);
    assert!(body.ok);
    assert_eq!(body.sqlite, "ready");
}

#[test]
fn dependency_probes_have_a_short_deadline() {
    assert_eq!(READINESS_PROBE_TIMEOUT, Duration::from_secs(2));
}
