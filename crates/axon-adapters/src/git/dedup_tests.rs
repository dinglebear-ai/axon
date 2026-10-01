use super::*;

#[tokio::test]
async fn basename_sensitive_files_keep_production_parser_semantics() {
    let repo = fixture_repo();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let adapter = GitSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    let template = manifest
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "src/lib.rs")
        .unwrap()
        .clone();
    let acquired = adapter
        .acquire(&plan, &diff_from(&plan, manifest.items))
        .await
        .unwrap();
    let normalized = adapter.normalize(&plan, acquired).await.unwrap();
    let document = normalized
        .data
        .into_iter()
        .find(|doc| doc.source_item_key.0 == "src/lib.rs")
        .unwrap();
    for (generic, special, kind, text, expected_fact) in [
        (
            "a.toml",
            "Cargo.toml",
            ContentKind::Toml,
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n[dependencies]\ntokio = \"1\"\n",
            "dependency",
        ),
        (
            "a.json",
            "package.json",
            ContentKind::Json,
            r#"{"name":"fixture","version":"1.0.0","dependencies":{"left-pad":"1.3.0"}}"#,
            "dependency",
        ),
        (
            "a.yaml",
            "docker-compose.yaml",
            ContentKind::Yaml,
            "# Configuration\nservices:\n  web:\n    image: nginx:alpine\n",
            "runtime_service",
        ),
        (
            ".copy/devcontainer.json",
            ".devcontainer/devcontainer.json",
            ContentKind::Json,
            r#"{"name":"fixture","services":{"web":{"image":"nginx:alpine"}}}"#,
            "runtime_service",
        ),
    ] {
        let copied = format!("z-copied/{special}");
        let items = [generic, special, copied.as_str()]
            .into_iter()
            .map(|path| {
                let mut item = template.clone();
                item.source_item_key = SourceItemKey::from(path);
                item.display_path = Some(path.into());
                item.canonical_uri = format!("git://fixture/{path}");
                item.content_kind = Some(kind);
                item.content_hash = Some("identical-test-bytes".into());
                item.metadata = MetadataMap::new();
                item
            })
            .collect();
        let deduped = dedup::deduplicate_items(items).unwrap();
        assert_eq!(deduped.len(), 2, "{special}");
        assert!(deduped.iter().any(|item| item.source_item_key.0 == generic));
        let retained = deduped
            .iter()
            .find(|item| item.source_item_key.0 == special)
            .unwrap();
        assert_eq!(retained.metadata["source_item_aliases"], json!([copied]));
        let mut input_document = document.clone();
        input_document.source_item_key = retained.source_item_key.clone();
        input_document.path = retained.display_path.clone();
        input_document.canonical_uri = retained.canonical_uri.clone();
        input_document.content_kind = kind;
        input_document.content = ContentRef::InlineText { text: text.into() };
        let parsed =
            axon_parse::builtins::production_registry().parse(&axon_parse::parser::ParseInput {
                job_id: plan.job_id,
                stage_id: StageId::new(Uuid::from_u128(912)),
                document: input_document,
                requested_parser: None,
            });
        assert!(
            parsed
                .facts
                .iter()
                .any(|fact| fact.fact_kind == expected_fact),
            "{special}: {:?}",
            parsed.facts
        );
        assert!(!parsed.graph_candidates.is_empty(), "{special}");
    }
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn non_markdown_basenames_are_preserved_while_markdown_aliases_pack() {
    let repo = fixture_repo();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let template = manifest
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "src/lib.rs")
        .unwrap();
    for (left, right, kind, expected_items) in [
        ("a/env.example", "z/.env.example", ContentKind::PlainText, 2),
        ("a.jsonl", "session.jsonl", ContentKind::PlainText, 2),
        ("a.jsonl", "tool-output.jsonl", ContentKind::PlainText, 2),
        ("a.txt", "cli-help.txt", ContentKind::PlainText, 2),
        ("a.yaml", "openapi.yaml", ContentKind::Yaml, 2),
        ("a.rs", "b.rs", ContentKind::Code, 2),
        ("a/lib.rs", "b/lib.rs", ContentKind::Code, 1),
        ("README.md", "copy.md", ContentKind::Markdown, 1),
    ] {
        let items = [left, right]
            .into_iter()
            .map(|path| {
                let mut item = template.clone();
                item.source_item_key = SourceItemKey::from(path);
                item.display_path = Some(path.into());
                item.canonical_uri = format!("git://fixture/{path}");
                item.content_kind = Some(kind);
                item.content_hash = Some("identical-test-bytes".into());
                item.metadata = MetadataMap::new();
                item
            })
            .collect();
        let deduped = dedup::deduplicate_items(items).unwrap();
        assert_eq!(deduped.len(), expected_items, "{left} / {right}");
        if expected_items == 1 {
            assert_eq!(deduped[0].metadata["source_item_aliases"], json!([right]));
        } else {
            assert!(
                deduped
                    .iter()
                    .all(|item| !item.metadata.contains_key("source_item_aliases"))
            );
        }
    }
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn identical_git_files_deduplicate_with_aliases_and_membership_versions() {
    let repo = fixture_repo();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let adapter = GitSourceAdapter::new();
    let before = adapter.discover(&plan).await.unwrap();
    let before_item = before
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "README.md")
        .unwrap();
    fs::write(repo.join("copy.md"), "# Fixture\n").unwrap();
    let manifest = adapter.discover(&plan).await.unwrap();
    let canonical = manifest
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "README.md")
        .unwrap();
    assert_eq!(
        canonical.metadata["source_item_aliases"],
        json!(["copy.md"])
    );
    assert_eq!(manifest.items.len(), 2);
    assert_eq!(canonical.content_hash, before_item.content_hash);
    assert_ne!(canonical.version, before_item.version);
    let prior_version = canonical.version.clone();
    let prior_hash = canonical.content_hash.clone();
    let acquired = adapter
        .acquire(&plan, &diff_from(&plan, manifest.items))
        .await
        .unwrap();
    let documents = adapter.normalize(&plan, acquired).await.unwrap();
    assert!(
        documents
            .data
            .iter()
            .any(|doc| doc.metadata.get("source_item_aliases") == Some(&json!(["copy.md"])))
    );
    fs::remove_file(repo.join("copy.md")).unwrap();
    let manifest = adapter.discover(&plan).await.unwrap();
    let canonical = manifest
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "README.md")
        .unwrap();
    assert_eq!(canonical.content_hash, prior_hash);
    assert_ne!(canonical.version, prior_version);
    fs::write(repo.join("copy.md"), "# Fixture\n").unwrap();
    fs::remove_file(repo.join("README.md")).unwrap();
    let manifest = adapter.discover(&plan).await.unwrap();
    assert!(
        manifest
            .items
            .iter()
            .any(|item| item.source_item_key.0 == "copy.md")
    );
    assert!(
        !manifest
            .items
            .iter()
            .any(|item| item.source_item_key.0 == "README.md")
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn identical_git_bytes_preserve_distinct_parser_extensions() {
    let repo = fixture_repo();
    fs::write(repo.join("src/copy.py"), "pub fn hi() {}\n").unwrap();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 3);
    assert!(
        manifest
            .items
            .iter()
            .all(|item| !item.metadata.contains_key("source_item_aliases"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn dedup_alias_budget_preserves_every_inventory_path() {
    let repo = fixture_repo();
    for index in 0..258 {
        fs::write(repo.join(format!("copy{index:03}.md")), "# Fixture\n").unwrap();
    }
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 260);
    assert!(
        manifest
            .items
            .iter()
            .all(|item| !item.metadata.contains_key("source_item_aliases"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn partial_git_inventory_does_not_collapse_aliases() {
    let repo = fixture_repo();
    fs::write(repo.join("copy.md"), "# Fixture\n").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.limits.effective.max_items = Some(2);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(
        manifest.inventory_completeness(),
        InventoryCompleteness::Partial
    );
    assert_eq!(manifest.items.len(), 2);
    assert!(
        manifest
            .items
            .iter()
            .all(|item| !item.metadata.contains_key("source_item_aliases"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn unknown_hash_files_are_never_deduplicated() {
    let repo = fixture_repo();
    fs::write(repo.join("large.rs"), "12345").unwrap();
    fs::write(repo.join("copy.rs"), "12345").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.limits.effective.max_bytes_per_item = Some(4);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 4);
    assert!(
        manifest
            .items
            .iter()
            .all(|item| item.content_hash.is_none())
    );
    assert!(
        manifest
            .items
            .iter()
            .all(|item| !item.metadata.contains_key("source_item_aliases"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn identical_test_and_non_test_code_keep_distinct_classification() {
    let repo = fixture_repo();
    fs::write(repo.join("src/lib_test.rs"), "pub fn hi() {}\n").unwrap();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let adapter = GitSourceAdapter::new();
    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 3);
    assert!(
        manifest
            .items
            .iter()
            .all(|item| !item.metadata.contains_key("source_item_aliases"))
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn alias_directory_prefixes_are_indexed_as_exact_ancestors() {
    let repo = fixture_repo();
    fs::create_dir_all(repo.join("copied/plugin")).unwrap();
    fs::write(repo.join("copied/plugin/README.md"), "# Fixture\n").unwrap();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let canonical = manifest
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "README.md")
        .unwrap();
    let prefixes = canonical.metadata["source_path_prefixes"]
        .as_array()
        .unwrap();
    assert!(prefixes.contains(&json!("copied/plugin")));
    assert!(prefixes.contains(&json!("copied/plugin/README.md")));
    assert!(prefixes.contains(&json!("/")));
    assert!(!prefixes.contains(&json!("copied/plug")));
    assert!(!prefixes.contains(&json!("plugin")));
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn changing_alias_content_splits_group_without_changing_canonical_hash() {
    let repo = fixture_repo();
    fs::write(repo.join("copy.md"), "# Fixture\n").unwrap();
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let adapter = GitSourceAdapter::new();
    let before = adapter.discover(&plan).await.unwrap();
    let prior = before
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "README.md")
        .unwrap();
    fs::write(repo.join("copy.md"), "# Changed alias\n").unwrap();
    let after = adapter.discover(&plan).await.unwrap();
    let current = after
        .items
        .iter()
        .find(|item| item.source_item_key.0 == "README.md")
        .unwrap();
    assert_eq!(after.items.len(), 3);
    assert_eq!(prior.content_hash, current.content_hash);
    assert_ne!(prior.version, current.version);
    assert!(!current.metadata.contains_key("source_item_aliases"));
    assert!(after.items.iter().any(
        |item| item.source_item_key.0 == "copy.md" && item.content_hash != current.content_hash
    ));
    fs::remove_dir_all(repo).unwrap();
}
