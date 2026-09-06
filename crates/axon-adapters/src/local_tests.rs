use std::fs;

use axon_api::source::*;

use crate::SourceAdapter;
use crate::local::LocalSourceAdapter;
use crate::local_test_support::*;

#[tokio::test]
async fn local_adapter_declares_task1_scopes_and_accepts_options() {
    let adapter = LocalSourceAdapter::new();

    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.name, "local");
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::Local))
    );
    assert_eq!(
        capability.0.limits.0.get("default_scope"),
        Some(&serde_json::json!(SourceScope::File))
    );
    for scope in [
        SourceScope::File,
        SourceScope::Directory,
        SourceScope::Workspace,
        SourceScope::Repo,
        SourceScope::Map,
    ] {
        let tag = format!(
            "scope:{}",
            serde_json::to_value(scope).unwrap().as_str().unwrap()
        );
        assert!(
            capability.0.features.contains(&tag),
            "missing local scope {scope:?}"
        );
    }

    let mut plan = source_plan(temp_source_dir(), SourceScope::Directory);
    plan.route.validated_options.values = local_options();

    adapter
        .discover(&plan)
        .await
        .expect("task1 local options should validate");
}

#[tokio::test]
async fn local_adapter_rejects_unknown_options() {
    let adapter = LocalSourceAdapter::new();
    let mut plan = source_plan(temp_source_dir(), SourceScope::Directory);
    plan.route
        .validated_options
        .values
        .insert("surprise".to_string(), "nope".into());

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("unknown local options should fail validation");

    assert_eq!(err.code.0, "adapter.local.option.unsupported");
    assert_eq!(err.stage, ErrorStage::Routing);
}

#[tokio::test]
async fn local_file_discovery_uses_public_stable_identity() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    let file_path = root.join("notes.md");
    fs::write(&file_path, "# local").unwrap();

    let plan = source_plan(file_path, SourceScope::File);
    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(manifest.items.len(), 1);
    let item = &manifest.items[0];
    assert_eq!(item.source_item_key, SourceItemKey::from("notes.md"));
    assert_eq!(
        item.canonical_uri,
        format!("{}/notes.md", plan.route.source.canonical_uri)
    );
    assert!(!item.source_item_key.0.contains("/home/"));
    assert!(!item.canonical_uri.contains("/home/"));
}

#[tokio::test]
async fn local_file_scope_does_not_bypass_sensitive_path_policy() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    let file_path = root.join(".env");
    fs::write(&file_path, "AXON_TOKEN=secret").unwrap();

    let plan = source_plan(file_path, SourceScope::File);
    let manifest = adapter.discover(&plan).await.unwrap();

    assert!(
        manifest.items.is_empty(),
        "File scope must still deny sensitive files"
    );
}

#[tokio::test]
async fn local_directory_discovery_emits_sorted_relative_file_items() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn local() {}").unwrap();

    let plan = source_plan(root, SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["README.md", "src/lib.rs"]);
    for item in &manifest.items {
        assert!(item.canonical_uri.starts_with("local://"));
        assert!(!item.canonical_uri.contains("/home/"));
        assert!(!item.source_item_key.0.starts_with('/'));
    }
}

#[tokio::test]
async fn local_directory_discovery_applies_max_items_before_hashing_the_full_tree() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("z-last.rs"), "pub fn z() {}").unwrap();
    fs::write(root.join("a-first.rs"), "pub fn a() {}").unwrap();
    fs::write(root.join("m-middle.rs"), "pub fn m() {}").unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.limits.effective.max_items = Some(2);

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    // This must match the historical sort-then-truncate result regardless of
    // filesystem walk order, while discovery retains only the first N candidates.
    assert_eq!(keys, vec!["a-first.rs", "m-middle.rs"]);
}

#[tokio::test]
async fn local_directory_discovery_short_circuits_zero_max_items() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("ignored.rs"), "pub fn ignored() {}").unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.limits.effective.max_items = Some(0);

    let manifest = adapter.discover(&plan).await.unwrap();
    assert!(manifest.items.is_empty());
}

#[tokio::test]
async fn local_directory_discovery_honors_all_exclude_path_substrings() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join("docs/private")).unwrap();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("keep.md"), "keep").unwrap();
    fs::write(root.join("docs/private/excluded.md"), "private").unwrap();
    fs::write(root.join("vendor/dependency.rs"), "vendor").unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route.validated_options.values.insert(
        "exclude_paths".to_string(),
        serde_json::json!(["vendor/", "docs/private/"]),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["keep.md"]);
}

