use std::sync::Arc;

use axon_api::source::*;
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use serde_json::json;
use uuid::Uuid;

use crate::SourceAdapter;
use crate::boundary::FakeAdapterProviders;
use crate::web::WebSourceAdapter;

fn adapter(providers: FakeAdapterProviders) -> WebSourceAdapter {
    let providers = Arc::new(providers);
    WebSourceAdapter::new(providers.clone(), providers)
}

#[tokio::test]
async fn web_adapter_declares_page_site_docs_and_map_scopes() {
    let adapter = adapter(FakeAdapterProviders::new());

    let capability = adapter.capabilities().await.unwrap();

    assert_eq!(capability.0.name, "web");
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::Web))
    );
    assert_eq!(
        capability.0.limits.0.get("default_scope"),
        Some(&serde_json::json!(SourceScope::Site))
    );
    for scope in [
        SourceScope::Page,
        SourceScope::Site,
        SourceScope::Docs,
        SourceScope::Map,
    ] {
        let tag = format!(
            "scope:{}",
            serde_json::to_value(scope).unwrap().as_str().unwrap()
        );
        assert!(capability.0.features.contains(&tag), "missing {scope:?}");
    }
}

#[tokio::test]
async fn web_map_scope_discovers_candidates_without_fetching_bodies() {
    let adapter = adapter(FakeAdapterProviders::new());
    let mut plan = web_plan(
        "https://example.com/docs?utm_source=noise",
        SourceScope::Map,
    );
    plan.route.validated_options.values.insert(
        "map_urls".to_string(),
        json!([
            "https://example.com/docs/intro",
            "https://example.com/docs/api"
        ]),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert_eq!(manifest.items.len(), 2);
    assert_eq!(acquisition.fetched_items.len(), 0);
    assert_eq!(manifest.metadata["embed_requested"], false);
    assert_eq!(manifest.items[0].item_kind, ItemKind::WebPage);
    assert_eq!(
        manifest.items[0].metadata["web_seed_url"],
        plan.route.source.canonical_uri
    );
}

#[tokio::test]
async fn web_map_scope_discovers_urls_without_caller_supplied_map_urls() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let root = server.mock(|when, then| {
        when.method("HEAD").path("/docs");
        then.status(200);
    });
    let sitemap = server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>{}/docs/intro</loc></url>
  <url><loc>{}/docs/api</loc></url>
</urlset>"#,
                server.base_url(),
                server.base_url()
            ));
    });
    let adapter = adapter(FakeAdapterProviders::new());
    let plan = web_plan(&server.url("/docs"), SourceScope::Map);

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    root.assert();
    sitemap.assert();
    let urls = manifest
        .items
        .iter()
        .map(|item| item.canonical_uri.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        urls,
        vec![server.url("/docs/api"), server.url("/docs/intro")]
    );
    assert_eq!(manifest.metadata["map_source"], "sitemap");
    assert_eq!(manifest.metadata["sitemap_urls"], 2);
    assert_eq!(acquisition.fetched_items.len(), 0);
}

#[tokio::test]
async fn web_site_scope_hands_in_memory_manifest_candidates_to_acquisition() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let root = server.mock(|when, then| {
        when.method("HEAD").path("/docs");
        then.status(200);
    });
    let sitemap = server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>{}/docs/intro</loc></url>
  <url><loc>{}/docs/api</loc></url>
</urlset>"#,
                server.base_url(),
                server.base_url()
            ));
    });
    let providers = FakeAdapterProviders::new();
    let adapter = adapter(providers.clone());
    let mut plan = web_plan(&server.url("/docs"), SourceScope::Site);
    plan.route
        .validated_options
        .values
        .insert("render_mode".to_string(), json!("http"));

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    root.assert();
    sitemap.assert();
    assert_eq!(manifest.items.len(), 3);
    assert!(manifest.items.iter().all(|item| item.version.is_some()));
    assert_eq!(manifest.metadata["map_source"], "sitemap");
    assert_eq!(acquisition.fetched_items.len(), 3);
    assert_eq!(providers.calls().await, vec!["fetch", "fetch", "fetch"]);
}

