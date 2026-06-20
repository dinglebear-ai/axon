use super::*;
use tempfile::tempdir;

#[test]
fn project_key_is_scoped_to_checkout_collection_and_embedder() {
    let a = tempdir().unwrap();
    let b = tempdir().unwrap();
    let origin = "git:https://example.test/owner/repo\nworktree:";
    let identity_a = CodeIndexIdentity::for_test(
        a.path(),
        &format!("{origin}{}", a.path().display()),
        "axon",
        "tei-a",
    );
    let identity_b = CodeIndexIdentity::for_test(
        b.path(),
        &format!("{origin}{}", b.path().display()),
        "axon",
        "tei-a",
    );
    let identity_other_collection = CodeIndexIdentity::for_test(
        a.path(),
        &format!("{origin}{}", a.path().display()),
        "other",
        "tei-a",
    );
    let identity_other_embedder = CodeIndexIdentity::for_test(
        a.path(),
        &format!("{origin}{}", a.path().display()),
        "axon",
        "tei-b",
    );

    assert_ne!(identity_a.project_key, identity_b.project_key);
    assert_ne!(
        identity_a.project_key,
        identity_other_collection.project_key
    );
    assert_ne!(identity_a.project_key, identity_other_embedder.project_key);
}

#[tokio::test]
async fn manifest_uses_metadata_fast_path_for_unchanged_files() {
    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("lib.rs"), "pub fn one() {}\n")
        .await
        .unwrap();
    let store = store::CodeIndexStore::open_in_memory().await.unwrap();
    store.init_schema().await.unwrap();
    let identity = CodeIndexIdentity::for_test(dir.path(), "origin:axon", "axon", "tei-test");

    let first = manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
        .await
        .unwrap();
    assert_eq!(first.files.len(), 1);
    assert_eq!(first.files[0].relative_path, "lib.rs");
    assert!(first.files[0].hash.is_some());
    store.commit_manifest(&identity, &first).await.unwrap();

    let second = manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
        .await
        .unwrap();
    assert_eq!(second.files[0].hash, first.files[0].hash);
    assert_eq!(second.files[0].hash_source, manifest::HashSource::Stored);
}

#[tokio::test]
async fn sentinel_pending_file_is_modified_even_when_hash_matches() {
    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("lib.rs"), "pub fn one() {}\n")
        .await
        .unwrap();
    let store = store::CodeIndexStore::open_in_memory().await.unwrap();
    store.init_schema().await.unwrap();
    let identity = CodeIndexIdentity::for_test(dir.path(), "origin:axon", "axon", "tei-test");
    let manifest =
        manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
            .await
            .unwrap();
    store.commit_manifest(&identity, &manifest).await.unwrap();
    store.mark_file_pending(&identity, "lib.rs").await.unwrap();

    let diff = store.diff_manifest(&identity, &manifest).await.unwrap();
    assert_eq!(diff.modified_paths(), vec!["lib.rs"]);
}

#[tokio::test]
async fn failed_partial_generation_is_not_reused_for_deleted_file_cleanup() {
    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("lib.rs"), "pub fn one() {}\n")
        .await
        .unwrap();
    let store = store::CodeIndexStore::open_in_memory().await.unwrap();
    store.init_schema().await.unwrap();
    let identity = CodeIndexIdentity::for_test(dir.path(), "origin:axon", "axon", "tei-test");
    let manifest =
        manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
            .await
            .unwrap();
    let partial_generation = store.next_generation(&identity).await.unwrap();
    assert_eq!(partial_generation, 1);

    // Simulate a refresh that embedded/recorded generation 1, then timed out
    // before `commit_generation`. The next successful refresh must not reuse 1.
    store
        .mark_file_indexed(&identity, &manifest.files[0], partial_generation)
        .await
        .unwrap();
    tokio::fs::remove_file(dir.path().join("lib.rs"))
        .await
        .unwrap();

    let empty_manifest =
        manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
            .await
            .unwrap();
    let diff = store
        .diff_manifest(&identity, &empty_manifest)
        .await
        .unwrap();
    assert_eq!(diff.removed, vec!["lib.rs"]);
    let next_generation = store.next_generation(&identity).await.unwrap();
    assert_eq!(next_generation, 2);

    let deletes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    indexer::reindex_changed_files_for_test(
        &store,
        &identity,
        &empty_manifest,
        &diff,
        next_generation,
        deletes.clone(),
    )
    .await
    .unwrap();
    assert_eq!(deletes.lock().unwrap().as_slice(), &["lib.rs"]);
    assert_eq!(
        store.committed_generation(&identity).await.unwrap(),
        Some(next_generation)
    );
}

