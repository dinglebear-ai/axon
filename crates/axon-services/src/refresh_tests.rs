use std::sync::Arc;

use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;

use super::*;
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::test_support::NoopServiceRuntime;

fn origin(source_type: &str, seed_url: &str) -> RefreshOrigin {
    RefreshOrigin {
        seed_url: seed_url.to_string(),
        source_type: source_type.to_string(),
        chunks: 1,
        action: classify_action(source_type, seed_url),
        ledger_source_id: None,
    }
}

#[test]
fn web_origins_classify_as_crawl() {
    assert_eq!(
        classify_action("embed", "https://docs.example.com"),
        RefreshAction::Crawl
    );
    assert_eq!(
        classify_action("scrape", "http://example.com/page"),
        RefreshAction::Crawl
    );
    assert_eq!(
        classify_action("crawl", "https://example.com"),
        RefreshAction::Crawl
    );
}

#[test]
fn ingest_origins_classify_as_ingest() {
    assert_eq!(
        classify_action("github", "owner/repo"),
        RefreshAction::Ingest
    );
    assert_eq!(classify_action("git", "owner/repo"), RefreshAction::Ingest);
    assert_eq!(classify_action("reddit", "r/rust"), RefreshAction::Ingest);
    assert_eq!(
        classify_action("youtube", "https://youtube.com/watch?v=x"),
        RefreshAction::Ingest
    );
}

#[test]
fn sessions_and_non_url_embeds_are_skipped() {
    assert!(matches!(
        classify_action("sessions", "all"),
        RefreshAction::Skip(_)
    ));
    // A local file/dir embed: seed is a filesystem path, not re-crawlable.
    assert!(matches!(
        classify_action("embed", "/home/user/docs/file.md"),
        RefreshAction::Skip(_)
    ));
}

#[test]
fn payload_origin_reads_unified_web_contract_fields() {
    let payload = serde_json::json!({
        "source_family": "web",
        "source_kind": "web",
        "web_seed_url": "https://docs.example.com",
        "source_canonical_uri": "https://docs.example.com",
        "item_canonical_uri": "https://docs.example.com/page"
    });

    assert_eq!(
        payload_origin(&payload),
        Some((
            "web".to_string(),
            "https://docs.example.com".to_string(),
            None
        ))
    );
}

#[test]
fn payload_origin_keeps_legacy_seed_markers_for_migration_diagnostics() {
    let payload = serde_json::json!({
        "source_type": "github",
        "seed_url": "owner/repo",
        "url": "https://github.com/owner/repo"
    });

    assert_eq!(
        payload_origin(&payload),
        Some(("github".to_string(), "owner/repo".to_string(), None))
    );
}

#[test]
fn payload_origin_carries_unified_source_id() {
    let payload = serde_json::json!({
        "source_id": "src_0272b3e7006f0910",
        "source_kind": "web",
        "web_seed_url": "https://docs.example.com"
    });

    assert_eq!(
        payload_origin(&payload),
        Some((
            "web".to_string(),
            "https://docs.example.com".to_string(),
            Some("src_0272b3e7006f0910".to_string())
        ))
    );
}

#[test]
fn filter_matches_source_type_or_seed_substring() {
    let gh = origin("github", "octocat/hello");
    let web = origin("embed", "https://docs.rs/serde");

    assert!(matches_filter(&gh, None));
    assert!(matches_filter(&gh, Some("github")));
    assert!(!matches_filter(&gh, Some("reddit")));

    // Domain/substring narrowing against the seed URL.
    assert!(matches_filter(&web, Some("docs.rs")));
    assert!(matches_filter(&web, Some("DOCS.RS"))); // case-insensitive
    assert!(!matches_filter(&web, Some("github")));

    // Empty/whitespace filter behaves like no filter.
    assert!(matches_filter(&web, Some("   ")));
}

#[test]
fn plan_counts_by_action() {
    let plan = RefreshPlan {
        origins: vec![
            origin("embed", "https://a.example.com"),
            origin("github", "owner/repo"),
            origin("reddit", "r/rust"),
            origin("sessions", "all"),
        ],
    };
    assert_eq!(plan.crawl_count(), 1);
    assert_eq!(plan.ingest_count(), 2);
    assert_eq!(plan.skip_count(), 1);
}