#[tokio::test]
async fn local_adapter_acquires_and_normalizes_source_documents() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn local() {}").unwrap();
    let plan = source_plan(root, SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), 2);
    assert!(matches!(
        acquisition.fetched_items[0].content_ref,
        ContentRef::InlineText { .. }
    ));

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.data.len(), 2);
    let docs = normalized.data;
    assert_eq!(docs[0].source_id, SourceId::from("src_local_test"));
    assert_eq!(docs[0].source_item_key, SourceItemKey::from("README.md"));
    assert_eq!(docs[0].metadata["source_family"], "code");
    assert!(!docs[0].metadata.contains_key("source_type"));
    assert_eq!(docs[0].metadata["source_kind"], "local");
    assert_eq!(docs[0].metadata["source_adapter"], "local");
    assert_eq!(docs[0].metadata["source_scope"], "directory");
    assert_eq!(
        docs[0].metadata["item_canonical_uri"],
        docs[0].canonical_uri
    );
    assert!(!serde_json::to_string(&docs).unwrap().contains("/home/"));
}

#[tokio::test]
async fn local_document_ids_do_not_collide_for_lossy_path_shapes() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/b.rs"), "pub fn nested() {}").unwrap();
    fs::write(root.join("a_b.rs"), "pub fn flat() {}").unwrap();
    let plan = source_plan(root, SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let mut ids = normalized
        .data
        .iter()
        .map(|document| document.document_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();

    assert_eq!(ids.len(), 2);
}

#[tokio::test]
async fn local_adapter_rejects_diff_item_keys_that_escape_root() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    fs::write(root.join("..outside.md"), "# not this").unwrap();
    let plan = source_plan(root, SourceScope::Directory);
    let mut item = adapter.discover(&plan).await.unwrap().items[0].clone();
    item.source_item_key = SourceItemKey::from("../..outside.md");
    let diff = manifest_diff(&plan, vec![item]);

    let err = adapter
        .acquire(&plan, &diff)
        .await
        .expect_err("escaped source item key must not be read");

    assert_eq!(err.code.0, "adapter.local.item_key.escape");
}

#[cfg(unix)]
#[tokio::test]
async fn local_discovery_rejects_followed_symlink_that_escapes_root() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    let outside = temp_source_dir();
    fs::write(outside.join("secret.rs"), "pub fn leaked() {}\n").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.rs"), root.join("linked.rs")).unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route
        .validated_options
        .values
        .insert("follow_symlinks".to_string(), true.into());

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("followed symlinks must remain contained in the source root");

    assert_eq!(err.code.0, "adapter.local.symlinks_unsupported");
}

#[cfg(unix)]
#[tokio::test]
async fn local_discovery_rejects_a_symlinked_source_root() {
    let adapter = LocalSourceAdapter::new();
    let parent = temp_source_dir();
    let outside = temp_source_dir();
    fs::write(outside.join("secret.rs"), "pub fn leaked() {}\n").unwrap();
    let linked = parent.join("current");
    std::os::unix::fs::symlink(&outside, &linked).unwrap();
    let plan = source_plan(linked, SourceScope::Directory);

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("a source root symlink must not be traversed");

    assert_eq!(err.code.0, "adapter.local.root_unsafe");
    assert!(!err.message.contains(&outside.display().to_string()));
}

