use axon_api::source::*;

use crate::sqlite::SqliteLedgerStore;
use crate::store::LedgerStore;

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn ts_at(second: u32) -> Timestamp {
    Timestamp(format!("2026-07-01T00:00:{second:02}Z"))
}

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("src_sqlite"),
        canonical_uri: "file:///repo".to_string(),
        display_name: "repo".to_string(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 1,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 1,
            vector_points_total: 1,
            bytes_total: 12,
        },
        created_at: ts(),
        updated_at: ts(),
        tags: vec!["sqlite".to_string()],
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        user_label: None,
    }
}

fn manifest(hash: &str) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_sqlite"),
        generation: SourceGenerationId::new(format!("gen_{hash}")),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items: vec![manifest_item("src/lib.rs", hash)],
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn manifest_with_items(generation: &str, items: Vec<ManifestItem>) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_sqlite"),
        generation: SourceGenerationId::new(generation),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items,
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn manifest_item(path: &str, hash: &str) -> ManifestItem {
    ManifestItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new(path),
        canonical_uri: format!("file:///repo/{path}"),
        item_kind: ItemKind::LocalFile,
        content_kind: Some(ContentKind::Code),
        display_path: Some(path.to_string()),
        parent_key: None,
        size_bytes: Some(12),
        content_hash: Some(hash.to_string()),
        mtime: Some(ts()),
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    }
}

fn completed_generation_for_manifest(manifest: &SourceManifest) -> SourceGeneration {
    SourceGeneration {
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        status: LifecycleStatus::Completed,
        publish_state: PublishState::Writing,
        created_at: ts(),
        published_at: None,
        item_counts: ItemCounts {
            added: manifest.items.len() as u64,
            modified: 0,
            removed: 0,
            unchanged: 0,
            failed: 0,
        },
        document_counts: DocumentCounts {
            discovered: manifest.items.len() as u64,
            prepared: 0,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        cleanup_debt: Vec::new(),
        previous_generation: None,
    }
}

fn completed_generation_from(generation: &SourceGeneration) -> SourceGeneration {
    let mut generation = generation.clone();
    generation.status = LifecycleStatus::Completed;
    generation.publish_state = PublishState::Writing;
    generation
}

fn publish_request(generation: &SourceGeneration) -> PublishGenerationRequest {
    PublishGenerationRequest {
        source_id: generation.source_id.clone(),
        generation: generation.generation.clone(),
        expected_previous_generation: generation.previous_generation.clone(),
    }
}

async fn complete_and_publish(
    store: &SqliteLedgerStore,
    generation: SourceGeneration,
) -> SourceGeneration {
    let completed = store
        .complete_generation(generation)
        .await
        .expect("complete generation");
    store
        .publish_generation(publish_request(&completed))
        .await
        .expect("publish generation")
}

fn lease_request(key: &str, owner: &str) -> LeaseRequest {
    lease_request_ttl(key, owner, 30)
}

fn lease_request_ttl(key: &str, owner: &str, ttl_seconds: u64) -> LeaseRequest {
    LeaseRequest {
        lease_key: key.to_string(),
        owner_id: owner.to_string(),
        ttl_seconds,
        job_id: None,
        metadata: MetadataMap::new(),
    }
}

#[path = "sqlite_tests/sqlite_document_cleanup_tests.rs"]
mod sqlite_document_cleanup_tests;
#[path = "sqlite_tests/sqlite_generation_tests.rs"]
mod sqlite_generation_tests;
#[path = "sqlite_tests/sqlite_lease_tests.rs"]
mod sqlite_lease_tests;
#[path = "sqlite_tests/sqlite_listing_tests.rs"]
mod sqlite_listing_tests;
#[path = "sqlite_tests/sqlite_source_manifest_tests.rs"]
mod sqlite_source_manifest_tests;
