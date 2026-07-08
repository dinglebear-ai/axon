use axon_api::source::*;

use crate::store::{FakeLedgerStore, LedgerStore};

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn ts_at(second: u32) -> Timestamp {
    Timestamp(format!("2026-07-01T00:00:{second:02}Z"))
}

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("src_a"),
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
        tags: Vec::new(),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        user_label: None,
    }
}

fn manifest(hash: &str) -> SourceManifest {
    manifest_with_freshness(hash, None, ts())
}

fn manifest_with_freshness(hash: &str, version: Option<&str>, mtime: Timestamp) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_a"),
        generation: SourceGenerationId::new(format!("gen_{hash}")),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items: vec![ManifestItem {
            source_id: SourceId::new("src_a"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            canonical_uri: "file:///repo/src/lib.rs".to_string(),
            item_kind: ItemKind::LocalFile,
            content_kind: Some(ContentKind::Code),
            display_path: Some("src/lib.rs".to_string()),
            parent_key: None,
            size_bytes: Some(12),
            content_hash: Some(hash.to_string()),
            mtime: Some(mtime),
            version: version.map(str::to_string),
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        }],
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn manifest_for_generation(generation: &SourceGeneration, hash: &str) -> SourceManifest {
    let mut manifest = manifest(hash);
    manifest.generation = generation.generation.clone();
    manifest
}

fn manifest_with_items(generation: &str, items: Vec<ManifestItem>) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_a"),
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
        source_id: SourceId::new("src_a"),
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

fn completed_generation(mut generation: SourceGeneration) -> SourceGeneration {
    generation.status = LifecycleStatus::Completed;
    generation.publish_state = PublishState::Writing;
    generation
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

fn publish_request(generation: &SourceGeneration) -> PublishGenerationRequest {
    PublishGenerationRequest {
        source_id: generation.source_id.clone(),
        generation: generation.generation.clone(),
        expected_previous_generation: generation.previous_generation.clone(),
    }
}

async fn complete_and_publish(
    ledger: &FakeLedgerStore,
    generation: SourceGeneration,
) -> SourceGeneration {
    let completed = ledger
        .complete_generation(generation)
        .await
        .expect("complete generation");
    ledger
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

#[path = "store_tests/store_document_cleanup_tests.rs"]
mod store_document_cleanup_tests;
#[path = "store_tests/store_lease_tests.rs"]
mod store_lease_tests;
#[path = "store_tests/store_manifest_tests.rs"]
mod store_manifest_tests;
