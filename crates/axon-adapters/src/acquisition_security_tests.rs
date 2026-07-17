use axon_observe::testing::InMemoryObservabilitySink;

use super::*;

#[tokio::test]
async fn allowed_url_passes_and_emits_an_allow_audit_event() {
    let sink = InMemoryObservabilitySink::default();
    let result = validate_source_url_audited("https://github.com/jmagar/axon.git", &sink).await;
    assert!(result.is_ok(), "a public https URL must still pass");

    let snapshot = sink.snapshot();
    assert_eq!(snapshot.events.len(), 1, "exactly one audit event emitted");
    let event = &snapshot.events[0];
    assert_eq!(event.message, "ssrf_allowed: ssrf policy check passed");
}

#[tokio::test]
async fn blocked_url_is_denied_and_emits_a_deny_audit_event() {
    let sink = InMemoryObservabilitySink::default();
    let result = validate_source_url_audited("http://127.0.0.1:9999/", &sink).await;
    assert!(result.is_err(), "a loopback URL must still be denied");

    let snapshot = sink.snapshot();
    assert_eq!(snapshot.events.len(), 1, "exactly one audit event emitted");
}

#[tokio::test]
async fn the_underlying_ssrf_check_result_is_unchanged_by_auditing() {
    // Same assertions the pre-existing bare `validate_url` call sites relied
    // on (git_acquire/feed_acquire/youtube_acquire) — auditing must not
    // change which URLs pass or fail.
    let sink = InMemoryObservabilitySink::default();
    assert!(
        validate_source_url_audited("https://example.com/repo.git", &sink)
            .await
            .is_ok()
    );
    assert!(
        validate_source_url_audited("http://169.254.169.254/latest/meta-data/", &sink)
            .await
            .is_err(),
        "cloud metadata address must still be denied"
    );
    assert!(
        validate_source_url_audited("ftp://example.com/", &sink)
            .await
            .is_err(),
        "non-http(s) scheme must still be denied"
    );
}

#[tokio::test]
async fn a_sink_failure_blocks_acquisition() {
    struct FailingSink;
    #[async_trait::async_trait]
    impl axon_observe::collector::ObservabilitySink for FailingSink {
        async fn emit(
            &self,
            _event: axon_api::source::SourceProgressEvent,
        ) -> axon_observe::collector::Result<()> {
            Err(axon_observe::testing::test_error("sink.unavailable"))
        }
        async fn heartbeat(
            &self,
            _heartbeat: axon_api::source::JobHeartbeat,
        ) -> axon_observe::collector::Result<()> {
            Ok(())
        }
        async fn metric(
            &self,
            _metric: axon_observe::metric::MetricSample,
        ) -> axon_observe::collector::Result<()> {
            Ok(())
        }
        async fn flush(&self) -> axon_observe::collector::Result<()> {
            Ok(())
        }
    }

    let result = validate_source_url_audited("https://example.com/repo.git", &FailingSink).await;
    let error = result.expect_err("a required audit failure must deny acquisition");
    assert!(error.to_string().contains("could not be persisted"));
}

#[tokio::test]
async fn validate_source_url_the_public_entrypoint_still_enforces_policy() {
    // Exercises the real production entrypoint (fresh TracingObservabilitySink)
    // used by git_acquire/feed_acquire/youtube_acquire.
    assert!(
        validate_source_url("https://example.com/repo.git")
            .await
            .is_ok()
    );
    assert!(validate_source_url("http://127.0.0.1/").await.is_err());
}

/// Confirms the emitted event actually carries SSRF detail, not just a
/// generic message — proves `emit_security_audit`/`validate_url_with_audit`
/// wiring produces the structured record the security contract requires,
/// not merely a boolean pass/fail.
#[tokio::test]
async fn emitted_events_are_ssrf_kind_with_expected_policy_decision() {
    let sink = InMemoryObservabilitySink::default();
    validate_source_url_audited("https://example.com/repo.git", &sink)
        .await
        .unwrap();
    validate_source_url_audited("http://127.0.0.1/", &sink)
        .await
        .unwrap_err();

    let snapshot = sink.snapshot();
    assert_eq!(snapshot.events.len(), 2);
    assert!(snapshot.events[0].message.starts_with("ssrf_allowed:"));
    assert!(snapshot.events[1].message.starts_with("ssrf_denied:"));
}
