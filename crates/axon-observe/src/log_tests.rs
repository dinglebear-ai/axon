use super::*;
use axon_api::source::{JobId, SourceId};
use chrono::Utc;

fn now() -> Timestamp {
    Timestamp::from(Utc::now())
}

#[test]
fn new_redacts_secrets_in_message() {
    let field_set = LogFieldSet::new(
        now(),
        LogLevel::Error,
        "axon_core::llm",
        "auth failed: Authorization: Bearer sk-abcdefghijklmnop",
    );

    assert!(!field_set.message.contains("sk-abcdefghijklmnop"));
    assert!(field_set.message.contains("[REDACTED]"));
}

#[test]
fn redact_message_reapplies_the_hook() {
    let mut field_set = LogFieldSet::new(now(), LogLevel::Info, "target", "clean message");
    field_set.message = "leaked AIzaSyD-abcdefghijklmnopqrstuvwxyz0123456".to_string();
    field_set.redact_message();

    assert!(!field_set.message.contains("AIzaSyD"));
}

#[test]
fn builder_setters_populate_correlation_fields() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let field_set = LogFieldSet::new(now(), LogLevel::Warn, "axon_jobs::runtime", "waiting")
        .with_job_id(job_id)
        .with_request_id("req_1")
        .with_source_id(SourceId::from("src_1"))
        .with_phase(PipelinePhase::Embedding)
        .with_provider_id(ProviderId::from("tei"))
        .with_error_code("provider.unavailable");

    assert_eq!(field_set.job_id, Some(job_id));
    assert_eq!(field_set.request_id.as_deref(), Some("req_1"));
    assert_eq!(field_set.source_id, Some(SourceId::from("src_1")));
    assert_eq!(field_set.phase, Some(PipelinePhase::Embedding));
    assert_eq!(field_set.provider_id, Some(ProviderId::from("tei")));
    assert_eq!(
        field_set.error_code.as_deref(),
        Some("provider.unavailable")
    );
}

#[test]
fn log_level_as_str_matches_serde_wire_form() {
    assert_eq!(LogLevel::Trace.as_str(), "trace");
    assert_eq!(LogLevel::Debug.as_str(), "debug");
    assert_eq!(LogLevel::Info.as_str(), "info");
    assert_eq!(LogLevel::Warn.as_str(), "warn");
    assert_eq!(LogLevel::Error.as_str(), "error");

    for level in [
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ] {
        let value = serde_json::to_value(level).expect("serialize log level");
        assert_eq!(value.as_str(), Some(level.as_str()));
    }
}

#[test]
fn from_tracing_level_maps_all_variants() {
    assert_eq!(LogLevel::from(tracing::Level::TRACE), LogLevel::Trace);
    assert_eq!(LogLevel::from(tracing::Level::DEBUG), LogLevel::Debug);
    assert_eq!(LogLevel::from(tracing::Level::INFO), LogLevel::Info);
    assert_eq!(LogLevel::from(tracing::Level::WARN), LogLevel::Warn);
    assert_eq!(LogLevel::from(tracing::Level::ERROR), LogLevel::Error);
}

#[test]
fn field_set_round_trips_through_json() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let field_set = LogFieldSet::new(now(), LogLevel::Info, "target", "hello").with_job_id(job_id);

    let json = serde_json::to_value(&field_set).expect("serialize field set");
    let round_trip: LogFieldSet = serde_json::from_value(json).expect("deserialize field set");
    assert_eq!(round_trip, field_set);
}
