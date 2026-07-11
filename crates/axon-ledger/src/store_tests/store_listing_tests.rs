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

#[tokio::test]
async fn fake_ledger_list_sources_filters_by_kind() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap(); // Local, src_a
    ledger
        .upsert_source(web_source("src_web", "https://docs.example.com"))
        .await
        .unwrap();

    let all = ledger
        .list_sources(SourceListRequest {
            source_kind: None,
            adapter: None,
            status: None,
            authority: None,
            watch_enabled: None,
            tag: None,
            query: None,
            limit: None,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(all.total, Some(2));
    assert_eq!(all.items.len(), 2);

    let web_only = ledger
        .list_sources(SourceListRequest {
            source_kind: Some(SourceKind::Web),
            adapter: None,
            status: None,
            authority: None,
            watch_enabled: None,
            tag: None,
            query: None,
            limit: None,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(web_only.total, Some(1));
    assert_eq!(web_only.items[0].source_id, SourceId::new("src_web"));
}

#[tokio::test]
async fn fake_ledger_list_sources_paginates_with_cursor() {
    let ledger = FakeLedgerStore::new();
    for id in ["src_1", "src_2", "src_3"] {
        ledger
            .upsert_source(web_source(id, &format!("https://{id}.example.com")))
            .await
            .unwrap();
    }

    let first = ledger
        .list_sources(SourceListRequest {
            source_kind: None,
            adapter: None,
            status: None,
            authority: None,
            watch_enabled: None,
            tag: None,
            query: None,
            limit: Some(2),
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.total, Some(3));
    let cursor = first.next_cursor.clone().expect("more pages remain");

    let second = ledger
        .list_sources(SourceListRequest {
            source_kind: None,
            adapter: None,
            status: None,
            authority: None,
            watch_enabled: None,
            tag: None,
            query: None,
            limit: Some(2),
            cursor: Some(cursor),
        })
        .await
        .unwrap();
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
