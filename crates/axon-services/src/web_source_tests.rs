use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use serde_json::json;

use crate::test_support::committed_generation_payload;

use super::{WebSourceIndexInput, index_web_source};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x29812))
}

fn input(root: &std::path::Path, manifest_path: std::path::PathBuf) -> WebSourceIndexInput {
    WebSourceIndexInput {
        source: "https://example.com/docs?utm_source=noise".to_string(),
        scope: SourceScope::Docs,
        manifest_path: Some(manifest_path),
        markdown_root: Some(root.to_path_buf()),
        map_urls: Vec::new(),
        collection: "axon-web-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        embed: true,
    }
}

#[tokio::test]
async fn web_source_refresh_writes_vectors_then_commits_generation() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_web_source(
        input(fixture.root.path(), fixture.manifest_path.clone()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(embedder.calls().await.len(), 1);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "mark_generation_committed"]
    );
    assert!(output.documents_prepared >= 1);
    assert!(output.chunks_prepared >= 1);
    assert!(output.vector_points_written >= 1);

    let points = vectors.points("axon-web-test").await;
    assert!(!points.is_empty());
    assert!(points.iter().all(|point| {
        point.payload["source_kind"].as_str() == Some("web")
            && point.payload["source_adapter"].as_str() == Some("web")
            && point.payload["source_scope"].as_str() == Some("docs")
            && point.payload["web_domain"].as_str() == Some("example.com")
            && point.payload["visibility"].as_str() == Some("internal")
            && point.payload["redaction_status"].as_str() == Some("clean")
            && point.payload["committed_generation"]
                == committed_generation_payload(&output.generation)
            && point.payload["document_status"].as_str() == Some("published")
    }));
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors") must produce zero embedding-provider calls and zero
/// vector-store upserts while still returning a `SourceResult`-shaped output
/// (a valid, non-error generation).
#[tokio::test]
async fn embed_false_writes_no_vectors_but_still_completes() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input(fixture.root.path(), fixture.manifest_path.clone());
    no_embed_input.embed = false;

    let output = index_web_source(no_embed_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(output.vector_points_written, 0);
    assert_eq!(
        embedder.calls().await.len(),
        0,
        "embed=false must not call the embedding provider"
    );
    assert!(
        vectors.calls().await.is_empty(),
        "embed=false must not call the vector store"
    );
    assert!(vectors.points("axon-web-test").await.is_empty());
}

#[tokio::test]
async fn unchanged_web_refresh_reuses_committed_generation_without_vector_work() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_web_source(
        input(fixture.root.path(), fixture.manifest_path.clone()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_web_source(
        input(fixture.root.path(), fixture.manifest_path),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    assert_eq!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 0);
    assert_eq!(second.chunks_prepared, 0);
    assert_eq!(second.vector_points_written, 0);
    assert_eq!(embedder.calls().await.len(), embedding_calls);
    assert_eq!(vectors.calls().await, vector_calls);
}

#[tokio::test]
async fn map_scope_publishes_manifest_without_embedding_or_vectors() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let input = WebSourceIndexInput {
        source: "https://example.com/docs".to_string(),
        scope: SourceScope::Map,
        manifest_path: None,
        markdown_root: None,
        map_urls: vec![
            "https://example.com/docs/intro".to_string(),
            "https://example.com/docs/api".to_string(),
        ],
        collection: "axon-web-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        embed: true,
    };

    let output = index_web_source(input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(output.documents_prepared, 0);
    assert_eq!(output.chunks_prepared, 0);
    assert_eq!(output.vector_points_written, 0);
    let generation = ledger
        .generation(&output.source_id, &output.generation)
        .await
        .unwrap();
    assert_eq!(generation.document_counts.discovered, 2);
    assert_eq!(generation.document_counts.prepared, 0);
    assert_eq!(generation.document_counts.embedded, 0);
    assert_eq!(generation.document_counts.published, 0);
    assert_eq!(embedder.calls().await.len(), 0);
    assert!(vectors.calls().await.is_empty());
}

#[tokio::test]
async fn mixed_web_refresh_carries_unchanged_vectors_into_new_generation() {
    let fixture = web_fixture_pages(&[
        (
            "docs-intro.md",
            "https://example.com/docs/intro",
            "# Intro\n\nv1",
        ),
        ("docs-api.md", "https://example.com/docs/api", "# API\n\nv1"),
    ]);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_web_source(
        input(fixture.root.path(), fixture.manifest_path.clone()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();
    write_web_fixture_pages(
        &fixture,
        &[
            (
                "docs-intro.md",
                "https://example.com/docs/intro",
                "# Intro\n\nversion two",
            ),
            ("docs-api.md", "https://example.com/docs/api", "# API\n\nv1"),
        ],
    );

    let second = index_web_source(
        input(fixture.root.path(), fixture.manifest_path.clone()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    assert_ne!(second.generation, first.generation);
    assert_eq!(
        ledger.committed_generation(&second.source_id).await,
        Some(second.generation.clone())
    );
    assert!(
        vectors
            .calls()
            .await
            .contains(&"mark_unchanged_items_committed")
    );
    let api_points = vectors
        .points("axon-web-test")
        .await
        .into_iter()
        .filter(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("docs/api")
        })
        .collect::<Vec<_>>();
    assert!(api_points.iter().any(|point| {
        point.payload["committed_generation"] == committed_generation_payload(&second.generation)
    }));
}

#[tokio::test]
async fn publish_failure_rolls_back_web_vectors() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new().with_publish_generation_failure();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_web_source(
        input(fixture.root.path(), fixture.manifest_path),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("publish_failed"),
        "unexpected error: {err:#}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
}

struct WebFixture {
    root: tempfile::TempDir,
    manifest_path: std::path::PathBuf,
}

fn web_fixture(markdown: &str) -> WebFixture {
    web_fixture_pages(&[(
        "docs-intro.md",
        "https://example.com/docs/intro?utm_source=noise&token=secret",
        markdown,
    )])
}

fn web_fixture_pages(pages: &[(&str, &str, &str)]) -> WebFixture {
    let root = tempfile::tempdir().unwrap();
    let markdown_dir = root.path().join("markdown");
    std::fs::create_dir_all(&markdown_dir).unwrap();
    let manifest_path = root.path().join("manifest.jsonl");
    let fixture = WebFixture {
        root,
        manifest_path,
    };
    write_web_fixture_pages(&fixture, pages);
    fixture
}

fn write_web_fixture_pages(fixture: &WebFixture, pages: &[(&str, &str, &str)]) {
    let markdown_dir = fixture.root.path().join("markdown");
    let lines = pages
        .iter()
        .map(|(file_name, url, markdown)| {
            std::fs::write(markdown_dir.join(file_name), markdown).unwrap();
            manifest_line(file_name, url, markdown)
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&fixture.manifest_path, lines).unwrap();
}

fn manifest_line(file_name: &str, url: &str, markdown: &str) -> String {
    serde_json::to_string(&json!({
        "url": url,
        "relative_path": format!("markdown/{file_name}"),
        "markdown_chars": markdown.len() as u64,
        "content_hash": format!("sha256:{}", markdown.len()),
        "changed": true,
        "structured": {
            "title": "Intro"
        }
    }))
    .unwrap()
}
