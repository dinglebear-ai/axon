use super::{IndexCounts, adapter_ref, to_source_result};
use axon_api::source::{
    GraphWriteSummary, JobId, LifecycleStatus, Severity, SourceGenerationId, SourceId, SourceKind,
    SourceScope, SourceWarning,
};

fn counts(warnings: Vec<SourceWarning>) -> IndexCounts {
    IndexCounts {
        job_id: JobId::new(uuid::Uuid::nil()),
        source_id: SourceId::new("src_result_status"),
        generation: SourceGenerationId::new("gen_result_status"),
        items_discovered: 1,
        documents_prepared: 1,
        chunks_prepared: 1,
        vector_points_written: 1,
        removed: 0,
        graph_candidates: Vec::new(),
        warnings,
        artifacts: Vec::new(),
        inline: None,
    }
}

fn graph() -> GraphWriteSummary {
    GraphWriteSummary {
        nodes_upserted: 0,
        edges_upserted: 0,
        evidence_records: 0,
        degraded: false,
    }
}

#[test]
fn warning_bearing_source_result_is_completed_degraded() {
    let result = to_source_result(
        SourceKind::Web,
        adapter_ref("web"),
        SourceScope::Page,
        "https://example.test".to_string(),
        counts(vec![SourceWarning {
            code: "source.test.degraded".to_string(),
            severity: Severity::Warning,
            message: "deferred cleanup".to_string(),
            source_item_key: None,
            retryable: true,
        }]),
        graph(),
    );

    assert_eq!(result.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(result.ledger.status, LifecycleStatus::CompletedDegraded);
}

#[test]
fn warning_free_source_result_is_completed() {
    let result = to_source_result(
        SourceKind::Web,
        adapter_ref("web"),
        SourceScope::Page,
        "https://example.test".to_string(),
        counts(Vec::new()),
        graph(),
    );

    assert_eq!(result.status, LifecycleStatus::Completed);
    assert_eq!(result.ledger.status, LifecycleStatus::Completed);
}

#[test]
fn fallback_source_counts_preserve_discovered_items() {
    let mut counts = counts(Vec::new());
    counts.items_discovered = 7;
    counts.documents_prepared = 3;

    let result = to_source_result(
        SourceKind::Web,
        adapter_ref("web"),
        SourceScope::Site,
        "https://example.test".to_string(),
        counts,
        graph(),
    );

    assert_eq!(result.counts.items_total, 7);
    assert_eq!(result.counts.documents_total, 3);
}
