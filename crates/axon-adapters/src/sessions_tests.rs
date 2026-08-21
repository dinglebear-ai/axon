use super::*;
use crate::sessions::project_filter::matches_project_filter;

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

fn fixture_mixed_extension_dir() -> PathBuf {
    let dir = fixture_claude_dir();
    fs::write(
        dir.join("chat.json"),
        r#"{"messages":[{"type":"human","content":[{"text":"wrong provider"}]}]}"#,
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
        name: "sessions".to_string(),
        version: crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION.to_string(),
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
            option_schema_id: "adapter:sessions:options:v1".to_string(),
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

fn insert_project_filter(plan: &mut SourcePlan, project: &str) {
    plan.route
        .validated_options
        .values
        .insert("project_filter".to_string(), serde_json::json!(project));
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
    assert!(
        manifest
            .items
            .iter()
            .all(|item| item.version.as_deref() == Some(SESSION_DOCUMENT_VERSION))
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_applies_max_items_before_hashing_full_session_tree() {
    let root = temp_dir("claude-max-items");
    fs::create_dir_all(&root).unwrap();
    for name in ["z-last.jsonl", "a-first.jsonl", "m-middle.jsonl"] {
        fs::write(
            root.join(name),
            r#"{"type":"user","message":{"content":"hello"}}"#,
        )
        .unwrap();
    }
    let mut plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    plan.limits.effective.max_items = Some(2);

    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["a-first.jsonl", "m-middle.jsonl"]);
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_filters_session_extensions_by_provider() {
    let root = fixture_mixed_extension_dir();
    for (target, expected, rejected) in [
        (CLAUDE_TARGET, "session.jsonl", "chat.json"),
        (CODEX_TARGET, "session.jsonl", "chat.json"),
        (GEMINI_TARGET, "chat.json", "session.jsonl"),
    ] {
        let plan = session_plan(target, &root, SourceScope::Thread, true);
        let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();
        let keys: Vec<_> = manifest
            .items
            .iter()
            .filter_map(|i| i.display_path.as_deref())
            .collect();
        assert!(
            keys.contains(&expected),
            "{target} should include {expected}"
        );
        assert!(
            !keys.contains(&rejected),
            "{target} should reject {rejected}"
        );
    }
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_applies_project_filter_to_manifest_items() {
    let root = temp_dir("claude-projects");
    let axon_dir = root.join("-home-j-workspace-axon");
    let other_dir = root.join("-home-j-workspace-other");
    fs::create_dir_all(&axon_dir).unwrap();
    fs::create_dir_all(&other_dir).unwrap();
    fs::write(
        axon_dir.join("axon-session.jsonl"),
        r#"{"type":"user","message":{"content":"axon"}}"#,
    )
    .unwrap();
    fs::write(
        other_dir.join("other-session.jsonl"),
        r#"{"type":"user","message":{"content":"other"}}"#,
    )
    .unwrap();

    let mut plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    insert_project_filter(&mut plan, "/home/j/workspace/axon");
    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();
    let keys: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect();

    assert_eq!(keys, vec!["-home-j-workspace-axon/axon-session.jsonl"]);
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_applies_project_filter_to_codex_cwd_content() {
    let root = fixture_codex_dir();
    let mut plan = session_plan(CODEX_TARGET, &root, SourceScope::Thread, true);
    insert_project_filter(&mut plan, "/home/j/proj");

    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();
    let keys: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect();

    assert_eq!(keys, vec!["rollout.jsonl"]);
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn codex_project_filter_uses_session_metadata_not_arbitrary_transcript_text() {
    let root = temp_dir("codex-project-filter");
    fs::create_dir_all(&root).unwrap();
    let file = root.join("rollout.jsonl");
    fs::write(
        &file,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/home/j/other\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"text\":\"mentions /home/j/proj later\"}}\n"
        ),
    )
    .unwrap();
    assert!(
        !matches_project_filter(Some("/home/j/proj"), &root, &file, "rollout.jsonl",),
        "the matcher must inspect structured project metadata, not the whole transcript"
    );
    let mut plan = session_plan(CODEX_TARGET, &root, SourceScope::Thread, true);
    insert_project_filter(&mut plan, "/home/j/proj");

    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();

    assert!(
        manifest.items.is_empty(),
        "project selection must not scan arbitrary transcript bodies"
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discover_project_filter_keeps_gemini_unmatched_when_export_lacks_project() {
    let root = fixture_gemini_dir();
    let mut plan = session_plan(GEMINI_TARGET, &root, SourceScope::Thread, true);
    insert_project_filter(&mut plan, "axon");

    let manifest = SessionSourceAdapter::new().discover(&plan).await.unwrap();

    assert!(manifest.items.is_empty());
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
    let doc = normalized.data.first().expect("session document present");
    assert_eq!(
        doc.metadata.get("source_family").and_then(|v| v.as_str()),
        Some("session")
    );
    assert!(!doc.metadata.contains_key("source_type"));
    assert_eq!(
        doc.metadata
            .get("session_provider")
            .and_then(|v| v.as_str()),
        Some("claude")
    );
    let projected_session_id = doc
        .metadata
        .get("session_id")
        .and_then(|v| v.as_str())
        .expect("opaque session id");
    assert!(projected_session_id.starts_with("session_"));
    assert_ne!(projected_session_id, "abc123");
    assert!(!doc.metadata.contains_key("session_turn_count"));
    assert!(!doc.metadata.contains_key("session_model"));
    assert_eq!(doc.content_kind, ContentKind::PlainText);
    assert!(
        doc.canonical_uri
            .starts_with("session://claude/doc_session_")
    );
    assert!(!doc.canonical_uri.contains(root.to_string_lossy().as_ref()));
    assert_eq!(doc.title.as_deref(), Some("claude AI session"));
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn normalize_embeds_decoded_session_text_not_raw_transport_jsonl() {
    let root = fixture_claude_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    let adapter = SessionSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let acquisition = adapter
        .acquire(&plan, &diff_from(&plan, manifest.items))
        .await
        .unwrap();

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let document = &normalized.data[0];
    let ContentRef::InlineText { text } = &document.content else {
        panic!("normalized session content must be inline text");
    };

    assert_eq!(text, "\n\n### USER:\nhello\n\n### ASSISTANT:\nhi there");
    assert!(!text.contains(r#"{"type":"user""#));
    assert_eq!(document.content_kind, ContentKind::PlainText);
    assert_eq!(document.path, None);
    assert_eq!(
        document.chunk_hints.first().map(|hint| &hint.profile),
        Some(&ChunkProfile::SessionTurns)
    );
    assert!(document.parser_hints.is_empty());
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn normalize_marks_already_redacted_session_text_clean_for_retrieval() {
    let root = fixture_claude_dir();
    let plan = session_plan(CLAUDE_TARGET, &root, SourceScope::Thread, true);
    let adapter = SessionSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let acquisition = adapter
        .acquire(&plan, &diff_from(&plan, manifest.items))
        .await
        .unwrap();

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();

    assert_eq!(
        normalized.data[0]
            .metadata
            .get("redaction_status")
            .and_then(Value::as_str),
        Some("clean")
    );
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
    let doc = normalized.data.first().expect("session document present");
    assert_eq!(
        doc.metadata
            .get("session_provider")
            .and_then(|v| v.as_str()),
        Some("codex")
    );
    assert!(!doc.metadata.contains_key("session_model"));
    assert!(!doc.metadata.contains_key("session_workspace_path"));
    assert_eq!(doc.content_kind, ContentKind::PlainText);
    assert_eq!(doc.path, None);
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
    let doc = normalized.data.first().expect("session document present");
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
    assert_eq!(doc.content_kind, ContentKind::PlainText);
    assert_eq!(doc.path, None);
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
    assert!(!doc.metadata.contains_key("session_turn_count"));
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
