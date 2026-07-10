use super::*;

use std::fs;
use std::path::{Path, PathBuf};

const TARGET_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

const DUMP_WITH_TWO_VIDEOS: &str = r#"{
  "videos": [
    {
      "video_id": "dQw4w9WgXcQ",
      "title": "Never Gonna Give You Up",
      "channel": "Rick Astley",
      "channel_url": "https://www.youtube.com/@RickAstleyYT",
      "uploader_id": "RickAstleyYT",
      "upload_date": "20091025",
      "description": "The official video.",
      "duration_string": "3:33",
      "view_count": 1000000,
      "like_count": 10000,
      "tags": ["music"],
      "categories": ["Music"],
      "thumbnail": "https://i.ytimg.com/vi/dQw4w9WgXcQ/default.jpg",
      "transcript": "Never gonna give you up, never gonna let you down"
    },
    {
      "video_id": "secondvid01",
      "title": "Second Video",
      "channel": "Rick Astley",
      "transcript": "This is the second transcript"
    }
  ]
}"#;

fn dump_file(contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-youtube-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dump.json");
    fs::write(&path, contents).unwrap();
    path
}

fn youtube_plan(dump_path: &Path, scope: SourceScope, with_dump_path: bool) -> SourcePlan {
    let mut values = MetadataMap::new();
    if with_dump_path {
        values.insert(
            "youtube_dump_path".to_string(),
            dump_path.to_string_lossy().to_string().into(),
        );
    }
    let adapter = AdapterRef {
        name: "youtube".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298298)),
        request: SourceRequest::new(TARGET_URL.to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                source: TARGET_URL.to_string(),
                canonical_uri: TARGET_URL.to_string(),
                source_id: SourceId::from("src_youtube_test"),
                source_kind: SourceKind::Youtube,
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
            option_schema_id: "adapter:youtube:options:v1".to_string(),
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_youtube_test"),
        provider_reservations: Vec::new(),
    }
}

fn diff_from(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added = items.len() as u64;
    SourceManifestDiff {
        header: stage_header(
            plan.job_id,
            "youtube_diff",
            PipelinePhase::Diffing,
            items.len(),
        ),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_youtube_test"),
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
async fn capabilities_advertise_video_and_channel_scope() {
    let cap = YoutubeSourceAdapter::new().capabilities().await.unwrap();
    assert!(cap.0.features.contains(&"scope:video".to_string()));
    assert!(cap.0.features.contains(&"scope:channel".to_string()));
    assert!(!cap.0.features.contains(&"scope:page".to_string()));
}

#[tokio::test]
async fn discover_lists_one_manifest_item_per_video() {
    let dump = dump_file(DUMP_WITH_TWO_VIDEOS);
    let plan = youtube_plan(&dump, SourceScope::Video, true);
    let manifest = YoutubeSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 2);
    let ids: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|i| i.display_path.clone())
        .collect();
    assert!(ids.contains(&"dQw4w9WgXcQ".to_string()));
    assert!(ids.contains(&"secondvid01".to_string()));
    assert!(
        manifest
            .items
            .iter()
            .all(|i| i.item_kind == ItemKind::Transcript)
    );
    fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn discover_on_empty_dump_yields_no_items() {
    let dump = dump_file(r#"{"videos": []}"#);
    let plan = youtube_plan(&dump, SourceScope::Channel, true);
    let manifest = YoutubeSourceAdapter::new().discover(&plan).await.unwrap();
    assert!(manifest.items.is_empty());
    fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn discover_on_malformed_dump_errors() {
    let dump = dump_file("{ not json ");
    let plan = youtube_plan(&dump, SourceScope::Video, true);
    let err = YoutubeSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.youtube.dump.invalid");
    fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn discover_without_dump_path_option_errors() {
    let plan = youtube_plan(Path::new("/does/not/matter"), SourceScope::Video, false);
    let err = YoutubeSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert_eq!(
        err.code.to_string(),
        "adapter.youtube.youtube_dump_path.required"
    );
}

#[tokio::test]
async fn discover_rejects_unsupported_scope() {
    let dump = dump_file(DUMP_WITH_TWO_VIDEOS);
    let plan = youtube_plan(&dump, SourceScope::Page, true);
    let err = YoutubeSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert!(err.code.to_string().contains("scope"));
    fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn acquire_then_normalize_stamps_youtube_metadata() {
    let dump = dump_file(DUMP_WITH_TWO_VIDEOS);
    let plan = youtube_plan(&dump, SourceScope::Video, true);
    let adapter = YoutubeSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), manifest.items.len());

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.data.len(), 2);

    let rick = normalized
        .data
        .iter()
        .find(|d| d.metadata.get("video_id").and_then(|v| v.as_str()) == Some("dQw4w9WgXcQ"))
        .expect("first video document present");
    assert_eq!(
        rick.metadata.get("source_type").and_then(|v| v.as_str()),
        Some("youtube")
    );
    assert_eq!(
        rick.metadata.get("source_kind").and_then(|v| v.as_str()),
        Some("youtube")
    );
    assert_eq!(
        rick.metadata.get("channel").and_then(|v| v.as_str()),
        Some("Rick Astley")
    );
    assert_eq!(
        rick.metadata.get("yt_upload_date").and_then(|v| v.as_str()),
        Some("20091025")
    );
    assert_eq!(rick.title.as_deref(), Some("Never Gonna Give You Up"));
    assert!(
        matches!(&rick.content, ContentRef::InlineText { text } if text.contains("Never gonna give you up"))
    );

    let second = normalized
        .data
        .iter()
        .find(|d| d.metadata.get("video_id").and_then(|v| v.as_str()) == Some("secondvid01"))
        .expect("second video document present");
    assert_eq!(second.title.as_deref(), Some("Second Video"));
    // Fields absent from the dump entry must not appear in the document metadata.
    assert!(second.metadata.get("yt_upload_date").is_none());
    assert!(second.metadata.get("channel_url").is_none());

    fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn normalize_channel_scope_video() {
    // Channel-scoped plans still normalize per-video documents once the
    // bridge has resolved individual videos into the dump.
    let dump = dump_file(DUMP_WITH_TWO_VIDEOS);
    let plan = youtube_plan(&dump, SourceScope::Channel, true);
    let adapter = YoutubeSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.data.len(), 2);
    assert!(
        normalized
            .data
            .iter()
            .all(|d| d.metadata.get("source_scope").and_then(|v| v.as_str()) == Some("channel"))
    );
    fs::remove_dir_all(dump.parent().unwrap()).ok();
}
