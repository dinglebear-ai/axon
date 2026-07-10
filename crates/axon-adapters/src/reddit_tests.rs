use super::*;

use std::fs;
use std::path::{Path, PathBuf};

const TARGET_SUBREDDIT: &str = "rust";

fn sample_dump() -> serde_json::Value {
    serde_json::json!([
        {
            "title": "Rust chunking",
            "selftext": "Post body",
            "permalink": "/r/rust/comments/abc123/rust_chunking/",
            "author": "alice",
            "score": 42,
            "subreddit": "rust",
            "domain": "self.rust",
            "num_comments": 2,
            "upvote_ratio": 0.97,
            "is_video": false,
            "distinguished": null,
            "gilded": 0,
            "link_flair_text": "Discussion",
            "created_utc": 1767225600,
            "comments": [
                {"body": "Great post!", "parent_text": null},
                {"body": "Agreed.", "parent_text": "Great post!"}
            ]
        },
        {
            "title": "Second post",
            "selftext": "",
            "permalink": "/r/rust/comments/def456/second_post/",
            "author": "bob",
            "score": 7,
            "subreddit": "rust",
            "domain": "self.rust",
            "num_comments": 0,
            "upvote_ratio": 0.5,
            "is_video": false,
            "distinguished": null,
            "gilded": 0,
            "link_flair_text": null,
            "created_utc": 1767225700,
            "comments": []
        }
    ])
}

fn write_dump(dir: &Path, value: &serde_json::Value) -> PathBuf {
    let path = dir.join("dump.json");
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-reddit-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn reddit_plan(dump_path: Option<&Path>, scope: SourceScope, target: &str) -> SourcePlan {
    let mut values = MetadataMap::new();
    if let Some(dump_path) = dump_path {
        values.insert(
            "reddit_dump_path".to_string(),
            dump_path.to_string_lossy().to_string().into(),
        );
    }
    let adapter = AdapterRef {
        name: "reddit".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298298)),
        request: SourceRequest::new(target.to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                source: target.to_string(),
                canonical_uri: format!("reddit://r/{target}"),
                source_id: SourceId::from("src_reddit_test"),
                source_kind: SourceKind::Reddit,
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
            safety_class: SafetyClass::AuthenticatedNetwork,
            option_schema_id: "adapter:reddit:options:v1".to_string(),
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_reddit_test"),
        provider_reservations: Vec::new(),
    }
}

fn diff_from(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added = items.len() as u64;
    SourceManifestDiff {
        header: stage_header(
            plan.job_id,
            "reddit_diff",
            PipelinePhase::Diffing,
            items.len(),
        ),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_reddit_test"),
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
async fn capabilities_advertise_subreddit_and_thread_scopes() {
    let cap = RedditSourceAdapter::new().capabilities().await.unwrap();
    assert!(cap.0.features.contains(&"scope:subreddit".to_string()));
    assert!(cap.0.features.contains(&"scope:thread".to_string()));
    assert!(!cap.0.features.contains(&"scope:page".to_string()));
}

#[tokio::test]
async fn discover_lists_posts_from_valid_dump() {
    let dir = temp_dir();
    let dump_path = write_dump(&dir, &sample_dump());
    let plan = reddit_plan(Some(&dump_path), SourceScope::Subreddit, TARGET_SUBREDDIT);
    let manifest = RedditSourceAdapter::new().discover(&plan).await.unwrap();

    assert_eq!(manifest.items.len(), 2);
    let keys: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.clone())
        .collect();
    assert!(keys.contains(&"r/rust/comments/abc123/rust_chunking".to_string()));
    assert!(keys.contains(&"r/rust/comments/def456/second_post".to_string()));
    assert!(
        manifest
            .items
            .iter()
            .all(|item| item.item_kind == ItemKind::FeedEntry)
    );
    assert_eq!(
        manifest
            .metadata
            .get("reddit_target_kind")
            .and_then(|v| v.as_str()),
        Some("subreddit")
    );
    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn acquire_then_normalize_stamps_reddit_metadata() {
    let dir = temp_dir();
    let dump_path = write_dump(&dir, &sample_dump());
    let plan = reddit_plan(Some(&dump_path), SourceScope::Subreddit, TARGET_SUBREDDIT);
    let adapter = RedditSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), manifest.items.len());

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let post = normalized
        .data
        .iter()
        .find(|doc| doc.path.as_deref() == Some("r/rust/comments/abc123/rust_chunking"))
        .expect("post document present");

    assert_eq!(
        post.metadata.get("source_type").and_then(|v| v.as_str()),
        Some("reddit")
    );
    assert_eq!(
        post.metadata.get("source_kind").and_then(|v| v.as_str()),
        Some("reddit")
    );
    assert_eq!(
        post.metadata.get("reddit_author").and_then(|v| v.as_str()),
        Some("alice")
    );
    assert_eq!(
        post.metadata.get("reddit_score").and_then(|v| v.as_i64()),
        Some(42)
    );
    assert_eq!(
        post.metadata
            .get("reddit_subreddit")
            .and_then(|v| v.as_str()),
        Some("rust")
    );
    assert_eq!(
        post.metadata
            .get("reddit_num_comments")
            .and_then(|v| v.as_u64()),
        Some(2)
    );
    assert_eq!(post.title.as_deref(), Some("Rust chunking"));
    assert!(matches!(
        &post.content,
        ContentRef::InlineText { text } if text.contains("Post body") && text.contains("Great post!")
    ));
    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn discover_without_dump_path_option_errors() {
    let plan = reddit_plan(None, SourceScope::Subreddit, TARGET_SUBREDDIT);
    let err = RedditSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.dump_path.required");
}

#[tokio::test]
async fn discover_rejects_unsupported_scope() {
    let dir = temp_dir();
    let dump_path = write_dump(&dir, &sample_dump());
    let plan = reddit_plan(Some(&dump_path), SourceScope::Page, TARGET_SUBREDDIT);
    let err = RedditSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert!(err.code.to_string().contains("scope"));
    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn discover_rejects_malformed_dump() {
    let dir = temp_dir();
    let dump_path = dir.join("dump.json");
    fs::write(&dump_path, b"{not valid json").unwrap();
    let plan = reddit_plan(Some(&dump_path), SourceScope::Subreddit, TARGET_SUBREDDIT);
    let err = RedditSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.dump_invalid");
    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn discover_rejects_missing_dump_file() {
    let dir = temp_dir();
    let missing_path = dir.join("does-not-exist.json");
    let plan = reddit_plan(
        Some(&missing_path),
        SourceScope::Subreddit,
        TARGET_SUBREDDIT,
    );
    let err = RedditSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.dump_read_failed");
    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn discover_handles_empty_dump_array() {
    let dir = temp_dir();
    let dump_path = write_dump(&dir, &serde_json::json!([]));
    let plan = reddit_plan(Some(&dump_path), SourceScope::Subreddit, TARGET_SUBREDDIT);
    let manifest = RedditSourceAdapter::new().discover(&plan).await.unwrap();
    assert!(manifest.items.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn discover_and_normalize_supports_thread_scope() {
    let dir = temp_dir();
    let dump_path = write_dump(&dir, &sample_dump());
    let plan = reddit_plan(
        Some(&dump_path),
        SourceScope::Thread,
        "r/rust/comments/abc123/rust_chunking/",
    );
    let adapter = RedditSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 2);
    assert_eq!(
        manifest
            .metadata
            .get("reddit_target_kind")
            .and_then(|v| v.as_str()),
        Some("thread")
    );
    fs::remove_dir_all(&dir).ok();
}
