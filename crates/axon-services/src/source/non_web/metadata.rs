use axon_api::source::*;

use super::{NonWebPipelineInput, timestamp};

pub(super) fn source_summary(
    input: &NonWebPipelineInput<'_>,
    status: LifecycleStatus,
    counts: SourceCounts,
    previous: Option<&SourceSummary>,
) -> SourceSummary {
    SourceSummary {
        source_id: input.plan.route.source.source_id.clone(),
        canonical_uri: input.plan.route.source.canonical_uri.clone(),
        display_name: input.plan.route.source.canonical_uri.clone(),
        source_kind: input.plan.route.source.source_kind,
        adapter: input.plan.route.adapter.clone(),
        authority: input.plan.route.source.authority,
        status,
        counts,
        created_at: previous
            .map(|source| source.created_at.clone())
            .unwrap_or_else(timestamp),
        updated_at: timestamp(),
        graph_node_ids: previous
            .map(|source| source.graph_node_ids.clone())
            .unwrap_or_default(),
        last_refreshed_at: if status == LifecycleStatus::Completed {
            Some(timestamp())
        } else {
            previous.and_then(|source| source.last_refreshed_at.clone())
        },
        user_label: previous.and_then(|source| source.user_label.clone()),
        tags: previous
            .map(|source| source.tags.clone())
            .unwrap_or_default(),
        watch_id: previous.and_then(|source| source.watch_id.clone()),
        last_job_id: Some(input.plan.job_id),
    }
}

pub(super) fn sanitize_documents(kind: SourceKind, documents: &mut [SourceDocument]) {
    for document in documents {
        if kind == SourceKind::Registry {
            remap_registry_metadata(document);
        } else if kind == SourceKind::Session {
            retain_session_metadata(document);
        }
    }
}

fn remap_registry_metadata(document: &mut SourceDocument) {
    let ecosystem = document.metadata.remove("pkg_registry");
    let name = document.metadata.remove("pkg_name");
    let version = document.metadata.remove("pkg_version");
    document.metadata.retain(|key, _| !key.starts_with("pkg_"));
    document
        .metadata
        .insert("source_family".to_string(), serde_json::json!("package"));
    for (key, value) in [
        ("package_ecosystem", ecosystem),
        ("package_name", name),
        ("package_version", version),
    ] {
        if let Some(value) = value {
            document.metadata.insert(key.to_string(), value);
        }
    }
}

fn retain_session_metadata(document: &mut SourceDocument) {
    const ALLOWED: &[&str] = &[
        "session_provider",
        "session_id",
        "session_turn_index",
        "session_tool_name",
        "session_skill_name",
    ];
    document.metadata.retain(|key, _| {
        axon_vectors::payload::VECTOR_SHARED_FIELDS.contains(&key.as_str())
            || ALLOWED.contains(&key.as_str())
    });
}
