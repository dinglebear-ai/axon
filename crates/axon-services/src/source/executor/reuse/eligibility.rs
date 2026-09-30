//! Prepared reuse requires both policy compatibility and committed status evidence.
use super::*;

pub(super) async fn prepared_reuse_keys(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
) -> anyhow::Result<BTreeSet<SourceItemKey>> {
    let Some(previous) = &diff.previous_generation else {
        return Ok(BTreeSet::new());
    };
    let compatible = runtime
        .ledger
        .get_manifest_metadata(diff.source_id.clone(), previous.clone())
        .await?
        .is_some_and(|metadata| {
            super::super::helpers::publication_config_metadata_matches(
                &metadata,
                &super::super::retention::processing_identity(runtime, input),
            )
        });
    if !compatible {
        return Ok(BTreeSet::new());
    }
    let keys = diff
        .modified
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect();
    let statuses = runtime
        .ledger
        .document_statuses_for_items(diff.source_id.clone(), keys)
        .await?;
    let mut grouped = BTreeMap::<SourceItemKey, Vec<DocumentStatus>>::new();
    for status in statuses {
        grouped
            .entry(status.source_item_key.clone())
            .or_default()
            .push(status);
    }
    Ok(grouped
        .into_iter()
        .filter_map(|(key, rows)| {
            rows.iter()
                .all(|status| super::super::retention::eligible(status, previous))
                .then_some(key)
        })
        .collect())
}
