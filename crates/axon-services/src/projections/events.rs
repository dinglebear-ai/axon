use axon_api::source::*;
/// Build a redacted batch lifecycle event. Raw inputs and credentials are
/// intentionally not accepted by this interface.
pub fn batch_lifecycle_event(
    batch_id: BatchId,
    operation: ProjectionOperation,
    item_count: usize,
    status: LifecycleStatus,
    message: &str,
) -> SourceProgressEvent {
    let mut event = SourceProgressEvent::minimal(
        JobId::new(uuid::Uuid::nil()),
        0,
        PipelinePhase::Queued,
        status,
        Severity::Info,
        format!("{message}; operation={operation:?}; items={item_count}"),
    );
    event.batch_id = Some(batch_id);
    event
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
