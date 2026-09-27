use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// Isolated by a unique source ID, so unrelated concurrent ledger tests cannot
// affect the count. The production query boundary calls this only in test builds.
static QUERIES: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

pub(crate) fn record_query(source: &SourceId) {
    if let Some(count) = QUERIES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .get_mut(&source.0)
    {
        *count += 1;
    }
}

struct QueryCounter(SourceId);

impl QueryCounter {
    fn new(source: SourceId) -> Self {
        QUERIES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(source.0.clone(), 0);
        Self(source)
    }

    fn take(&self) -> usize {
        let mut counts = QUERIES.get().unwrap().lock().unwrap();
        std::mem::take(counts.get_mut(&self.0.0).unwrap())
    }
}

impl Drop for QueryCounter {
    fn drop(&mut self) {
        QUERIES.get().unwrap().lock().unwrap().remove(&self.0.0);
    }
}

fn status(source: &SourceId, generation: &SourceGenerationId, index: usize) -> DocumentStatus {
    DocumentStatus {
        document_id: DocumentId::new(format!("{}-doc-{index:04}", source.0)),
        source_id: source.clone(),
        source_item_key: SourceItemKey::new(format!("item-{index:04}")),
        generation: Some(generation.clone()),
        status: DocumentLifecycleStatus::Published,
        updated_at: ts(),
        chunk_count: index as u32,
        vector_point_count: index as u32,
        error: None,
        cleanup_status: None,
    }
}

async fn seed_inventory(
    store: &SqliteLedgerStore,
    source: &SourceId,
    generation: &SourceGenerationId,
    count: usize,
) {
    let mut inventory = manifest("bulk");
    inventory.source_id = source.clone();
    inventory.generation = generation.clone();
    inventory.items = (0..count)
        .map(|index| {
            let mut item = manifest_item(&format!("item-{index:04}"), "bulk-hash");
            item.source_id = source.clone();
            item
        })
        .collect();
    store.put_manifest(inventory).await.unwrap();
}

#[tokio::test]
async fn bulk_lookup_batches_401_unique_keys_and_preserves_all_siblings() {
    let store = SqliteLedgerStore::in_memory().await.unwrap();
    let id = SourceId::new(format!("bulk-{}", uuid::Uuid::new_v4()));
    let mut summary = source();
    summary.source_id = id.clone();
    store.upsert_source(summary.clone()).await.unwrap();
    let generation = store
        .create_generation(id.clone())
        .await
        .unwrap()
        .generation;
    let mut expected = (0..401)
        .map(|i| status(&id, &generation, i))
        .collect::<Vec<_>>();
    let keys = expected
        .iter()
        .map(|s| s.source_item_key.clone())
        .collect::<Vec<_>>();
    for index in [0, 200, 400] {
        let mut sibling = expected[index].clone();
        sibling.document_id.0.push_str("-sibling");
        sibling.status = DocumentLifecycleStatus::Skipped;
        sibling.chunk_count = 0;
        sibling.vector_point_count = 0;
        expected.push(sibling);
    }
    let mut seeded = expected.clone();
    seeded.push(status(&id, &generation, 401)); // Unrequested key in the same source.
    summary.source_id = SourceId::new(format!("{}-other", id.0));
    summary.canonical_uri.push_str("/other");
    store.upsert_source(summary.clone()).await.unwrap();
    let other_generation = store
        .create_generation(summary.source_id.clone())
        .await
        .unwrap()
        .generation;
    seeded.push(status(&summary.source_id, &other_generation, 0)); // Requested key, wrong source.
    seed_inventory(&store, &id, &generation, 402).await;
    seed_inventory(&store, &summary.source_id, &other_generation, 1).await;
    store.update_document_statuses(seeded).await.unwrap();
    expected.sort_by(|a, b| a.document_id.cmp(&b.document_id));

    let counter = QueryCounter::new(id.clone());
    let first = store
        .document_statuses_for_items(id.clone(), keys[..400].to_vec())
        .await
        .unwrap();
    let first_expected = expected
        .iter()
        .filter(|s| s.source_item_key != keys[400])
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(first, first_expected);
    assert_eq!(counter.take(), 1, "400 keys fit in one SQL execution");

    let duplicated = keys.iter().chain(&keys).cloned().collect();
    let actual = store
        .document_statuses_for_items(id.clone(), duplicated)
        .await
        .unwrap();
    assert_eq!(
        actual, expected,
        "all siblings, counts and statuses must survive batching"
    );
    assert_eq!(
        counter.take(),
        2,
        "401 unique keys require two queries, despite 802 input keys"
    );

    let missing = store
        .document_statuses_for_items(id.clone(), vec![SourceItemKey::new("missing")])
        .await
        .unwrap();
    assert!(missing.is_empty());
    assert_eq!(counter.take(), 1);
    assert!(
        store
            .document_statuses_for_items(id, vec![])
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(counter.take(), 0, "empty input must not execute SQL");
}