#[tokio::test]
async fn web_page_scope_discover_fetches_once_for_a_real_content_hash() {
    let providers = FakeAdapterProviders::new();
    let adapter = adapter(providers.clone());
    let mut plan = web_plan("https://example.com/docs/intro", SourceScope::Page);
    plan.route
        .validated_options
        .values
        .insert("render_mode".to_string(), json!("http"));

    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(manifest.items.len(), 1);
    assert_eq!(
        manifest.items[0].canonical_uri,
        "https://example.com/docs/intro"
    );
    // Page scope's discover now fetches the page once (issue #298 Wave 2b
    // regression fix) so the manifest item carries a real content_hash —
    // without it, `ledger.diff_manifest` could never tell "unchanged" apart
    // from "never acquired" across successive Page-scope discover
    // generations (see `manifest_items::page_manifest_item`).
    let first_hash = manifest.items[0].content_hash.clone();
    assert!(first_hash.is_some());
    assert_eq!(manifest.items[0].content_kind, None);
    assert_eq!(providers.calls().await, vec!["fetch"]);

    // Discovering again against unchanged (fake) content yields the same hash.
    let manifest_again = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest_again.items[0].content_hash, first_hash);

    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert_eq!(acquisition.fetched_items.len(), 1);
    assert_eq!(
        acquisition.fetched_items[0].manifest_item.content_kind,
        Some(ContentKind::Html)
    );
    assert_eq!(
        acquisition.fetched_items[0].metadata["web_fetch_method"],
        "http_fetch"
    );
    // acquire re-fetches independently of discover's own fetch (a deliberate
    // "correctness over one extra request" tradeoff — see `web/manifest_items.rs`).
    assert_eq!(providers.calls().await, vec!["fetch", "fetch", "fetch"]);

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.data.len(), 1);
    assert_eq!(normalized.data[0].content_kind, ContentKind::Html);
    assert_eq!(normalized.data[0].mime_type.as_deref(), Some("text/html"));
}

/// Dedicated regression coverage for the content_hash correctness fix (issue
/// #298 Wave 2b): a `ledger.diff_manifest`-style comparison of two Page-scope
/// discover generations must see `modified` when the fetched body changed and
/// `unchanged` when it didn't. Before this fix, `content_hash` was always
/// `None` for Page scope, so `None == None` made every repeat discover look
/// "unchanged" regardless of the real content.
#[tokio::test]
async fn web_page_scope_discover_content_hash_reflects_fetched_body_changes() {
    let plan = web_plan("https://example.com/docs/intro", SourceScope::Page);

    let same_body_a = adapter(FakeAdapterProviders::new().with_fetch_text("body-v1"))
        .discover(&plan)
        .await
        .unwrap();
    let same_body_b = adapter(FakeAdapterProviders::new().with_fetch_text("body-v1"))
        .discover(&plan)
        .await
        .unwrap();
    assert_eq!(
        same_body_a.items[0].content_hash, same_body_b.items[0].content_hash,
        "identical fetched bodies must hash identically (diff would see 'unchanged')"
    );

    let changed_body = adapter(FakeAdapterProviders::new().with_fetch_text("body-v2"))
        .discover(&plan)
        .await
        .unwrap();
    assert_ne!(
        same_body_a.items[0].content_hash, changed_body.items[0].content_hash,
        "a changed fetched body must hash differently (diff would see 'modified')"
    );
}

#[tokio::test]
async fn web_docs_scope_acquire_dispatches_chrome_render_for_changed_items() {
    let providers = FakeAdapterProviders::new();
    let adapter = adapter(providers.clone());
    let mut plan = web_plan("https://example.com/docs", SourceScope::Docs);
    plan.route
        .validated_options
        .values
        .insert("render_mode".to_string(), json!("chrome"));

    let item = ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: SourceItemKey::from("docs/intro"),
        canonical_uri: "https://example.com/docs/intro".to_string(),
        item_kind: ItemKind::WebPage,
        content_kind: None,
        display_path: Some("docs/intro".to_string()),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    };
    let diff = manifest_diff(&plan, vec![item]);

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert_eq!(acquisition.fetched_items.len(), 1);
    assert_eq!(
        acquisition.fetched_items[0].metadata["web_fetch_method"],
        "chrome_render"
    );
    assert_eq!(providers.calls().await, vec!["render"]);

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.data[0].content_kind, ContentKind::Markdown);
}

