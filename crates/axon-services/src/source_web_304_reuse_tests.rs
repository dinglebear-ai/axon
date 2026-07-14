use std::sync::Arc;
use std::sync::OnceLock;

use async_trait::async_trait;
use axon_adapters::boundary::FakeAdapterProviders;
use axon_adapters::boundary::{FetchProvider, RenderProvider};
use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use tokio::sync::Mutex;

use super::run::{
    apply_reused_item_keys, overlay_previous_web_etags, resolve_web_run, source_summary,
};
use super::vectorize::{collection_spec, normalize_changed_documents, vectorize_changed_documents};
use super::{WebSourceIndexInput, reuse};

#[derive(Debug, Default)]
struct ConditionalFetchState {
    body: String,
    etag: String,
    conditional_304: bool,
    unconditional_304: bool,
    conditional_fetches: usize,
    full_fetches: usize,
}

#[derive(Clone)]
struct ConditionalFetchProvider {
    state: Arc<Mutex<ConditionalFetchState>>,
    capabilities: FakeAdapterProviders,
}

impl ConditionalFetchProvider {
    fn new(body: &str, etag: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(ConditionalFetchState {
                body: body.to_string(),
                etag: etag.to_string(),
                conditional_304: false,
                unconditional_304: false,
                conditional_fetches: 0,
                full_fetches: 0,
            })),
            capabilities: FakeAdapterProviders::new(),
        }
    }

    async fn set_conditional_304(&self, enabled: bool) {
        self.state.lock().await.conditional_304 = enabled;
    }

    async fn set_unconditional_304(&self, enabled: bool) {
        self.state.lock().await.unconditional_304 = enabled;
    }

    async fn set_revision(&self, body: &str, etag: &str) {
        let mut state = self.state.lock().await;
        state.body = body.to_string();
        state.etag = etag.to_string();
    }

    async fn conditional_fetches(&self) -> usize {
        self.state.lock().await.conditional_fetches
    }

    async fn full_fetches(&self) -> usize {
        self.state.lock().await.full_fetches
    }
}

#[async_trait]
impl FetchProvider for ConditionalFetchProvider {
    async fn fetch(
        &self,
        request: FetchRequest,
    ) -> axon_adapters::boundary::Result<FetchedResource> {
        let mut state = self.state.lock().await;
        let conditional = request
            .headers
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("If-None-Match"))
            .map(|header| header.value.clone());
        if conditional.is_some() {
            state.conditional_fetches += 1;
        } else {
            state.full_fetches += 1;
        }
        let status = if conditional.is_some() {
            if state.conditional_304 && conditional.as_deref() == Some(state.etag.as_str()) {
                304
            } else {
                200
            }
        } else if state.unconditional_304 {
            304
        } else {
            200
        };
        Ok(FetchedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            status,
            content: ContentRef::InlineText {
                text: if status == 304 {
                    String::new()
                } else {
                    state.body.clone()
                },
            },
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
            etag: Some(state.etag.clone()),
            redirect_chain: Vec::new(),
            bytes: Some(state.body.len() as u64),
            metadata: MetadataMap::new(),
        })
    }

    async fn capabilities(&self) -> axon_adapters::boundary::Result<ProviderCapability> {
        FetchProvider::capabilities(&self.capabilities).await
    }
}

#[async_trait]
impl RenderProvider for ConditionalFetchProvider {
    async fn render(
        &self,
        request: RenderRequest,
    ) -> axon_adapters::boundary::Result<RenderedResource> {
        self.capabilities.render(request).await
    }

    async fn capabilities(&self) -> axon_adapters::boundary::Result<ProviderCapability> {
        <FakeAdapterProviders as RenderProvider>::capabilities(&self.capabilities).await
    }
}

fn web_input(
    provider: Arc<ConditionalFetchProvider>,
    etag_conditional: bool,
) -> WebSourceIndexInput {
    let mut crawl_options = MetadataMap::new();
    crawl_options.insert("render_mode".to_string(), serde_json::json!("http"));
    crawl_options.insert(
        "etag_conditional".to_string(),
        serde_json::json!(etag_conditional),
    );
    WebSourceIndexInput {
        source: "https://docs.example.test/intro".to_string(),
        scope: SourceScope::Page,
        map_urls: Vec::new(),
        crawl_options,
        output: OutputPolicy::default(),
        collection: "axon-web-304-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: JobId::new(uuid::Uuid::from_u128(0x304)),
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        auth_snapshot: None,
        embed: true,
        fetch_provider: provider.clone(),
        render_provider: provider,
        artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        event_store: None,
    }
}