#[cfg(unix)]
#[tokio::test]
async fn local_acquire_rejects_directory_symlink_swap_after_discovery() {
    let adapter = LocalSourceAdapter::new();
    let parent = temp_source_dir();
    let root = parent.join("current");
    let outside = temp_source_dir();
    fs::create_dir(&root).unwrap();
    fs::write(root.join("README.md"), "# safe").unwrap();
    fs::write(outside.join("README.md"), "# secret").unwrap();
    let plan = source_plan(root.clone(), SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items);

    fs::remove_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(&outside, &root).unwrap();

    let err = adapter
        .acquire(&plan, &diff)
        .await
        .expect_err("a swapped source-root symlink must not be traversed");

    assert_eq!(err.code.0, "adapter.local.item_key.escape");
    assert_eq!(err.message, "local source containment denied");
    assert!(!err.message.contains(&outside.display().to_string()));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn contained_local_adapter_holds_allowed_root_across_path_replacement() {
    let allowed = temp_source_dir();
    let root = allowed.join("current");
    let retired = allowed.join("retired");
    let outside = temp_source_dir();
    fs::create_dir(&root).unwrap();
    fs::write(root.join("README.md"), "# safe").unwrap();
    fs::write(outside.join("README.md"), "# secret").unwrap();
    let adapter = LocalSourceAdapter::new_contained(
        &root,
        SourceScope::Directory,
        std::slice::from_ref(&allowed),
    )
    .unwrap();
    let plan = source_plan(root.clone(), SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items);

    fs::rename(&root, &retired).unwrap();
    std::os::unix::fs::symlink(&outside, &root).unwrap();

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert!(matches!(
        &acquisition.fetched_items[0].content_ref,
        ContentRef::InlineText { text } if text == "# safe"
    ));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn overlapping_local_jobs_keep_distinct_root_handles() {
    let adapter = LocalSourceAdapter::new();
    let parent = temp_source_dir();
    let first_root = parent.join("first");
    let first_retired = parent.join("first-retired");
    let second_root = parent.join("second");
    fs::create_dir(&first_root).unwrap();
    fs::create_dir(&second_root).unwrap();
    fs::write(first_root.join("README.md"), "# first").unwrap();
    fs::write(second_root.join("README.md"), "# second").unwrap();
    let mut first = source_plan(first_root.clone(), SourceScope::Directory);
    first.job_id = JobId::new(uuid::Uuid::new_v4());
    let mut second = source_plan(second_root, SourceScope::Directory);
    second.job_id = JobId::new(uuid::Uuid::new_v4());
    let first_manifest = adapter.discover(&first).await.unwrap();
    let first_diff = manifest_diff(&first, first_manifest.items);
    adapter.discover(&second).await.unwrap();

    fs::rename(&first_root, &first_retired).unwrap();
    fs::create_dir(&first_root).unwrap();
    fs::write(first_root.join("README.md"), "# replacement").unwrap();

    let acquisition = adapter.acquire(&first, &first_diff).await.unwrap();
    assert!(matches!(
        &acquisition.fetched_items[0].content_ref,
        ContentRef::InlineText { text } if text == "# first"
    ));
}

#[tokio::test]
async fn failed_local_discoveries_do_not_retain_root_handles() {
    let adapter = LocalSourceAdapter::new();
    for _ in 0..128 {
        let root = temp_source_dir();
        let mut plan = source_plan(root, SourceScope::Directory);
        plan.job_id = JobId::new(uuid::Uuid::new_v4());
        plan.route
            .validated_options
            .values
            .insert("follow_symlinks".to_string(), true.into());
        adapter
            .discover(&plan)
            .await
            .expect_err("unsafe discovery must fail");
    }

    assert_eq!(adapter.held_root_count(), 0);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn contained_local_adapter_reuses_root_handle_across_large_batches() {
    let allowed = temp_source_dir();
    let root = allowed.join("source");
    fs::create_dir(&root).unwrap();
    for index in 0..130 {
        fs::write(
            root.join(format!("item-{index:03}.md")),
            format!("# {index}"),
        )
        .unwrap();
    }
    let adapter = LocalSourceAdapter::new_contained(
        &root,
        SourceScope::Directory,
        std::slice::from_ref(&allowed),
    )
    .unwrap();
    let plan = source_plan(root, SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let full_diff = manifest_diff(&plan, manifest.items);

    let mut fetched = 0;
    for items in full_diff.added.chunks(64) {
        let mut batch = manifest_diff(&plan, items.to_vec());
        batch.next_generation = full_diff.next_generation.clone();
        fetched += adapter
            .acquire(&plan, &batch)
            .await
            .unwrap()
            .fetched_items
            .len();
    }

    assert_eq!(fetched, 130);
    assert_eq!(adapter.held_root_count(), 1);
    adapter
        .release(&AdapterReleaseRequest {
            job_id: plan.job_id,
            source_id: plan.route.source.source_id.clone(),
            source_kind: plan.route.source.source_kind,
        })
        .unwrap();
    assert_eq!(adapter.held_root_count(), 0);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unchanged_refresh_lifecycles_release_root_handles() {
    let allowed = temp_source_dir();
    let root = allowed.join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("README.md"), "# unchanged").unwrap();
    let adapter = LocalSourceAdapter::new_contained(
        &root,
        SourceScope::Directory,
        std::slice::from_ref(&allowed),
    )
    .unwrap();

    for _ in 0..128 {
        let mut plan = source_plan(root.clone(), SourceScope::Directory);
        plan.job_id = JobId::new(uuid::Uuid::new_v4());
        adapter.discover(&plan).await.unwrap();
        adapter
            .release(&AdapterReleaseRequest {
                job_id: plan.job_id,
                source_id: plan.route.source.source_id.clone(),
                source_kind: plan.route.source.source_kind,
            })
            .unwrap();
    }

    assert_eq!(adapter.held_root_count(), 0);
}

#[tokio::test]
async fn local_manifest_fingerprint_changes_for_same_size_file_edits() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    let file = root.join("README.md");
    let timestamp_source = root.join("timestamp-source");
    fs::write(&file, "abcd").unwrap();
    fs::write(&timestamp_source, "time anchor").unwrap();
    std::process::Command::new("touch")
        .arg("-r")
        .arg(&timestamp_source)
        .arg(&file)
        .status()
        .expect("restore first mtime");
    let plan = source_plan(root, SourceScope::Directory);

    let first = adapter.discover(&plan).await.unwrap();
    fs::write(&file, "wxyz").unwrap();
    std::process::Command::new("touch")
        .arg("-r")
        .arg(&timestamp_source)
        .arg(&file)
        .status()
        .expect("restore second mtime");
    let second = adapter.discover(&plan).await.unwrap();

    let first_item = first
        .items
        .iter()
        .find(|item| item.source_item_key == SourceItemKey::from("README.md"))
        .expect("first README item");
    let second_item = second
        .items
        .iter()
        .find(|item| item.source_item_key == SourceItemKey::from("README.md"))
        .expect("second README item");
    assert_eq!(first_item.source_item_key, second_item.source_item_key);
    assert_eq!(first_item.size_bytes, second_item.size_bytes);
    assert_eq!(first_item.mtime, second_item.mtime);
    assert_ne!(first_item.content_hash, second_item.content_hash);
    assert!(second_item.mtime.is_some());
}

#[tokio::test]
async fn local_acquisition_uses_verified_discovery_spool_after_source_changes() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    let file = root.join("README.md");
    fs::write(&file, "abcd").unwrap();
    let plan = source_plan(root, SourceScope::Directory);
    let discovered = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, discovered.items);

    fs::write(&file, "wxyz").unwrap();

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let fetched = acquisition.fetched_items.first().unwrap();
    assert_eq!(
        fetched.content_ref,
        ContentRef::InlineText {
            text: "abcd".to_string()
        }
    );
    assert_eq!(
        fetched.manifest_item.content_hash,
        diff.added[0].content_hash
    );
    assert_eq!(adapter.discovery_spool_count(), 1);
    adapter
        .release(&AdapterReleaseRequest {
            job_id: plan.job_id,
            source_id: plan.route.source.source_id.clone(),
            source_kind: plan.route.source.source_kind,
        })
        .unwrap();
    assert_eq!(adapter.discovery_spool_count(), 0);
}

#[tokio::test]
async fn local_adapter_applies_include_exclude_gitignore_and_binary_policy() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join(".gitignore"), "ignored.md\n").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn local() {}").unwrap();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    fs::write(root.join("ignored.md"), "# ignored").unwrap();
    fs::write(root.join("target/generated.rs"), "pub fn generated() {}").unwrap();
    fs::write(root.join("image.png"), [0, 159, 146, 150]).unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route.validated_options.values = local_options();

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["src/lib.rs"]);
}

