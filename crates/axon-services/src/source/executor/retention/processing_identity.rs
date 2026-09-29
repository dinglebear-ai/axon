//! Secret-free semantic identity of prepared output, independent of scan coverage.
use crate::config_snapshot_hash::{JobConfigSnapshot, config_snapshot_id_from_json};
use axon_api::source::*;
use axon_document::{DocumentPreparerConfig, preparer::PREPARATION_SCHEMA_VERSION};

pub(in crate::source::executor) fn processing_identity(
    runtime: &crate::context::TargetLocalSourceRuntime,
    input: &super::super::SourcePipelineInput<'_>,
) -> ConfigSnapshotId {
    let snapshot = JobConfigSnapshot {
        source_kind: input.adapter.name(),
        source_ref: &input.plan.route.source.canonical_uri,
        collection: input.collection,
        embedding_provider_id: &runtime.embedding_provider_id.0,
        vector_provider_id: &runtime.vector_provider_id.0,
        embedding_model: &runtime.embedding_model,
        embedding_dimensions: runtime.embedding_dimensions,
        embed: input.plan.request.embed,
        max_items: None,
    };
    configured_identity(
        snapshot,
        &runtime.document_preparer,
        runtime.document_prepare_max_in_flight_bytes,
        &input.plan,
    )
}

fn configured_identity(
    snapshot: JobConfigSnapshot<'_>,
    preparer: &axon_document::DocumentPreparer,
    max_in_flight_bytes: usize,
    plan: &SourcePlan,
) -> ConfigSnapshotId {
    let mut config = preparer.semantic_config();
    config.max_content_bytes = super::super::preparation::effective_content_limit(
        preparer,
        max_in_flight_bytes,
        plan.limits.effective.max_bytes_per_item,
    );
    identity(
        snapshot,
        config,
        local_file_limit(plan),
        PREPARATION_SCHEMA_VERSION,
    )
}

fn local_file_limit(plan: &SourcePlan) -> Option<u64> {
    if plan.route.source.source_kind != SourceKind::Local {
        return None;
    }
    let configured = plan
        .route
        .validated_options
        .values
        .get("max_file_bytes")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(axon_adapters::DEFAULT_LOCAL_MAX_FILE_BYTES);
    Some(
        configured
            .min(plan.limits.effective.max_bytes_per_item.unwrap_or(u64::MAX))
            .min(axon_adapters::acquisition::MAX_FILE_CONTENT_BYTES),
    )
}

fn identity(
    mut snapshot: JobConfigSnapshot<'_>,
    config: DocumentPreparerConfig,
    local_file_limit: Option<u64>,
    schema: &str,
) -> ConfigSnapshotId {
    snapshot.max_items = None;
    let material = serde_json::json!({
        "source_and_providers": snapshot.canonical_material(),
        "preparation_schema": schema,
        "preparation": {
            "max_content_bytes": config.max_content_bytes,
            "markdown_max_chars": config.markdown_max_chars,
            "markdown_min_chars": config.markdown_min_chars,
            "markdown_overlap_chars": config.markdown_overlap_chars,
        },
        "local_file_limit": local_file_limit,
    });
    config_snapshot_id_from_json(&material.to_string())
}

#[cfg(test)]
#[path = "processing_identity_tests.rs"]
mod tests;