#[tokio::test]
async fn execute_refresh_fails_legacy_web_origin_without_crawl_enqueue() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = dir.path().join("jobs.db");
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(NoopServiceRuntime));
    let plan = RefreshPlan {
        origins: vec![origin("embed", "https://docs.example.com")],
    };

    let outcome = execute_refresh(&cfg, &ctx, &plan).await.expect("refresh");

    assert_eq!(outcome.crawl_enqueued, 0);
    assert_eq!(outcome.ingest_enqueued, 0);
    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].0, "https://docs.example.com");
    assert!(
        outcome.failures[0].1.contains("ledger registration"),
        "expected migration-required failure, got: {:?}",
        outcome.failures
    );
}

#[tokio::test]
async fn execute_refresh_fails_ledger_web_origin_without_unified_job_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = dir.path().join("jobs.db");
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(NoopServiceRuntime));
    let mut web = origin("embed", "https://docs.example.com");
    web.ledger_source_id = Some("src_web".to_string());
    let plan = RefreshPlan { origins: vec![web] };

    let outcome = execute_refresh(&cfg, &ctx, &plan).await.expect("refresh");

    assert_eq!(outcome.crawl_enqueued, 0);
    assert_eq!(outcome.failures.len(), 1);
    assert!(
        outcome.failures[0].1.contains("unified job store"),
        "expected unified-store failure, got: {:?}",
        outcome.failures
    );
}

// --- Ledger-driven discovery (issue #298 WS-B) ---------------------------

fn ledger_source(id: &str, kind: SourceKind, uri: &str, chunks: u64) -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new(id),
        canonical_uri: uri.to_string(),
        display_name: id.to_string(),
        source_kind: kind,
        adapter: AdapterRef {
            name: "test".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::Verified,
        status: LifecycleStatus::Completed,
        counts: SourceCounts {
            items_total: 1,
            items_changed: 0,
            documents_total: 1,
            chunks_total: chunks,
            vector_points_total: chunks,
            bytes_total: 100,
        },
        created_at: Timestamp::from(chrono::Utc::now()),
        updated_at: Timestamp::from(chrono::Utc::now()),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        tags: Vec::new(),
        user_label: None,
    }
}

/// Build a `ServiceContext` wired to `ledger` via a fake target local-source
/// runtime (fake jobs/embedding/vector stores — only the ledger matters for
/// discovery tests).
fn context_with_ledger(ledger: FakeLedgerStore) -> ServiceContext {
    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    ServiceContext::from_runtime(cfg, service_jobs).with_target_local_source_runtime(
        TargetLocalSourceRuntime::new(
            source_jobs,
            Arc::new(ledger),
            embedder,
            vectors,
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ),
    )
}

#[test]
fn classify_action_for_kind_matches_source_pipeline_crosswalk() {
    assert_eq!(
        classify_action_for_kind(SourceKind::Web),
        RefreshAction::Crawl
    );
    assert_eq!(
        classify_action_for_kind(SourceKind::Registry),
        RefreshAction::Crawl
    );
    for kind in [
        SourceKind::Git,
        SourceKind::Feed,
        SourceKind::Youtube,
        SourceKind::Reddit,
    ] {
        assert_eq!(classify_action_for_kind(kind), RefreshAction::Ingest);
    }
    for kind in [
        SourceKind::Local,
        SourceKind::Session,
        SourceKind::Memory,
        SourceKind::Upload,
        SourceKind::CliTool,
        SourceKind::McpTool,
    ] {
        assert!(matches!(
            classify_action_for_kind(kind),
            RefreshAction::Skip(_)
        ));
    }
}

#[test]
fn refresh_origin_from_source_carries_ledger_id_and_counts() {
    let source = ledger_source("src_web", SourceKind::Web, "https://docs.example.com", 7);
    let origin = refresh_origin_from_source(source);
    assert_eq!(origin.seed_url, "https://docs.example.com");
    assert_eq!(origin.source_type, "web");
    assert_eq!(origin.chunks, 7);
    assert_eq!(origin.action, RefreshAction::Crawl);
    assert_eq!(origin.ledger_source_id, Some("src_web".to_string()));
}