#[tokio::test]
async fn local_include_globs_can_opt_into_default_pruned_directories() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target/generated.rs"), "pub fn generated() {}").unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route.validated_options.values.insert(
        "include_globs".to_string(),
        vec!["target/generated.rs"].into(),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["target/generated.rs"]);
}

#[tokio::test]
async fn local_directory_discovery_skips_hidden_and_secret_like_files_by_default() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join(".config")).unwrap();
    fs::write(root.join(".env"), "AXON_TOKEN=secret").unwrap();
    fs::write(root.join("api-token.txt"), "secret").unwrap();
    fs::write(root.join(".config/settings.toml"), "token = 'secret'").unwrap();
    fs::write(root.join("README.md"), "# public").unwrap();

    let plan = source_plan(root, SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["README.md"]);
}

#[tokio::test]
async fn local_include_globs_can_opt_into_hidden_and_secret_like_files() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join(".env"), "AXON_TOKEN=secret").unwrap();
    fs::write(root.join("api-token.txt"), "secret").unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route.validated_options.values.insert(
        "include_globs".to_string(),
        vec![".env", "api-token.txt"].into(),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec![".env", "api-token.txt"]);
}

#[tokio::test]
async fn broad_local_include_globs_do_not_opt_into_hidden_or_secret_like_files() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join(".env"), "AXON_TOKEN=secret").unwrap();
    fs::write(root.join("api-token.txt"), "secret").unwrap();
    fs::write(root.join("README.md"), "# public").unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route
        .validated_options
        .values
        .insert("include_globs".to_string(), vec!["**/*"].into());

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["README.md"]);
}