#[test]
fn path_prefix_rejects_absolute_parent_and_escape_segments() {
    assert!(config::validate_path_prefix("/etc").is_err());
    assert!(config::validate_path_prefix("../src").is_err());
    assert!(config::validate_path_prefix("src/../../secrets").is_err());
    assert_eq!(
        config::validate_path_prefix("src/vector").unwrap(),
        Some("src/vector/".to_string())
    );
}

#[test]
fn allowed_roots_parse_valid_roots_and_reject_unsafe_roots() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("repo");
    std::fs::create_dir(&nested).unwrap();
    let allowed = CodeSearchAllowedRoots::from_raw_for_test(&dir.path().to_string_lossy()).unwrap();
    assert!(allowed.contains(&std::fs::canonicalize(&nested).unwrap()));

    let err = CodeSearchAllowedRoots::from_raw_for_test("/")
        .unwrap_err()
        .to_string();
    assert!(err.contains("code search allowed root cannot be / or HOME"));
}

#[tokio::test]
async fn empty_file_deletes_old_vectors_and_marks_current_hash() {
    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("lib.rs"), "")
        .await
        .unwrap();
    let store = store::CodeIndexStore::open_in_memory().await.unwrap();
    store.init_schema().await.unwrap();
    let identity = CodeIndexIdentity::for_test(dir.path(), "origin:axon", "axon", "tei-test");
    let manifest =
        manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
            .await
            .unwrap();
    let diff = store.diff_manifest(&identity, &manifest).await.unwrap();

    let deletes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    indexer::reindex_changed_files_for_test(
        &store,
        &identity,
        &manifest,
        &diff,
        7,
        deletes.clone(),
    )
    .await
    .unwrap();
    assert_eq!(deletes.lock().unwrap().as_slice(), &["lib.rs"]);
    assert!(
        !store
            .lookup_file(&identity, "lib.rs")
            .await
            .unwrap()
            .unwrap()
            .pending
    );
}

#[tokio::test]
async fn changed_refresh_cleans_previous_generation_for_complete_snapshot() {
    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("a.rs"), "pub fn alpha() {}\n")
        .await
        .unwrap();
    tokio::fs::write(dir.path().join("b.rs"), "pub fn beta() {}\n")
        .await
        .unwrap();
    let store = store::CodeIndexStore::open_in_memory().await.unwrap();
    store.init_schema().await.unwrap();
    let identity = CodeIndexIdentity::for_test(dir.path(), "origin:axon", "axon", "tei-test");
    let manifest =
        manifest::build_manifest(&store, &identity, manifest::ManifestOptions::default())
            .await
            .unwrap();
    let diff = store.diff_manifest(&identity, &manifest).await.unwrap();

    let deletes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    indexer::reindex_changed_files_for_test(
        &store,
        &identity,
        &manifest,
        &diff,
        7,
        deletes.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        deletes.lock().unwrap().as_slice(),
        &["a.rs".to_string(), "b.rs".to_string()]
    );
    assert_eq!(store.cleanup_debt(&identity).await.unwrap().len(), 0);
}

#[tokio::test]
async fn concurrent_refresh_cannot_delete_newer_generation() {
    let body = crate::vector::ops::qdrant::local_code_batch_delete_body_for_test(
        "project-1",
        41,
        &["src/lib.rs".to_string()],
    );
    let must = body["filter"]["must"].as_array().unwrap();
    assert!(
        must.iter()
            .any(|c| c["key"] == "local_generation" && c["match"]["value"] == 41)
    );
    assert!(
        must.iter()
            .any(|c| c["key"] == "local_index_version" && c["match"]["value"] == 1)
    );
}