#[tokio::test]
async fn ledger_registered_sources_is_none_without_service_context() {
    assert_eq!(ledger_registered_sources(None, 100).await, None);
}

#[tokio::test]
async fn ledger_registered_sources_is_none_without_target_runtime() {
    let cfg = Arc::new(Config::test_default());
    let ctx = ServiceContext::from_runtime(cfg, Arc::new(NoopServiceRuntime));
    assert_eq!(ledger_registered_sources(Some(&ctx), 100).await, None);
}

#[tokio::test]
async fn ledger_registered_sources_is_empty_when_ledger_has_no_sources() {
    let ctx = context_with_ledger(FakeLedgerStore::new());
    let sources = ledger_registered_sources(Some(&ctx), 100)
        .await
        .expect("ledger reachable");
    assert!(sources.is_empty());
}

#[tokio::test]
async fn ledger_registered_sources_returns_all_registered_sources() {
    let ledger = FakeLedgerStore::new();
    ledger
        .upsert_source(ledger_source(
            "src_web",
            SourceKind::Web,
            "https://docs.example.com",
            3,
        ))
        .await
        .unwrap();
    ledger
        .upsert_source(ledger_source("src_git", SourceKind::Git, "owner/repo", 5))
        .await
        .unwrap();
    let ctx = context_with_ledger(ledger);

    let mut sources = ledger_registered_sources(Some(&ctx), 100)
        .await
        .expect("ledger reachable");
    sources.sort_by(|a, b| a.source_id.0.cmp(&b.source_id.0));
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].source_id, SourceId::new("src_git"));
    assert_eq!(sources[1].source_id, SourceId::new("src_web"));
}

#[tokio::test]
async fn plan_refresh_uses_ledger_driven_discovery_when_sources_are_registered() {
    let ledger = FakeLedgerStore::new();
    ledger
        .upsert_source(ledger_source(
            "src_web",
            SourceKind::Web,
            "https://docs.example.com",
            3,
        ))
        .await
        .unwrap();
    ledger
        .upsert_source(ledger_source("src_git", SourceKind::Git, "owner/repo", 5))
        .await
        .unwrap();
    ledger
        .upsert_source(ledger_source(
            "src_local",
            SourceKind::Local,
            "/home/user/notes",
            1,
        ))
        .await
        .unwrap();
    let ctx = context_with_ledger(ledger);
    let cfg = Config::test_default();

    // This must never reach the Qdrant facet path (no network available in
    // this test process) — ledger discovery finds >=1 source and short-
    // circuits before `facet_discovered_origins` is ever called.
    let plan = plan_refresh(&cfg, None, Some(&ctx))
        .await
        .expect("ledger-driven plan_refresh must not touch Qdrant");

    assert_eq!(plan.origins.len(), 3);
    assert_eq!(plan.crawl_count(), 1);
    assert_eq!(plan.ingest_count(), 1);
    assert_eq!(plan.skip_count(), 1);
    for origin in &plan.origins {
        assert!(origin.ledger_source_id.is_some());
    }
}

#[tokio::test]
async fn plan_refresh_applies_filter_to_ledger_discovered_origins() {
    let ledger = FakeLedgerStore::new();
    ledger
        .upsert_source(ledger_source(
            "src_web",
            SourceKind::Web,
            "https://docs.example.com",
            3,
        ))
        .await
        .unwrap();
    ledger
        .upsert_source(ledger_source("src_git", SourceKind::Git, "owner/repo", 5))
        .await
        .unwrap();
    let ctx = context_with_ledger(ledger);
    let cfg = Config::test_default();

    let plan = plan_refresh(&cfg, Some("git"), Some(&ctx))
        .await
        .expect("ledger-driven plan_refresh must not touch Qdrant");

    assert_eq!(plan.origins.len(), 1);
    assert_eq!(plan.origins[0].seed_url, "owner/repo");
}