#[tokio::test]
async fn local_repo_scope_prunes_generated_and_lock_files() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(".git/config"), "[core]").unwrap();
    fs::write(root.join("node_modules/pkg/index.js"), "export {}").unwrap();
    fs::write(root.join("Cargo.lock"), "# lock").unwrap();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn local() {}").unwrap();

    let plan = source_plan(root, SourceScope::Repo);
    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["README.md", "src/lib.rs"]);
}

#[tokio::test]
async fn local_repo_scope_respects_nested_gitignore_and_nested_globs() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("src/deep")).unwrap();
    fs::write(root.join("src/.gitignore"), "ignored-dir/\n").unwrap();
    fs::create_dir_all(root.join("src/ignored-dir")).unwrap();
    fs::write(root.join("src/deep/mod.rs"), "pub mod deep;").unwrap();
    fs::write(root.join("src/ignored-dir/lib.rs"), "pub fn ignored() {}").unwrap();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    let mut plan = source_plan(root, SourceScope::Repo);
    plan.route
        .validated_options
        .values
        .insert("include_globs".to_string(), vec!["src/**/*.rs"].into());
    plan.route
        .validated_options
        .values
        .insert("respect_gitignore".to_string(), true.into());

    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["src/deep/mod.rs"]);
}

#[tokio::test]
async fn local_repo_scope_respects_gitignore_by_default() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(root.join("ignored.rs"), "pub fn ignored() {}").unwrap();
    fs::write(root.join("visible.rs"), "pub fn visible() {}").unwrap();

    let plan = source_plan(root, SourceScope::Repo);
    let manifest = adapter.discover(&plan).await.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["visible.rs"]);
}

#[tokio::test]
async fn local_binary_policy_metadata_keeps_manifest_but_skips_document_body() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("image.png"), [0, 159, 146, 150]).unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route.validated_options.values = binary_options("metadata");
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();

    assert_eq!(manifest.items.len(), 1);
    assert_eq!(
        manifest.items[0].content_kind,
        Some(ContentKind::BinaryMetadata)
    );
    assert!(normalized.data.is_empty());
}

#[tokio::test]
async fn local_binary_policy_include_acquires_inline_bytes() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("image.png"), [0, 159, 146, 150]).unwrap();
    let mut plan = source_plan(root, SourceScope::Directory);
    plan.route.validated_options.values = binary_options("include");
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert_eq!(acquisition.fetched_items.len(), 1);
    assert_eq!(
        manifest.items[0].content_kind,
        Some(ContentKind::BinaryMetadata)
    );
    assert!(matches!(
        acquisition.fetched_items[0].content_ref,
        ContentRef::InlineBytes { .. }
    ));
}

#[tokio::test]
async fn local_adapter_errors_do_not_leak_absolute_paths() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    let missing = root.join("missing.md");
    let plan = source_plan(missing, SourceScope::File);

    let err = adapter.discover(&plan).await.unwrap_err();
    let serialized = serde_json::to_string(&err).unwrap();

    assert!(!serialized.contains(root.to_string_lossy().as_ref()));
    assert!(!serialized.contains("/home/"));
    assert_eq!(
        err.details.get("path_hint").map(String::as_str),
        Some("missing.md")
    );
}

#[tokio::test]
async fn local_map_scope_discovers_manifest_but_acquires_no_documents() {
    let adapter = LocalSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("README.md"), "# Axon").unwrap();
    let plan = source_plan(root, SourceScope::Map);
    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();

    assert_eq!(manifest.items.len(), 1);
    assert!(normalized.data.is_empty());
}
