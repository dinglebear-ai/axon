use std::fs;
use std::path::PathBuf;

use axon_api::source::*;
use serde_json::json;
use uuid::Uuid;

use crate::SourceAdapter;
use crate::web::WebSourceAdapter;

#[tokio::test]
async fn web_adapter_declares_page_site_docs_and_map_scopes() {
    let adapter = WebSourceAdapter::new();

    let capability = adapter.capabilities().await.unwrap();

    assert_eq!(capability.adapter.name, "web");
    assert_eq!(capability.source_kind, SourceKind::Web);
    assert_eq!(capability.default_scope, SourceScope::Site);
    for scope in [
        SourceScope::Page,
        SourceScope::Site,
        SourceScope::Docs,
        SourceScope::Map,
    ] {
        assert!(capability.scopes.contains(&scope), "missing {scope:?}");
    }
}

#[tokio::test]
async fn web_map_scope_discovers_candidates_without_fetching_bodies() {
    let adapter = WebSourceAdapter::new();
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
async fn web_manifest_acquires_and_normalizes_source_documents() {
    let adapter = WebSourceAdapter::new();
    let root = temp_web_dir();
    let markdown_dir = root.join("markdown");
    fs::create_dir_all(&markdown_dir).unwrap();
    fs::write(
        markdown_dir.join("docs-intro.md"),
        "# Intro\n\nHello from docs.",
    )
    .unwrap();
    let manifest_path = root.join("manifest.jsonl");
    fs::write(
        &manifest_path,
        serde_json::to_string(&json!({
            "url": "https://example.com/docs/intro?utm_campaign=drop&token=secret",
            "relative_path": "markdown/docs-intro.md",
            "markdown_chars": 24,
            "content_hash": "sha256:intro",
            "changed": true,
            "structured": {
                "title": "Intro",
                "description": "getting started"
            }
        }))
        .unwrap()
            + "\n",
    )
    .unwrap();
    let mut plan = web_plan("https://example.com/docs", SourceScope::Docs);
    plan.route.validated_options.values.insert(
        "manifest_path".to_string(),
        manifest_path.display().to_string().into(),
    );
    plan.route.validated_options.values.insert(
        "markdown_root".to_string(),
        root.display().to_string().into(),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let item = &manifest.items[0];
    assert_eq!(item.source_item_key, SourceItemKey::from("docs/intro"));
    assert_eq!(item.canonical_uri, "https://example.com/docs/intro");
    assert_eq!(item.content_hash.as_deref(), Some("sha256:intro"));
    assert_eq!(
        item.metadata["web_normalized_url"],
        "https://example.com/docs/intro"
    );
    assert_eq!(item.metadata["web_domain"], "example.com");
    assert!(
        !serde_json::to_string(item)
            .unwrap()
            .contains("token=secret")
    );

    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), 1);
    assert!(matches!(
        acquisition.fetched_items[0].content_ref,
        ContentRef::InlineText { .. }
    ));
    assert_eq!(
        acquisition.fetched_items[0].metadata["web_fetch_method"],
        "crawl_manifest"
    );

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let docs = normalized.data;
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].source_id, SourceId::from("src_web_test"));
    assert_eq!(docs[0].source_item_key, SourceItemKey::from("docs/intro"));
    assert_eq!(docs[0].canonical_uri, "https://example.com/docs/intro");
    assert_eq!(docs[0].content_kind, ContentKind::Markdown);
    assert_eq!(docs[0].title.as_deref(), Some("Intro"));
    assert_eq!(docs[0].metadata["source_family"], "web");
    assert_eq!(docs[0].metadata["source_kind"], "web");
    assert_eq!(docs[0].metadata["source_adapter"], "web");
    assert_eq!(docs[0].metadata["source_scope"], "docs");
    assert_eq!(docs[0].metadata["source_item_key"], "docs/intro");
    assert_eq!(
        docs[0].metadata["item_canonical_uri"],
        docs[0].canonical_uri
    );
    assert_eq!(docs[0].metadata["web_seed_url"], "https://example.com/docs");
    assert_eq!(
        docs[0].metadata["web_url"],
        "https://example.com/docs/intro"
    );
    assert_eq!(docs[0].metadata["web_fetch_method"], "crawl_manifest");
    assert!(docs[0].structured_payload.is_some());
    assert!(
        !serde_json::to_string(&docs)
            .unwrap()
            .contains("token=secret")
    );
}

