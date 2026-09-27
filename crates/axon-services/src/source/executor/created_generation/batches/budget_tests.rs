use super::*;
use axon_adapters::{FakeSourceAdapter, SourceAdapter};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};

async fn fixture() -> (SourcePlan, SourceManifestDiff, SourceAcquisition) {
    let request = SourceRequest::new("https://example.com/budget".to_owned());
    let route = crate::source::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    let mut plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        false,
        None,
        None,
    );
    let mut adapter = FakeSourceAdapter::new(plan.route.adapter.clone());
    for i in 0..5 {
        adapter = adapter.with_item(format!("item-{i}"), ContentKind::PlainText, "body");
    }
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = FakeLedgerStore::default()
        .diff_manifest(manifest)
        .await
        .unwrap();
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    plan.route.source.source_kind = SourceKind::Git;
    (plan, diff, acquisition)
}

#[tokio::test]
async fn byte_batches_keep_unknown_sizes_single_and_preserve_inventory() {
    let (_, mut diff, _) = fixture().await;
    let sizes = [Some(4), Some(7), None, Some(2), Some(2)];
    for (item, size) in diff.added.iter_mut().zip(sizes) {
        item.size_bytes = size;
    }
    let expected = diff
        .added
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect::<Vec<_>>();
    let batches = file_batches(&diff, 16, 16, 10).collect::<Vec<_>>();
    assert_eq!(
        batches
            .iter()
            .map(|batch| batch.added.len())
            .collect::<Vec<_>>(),
        [1, 1, 1, 2]
    );
    assert_eq!(
        batches
            .into_iter()
            .flat_map(|batch| batch.added)
            .map(|item| item.source_item_key)
            .collect::<Vec<_>>(),
        expected
    );
}

#[tokio::test]
async fn actual_reads_are_charged_once_across_batches() {
    let (mut plan, _, mut acquisition) = fixture().await;
    plan.limits.effective.max_total_bytes = Some(11);
    let mut budget = AcquisitionBudget::new(&plan, 1024);
    acquisition.header.counts.bytes_done = 7;
    budget.charge(&acquisition).unwrap();
    let next = budget.plan(&plan);
    assert_eq!(next.limits.effective.max_total_bytes, Some(4));
    assert_eq!(plan.limits.effective.max_total_bytes, Some(11));
    acquisition.header.counts.bytes_done = 5;
    assert!(budget.charge(&acquisition).is_err());
    acquisition.header.counts.bytes_done = 4;
    budget.charge(&acquisition).unwrap();
    assert_eq!(budget.plan(&plan).limits.effective.max_total_bytes, Some(0));
}

#[tokio::test]
async fn omitted_total_still_has_finite_batch_and_preparation_caps() {
    let (plan, _, mut acquisition) = fixture().await;
    let mut budget = AcquisitionBudget::new(&plan, 100);
    let bounded = budget.plan(&plan);
    assert_eq!(bounded.limits.effective.max_total_bytes, None);
    assert_eq!(bounded.limits.effective.max_bytes_per_item, Some(20));
    assert_eq!(
        bounded.route.source.metadata[ACQUISITION_BATCH_BYTES_KEY],
        serde_json::json!(DEFAULT_ACQUISITION_BATCH_BYTES)
    );
    acquisition.header.counts.bytes_done = DEFAULT_ACQUISITION_BATCH_BYTES + 1;
    assert!(budget.charge(&acquisition).is_err());
}

#[tokio::test]
async fn file_batches_have_one_final_batch_even_when_bytes_split_item_groups() {
    let (plan, mut diff, _) = fixture().await;
    for item in &mut diff.added {
        item.size_bytes = Some(DEFAULT_ACQUISITION_BATCH_BYTES);
    }
    let batches = changed_batches(&plan, &diff, 16, 16).collect::<Vec<_>>();
    assert_eq!(batches.len(), 5);
    assert!(batches[..4].iter().all(|batch| !batch.is_final));
    assert!(batches[4].is_final);
}
