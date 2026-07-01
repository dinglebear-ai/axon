use axon_api::source::*;

use crate::sqlite::SqliteLedgerStore;
use crate::store::LedgerStore;

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
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
        last_job_id: None,
    }
}

#[tokio::test]
async fn sqlite_source_round_trips() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let source = source();

    store
        .upsert_source(source.clone())
        .await
        .expect("upsert source");

    let stored = store
        .get_source(SourceId::new("src_sqlite"))
        .await
        .expect("get source");

    assert_eq!(stored, Some(source));
}
