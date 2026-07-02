use super::*;

use std::fs;
use std::path::{Path, PathBuf};

const TARGET_URL: &str = "https://github.com/jmagar/fixture-repo";

fn fixture_repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-git-test-{}", Uuid::new_v4()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("README.md"), "# Fixture\n").unwrap();
    fs::write(dir.join("src/lib.rs"), "pub fn hi() {}\n").unwrap();
    // A .git directory that must be excluded from the walk.
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(dir.join(".git/config"), "[core]\n").unwrap();
    dir
}

fn git_plan(repo_root: &Path, scope: SourceScope, with_repo_root: bool) -> SourcePlan {
    let mut values = MetadataMap::new();
    if with_repo_root {
        values.insert(
            "repo_root".to_string(),
            repo_root.to_string_lossy().to_string().into(),
        );
    }
    let adapter = AdapterRef {
        name: "git".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298298)),
        request: SourceRequest::new(TARGET_URL.to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                requested_uri: TARGET_URL.to_string(),
                canonical_uri: "git://github.com/jmagar/fixture-repo".to_string(),
                source_id: SourceId::from("src_git_test"),
                source_kind: SourceKind::Git,
                display_name: "git test".to_string(),
                candidate_adapters: vec![AdapterCandidate {
                    adapter: adapter.clone(),
                    supported_scopes: vec![scope],
                    confidence: 1.0,
                    reason: "test".to_string(),
                }],
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "test".to_string(),
                authority_hint: None,
                warnings: Vec::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:git:options:v1".to_string(),
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_git_test"),
        provider_reservations: Vec::new(),
    }
}

fn diff_from(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added = items.len() as u64;
    SourceManifestDiff {
        header: stage_header(plan.job_id, "git_diff", PipelinePhase::Diffing, items.len()),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_git_test"),
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
async fn capabilities_advertise_git_repo_scope() {
    let cap = GitSourceAdapter::new().capabilities().await.unwrap();
    assert!(cap.validate_scope(SourceScope::Repo).is_ok());
    assert!(cap.validate_scope(SourceScope::Directory).is_ok());
    assert!(cap.validate_scope(SourceScope::Page).is_err());
}

#[tokio::test]
async fn discover_lists_repo_files_and_excludes_git_dir() {
    let repo = fixture_repo();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let keys: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|i| i.display_path.clone())
        .collect();
    assert!(keys.contains(&"README.md".to_string()));
    assert!(keys.contains(&"src/lib.rs".to_string()));
    assert!(
        !keys.iter().any(|k| k.starts_with(".git")),
        "the .git directory must be excluded, got {keys:?}"
    );
    assert!(
        manifest
            .items
            .iter()
            .all(|i| i.item_kind == ItemKind::RepoFile)
    );
    assert_eq!(
        manifest
            .metadata
            .get("git_provider")
            .and_then(|v| v.as_str()),
        Some("github")
    );
    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn acquire_then_normalize_stamps_git_metadata() {
    let repo = fixture_repo();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let adapter = GitSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = diff_from(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), manifest.items.len());

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let readme = normalized
        .data
        .iter()
        .find(|d| d.path.as_deref() == Some("README.md"))
        .expect("README document present");
    assert_eq!(
        readme.metadata.get("source_type").and_then(|v| v.as_str()),
        Some("git_code")
    );
    assert_eq!(
        readme.metadata.get("git_repo").and_then(|v| v.as_str()),
        Some("fixture-repo")
    );
    assert_eq!(
        readme.metadata.get("git_owner").and_then(|v| v.as_str()),
        Some("jmagar")
    );
    assert!(matches!(&readme.content, ContentRef::InlineText { text } if text.contains("Fixture")));
    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn discover_without_repo_root_option_errors() {
    let plan = git_plan(Path::new("/does/not/matter"), SourceScope::Repo, false);
    let err = GitSourceAdapter::new().discover(&plan).await.unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.git.repo_root.required");
}

#[tokio::test]
async fn discover_rejects_unsupported_scope() {
    let repo = fixture_repo();
    let plan = git_plan(&repo, SourceScope::Page, true);
    let err = GitSourceAdapter::new().discover(&plan).await.unwrap_err();
    assert!(err.code.to_string().contains("scope"));
    fs::remove_dir_all(&repo).ok();
}
