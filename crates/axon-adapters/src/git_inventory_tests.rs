//! Inventory classification must agree for capped, full, and retained discovery.
use super::*;

#[tokio::test]
async fn extensionless_scripts_are_content_checked_in_all_inventory_paths() {
    let repo = fixture_repo();
    for (name, bytes) in [
        (
            "update",
            b"#!/usr/bin/env python3\nprint('update')\n".as_slice(),
        ),
        ("launch", b"#!/bin/bash\necho launch\n".as_slice()),
        ("binary", b"#!/bin/bash\n\0ELF".as_slice()),
        ("opaque", b"unrecognized text".as_slice()),
        ("credential", b"TOKEN=private-value".as_slice()),
        ("unknown", b"#!/usr/bin/not-a-language\nx".as_slice()),
        (".hidden", b"#!/bin/bash\necho hidden".as_slice()),
    ] {
        fs::write(repo.join(name), bytes).unwrap();
    }
    let plan = git_plan(&repo, SourceScope::Repo, true);
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.display_path.clone().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["README.md", "launch", "src/lib.rs", "update"]);
    assert_eq!(
        manifest
            .items
            .iter()
            .find(|item| item.display_path.as_deref() == Some("update"))
            .unwrap()
            .content_kind,
        Some(ContentKind::Code)
    );
    let (capped, truncated) =
        collect_capped_git_keys(&repo, &policy::GitInventoryPolicy::default(), 10).unwrap();
    assert_eq!(capped, keys);
    assert!(!truncated);
    let wanted = keys.into_iter().collect();
    let retained = retained_repository_paths(plan.clone(), wanted)
        .await
        .unwrap();
    assert_eq!(retained.len(), 4);
    let before = manifest
        .items
        .iter()
        .find(|item| item.display_path.as_deref() == Some("update"))
        .unwrap()
        .clone();
    fs::write(
        repo.join("update"),
        "#!/usr/bin/python3\nprint('modified')\n",
    )
    .unwrap();
    let changed = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let after = changed
        .items
        .iter()
        .find(|item| item.display_path.as_deref() == Some("update"))
        .unwrap();
    assert_eq!(before.source_item_key, after.source_item_key);
    assert_ne!(before.content_hash, after.content_hash);
    let repeated = GitSourceAdapter::new().discover(&plan).await.unwrap();
    assert_eq!(
        after.content_hash,
        repeated
            .items
            .iter()
            .find(|item| item.display_path == after.display_path)
            .unwrap()
            .content_hash
    );
    fs::write(repo.join("update"), "opaque replacement").unwrap();
    fs::remove_file(repo.join("launch")).unwrap();
    assert_eq!(
        retained_repository_paths(plan, retained).await.unwrap(),
        ["README.md", "src/lib.rs"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    fs::remove_dir_all(repo).unwrap();
}

#[tokio::test]
async fn new_classification_preserves_ignore_exclusion_and_historical_policy() {
    let repo = fixture_repo();
    fs::create_dir_all(repo.join("plans")).unwrap();
    for (name, content) in [
        ("ignored", "#!/bin/sh\necho ignored"),
        ("excluded", "#!/bin/sh\necho excluded"),
        ("settings.toml", "[runtime]\nenabled=true"),
        ("schema.xml", "<schema/>"),
        ("runtime.service", "[Service]\nExecStart=/usr/bin/example"),
        ("service.conf", "enabled=true"),
        ("Containerfile", "FROM alpine"),
        ("plans/old.md", "# Historical"),
    ] {
        fs::write(repo.join(name), content).unwrap();
    }
    fs::write(repo.join(".gitignore"), "ignored\n").unwrap();
    let mut plan = git_plan(&repo, SourceScope::Repo, true);
    plan.request
        .options
        .values
        .insert("exclude_paths".into(), json!(["excluded"]));
    let manifest = GitSourceAdapter::new().discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.display_path.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "Containerfile",
            "README.md",
            "runtime.service",
            "schema.xml",
            "service.conf",
            "settings.toml",
            "src/lib.rs"
        ]
    );
    let retained = retained_repository_paths(
        plan,
        ["ignored", "excluded", "plans/old.md", "settings.toml"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    )
    .await
    .unwrap();
    assert_eq!(retained, ["settings.toml".to_owned()].into_iter().collect());
    fs::remove_dir_all(repo).unwrap();
}
