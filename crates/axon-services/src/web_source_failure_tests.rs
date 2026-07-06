use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore};
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
    }
}

#[tokio::test]
async fn partial_vector_write_failure_rolls_back_web_generation_vectors() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);

    let err = index_web_source(
        input(fixture.root.path(), fixture.manifest_path),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap_err();

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("partial_failure") || rendered.contains("partial.failure"),
        "unexpected error: {rendered}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

#[tokio::test]
async fn lost_lease_before_publish_rolls_back_web_generation_vectors() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new().with_heartbeat_lost();
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
        err.to_string().contains("lost lease"),
        "unexpected error: {err:#}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

#[tokio::test]
async fn vector_commit_failure_rolls_back_web_generation_vectors() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::CommitFailure);

    let err = index_web_source(
        input(fixture.root.path(), fixture.manifest_path),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("commit_failed"),
        "unexpected error: {err:#}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

#[tokio::test]
async fn partial_unchanged_vector_copy_failure_keeps_previous_web_generation_visible() {
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
    let failing_vectors = vectors
        .clone()
        .with_mode(FakeVectorMode::PartialCommitFailure);

    let err = index_web_source(
        input(fixture.root.path(), fixture.manifest_path.clone()),
        &ledger,
        &embedder,
        &failing_vectors,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("partial_commit_failure"),
        "unexpected error: {err:#}"
    );
    assert_eq!(
        ledger.committed_generation(&first.source_id).await,
        Some(first.generation.clone())
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
    assert!(!api_points.is_empty());
    assert!(api_points.iter().all(|point| {
        point.payload["committed_generation"] == committed_generation_payload(&first.generation)
    }));
}

#[tokio::test]
async fn web_vectorization_batches_more_than_changed_document_limit() {
    let pages = (0..65)
        .map(|idx| {
            (
                format!("docs-{idx}.md"),
                format!("https://example.com/docs/{idx}"),
                format!("# Page {idx}\n\ncontent"),
            )
        })
        .collect::<Vec<_>>();
    let page_refs = pages
        .iter()
        .map(|(file, url, markdown)| (file.as_str(), url.as_str(), markdown.as_str()))
        .collect::<Vec<_>>();
    let fixture = web_fixture_pages(&page_refs);
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

    assert_eq!(output.documents_prepared, 65);
    assert_eq!(embedder.calls().await.len(), 2);
    assert_eq!(vectors.points("axon-web-test").await.len(), 65);
}

#[tokio::test]
async fn missing_embedding_vector_aborts_without_publishing_web_generation() {
    let fixture = web_fixture("# Intro\n\nHello docs.");
    let ledger = FakeLedgerStore::new();
    let embedder = MissingVectorEmbeddingProvider {
        inner: FakeEmbeddingProvider::new("fake-embedding", 8),
    };
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_web_source(
        input(fixture.root.path(), fixture.manifest_path),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap_err();

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("missing vector"),
        "unexpected error: {rendered}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

struct WebFixture {
    root: tempfile::TempDir,
    manifest_path: std::path::PathBuf,
}

struct MissingVectorEmbeddingProvider {
    inner: FakeEmbeddingProvider,
}

#[async_trait::async_trait]
impl EmbeddingProvider for MissingVectorEmbeddingProvider {
    async fn embed(
        &self,
        batch: EmbeddingBatch,
    ) -> axon_embedding::provider::Result<EmbeddingResult> {
        let mut result = self.inner.embed(batch).await?;
        result.vectors.pop();
        Ok(result)
    }

    async fn capabilities(&self) -> axon_embedding::provider::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
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
