use super::*;

use std::fs;
use std::path::{Path, PathBuf};

const TARGET_URL: &str = "https://example.com/feed.xml";

const RSS_TWO_ITEMS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <link>https://example.com/</link>
  <item>
    <title>First Post</title>
    <link>https://example.com/a</link>
    <description>Hello &lt;b&gt;world&lt;/b&gt;</description>
    <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
  </item>
  <item>
    <title>Second Post</title>
    <link>https://example.com/b</link>
    <description>Body two</description>
  </item>
</channel></rss>"#;

const ATOM_ONE_ENTRY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Example</title>
  <link href="https://example.com/atom/"/>
  <entry>
    <title>Atom Post</title>
    <link href="https://example.com/x"/>
    <content type="html">&lt;p&gt;Hi there&lt;/p&gt;</content>
  </entry>
</feed>"#;

const MALFORMED_XML: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Broken</title>
  <item>
    <title>Truncated"#;

const EMPTY_RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Empty Feed</title>
  <link>https://example.com/</link>
</channel></rss>"#;

fn fixture_feed_file(contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("axon-feed-test-{}.xml", Uuid::new_v4()));
    fs::write(&path, contents).unwrap();
    path
}

fn feed_plan(feed_path: &Path, scope: SourceScope, with_feed_path: bool) -> SourcePlan {
    let mut values = MetadataMap::new();
    if with_feed_path {
        values.insert(
            "feed_path".to_string(),
            feed_path.to_string_lossy().to_string().into(),
        );
    }
    let adapter = AdapterRef {
        name: "feed".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298299)),
        request: SourceRequest::new(TARGET_URL.to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                source: TARGET_URL.to_string(),
                canonical_uri: TARGET_URL.to_string(),
                source_id: SourceId::from("src_feed_test"),
                source_kind: SourceKind::Feed,
                adapter: adapter.clone(),
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "test".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:feed:options:v1".to_string(),
            validated_options: AdapterOptions { values },
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_feed_test"),
        provider_reservations: Vec::new(),
    }
}

fn diff_from(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added = items.len() as u64;
    SourceManifestDiff {
        header: stage_header(
            plan.job_id,
            "feed_diff",
            PipelinePhase::Diffing,
            items.len(),
        ),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_feed_test"),
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

#[tokio::test]
async fn capabilities_advertise_feed_scope() {
    let cap = FeedSourceAdapter::new().capabilities().await.unwrap();
    assert_eq!(
        cap.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::Feed))
    );
    assert!(cap.0.features.contains(&"scope:feed".to_string()));
    assert!(!cap.0.features.contains(&"scope:page".to_string()));
}

#[tokio::test]
async fn discover_rss_lists_one_item_per_entry() {
    let path = fixture_feed_file(RSS_TWO_ITEMS);
    let plan = feed_plan(&path, SourceScope::Feed, true);
    let manifest = FeedSourceAdapter::new().discover(&plan).await.unwrap();

    assert_eq!(manifest.items.len(), 2);
    assert!(
        manifest
            .items
            .iter()
            .all(|i| i.item_kind == ItemKind::FeedEntry)
    );
    let links: Vec<_> = manifest
        .items
        .iter()
        .map(|i| i.canonical_uri.clone())
        .collect();
    assert!(links.contains(&"https://example.com/a".to_string()));
    assert!(links.contains(&"https://example.com/b".to_string()));
    assert_eq!(
        manifest.metadata.get("feed_title").and_then(|v| v.as_str()),
        Some("Example Feed")
    );
    fs::remove_file(&path).ok();
}

#[tokio::test]
async fn discover_atom_lists_one_item() {
    let path = fixture_feed_file(ATOM_ONE_ENTRY);
    let plan = feed_plan(&path, SourceScope::Feed, true);
    let manifest = FeedSourceAdapter::new().discover(&plan).await.unwrap();

    assert_eq!(manifest.items.len(), 1);
    assert_eq!(manifest.items[0].canonical_uri, "https://example.com/x");
    fs::remove_file(&path).ok();
}

#[tokio::test]
async fn acquire_then_normalize_stamps_feed_metadata() {
    let path = fixture_feed_file(RSS_TWO_ITEMS);
    let plan = feed_plan(&path, SourceScope::Feed, true);
    let adapter = FeedSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), manifest.items.len());

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let first = normalized
        .data
        .iter()
        .find(|d| d.canonical_uri == "https://example.com/a")
        .expect("entry a present");
    assert_eq!(
        first.metadata.get("source_type").and_then(|v| v.as_str()),
        Some("feed")
    );
    assert_eq!(
        first.metadata.get("feed_title").and_then(|v| v.as_str()),
        Some("Example Feed")
    );
    assert_eq!(
        first
            .metadata
            .get("feed_entry_published")
            .and_then(|v| v.as_str()),
        Some("2024-01-01T00:00:00+00:00")
    );
    assert!(
        matches!(&first.content, ContentRef::InlineText { text } if text.contains("Hello world"))
    );
    fs::remove_file(&path).ok();
}

#[tokio::test]
async fn discover_without_feed_path_option_errors() {
    let plan = feed_plan(Path::new("/does/not/matter"), SourceScope::Feed, false);
    let err = FeedSourceAdapter::new().discover(&plan).await.unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.feed.feed_path.required");
}

#[tokio::test]
async fn discover_rejects_unsupported_scope() {
    let path = fixture_feed_file(RSS_TWO_ITEMS);
    let plan = feed_plan(&path, SourceScope::Page, true);
    let err = FeedSourceAdapter::new().discover(&plan).await.unwrap_err();
    assert!(err.code.to_string().contains("scope"));
    fs::remove_file(&path).ok();
}

#[tokio::test]
async fn discover_malformed_feed_degrades_gracefully() {
    let path = fixture_feed_file(MALFORMED_XML);
    let plan = feed_plan(&path, SourceScope::Feed, true);
    let err = FeedSourceAdapter::new().discover(&plan).await.unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.feed.parse_failed");
    fs::remove_file(&path).ok();
}

#[tokio::test]
async fn discover_empty_feed_yields_zero_items_without_panic() {
    let path = fixture_feed_file(EMPTY_RSS);
    let plan = feed_plan(&path, SourceScope::Feed, true);
    let manifest = FeedSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 0);
    fs::remove_file(&path).ok();
}
