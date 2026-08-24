use super::*;

#[test]
fn projection_events_exclude_raw_inputs_and_credentials() {
    let event = batch_lifecycle_event(
        BatchId::new(uuid::Uuid::new_v4()),
        ProjectionOperation::Ingest,
        2,
        LifecycleStatus::Queued,
        "projection batch accepted",
    );
    let json = serde_json::to_string(&event).unwrap();
    for secret in ["https://", "/home/", "bearer", "token", "idempotency"] {
        assert!(!json.to_ascii_lowercase().contains(secret));
    }
}
