use crate::crates::jobs::common::{resolve_test_qdrant_url, test_config};
use std::collections::HashSet;
use std::error::Error;
use uuid::Uuid;

use super::client::{
    qdrant_delete_by_url_filter, qdrant_delete_stale_domain_urls, qdrant_domain_facets,
    qdrant_retrieve_by_url, qdrant_scroll_pages, qdrant_scroll_pages_with_vectors, qdrant_search,
    qdrant_url_facets,
};

/// Helper: create an isolated test collection via the Qdrant REST API.
async fn create_test_collection(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    dim: usize,
) -> Result<(), Box<dyn Error>> {
    client
        .put(format!("{base}/collections/{name}"))
        .json(&serde_json::json!({
            "vectors": {"size": dim, "distance": "Cosine"}
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Helper: create a keyword payload index on the given field (required for /facet).
async fn create_keyword_index(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    field: &str,
) -> Result<(), Box<dyn Error>> {
    client
        .put(format!("{base}/collections/{name}/index?wait=true"))
        .json(&serde_json::json!({
            "field_name": field,
            "field_schema": "keyword"
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Helper: upsert points into a collection via the Qdrant REST API.
async fn upsert_points(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    points: serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    client
        .put(format!("{base}/collections/{name}/points?wait=true"))
        .json(&serde_json::json!({"points": points}))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Helper: delete a test collection (best-effort cleanup).
async fn delete_collection(client: &reqwest::Client, base: &str, name: &str) {
    let _ = client
        .delete(format!("{base}/collections/{name}"))
        .send()
        .await;
}

/// `qdrant_url_facets` must return correct (url, chunk_count) pairs for the indexed data.
#[tokio::test]
async fn qdrant_url_facets_returns_correct_shape() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;
    create_keyword_index(&client, &base, &cfg.collection, "url").await?;

    // 2 points for url-a, 3 points for url-b.
    let points = serde_json::json!([
        {"id": Uuid::new_v4().to_string(), "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"url": "https://url-a.example"}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.9f32, 0.1, 0.0, 0.0], "payload": {"url": "https://url-a.example"}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 1.0, 0.0, 0.0], "payload": {"url": "https://url-b.example"}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 0.9, 0.1, 0.0], "payload": {"url": "https://url-b.example"}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 0.8, 0.2, 0.0], "payload": {"url": "https://url-b.example"}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    let facets = qdrant_url_facets(&cfg, 100).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    let url_a = facets.iter().find(|(u, _)| u == "https://url-a.example");
    let url_b = facets.iter().find(|(u, _)| u == "https://url-b.example");
    assert!(url_a.is_some(), "url-a must appear in facets");
    assert!(url_b.is_some(), "url-b must appear in facets");
    assert_eq!(url_a.unwrap().1, 2, "url-a must have 2 chunks");
    assert_eq!(url_b.unwrap().1, 3, "url-b must have 3 chunks");
    Ok(())
}

/// Upsert a point then search with its own vector — top result must match.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "integration test — requires running Qdrant; run with cargo test -- --ignored"]
async fn upsert_and_search_roundtrip() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;

    let target_url = "https://roundtrip.example/page";
    let vector = [1.0f32, 0.0, 0.0, 0.0];
    let point_id = Uuid::new_v4().to_string();

    let points = serde_json::json!([{
        "id": point_id,
        "vector": vector,
        "payload": {
            "url": target_url,
            "chunk_text": "roundtrip test content",
        }
    }]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    let hits = qdrant_search(&cfg, &vector, 1, None).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(hits.len(), 1, "search must return exactly one hit");
    assert_eq!(
        hits[0].payload.url, target_url,
        "top hit payload url must match the upserted point"
    );
    Ok(())
}

/// `qdrant_scroll_pages` must invoke the callback for every point in the collection.
#[tokio::test]
async fn qdrant_scroll_pages_visits_all_inserted_points() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;

    let points = serde_json::json!([
        {"id": Uuid::new_v4().to_string(), "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"url": "https://scroll-test.example", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.9f32, 0.1, 0.0, 0.0], "payload": {"url": "https://scroll-test.example", "chunk_index": 1}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.8f32, 0.2, 0.0, 0.0], "payload": {"url": "https://scroll-test.example", "chunk_index": 2}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.7f32, 0.3, 0.0, 0.0], "payload": {"url": "https://scroll-test.example", "chunk_index": 3}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.6f32, 0.4, 0.0, 0.0], "payload": {"url": "https://scroll-test.example", "chunk_index": 4}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    let mut collected: Vec<serde_json::Value> = Vec::new();
    qdrant_scroll_pages(&cfg, |page| {
        collected.extend(page.iter().cloned());
    })
    .await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(
        collected.len(),
        5,
        "scroll must visit all 5 inserted points"
    );
    Ok(())
}

/// `qdrant_retrieve_by_url` must return only points whose payload url matches exactly.
#[tokio::test]
async fn qdrant_retrieve_by_url_returns_only_matching_points() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;

    let points = serde_json::json!([
        {"id": Uuid::new_v4().to_string(), "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"url": "https://retrieve-a.example", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.9f32, 0.1, 0.0, 0.0], "payload": {"url": "https://retrieve-a.example", "chunk_index": 1}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 1.0, 0.0, 0.0], "payload": {"url": "https://retrieve-b.example", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 0.9, 0.1, 0.0], "payload": {"url": "https://retrieve-b.example", "chunk_index": 1}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    let result = qdrant_retrieve_by_url(&cfg, "https://retrieve-a.example", None).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(
        result.len(),
        2,
        "must return exactly 2 points for retrieve-a"
    );
    for point in &result {
        assert_eq!(
            point.payload.url, "https://retrieve-a.example",
            "every returned point must have the matching url in payload"
        );
    }
    Ok(())
}

/// `qdrant_delete_by_url_filter` must remove all points with the target url and leave others intact.
#[tokio::test]
async fn qdrant_delete_by_url_filter_removes_matching_points() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;

    let points = serde_json::json!([
        {"id": Uuid::new_v4().to_string(), "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"url": "https://delete-target.example", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.9f32, 0.1, 0.0, 0.0], "payload": {"url": "https://delete-target.example", "chunk_index": 1}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 1.0, 0.0, 0.0], "payload": {"url": "https://keep.example", "chunk_index": 0}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    qdrant_delete_by_url_filter(&cfg, "https://delete-target.example").await?;

    let remaining_target =
        qdrant_retrieve_by_url(&cfg, "https://delete-target.example", None).await?;
    let remaining_keep = qdrant_retrieve_by_url(&cfg, "https://keep.example", None).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(
        remaining_target.len(),
        0,
        "delete-target points must all be removed"
    );
    assert_eq!(
        remaining_keep.len(),
        1,
        "keep.example point must remain untouched"
    );
    Ok(())
}

/// `qdrant_domain_facets` must return correct per-domain point counts.
#[tokio::test]
async fn qdrant_domain_facets_returns_domain_counts() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;
    create_keyword_index(&client, &base, &cfg.collection, "domain").await?;

    // 3 points for docs.example.com, 1 for api.example.com.
    let points = serde_json::json!([
        {"id": Uuid::new_v4().to_string(), "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"url": "https://docs.example.com/a", "domain": "docs.example.com", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.9f32, 0.1, 0.0, 0.0], "payload": {"url": "https://docs.example.com/a", "domain": "docs.example.com", "chunk_index": 1}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.8f32, 0.2, 0.0, 0.0], "payload": {"url": "https://docs.example.com/b", "domain": "docs.example.com", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 1.0, 0.0, 0.0], "payload": {"url": "https://api.example.com/ref", "domain": "api.example.com", "chunk_index": 0}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    let facets = qdrant_domain_facets(&cfg, 100).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    let docs = facets.iter().find(|(d, _)| d == "docs.example.com");
    let api = facets.iter().find(|(d, _)| d == "api.example.com");
    assert!(
        docs.is_some(),
        "docs.example.com must appear in domain facets"
    );
    assert!(
        api.is_some(),
        "api.example.com must appear in domain facets"
    );
    assert_eq!(docs.unwrap().1, 3, "docs.example.com must have count 3");
    assert_eq!(api.unwrap().1, 1, "api.example.com must have count 1");
    Ok(())
}

/// `qdrant_delete_stale_domain_urls` must delete only URLs absent from `current_urls`,
/// leaving still-current URLs and their chunks untouched.
#[tokio::test]
async fn qdrant_delete_stale_domain_urls_removes_only_stale_points() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;
    // Domain facet index is needed by qdrant_urls_for_domain (filters on "domain" field).
    create_keyword_index(&client, &base, &cfg.collection, "domain").await?;
    // URL keyword index is needed by qdrant_retrieve_by_url scroll filter on "url" field.
    create_keyword_index(&client, &base, &cfg.collection, "url").await?;

    // 2 chunks for page-a (current), 2 chunks for page-b (stale).
    // qdrant_urls_for_domain uses chunk_index==0 to enumerate unique URLs — both pages need it.
    let points = serde_json::json!([
        {"id": Uuid::new_v4().to_string(), "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"url": "https://docs.example.com/page-a", "domain": "docs.example.com", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.9f32, 0.1, 0.0, 0.0], "payload": {"url": "https://docs.example.com/page-a", "domain": "docs.example.com", "chunk_index": 1}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 1.0, 0.0, 0.0], "payload": {"url": "https://docs.example.com/page-b", "domain": "docs.example.com", "chunk_index": 0}},
        {"id": Uuid::new_v4().to_string(), "vector": [0.0f32, 0.9, 0.1, 0.0], "payload": {"url": "https://docs.example.com/page-b", "domain": "docs.example.com", "chunk_index": 1}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    // Only page-a is current; page-b is stale and should be deleted.
    let current_urls: HashSet<String> = ["https://docs.example.com/page-a".to_string()]
        .into_iter()
        .collect();

    let deleted_count =
        qdrant_delete_stale_domain_urls(&cfg, "docs.example.com", &current_urls).await?;

    let page_a_points =
        qdrant_retrieve_by_url(&cfg, "https://docs.example.com/page-a", None).await?;
    let page_b_points =
        qdrant_retrieve_by_url(&cfg, "https://docs.example.com/page-b", None).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(
        deleted_count, 1,
        "exactly 1 stale URL (page-b) must be reported as deleted"
    );
    assert_eq!(
        page_a_points.len(),
        2,
        "page-a chunks must be untouched (2 points)"
    );
    assert_eq!(page_b_points.len(), 0, "page-b chunks must all be deleted");
    Ok(())
}

/// `qdrant_scroll_pages_with_vectors` must return vector data alongside payload for each point.
#[tokio::test]
async fn qdrant_scroll_pages_with_vectors_returns_vector_data() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;

    let points = serde_json::json!([
        {"id": "11111111-0000-0000-0000-000000000001", "vector": [1.0f32, 0.0, 0.0, 0.0], "payload": {"chunk_text": "hello world"}},
        {"id": "11111111-0000-0000-0000-000000000002", "vector": [0.0f32, 1.0, 0.0, 0.0], "payload": {"chunk_text": "foo bar"}},
    ]);
    upsert_points(&client, &base, &cfg.collection, points).await?;

    let mut vectors_seen = 0usize;
    qdrant_scroll_pages_with_vectors(&cfg, |page| {
        for p in page {
            // Unnamed collection → flat float array under "vector" key.
            assert!(
                p["vector"].is_array(),
                "expected flat array vector, got: {p}"
            );
            vectors_seen += 1;
        }
        true
    })
    .await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(
        vectors_seen, 2,
        "must visit all 2 inserted points with vectors"
    );
    Ok(())
}

/// `qdrant_delete_stale_domain_urls` processes stale URLs in 500-item chunks internally.
/// This test crosses the chunk boundary (620 stale URLs > 500) to ensure both the first
/// batch [0..500] and the remainder [500..620] are deleted correctly, and that the 10
/// current (preserved) URLs survive untouched.
#[tokio::test]
async fn qdrant_delete_stale_domain_urls_handles_large_batch_across_chunk_boundary()
-> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", Uuid::new_v4().simple());

    let base = cfg.qdrant_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    create_test_collection(&client, &base, &cfg.collection, 4).await?;
    // Domain index required by qdrant_urls_for_domain (filters on "domain" field).
    create_keyword_index(&client, &base, &cfg.collection, "domain").await?;
    // URL index required by qdrant_retrieve_by_url (scroll filter on "url" field).
    create_keyword_index(&client, &base, &cfg.collection, "url").await?;

    const STALE_COUNT: usize = 620;
    const CURRENT_COUNT: usize = 10;
    const CHUNKS_PER_URL: usize = 2; // chunk_index 0 and 1
    let domain = "large-batch.example";

    // Build stale points: 620 unique URLs × 2 chunks = 1240 points.
    // qdrant_urls_for_domain filters on chunk_index==0, so every URL must have one.
    let mut stale_points: Vec<serde_json::Value> = Vec::with_capacity(STALE_COUNT * CHUNKS_PER_URL);
    for i in 0..STALE_COUNT {
        let url = format!("https://{domain}/stale/{i}");
        for chunk_index in 0..CHUNKS_PER_URL {
            let v = (1.0_f32 - (i as f32 * 0.001)).clamp(0.001, 1.0);
            stale_points.push(serde_json::json!({
                "id": Uuid::new_v4().to_string(),
                "vector": [v, 0.0f32, 0.0, 0.0],
                "payload": {
                    "url": url,
                    "domain": domain,
                    "chunk_index": chunk_index
                }
            }));
        }
    }

    // Build current (preserved) points: 10 unique URLs × 2 chunks = 20 points.
    let mut current_points: Vec<serde_json::Value> =
        Vec::with_capacity(CURRENT_COUNT * CHUNKS_PER_URL);
    let mut current_urls: HashSet<String> = HashSet::new();
    for i in 0..CURRENT_COUNT {
        let url = format!("https://{domain}/current/{i}");
        current_urls.insert(url.clone());
        for chunk_index in 0..CHUNKS_PER_URL {
            current_points.push(serde_json::json!({
                "id": Uuid::new_v4().to_string(),
                "vector": [0.0f32, 1.0, 0.0, 0.0],
                "payload": {
                    "url": url,
                    "domain": domain,
                    "chunk_index": chunk_index
                }
            }));
        }
    }

    // Qdrant upsert in batches of 200 to stay well within default request size limits.
    for batch in stale_points.chunks(200) {
        upsert_points(
            &client,
            &base,
            &cfg.collection,
            serde_json::Value::Array(batch.to_vec()),
        )
        .await?;
    }
    upsert_points(
        &client,
        &base,
        &cfg.collection,
        serde_json::Value::Array(current_points),
    )
    .await?;

    // Delete stale URLs. Only current_urls should survive.
    let deleted_count = qdrant_delete_stale_domain_urls(&cfg, domain, &current_urls).await?;

    // Verify current URLs are fully intact (2 points each).
    let mut surviving_current_points: usize = 0;
    for i in 0..CURRENT_COUNT {
        let url = format!("https://{domain}/current/{i}");
        let pts = qdrant_retrieve_by_url(&cfg, &url, None).await?;
        surviving_current_points += pts.len();
    }

    // Spot-check: first, middle, and last stale URLs must be gone.
    let first_stale =
        qdrant_retrieve_by_url(&cfg, &format!("https://{domain}/stale/0"), None).await?;
    let mid_stale =
        qdrant_retrieve_by_url(&cfg, &format!("https://{domain}/stale/499"), None).await?;
    let last_stale =
        qdrant_retrieve_by_url(&cfg, &format!("https://{domain}/stale/619"), None).await?;

    delete_collection(&client, &base, &cfg.collection).await;

    assert_eq!(
        deleted_count, STALE_COUNT,
        "must report exactly {STALE_COUNT} stale URLs deleted (both 500-item chunks processed)"
    );
    assert_eq!(
        surviving_current_points,
        CURRENT_COUNT * CHUNKS_PER_URL,
        "all {n} current-URL points must survive",
        n = CURRENT_COUNT * CHUNKS_PER_URL
    );
    assert_eq!(
        first_stale.len(),
        0,
        "stale/0 (first chunk batch) must be deleted"
    );
    assert_eq!(
        mid_stale.len(),
        0,
        "stale/499 (boundary URL, last of first batch) must be deleted"
    );
    assert_eq!(
        last_stale.len(),
        0,
        "stale/619 (second chunk batch) must be deleted"
    );
    Ok(())
}
