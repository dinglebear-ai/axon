use super::*;

fn web_source(id: &str, uri: &str) -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new(id),
        canonical_uri: uri.to_string(),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        ..source()
    }
}

fn list_request(
    source_kind: Option<SourceKind>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> SourceListRequest {
    SourceListRequest {
        source_kind,
        adapter: None,
        status: None,
        authority: None,
        watch_enabled: None,
        tag: None,
        query: None,
        limit,
        cursor,
    }
}

#[tokio::test]
async fn sqlite_ledger_list_sources_filters_by_kind() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert local"); // Local, src_sqlite
    store
        .upsert_source(web_source("src_web", "https://docs.example.com"))
        .await
        .expect("upsert web");

    let all = store
        .list_sources(list_request(None, None, None))
        .await
        .expect("list all");
    assert_eq!(all.total, Some(2));
    assert_eq!(all.items.len(), 2);

    let web_only = store
        .list_sources(list_request(Some(SourceKind::Web), None, None))
        .await
        .expect("list web");
    assert_eq!(web_only.total, Some(1));
    assert_eq!(web_only.items[0].source_id, SourceId::new("src_web"));
}

#[tokio::test]
async fn sqlite_ledger_list_sources_paginates_with_cursor() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    for id in ["src_1", "src_2", "src_3"] {
        store
            .upsert_source(web_source(id, &format!("https://{id}.example.com")))
            .await
            .expect("upsert source");
    }

    let first = store
        .list_sources(list_request(None, Some(2), None))
        .await
        .expect("first page");
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.total, Some(3));
    let cursor = first.next_cursor.clone().expect("more pages remain");

    let second = store
        .list_sources(list_request(None, Some(2), Some(cursor)))
        .await
        .expect("second page");
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.next_cursor, None);

    let mut seen = first
        .items
        .iter()
        .chain(second.items.iter())
        .map(|s| s.source_id.0.clone())
        .collect::<Vec<_>>();
    seen.sort();
    assert_eq!(seen, vec!["src_1", "src_2", "src_3"]);
}
