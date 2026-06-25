use super::*;

fn info(server_max: u64) -> Value {
    serde_json::json!({ "max_concurrent_requests": server_max })
}

#[test]
fn warns_when_env_expectation_disagrees_with_live_server() {
    // The production drift: env says 256, the running container was started
    // without --env-file and reports the compose default of 32.
    let warning = tei_concurrency_warning_inner(Some(&info(32)), Some(256), 8)
        .expect("env/server mismatch must warn");
    assert!(warning.contains("max_concurrent_requests=32"), "{warning}");
    assert!(
        warning.contains("TEI_MAX_CONCURRENT_REQUESTS=256"),
        "{warning}"
    );
    assert!(warning.contains("--env-file"), "{warning}");
}

#[test]
fn warns_when_client_cap_exceeds_server_budget() {
    let warning = tei_concurrency_warning_inner(Some(&info(8)), None, 64)
        .expect("client cap over server budget must warn");
    assert!(warning.contains("AXON_TEI_MAX_CONCURRENT=64"), "{warning}");
    assert!(warning.contains("max_concurrent_requests=8"), "{warning}");
}

#[test]
fn silent_when_settings_agree() {
    assert_eq!(
        tei_concurrency_warning_inner(Some(&info(256)), Some(256), 8),
        None
    );
    // No env expectation set and the client cap fits the budget.
    assert_eq!(
        tei_concurrency_warning_inner(Some(&info(32)), None, 8),
        None
    );
}

#[test]
fn silent_when_info_is_missing_or_unshaped() {
    assert_eq!(tei_concurrency_warning_inner(None, Some(256), 8), None);
    let no_field = serde_json::json!({ "model_id": "x" });
    assert_eq!(
        tei_concurrency_warning_inner(Some(&no_field), Some(256), 8),
        None
    );
}