#[tokio::test]
async fn web_acquire_surfaces_per_item_failures_as_stage_warnings() {
    let providers = FakeAdapterProviders::new().with_mode(crate::boundary::FakeAdapterMode::Fatal);
    let adapter = adapter(providers);
    let mut plan = web_plan("https://example.com/docs", SourceScope::Docs);
    plan.route
        .validated_options
        .values
        .insert("render_mode".to_string(), json!("http"));

    let item = ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: SourceItemKey::from("docs/intro"),
        canonical_uri: "https://example.com/docs/intro".to_string(),
        item_kind: ItemKind::WebPage,
        content_kind: None,
        display_path: Some("docs/intro".to_string()),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    };
    let diff = manifest_diff(&plan, vec![item]);

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert!(acquisition.fetched_items.is_empty());
    assert_eq!(acquisition.header.warnings.len(), 1);
    assert_eq!(
        acquisition.header.warnings[0].source_item_key,
        Some(SourceItemKey::from("docs/intro"))
    );
}

#[tokio::test]
async fn web_urls_keep_safe_queries_in_item_keys_without_leaking_secrets() {
    let adapter = adapter(FakeAdapterProviders::new());
    let mut plan = web_plan("https://example.com/search", SourceScope::Map);
    plan.route.validated_options.values.insert(
        "map_urls".to_string(),
        json!([
            "https://example.com/search?q=rust&code=oauth-secret&session=s1",
            "https://example.com/search?q=go&password=secret"
        ]),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();
    let serialized = serde_json::to_string(&manifest).unwrap();

    assert_eq!(keys, vec!["search?q=go", "search?q=rust"]);
    assert!(manifest.items.iter().all(|item| {
        item.canonical_uri == format!("https://example.com/{}", item.source_item_key.0)
    }));
    for secret in ["oauth-secret", "session=", "password=", "code="] {
        assert!(!serialized.contains(secret), "leaked {secret}");
    }
}

#[tokio::test]
async fn web_url_errors_redact_userinfo_and_query_values() {
    let adapter = adapter(FakeAdapterProviders::new());
    let mut plan = web_plan("https://example.com/docs", SourceScope::Map);
    plan.route.validated_options.values.insert(
        "map_urls".to_string(),
        json!(["ftp://user:pass@example.com/path?token=secret&q=visible"]),
    );

    let err = adapter.discover(&plan).await.unwrap_err();
    let rendered = format!("{err:?}");

    assert!(rendered.contains("unsupported_scheme"));
    for secret in ["user:pass", "token=secret", "q=visible"] {
        assert!(!rendered.contains(secret), "leaked {secret}: {rendered}");
    }
    assert!(rendered.contains("REDACTED"));
}

pub(crate) fn web_plan(source: &str, scope: SourceScope) -> SourcePlan {
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(29812)),
        request: SourceRequest::new(source),
        route: RoutePlan {
            source: ResolvedSource {
                source: source.to_string(),
                canonical_uri: strip_tracking_query(source),
                source_id: SourceId::from("src_web_test"),
                source_kind: SourceKind::Web,
                adapter: AdapterRef {
                    name: "web".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                default_scope: scope,
                available_scopes: vec![
                    SourceScope::Page,
                    SourceScope::Site,
                    SourceScope::Docs,
                    SourceScope::Map,
                ],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "test".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter: AdapterRef {
                name: "web".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::PublicNetwork,
            option_schema_id: "adapter:web:options:v1".to_string(),
            validated_options: AdapterOptions::default(),
            chunking_hints: Vec::new(),
            parser_hints: Vec::new(),
            graph_fact_kinds: Vec::new(),
            watch_supported: true,
            refresh_supported: true,
        },
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::from("cfg_web_test"),
        provider_reservations: Vec::new(),
    }
}

fn manifest_diff(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added = items.len() as u64;
    SourceManifestDiff {
        header: stage_header(plan.job_id, PipelinePhase::Diffing, items.len()),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_web_test"),
        added: items,
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

fn stage_header(job_id: JobId, phase: PipelinePhase, item_count: usize) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: StageId::new(Uuid::from_u128(2981201)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: Some(item_count as u64),
            items_done: item_count as u64,
            documents_total: Some(item_count as u64),
            documents_done: item_count as u64,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-02T00:00:00Z".to_string())
}

fn strip_tracking_query(value: &str) -> String {
    value
        .split('?')
        .next()
        .unwrap_or(value)
        .trim_end_matches('/')
        .to_string()
}
