//! Shared policy exercised after actual adapter acquisition and normalization.
use axon_adapters::{
    SourceAdapter,
    boundary::{FakeAdapterProviders, FetchProvider},
    git::GitSourceAdapter,
    local::LocalSourceAdapter,
    web::WebSourceAdapter,
};
use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::sync::{Arc, Mutex};

struct RawFetch {
    bytes: Vec<u8>,
    requests: Mutex<Vec<FetchRequest>>,
}
#[async_trait::async_trait]
impl FetchProvider for RawFetch {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource, ApiError> {
        self.requests.lock().unwrap().push(request.clone());
        let mut fetched = FakeAdapterProviders::new().fetch(request).await?;
        fetched.content = ContentRef::InlineBytes {
            bytes_base64: STANDARD.encode(&self.bytes),
            mime_type: "application/octet-stream".into(),
        };
        fetched.bytes = Some(self.bytes.len() as u64);
        Ok(fetched)
    }
    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        FetchProvider::capabilities(&FakeAdapterProviders::new()).await
    }
}

#[derive(Clone)]
enum Expected {
    Text(&'static str),
    Skip(ContentSkipReason),
}
fn fixtures() -> Vec<(&'static str, Vec<u8>, Expected)> {
    let text = "Hello café";
    let le = [
        vec![0xff, 0xfe],
        text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ]
    .concat();
    let be = [
        vec![0xfe, 0xff],
        text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
    ]
    .concat();
    vec![
        ("utf8", text.as_bytes().to_vec(), Expected::Text(text)),
        (
            "utf8-bom",
            [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
            Expected::Text(text),
        ),
        ("utf16-le", le, Expected::Text(text)),
        ("utf16-be", be, Expected::Text(text)),
        (
            "png",
            b"\x89PNG\r\n\x1a\nfixture".to_vec(),
            Expected::Skip(ContentSkipReason::UnsupportedBinary),
        ),
        (
            "pdf",
            b"%PDF-1.7\nfixture".to_vec(),
            Expected::Skip(ContentSkipReason::UnsupportedBinary),
        ),
        (
            "zip",
            b"PK\x03\x04fixture".to_vec(),
            Expected::Skip(ContentSkipReason::UnsupportedBinary),
        ),
        (
            "empty",
            vec![],
            Expected::Skip(ContentSkipReason::EmptyContent),
        ),
    ]
}

fn plan(source: &str, options: MetadataMap) -> SourcePlan {
    let mut request = SourceRequest::new(source);
    request.options = AdapterOptions { values: options };
    let route = super::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    super::dispatch::family_source_plan(source, &route, false, None, None)
}

async fn normalized(
    adapter: &dyn SourceAdapter,
    plan: &SourcePlan,
    expected: &[u8],
) -> SourceDocument {
    let manifest = adapter.discover(plan).await.unwrap();
    assert_eq!(
        manifest.items.len(),
        1,
        "inventory preserves the actual file"
    );
    let diff = FakeLedgerStore::new()
        .diff_manifest(manifest)
        .await
        .unwrap();
    let acquired = adapter.acquire(plan, &diff).await.unwrap();
    assert_eq!(acquired.fetched_items.len(), 1);
    let ContentRef::InlineBytes { bytes_base64, .. } = &acquired.fetched_items[0].content_ref
    else {
        panic!("actual acquired bytes must remain bytes until shared preparation");
    };
    assert_eq!(STANDARD.decode(bytes_base64).unwrap(), expected);
    let mut documents = adapter.normalize(plan, acquired).await.unwrap().data;
    assert_eq!(documents.len(), 1);
    documents.remove(0)
}

fn check(document: SourceDocument, expected: &Expected, context: &str) {
    let item = document.source_item_key.clone();
    let result = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document,
            generation: SourceGenerationId::new("gen_policy"),
            profile: None,
            parse_facts: vec![],
            graph_candidates: vec![],
            warnings: vec![],
            errors: vec![],
        })
        .unwrap();
    match (result, expected) {
        (PrepareSourceDocumentResult::Prepared(document), Expected::Text(text)) => {
            assert_eq!(document.chunks.len(), 1, "{context}");
            assert_eq!(document.chunks[0].content, *text, "{context}");
        }
        (PrepareSourceDocumentResult::Skipped(skipped), Expected::Skip(reason)) => {
            assert_eq!(&skipped.reason, reason, "{context}");
            assert_eq!(skipped.source_item_key, item);
        }
        (actual, _) => panic!("unexpected preparation outcome for {context}: {actual:?}"),
    }
}

