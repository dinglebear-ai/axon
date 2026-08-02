use axon_api::source::*;

use super::{SourcePipelineInput, timestamp};

pub(super) fn source_summary(
    input: &SourcePipelineInput<'_>,
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
        last_refreshed_at: if matches!(
            status,
            LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded
        ) {
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