fn manifest_item(run: &super::run::WebAdapterRun, etag: Option<&str>) -> ManifestItem {
    let mut metadata = MetadataMap::new();
    if let Some(etag) = etag {
        metadata.insert("web_etag".to_string(), serde_json::json!(etag));
    }
    ManifestItem {
        source_id: run.source_id.clone(),
        source_item_key: SourceItemKey::new("https://docs.example.test/intro"),
        canonical_uri: "https://docs.example.test/intro".to_string(),
        item_kind: ItemKind::WebPage,
        content_kind: None,
        display_path: Some("/intro".to_string()),
        parent_key: None,
        size_bytes: Some(12),
        content_hash: Some("hash-intro".to_string()),
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata,
        graph_hints: Vec::new(),
    }
}

fn diff_for(
    run: &super::run::WebAdapterRun,
    previous_generation: Option<&str>,
    next_generation: &str,
    added: Vec<ManifestItem>,
    modified: Vec<ManifestItem>,
    removed: Vec<ManifestItem>,
) -> SourceManifestDiff {
    SourceManifestDiff {
        header: StageResultHeader {
            job_id: run.plan.job_id,
            stage_id: StageId::new(uuid::Uuid::nil()),
            phase: PipelinePhase::Fetching,
            status: LifecycleStatus::Completed,
            started_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
            completed_at: Some(Timestamp("2026-07-13T00:00:00Z".to_string())),
            counts: StageCounts {
                items_total: Some((added.len() + modified.len() + removed.len()) as u64),
                items_done: (added.len() + modified.len() + removed.len()) as u64,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: run.source_id.clone(),
        previous_generation: previous_generation.map(SourceGenerationId::new),
        next_generation: SourceGenerationId::new(next_generation),
        counts: DiffCounts {
            added: added.len() as u64,
            modified: modified.len() as u64,
            removed: removed.len() as u64,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
        added,
        modified,
        removed,
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    }
}

fn manifest_for(
    run: &super::run::WebAdapterRun,
    generation: &SourceGenerationId,
    items: Vec<ManifestItem>,
) -> SourceManifest {
    SourceManifest {
        source_id: run.source_id.clone(),
        generation: generation.clone(),
        adapter: run.adapter.clone(),
        scope: run.scope,
        items,
        created_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
        metadata: MetadataMap::new(),
    }
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn seed_cached_generation(
    provider: Arc<ConditionalFetchProvider>,
    ledger: &FakeLedgerStore,
) -> anyhow::Result<(
    WebSourceIndexInput,
    super::run::WebAdapterRun,
    SourceGenerationId,
)> {
    let input = web_input(provider, false);
    let run = resolve_web_run(&input)?;
    ledger.upsert_source(source_summary(&input, &run)).await?;
    let first_diff = diff_for(
        &run,
        None,
        "gen-1",
        vec![manifest_item(&run, Some("\"abc\""))],
        Vec::new(),
        Vec::new(),
    );
    let normalized = normalize_changed_documents(&input, &run, &first_diff).await?;
    assert_eq!(
        normalized.documents.len(),
        1,
        "seed run should normalize one document"
    );
    ledger
        .put_manifest(manifest_for(
            &run,
            &first_diff.next_generation,
            vec![manifest_item(&run, Some("\"abc\""))],
        ))
        .await?;
    Ok((input, run, first_diff.next_generation))
}

#[tokio::test]
async fn second_run_304_reuses_previous_document_without_embedding() {
    let _guard = test_lock().lock().await;
    reuse::reset_cache();
    let provider = Arc::new(ConditionalFetchProvider::new("# Intro\n\nv1", "\"abc\""));
    let ledger = FakeLedgerStore::new();
    let (_first_input, first_run, first_generation) =
        seed_cached_generation(provider.clone(), &ledger)
            .await
            .expect("seed initial cached generation");

    provider.set_conditional_304(true).await;
    let second_input = web_input(provider.clone(), true);
    let second_run = resolve_web_run(&second_input).expect("resolve second run");
    assert_eq!(second_run.source_id, first_run.source_id);
    let second_diff = diff_for(
        &second_run,
        Some(&first_generation.0),
        "gen-2",
        Vec::new(),
        vec![manifest_item(&second_run, Some("\"abc\""))],
        Vec::new(),
    );

    let second_diff = overlay_previous_web_etags(&ledger, &second_diff)
        .await
        .expect("overlay previous manifest etag");
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let result = vectorize_changed_documents(
        &second_input,
        &second_run,
        &second_diff,
        &second_diff.next_generation,
        &ledger,
        &embedder,
        &vectors,
        collection_spec(&second_input),
    )
    .await
    .expect("304 reuse should vectorize cleanly");

    assert_eq!(result.documents_prepared, 0);
    assert_eq!(result.chunks_prepared, 0);
    assert_eq!(result.reused_item_keys.len(), 1);
    assert_eq!(embedder.calls().await.len(), 0, "reused pages skip TEI");
    assert!(
        vectors.calls().await.is_empty(),
        "reused pages should not upsert vectors"
    );
    assert_eq!(provider.conditional_fetches().await, 1);
    assert_eq!(
        provider.full_fetches().await,
        1,
        "only the seed fetch should be unconditional"
    );
}

#[tokio::test]
async fn missing_prior_content_refetches_before_publish() {
    let _guard = test_lock().lock().await;
    reuse::reset_cache();
    let provider = Arc::new(ConditionalFetchProvider::new("# Intro\n\nv1", "\"abc\""));
    let ledger = FakeLedgerStore::new();
    let (_first_input, first_run, first_generation) =
        seed_cached_generation(provider.clone(), &ledger)
            .await
            .expect("seed initial cached generation");
    reuse::evict_document(
        &first_run.source_id,
        &first_generation,
        &SourceItemKey::new("https://docs.example.test/intro"),
    );

    provider.set_conditional_304(true).await;
    let second_input = web_input(provider.clone(), true);
    let second_run = resolve_web_run(&second_input).expect("resolve second run");
    let second_diff = diff_for(
        &second_run,
        Some(&first_generation.0),
        "gen-2",
        Vec::new(),
        vec![manifest_item(&second_run, Some("\"abc\""))],
        Vec::new(),
    );
    let second_diff = overlay_previous_web_etags(&ledger, &second_diff)
        .await
        .expect("overlay previous manifest etag");

    let normalized = normalize_changed_documents(&second_input, &second_run, &second_diff)
        .await
        .expect("cache miss should refetch");

    assert_eq!(normalized.documents.len(), 1);
    assert!(normalized.reused_item_keys.is_empty());
    assert_eq!(provider.conditional_fetches().await, 1);
    assert_eq!(
        provider.full_fetches().await,
        2,
        "cache miss should force one extra full fetch"
    );
    assert!(
        normalized
            .warnings
            .iter()
            .any(|warning| warning.code == "web.reuse.cache_miss_refetch")
    );
}

#[tokio::test]
async fn cache_miss_refetch_fails_if_unconditional_fetch_still_returns_304() {
    let _guard = test_lock().lock().await;
    reuse::reset_cache();
    let provider = Arc::new(ConditionalFetchProvider::new("# Intro\n\nv1", "\"abc\""));
    let ledger = FakeLedgerStore::new();
    let (_first_input, first_run, first_generation) =
        seed_cached_generation(provider.clone(), &ledger)
            .await
            .expect("seed initial cached generation");
    reuse::evict_document(
        &first_run.source_id,
        &first_generation,
        &SourceItemKey::new("https://docs.example.test/intro"),
    );

    provider.set_conditional_304(true).await;
    provider.set_unconditional_304(true).await;
    let second_input = web_input(provider.clone(), true);
    let second_run = resolve_web_run(&second_input).expect("resolve second run");
    let second_diff = diff_for(
        &second_run,
        Some(&first_generation.0),
        "gen-2",
        Vec::new(),
        vec![manifest_item(&second_run, Some("\"abc\""))],
        Vec::new(),
    );
    let second_diff = overlay_previous_web_etags(&ledger, &second_diff)
        .await
        .expect("overlay previous manifest etag");

    let err = normalize_changed_documents(&second_input, &second_run, &second_diff)
        .await
        .err()
        .expect("unexpected second 304 should fail publish");

    assert!(
        err.to_string().contains("304 Not Modified"),
        "error should describe the invalid second 304: {err}"
    );
    assert_eq!(provider.conditional_fetches().await, 1);
    assert_eq!(
        provider.full_fetches().await,
        2,
        "cache miss should still attempt one unconditional refetch"
    );
}

#[tokio::test]
async fn changed_page_uses_previous_manifest_etag_not_current_discovery_etag() {
    let _guard = test_lock().lock().await;
    reuse::reset_cache();
    let provider = Arc::new(ConditionalFetchProvider::new("# Intro\n\nv1", "\"abc\""));
    let ledger = FakeLedgerStore::new();
    let (_first_input, first_run, first_generation) =
        seed_cached_generation(provider.clone(), &ledger)
            .await
            .expect("seed initial cached generation");

    provider.set_revision("# Intro\n\nv2", "\"v2\"").await;
    provider.set_conditional_304(true).await;

    let second_input = web_input(provider.clone(), true);
    let second_run = resolve_web_run(&second_input).expect("resolve second run");
    assert_eq!(second_run.source_id, first_run.source_id);
    let current_manifest_item = manifest_item(&second_run, Some("\"v2\""));
    let second_diff = diff_for(
        &second_run,
        Some(&first_generation.0),
        "gen-2",
        Vec::new(),
        vec![current_manifest_item],
        Vec::new(),
    );
    let second_diff = overlay_previous_web_etags(&ledger, &second_diff)
        .await
        .expect("overlay previous manifest etag");

    assert_eq!(
        second_diff.modified[0].metadata["web_prior_etag"],
        serde_json::json!("\"abc\"")
    );
    assert_eq!(
        second_diff.modified[0].metadata["web_etag"],
        serde_json::json!("\"v2\"")
    );

    let normalized = normalize_changed_documents(&second_input, &second_run, &second_diff)
        .await
        .expect("changed page should refetch instead of reusing stale content");

    assert_eq!(normalized.documents.len(), 1);
    assert!(normalized.reused_item_keys.is_empty());
    assert!(matches!(
        &normalized.documents[0].content,
        ContentRef::InlineText { text } if text.contains("v2")
    ));
    assert_eq!(provider.conditional_fetches().await, 1);
    assert_eq!(
        provider.full_fetches().await,
        1,
        "the changed page should not fall into a false 304 reuse path"
    );
}

#[test]
fn mixed_modified_304_and_removed_counts_are_distinct() {
    let _guard = test_lock().blocking_lock();
    let provider = Arc::new(ConditionalFetchProvider::new("# Intro\n\nv1", "\"abc\""));
    let input = web_input(provider, true);
    let run = resolve_web_run(&input).expect("resolve run");
    let reused = manifest_item(&run, Some("\"abc\""));
    let mut changed = manifest_item(&run, Some("\"def\""));
    changed.source_item_key = SourceItemKey::new("https://docs.example.test/guide");
    changed.canonical_uri = "https://docs.example.test/guide".to_string();
    let mut removed = manifest_item(&run, None);
    removed.source_item_key = SourceItemKey::new("https://docs.example.test/gone");
    removed.canonical_uri = "https://docs.example.test/gone".to_string();
    let diff = diff_for(
        &run,
        Some("gen-1"),
        "gen-2",
        Vec::new(),
        vec![reused.clone(), changed.clone()],
        vec![removed.clone()],
    );

    let adjusted = apply_reused_item_keys(&diff, std::slice::from_ref(&reused.source_item_key));
    assert_eq!(adjusted.counts.modified, 1);
    assert_eq!(adjusted.counts.unchanged, 1);
    assert_eq!(adjusted.counts.removed, 1);
    assert_eq!(adjusted.modified, vec![changed]);
    assert_eq!(adjusted.unchanged, vec![reused]);
    assert_eq!(adjusted.removed, vec![removed]);
}
