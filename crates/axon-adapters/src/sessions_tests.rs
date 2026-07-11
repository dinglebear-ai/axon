use super::*;

use std::fs;
use std::path::{Path, PathBuf};

const CLAUDE_TARGET: &str = "session:claude:abc123";
const CODEX_TARGET: &str = "session:codex:def456";
const GEMINI_TARGET: &str = "session:gemini:ghi789";

fn temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("axon-session-test-{label}-{}", Uuid::new_v4()))
}

fn fixture_claude_dir() -> PathBuf {
    let dir = temp_dir("claude");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("session.jsonl"),
        concat!(
            r#"{"type":"user","cwd":"/home/j/proj","gitBranch":"main","timestamp":"2026-01-01T00:00:00Z","message":{"content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"claude-x","content":[{"type":"text","text":"hi there"}]}}"#,
        ),
    )
    .unwrap();
    dir
}

fn fixture_codex_dir() -> PathBuf {
    let dir = temp_dir("codex");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("rollout.jsonl"),
        concat!(
            r#"{"type":"session_meta","payload":{"cwd":"/home/j/proj","model":"gpt-5-codex"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"do the thing"}]}}"#,
        ),
    )
    .unwrap();
    dir
}

fn fixture_gemini_dir() -> PathBuf {
    let dir = temp_dir("gemini");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("chat.json"),
        r#"{"messages":[{"type":"human","content":[{"text":"What is the capital of France?"}]},{"type":"model","content":[{"text":"Paris."}]}]}"#,
    )
    .unwrap();
    dir
}

fn fixture_degraded_claude_dir() -> PathBuf {
    let dir = temp_dir("degraded");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("broken.jsonl"), "not json\nalso not json\n").unwrap();
    dir
}

fn fixture_empty_dir() -> PathBuf {
    let dir = temp_dir("empty");
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn session_plan(
    target: &str,
    sessions_root: &Path,
    scope: SourceScope,
    with_root: bool,
) -> SourcePlan {
    let mut values = MetadataMap::new();
    if with_root {
        values.insert(
            "sessions_root".to_string(),
            sessions_root.to_string_lossy().to_string().into(),
        );
    }
    let adapter = AdapterRef {
        name: "session".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298298)),
        request: SourceRequest::new(target.to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                source: target.to_string(),
                canonical_uri: format!("session://{}", target.trim_start_matches("session:")),
                source_id: SourceId::from("src_session_test"),
                source_kind: SourceKind::Session,
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
            option_schema_id: "adapter:session:options:v1".to_string(),
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_session_test"),
        provider_reservations: Vec::new(),
    }
}

fn diff_from(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added = items.len() as u64;
    SourceManifestDiff {
        header: stage_header(
            plan.job_id,
            "session_diff",
            PipelinePhase::Diffing,
            items.len(),
        ),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_session_test"),
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
async fn capabilities_advertise_session_thread_scope() {
    let cap = SessionSourceAdapter::new().capabilities().await.unwrap();
    assert!(cap.0.features.contains(&"scope:thread".to_string()));
    assert!(cap.0.features.contains(&"scope:file".to_string()));
    assert!(!cap.0.features.contains(&"scope:page".to_string()));
}

#[tokio::test]
async fn discover_lists_claude_jsonl_files() {
    let root = fixture_claude_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();
    let keys: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|i| i.display_path.clone())
        .collect();
    assert!(keys.contains(&"session.jsonl".to_string()));
    assert!(
        manifest
            .items
            .iter()
            .all(|i| i.item_kind == ItemKind::Transcript)
    );
    assert_eq!(
        manifest
            .metadata
            .get("session_provider")
            .and_then(|v| v.as_str()),
        Some("claude")
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn acquire_then_normalize_claude_session_stamps_metadata() {
    let root = fixture_claude_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    let adapter = SessionSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), manifest.items.len());

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let doc = normalized
        .data
        .iter()
        .find(|d| d.path.as_deref() == Some("session.jsonl"))
        .expect("session document present");
    assert_eq!(
        doc.metadata.get("source_type").and_then(|v| v.as_str()),
        Some("session")
    );
    assert_eq!(
        doc.metadata
            .get("session_provider")
            .and_then(|v| v.as_str()),
        Some("claude")
    );
    assert_eq!(
        doc.metadata.get("session_id").and_then(|v| v.as_str()),
        Some("abc123")
    );
    assert_eq!(
        doc.metadata
            .get("session_turn_count")
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        doc.metadata.get("session_model").and_then(|v| v.as_str()),
        Some("claude-x")
    );
    assert_eq!(doc.content_kind, ContentKind::Transcript);
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn acquire_then_normalize_codex_session_stamps_metadata() {
    let root = fixture_codex_dir();
    let plan = session_plan(CODEX_TARGET, &root, SourceScope::Thread, true);
    let adapter = SessionSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let doc = normalized
        .data
        .iter()
        .find(|d| d.path.as_deref() == Some("rollout.jsonl"))
        .expect("session document present");
    assert_eq!(
        doc.metadata
            .get("session_provider")
            .and_then(|v| v.as_str()),
        Some("codex")
    );
    assert_eq!(
        doc.metadata.get("session_model").and_then(|v| v.as_str()),
        Some("gpt-5-codex")
    );
    assert_eq!(
        doc.metadata
            .get("session_workspace_path")
            .and_then(|v| v.as_str()),
        Some("/home/j/proj")
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn acquire_then_normalize_gemini_session_stamps_metadata() {
    let root = fixture_gemini_dir();
    let plan = session_plan(GEMINI_TARGET, &root, SourceScope::Thread, true);
    let adapter = SessionSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let doc = normalized
        .data
        .iter()
        .find(|d| d.path.as_deref() == Some("chat.json"))
        .expect("session document present");
    assert_eq!(
        doc.metadata
            .get("session_provider")
            .and_then(|v| v.as_str()),
        Some("gemini")
    );
    assert!(matches!(
        &doc.content,
        ContentRef::InlineText { text } if text.contains("Paris")
    ));
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn normalize_degraded_claude_file_still_produces_a_document() {
    // A malformed JSONL file decodes to an empty session (zero turns) rather
    // than failing the whole pipeline — matches legacy "skip malformed lines,
    // keep going" behavior. The caller can drop empty-text documents upstream.
    let root = fixture_degraded_claude_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    let adapter = SessionSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.data.len(), 1);
    let doc = &normalized.data[0];
    assert_eq!(
        doc.metadata
            .get("session_turn_count")
            .and_then(|v| v.as_u64()),
        Some(0)
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_on_empty_directory_returns_no_items() {
    let root = fixture_empty_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();
    assert!(manifest.items.is_empty());
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_without_sessions_root_option_errors() {
    let plan = session_plan(
        CLAUDE_TARGET,
        Path::new("/does/not/matter"),
        SourceScope::Thread,
        false,
    );
    let err = SessionSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert_eq!(
        err.code.to_string(),
        "adapter.session.sessions_root.required"
    );
}

#[tokio::test]
async fn discover_rejects_unsupported_scope() {
    let root = fixture_claude_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Page, true);
    let err = SessionSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert!(err.code.to_string().contains("scope"));
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_rejects_malformed_session_target() {
    let root = fixture_claude_dir();
    let plan = session_plan("not-a-session-target", &root, SourceScope::Thread, true);
    let err = SessionSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap_err();
    assert!(err.code.to_string().starts_with("adapter.session.target"));
    fs::remove_dir_all(&root).ok();
}
