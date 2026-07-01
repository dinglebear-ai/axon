use axon_api::source::*;

use crate::store::{FakeMemoryStore, MemoryStore};

fn request(body: &str) -> MemoryRequest {
    MemoryRequest {
        memory_type: MemoryType::Fact,
        body: body.to_string(),
        confidence: 0.8,
        salience: 0.7,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        title: Some("fact".to_string()),
        tags: vec!["test".to_string()],
        links: Vec::new(),
        decay: None,
        embed: true,
        visibility: Some(Visibility::Internal),
    }
}

#[tokio::test]
async fn fake_memory_store_remembers_gets_searches_and_contextualizes() {
    let store = FakeMemoryStore::new();
    let remembered = store
        .remember(request("Axon owns a source ledger"))
        .await
        .unwrap();

    let record = store
        .get(remembered.memory_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.body, "Axon owns a source ledger");

    let search = store
        .search(MemorySearchRequest {
            query: "source ledger".to_string(),
            limit: 5,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(search.results.len(), 1);

    let context = store
        .context(MemoryContextRequest {
            token_budget: 512,
            query: Some("ledger".to_string()),
            source_id: None,
            graph_node_id: None,
            filters: MetadataMap::new(),
            depth: None,
            include_working: false,
        })
        .await
        .unwrap();
    assert!(context.context.contains("source ledger"));

    let constrained = store
        .context(MemoryContextRequest {
            token_budget: 2,
            query: Some("ledger".to_string()),
            source_id: None,
            graph_node_id: None,
            filters: MetadataMap::new(),
            depth: None,
            include_working: false,
        })
        .await
        .unwrap();
    assert_eq!(constrained.token_estimate, 2);
    assert_eq!(constrained.exclusions, vec!["token_budget"]);
}

#[tokio::test]
async fn fake_memory_store_links_reinforces_and_reports_capabilities() {
    let store = FakeMemoryStore::new();
    let remembered = store
        .remember(request("Axon can reinforce memory"))
        .await
        .unwrap();
    let linked = store
        .link(MemoryLinkRequest {
            memory_id: remembered.memory_id.clone(),
            link: MemoryLink {
                link_type: "relates_to".to_string(),
                target: "graph://axon".to_string(),
                confidence: 0.9,
                evidence: Vec::new(),
            },
        })
        .await
        .unwrap();
    assert_eq!(linked.memory_id, remembered.memory_id);

    let reinforced = store
        .reinforce(
            remembered.memory_id.clone(),
            MemoryReinforcement {
                amount: 0.2,
                reason: "used in context".to_string(),
                timestamp: Timestamp("2026-07-01T00:00:00Z".to_string()),
            },
        )
        .await
        .unwrap();
    assert!(reinforced.memory_score > remembered.memory_score);

    let capability = store.capabilities().await.unwrap();
    assert_eq!(capability.0.owner_crate, "axon-memory");

    store.reset().await.unwrap();
    assert!(store.get(remembered.memory_id).await.unwrap().is_none());
}

#[tokio::test]
async fn fake_memory_store_rejects_unsupported_search_and_context_options() {
    let store = FakeMemoryStore::new();
    store
        .remember(request("Axon records graph facts"))
        .await
        .unwrap();

    let mut filters = MetadataMap::new();
    filters.insert("scope".to_string(), serde_json::json!("axon"));
    let err = store
        .search(MemorySearchRequest {
            query: "graph".to_string(),
            limit: 5,
            filters,
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "memory.unsupported_option");

    let err = store
        .context(MemoryContextRequest {
            token_budget: 512,
            query: Some("graph".to_string()),
            source_id: Some(SourceId::new("src")),
            graph_node_id: None,
            filters: MetadataMap::new(),
            depth: None,
            include_working: false,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "memory.unsupported_option");
}
