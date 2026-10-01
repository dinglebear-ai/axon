use super::*;

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