#[tokio::test]
async fn real_git_local_and_http_bytes_share_decoding_and_skip_policy() {
    for (name, bytes, expected) in fixtures() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("payload.txt"), &bytes).unwrap();
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .arg(root.path())
            .status()
            .unwrap();
        assert!(status.success());
        let local = LocalSourceAdapter::new();
        let local_plan = plan(root.path().to_str().unwrap(), MetadataMap::new());
        check(
            normalized(&local, &local_plan, &bytes).await,
            &expected,
            &format!("Local {name}"),
        );
        local
            .release(&AdapterReleaseRequest {
                job_id: local_plan.job_id,
                source_id: local_plan.route.source.source_id.clone(),
                source_kind: SourceKind::Local,
            })
            .unwrap();

        let mut git_plan = plan("https://github.com/example/byte-policy", MetadataMap::new());
        // Internal materialization seam: no network clone, caller options cannot set repo_root.
        git_plan.route.validated_options.values.insert(
            "repo_root".into(),
            root.path().to_string_lossy().into_owned().into(),
        );
        check(
            normalized(&GitSourceAdapter::new(), &git_plan, &bytes).await,
            &expected,
            &format!("Git {name}"),
        );

        let fetch = Arc::new(RawFetch {
            bytes: bytes.clone(),
            requests: Mutex::new(vec![]),
        });
        let renderer = Arc::new(FakeAdapterProviders::new());
        let web = WebSourceAdapter::new(fetch.clone(), renderer.clone());
        let mut options = MetadataMap::new();
        options.insert("render_mode".into(), "http".into());
        let web_plan = plan("https://example.test/payload", options);
        check(
            normalized(&web, &web_plan, &bytes).await,
            &expected,
            &format!("HTTP {name}"),
        );
        assert!(
            renderer.calls().await.is_empty(),
            "raw HTTP branch does not render"
        );
        assert!(
            fetch
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.uri == "https://example.test/payload")
        );
    }
}

#[tokio::test]
async fn web_dispatch_retains_requested_preparation_ceiling_after_raw_fetch() {
    use axon_core::config::{Config, RenderMode};
    use axon_embedding::fake::FakeEmbeddingProvider;
    use axon_jobs::boundary::FakeJobWatchStore;
    use axon_vectors::store::FakeVectorStore;
    let fetch = Arc::new(RawFetch {
        bytes: b"longer than three bytes".to_vec(),
        requests: Mutex::new(vec![]),
    });
    let renderer = Arc::new(FakeAdapterProviders::new());
    let adapter = Arc::new(WebSourceAdapter::new(fetch.clone(), renderer.clone()));
    let ledger = Arc::new(FakeLedgerStore::new());
    let runtime = crate::context::TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        ledger.clone(),
        Arc::new(FakeEmbeddingProvider::new("embedding", 8)),
        Arc::new(FakeVectorStore::new("vectors")),
        ProviderId::new("embedding"),
        "embedding",
        8,
    );
    let source = "https://example.test/payload";
    let route = super::routing::resolve_source_route(&SourceRequest::new(source))
        .unwrap()
        .route;
    let limits = SourceLimits {
        max_bytes_per_item: Some(3),
        max_total_bytes: Some(100),
        ..Default::default()
    };
    let execution = super::SourceExecutionContext::inline(SourceRequest::new(source), None);
    let cfg = Config {
        render_mode: RenderMode::Http,
        ..Default::default()
    };
    let counts = super::family_dispatch::dispatch_web_kind(
        adapter,
        &cfg,
        &runtime,
        source,
        "test",
        "byte-policy",
        route.scope,
        None,
        false,
        &OutputPolicy::default(),
        &limits,
        &route,
        &execution,
    )
    .await
    .unwrap();
    assert_eq!(counts.documents_prepared, 0);
    assert_eq!(counts.chunks_prepared, 0);
    assert_eq!(counts.vector_points_written, 0);
    assert_eq!(counts.documents_skipped, 1);
    let summary = ledger.get_source(counts.source_id).await.unwrap().unwrap();
    assert_eq!(summary.counts.documents_skipped, 1);
    {
        let requests = fetch.requests.lock().unwrap();
        // Vertical discovery may probe additional endpoints before raw fetch.
        let fetched = requests
            .iter()
            .rev()
            .find(|request| request.uri == source)
            .unwrap();
        assert_eq!(
            fetched.max_bytes, None,
            "this proves shared preparation, not bounded HTTP transport"
        );
    }
    assert!(renderer.calls().await.is_empty());
}
