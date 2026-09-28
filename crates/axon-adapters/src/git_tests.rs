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
                source: TARGET_URL.to_string(),
                canonical_uri: "git://github.com/jmagar/fixture-repo".to_string(),
                source_id: SourceId::from("src_git_test"),
                source_kind: SourceKind::Git,
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
    assert!(cap.0.features.contains(&"scope:repo".to_string()));
    assert!(cap.0.features.contains(&"scope:directory".to_string()));
    assert!(!cap.0.features.contains(&"scope:page".to_string()));
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
async fn discover_and_acquire_preserve_mixed_content() {
    let repo = fixture_repo();
    fs::write(repo.join("image.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    fs::write(repo.join("unknown.dat"), b"text\0binary").unwrap();
    fs::write(repo.join("document.pdf"), b"%PDF-1.7 all ASCII").unwrap();
    fs::write(repo.join("utf16.txt"), [0xff, 0xfe, 0x41, 0]).unwrap();
    let mut late_invalid = vec![b'a'; 70_000];
    late_invalid.push(0xff);
    fs::write(repo.join("late.txt"), late_invalid).unwrap();
    let mut unicode = "a".repeat(65_535);
    unicode.push_str("🌍\n");
    fs::write(repo.join("unicode.txt"), &unicode).unwrap();
    for limit in [None, Some(100)] {
        let mut plan = git_plan(&repo, SourceScope::Repo, true);
        plan.limits.effective.max_items = limit;
        let adapter = GitSourceAdapter::new();
        let manifest = adapter.discover(&plan).await.unwrap();
        let keys = manifest
            .items
            .iter()
            .map(|item| item.display_path.as_deref().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                "README.md",
                "document.pdf",
                "image.png",
                "late.txt",
                "src/lib.rs",
                "unicode.txt",
                "unknown.dat",
                "utf16.txt"
            ]
        );
        let acquisition = adapter
            .acquire(&plan, &diff_from(&plan, manifest.items))
            .await
            .unwrap();
        assert_eq!(acquisition.fetched_items.len(), 8);
        for item in &acquisition.fetched_items {
            let ContentRef::InlineBytes { bytes_base64, .. } = &item.content_ref else {
                panic!("raw repository file must remain bytes")
            };
            use base64::Engine as _;
            assert_eq!(
                STANDARD.decode(bytes_base64).unwrap(),
                fs::read(repo.join(item.manifest_item.display_path.as_ref().unwrap())).unwrap()
            );
        }
    }
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn discover_applies_max_items_before_hashing_the_full_repo() {
    let repo = fixture_repo();
    fs::write(
        repo.join("z-last.rs"),
        "pub fn z() {}
",
    )
    .unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.limits.effective.max_items = Some(1);

    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(
        manifest.inventory_completeness(),
        InventoryCompleteness::Partial
    );
    let keys = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["README.md"]);
    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn discover_honors_repo_relative_exclude_path_substrings() {
    let repo = fixture_repo();
    fs::create_dir_all(repo.join("docs/private")).unwrap();
    fs::create_dir_all(repo.join("vendor")).unwrap();
    fs::write(repo.join("docs/private/secret.md"), "secret\n").unwrap();
    fs::write(repo.join("vendor/dependency.rs"), "vendored\n").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.request.options.values.insert(
        "exclude_paths".to_string(),
        serde_json::json!(["src/", "docs/private/", "vendor/"]),
    );

    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let keys: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect();

    assert_eq!(keys, vec!["README.md"]);
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
        readme
            .metadata
            .get("source_family")
            .and_then(|v| v.as_str()),
        Some("code")
    );
    assert!(!readme.metadata.contains_key("source_type"));
    assert_eq!(
        readme.metadata.get("source_kind").and_then(|v| v.as_str()),
        Some("git")
    );
    assert_eq!(
        readme.metadata.get("git_repo").and_then(|v| v.as_str()),
        Some("fixture-repo")
    );
    assert_eq!(
        readme.metadata.get("git_owner").and_then(|v| v.as_str()),
        Some("jmagar")
    );
    let ContentRef::InlineBytes { bytes_base64, .. } = &readme.content else {
        panic!("repository content remains raw bytes")
    };
    use base64::Engine as _;
    assert!(
        String::from_utf8(STANDARD.decode(bytes_base64).unwrap())
            .unwrap()
            .contains("Fixture")
    );
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

/// Regression: the resolver routes forge URLs to git-family adapters named
/// `github`/`gitea`/`gitlab` (all `SourceKind::Git`), while `GitSourceAdapter`
/// is the single implementation behind them. Validation must accept those
/// routes — keying off the literal name `"git"` rejected every real GitHub
/// URL with `adapter.git.mismatch` (seen live).
#[tokio::test]
async fn discover_accepts_forge_family_adapter_names() {
    let repo = fixture_repo();
    for forge in ["github", "gitea", "gitlab"] {
        let mut plan = git_plan(&repo, SourceScope::Repo, true);
        plan.route.adapter.name = forge.to_string();
        plan.route.source.adapter.name = forge.to_string();
        let manifest = GitSourceAdapter::new()
            .discover(&plan)
            .await
            .unwrap_or_else(|error| panic!("forge adapter `{forge}` must be accepted: {error}"));
        assert!(
            !manifest.items.is_empty(),
            "forge adapter `{forge}` should discover repo files"
        );
    }
    fs::remove_dir_all(&repo).ok();
}

/// A route whose source kind is not `Git` (e.g. an accidental `Web` route
/// reaching the git adapter) is still rejected.
#[tokio::test]
async fn discover_rejects_non_git_source_kind() {
    let repo = fixture_repo();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.route.source.source_kind = SourceKind::Web;
    let err = GitSourceAdapter::new().discover(&plan).await.unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.git.mismatch");
    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn oversized_git_items_remain_inventoried_without_hash_or_payload() {
    let repo = fixture_repo();
    fs::write(repo.join("large.bin"), b"12345").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.limits.effective.max_bytes_per_item = Some(4);
    let adapter = GitSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let item = manifest
        .items
        .iter()
        .find(|i| i.display_path.as_deref() == Some("large.bin"))
        .unwrap();
    assert!(item.content_hash.is_none());
    assert_eq!(
        item.metadata.get(CONTENT_OMISSION_METADATA_KEY),
        Some(&json!("size_limit_exceeded"))
    );
    let acquisition = adapter
        .acquire(&plan, &diff_from(&plan, vec![item.clone()]))
        .await
        .unwrap();
    assert_eq!(acquisition.header.counts.bytes_done, 0);
    let documents = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(
        documents.data[0]
            .metadata
            .get(CONTENT_OMISSION_METADATA_KEY),
        Some(&json!("size_limit_exceeded"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn git_acquisition_enforces_actual_total_and_batch_allowance() {
    let repo = fixture_repo();
    fs::write(repo.join("a.txt"), b"abc").unwrap();
    fs::write(repo.join("b.txt"), b"def").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    let adapter = GitSourceAdapter::new();
    let items = adapter
        .discover(&plan)
        .await
        .unwrap()
        .items
        .into_iter()
        .filter(|i| matches!(i.display_path.as_deref(), Some("a.txt" | "b.txt")))
        .collect::<Vec<_>>();
    let diff = diff_from(&plan, items);
    plan.limits.effective.max_total_bytes = Some(6);
    assert_eq!(
        adapter
            .acquire(&plan, &diff)
            .await
            .unwrap()
            .header
            .counts
            .bytes_done,
        6
    );
    plan.limits.effective.max_total_bytes = Some(5);
    assert_eq!(
        adapter.acquire(&plan, &diff).await.unwrap_err().code.0,
        "source.acquire.byte_budget_exceeded"
    );
    plan.limits.effective.max_total_bytes = None;
    plan.route.source.metadata.insert(
        crate::acquisition::ACQUISITION_BATCH_BYTES_KEY.into(),
        json!(5),
    );
    assert!(adapter.acquire(&plan, &diff).await.is_err());
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn git_growth_after_discovery_obeys_acquisition_limit() {
    let repo = fixture_repo();
    fs::write(repo.join("growing.txt"), b"abc").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.limits.effective.max_bytes_per_item = Some(4);
    let adapter = GitSourceAdapter::new();
    let item = adapter
        .discover(&plan)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|i| i.display_path.as_deref() == Some("growing.txt"))
        .unwrap();
    assert!(item.content_hash.is_some());
    fs::write(repo.join("growing.txt"), b"abcde").unwrap();
    let acquisition = adapter
        .acquire(&plan, &diff_from(&plan, vec![item]))
        .await
        .unwrap();
    assert_eq!(acquisition.header.counts.bytes_done, 0);
    assert!(acquisition.manifest.items[0].content_hash.is_none());
    assert_eq!(
        acquisition.manifest.items[0]
            .metadata
            .get(CONTENT_OMISSION_METADATA_KEY),
        Some(&json!("size_limit_exceeded"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn git_inventory_completeness_distinguishes_exact_cap_from_truncation() {
    let repo = fixture_repo();
    for (cap, expected) in [
        (None, InventoryCompleteness::Complete),
        (Some(0), InventoryCompleteness::Partial),
        (Some(1), InventoryCompleteness::Partial),
        (Some(2), InventoryCompleteness::Complete),
        (Some(3), InventoryCompleteness::Complete),
    ] {
        let mut plan = git_plan(&repo, SourceScope::Repo, true);
        plan.limits.effective.max_items = cap;
        let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
        assert_eq!(manifest.inventory_completeness(), expected, "cap={cap:?}");
    }
    fs::remove_dir_all(repo).unwrap();
}