#[tokio::test]
async fn web_urls_keep_safe_queries_in_item_keys_without_leaking_secrets() {
    let adapter = WebSourceAdapter::new();
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
async fn web_manifest_rejects_invalid_jsonl() {
    let adapter = WebSourceAdapter::new();
    let root = temp_web_dir();
    let manifest_path = write_manifest(&root, "{not-json}\n");
    let plan = manifest_plan(&root, &manifest_path);

    let err = adapter.discover(&plan).await.unwrap_err();

    assert!(
        err.to_string().contains("manifest_invalid"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn web_manifest_rejects_missing_required_fields() {
    let adapter = WebSourceAdapter::new();
    let root = temp_web_dir();
    let manifest_path = write_manifest(
        &root,
        &serde_json::to_string(&json!({
            "relative_path": "markdown/docs-intro.md"
        }))
        .unwrap(),
    );
    let plan = manifest_plan(&root, &manifest_path);

    let err = adapter.discover(&plan).await.unwrap_err();

    assert!(
        err.to_string().contains("manifest_field_missing"),
        "unexpected error: {err}"
    );

    let manifest_path = write_manifest(
        &root,
        &(serde_json::to_string(&json!({
            "url": "https://example.com/docs/intro"
        }))
        .unwrap()
            + "\n"),
    );
    let plan = manifest_plan(&root, &manifest_path);

    let err = adapter.discover(&plan).await.unwrap_err();

    assert!(
        err.to_string().contains("manifest_field_missing"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn web_manifest_rejects_paths_outside_markdown_root() {
    let adapter = WebSourceAdapter::new();
    for relative_path in ["/tmp/secret.md", "../secret.md"] {
        let root = temp_web_dir();
        let manifest_path = write_manifest(
            &root,
            &(serde_json::to_string(&json!({
                "url": "https://example.com/docs/intro",
                "relative_path": relative_path,
                "markdown_chars": 12,
                "content_hash": "sha256:intro"
            }))
            .unwrap()
                + "\n"),
        );
        let plan = manifest_plan(&root, &manifest_path);
        let manifest = adapter.discover(&plan).await.unwrap();
        let diff = manifest_diff(&plan, manifest.items);

        let err = adapter.acquire(&plan, &diff).await.unwrap_err();

        assert!(
            err.to_string().contains("path.escape"),
            "unexpected error for {relative_path}: {err}"
        );
    }
}

#[tokio::test]
async fn web_url_errors_redact_userinfo_and_query_values() {
    let adapter = WebSourceAdapter::new();
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

fn web_plan(source: &str, scope: SourceScope) -> SourcePlan {
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(29812)),
        request: SourceRequest::new(source),
        route: RoutePlan {
            source: ResolvedSource {
                requested_uri: source.to_string(),
                canonical_uri: strip_tracking_query(source),
                source_id: SourceId::from("src_web_test"),
                source_kind: SourceKind::Web,
                display_name: "example.com".to_string(),
                candidate_adapters: vec![AdapterCandidate {
                    adapter: AdapterRef {
                        name: "web".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    },
                    supported_scopes: vec![SourceScope::Page, SourceScope::Site, scope],
                    confidence: 1.0,
                    reason: "test".to_string(),
                }],
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
                authority_hint: None,
                warnings: Vec::new(),
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

fn temp_web_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-web-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_manifest(root: &std::path::Path, content: &str) -> PathBuf {
    let manifest_path = root.join("manifest.jsonl");
    fs::write(&manifest_path, content).unwrap();
    manifest_path
}

fn manifest_plan(root: &std::path::Path, manifest_path: &std::path::Path) -> SourcePlan {
    let markdown_dir = root.join("markdown");
    fs::create_dir_all(&markdown_dir).unwrap();
    let mut plan = web_plan("https://example.com/docs", SourceScope::Docs);
    plan.route.validated_options.values.insert(
        "manifest_path".to_string(),
        manifest_path.display().to_string().into(),
    );
    plan.route.validated_options.values.insert(
        "markdown_root".to_string(),
        root.display().to_string().into(),
    );
    plan
}

fn strip_tracking_query(value: &str) -> String {
    value
        .split('?')
        .next()
        .unwrap_or(value)
        .trim_end_matches('/')
        .to_string()
}
