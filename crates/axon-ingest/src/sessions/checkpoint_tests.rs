use super::*;
use crate::sessions::watch::validate::{SessionProvider, ValidatedSessionPath};
use axon_core::sqlite::open_pool as open_sqlite_pool;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

#[tokio::test]
async fn checkpoint_skips_unchanged_file_and_records_success() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let path = temp.path().join(".codex/sessions/session.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "first").unwrap();

    let validated = test_validated_codex_path(&path);
    let meta = SessionFileMetadata::from_validated_path(&validated).unwrap();
    assert!(!checkpoint_metadata_matches(&pool, &meta).await.unwrap());
    let hash = stream_content_hash(&validated.canonical).await.unwrap();
    record_success(&pool, &meta, Some(&hash)).await.unwrap();
    assert!(checkpoint_metadata_matches(&pool, &meta).await.unwrap());

    std::fs::write(&path, "second").unwrap();
    let changed = SessionFileMetadata::from_validated_path(&validated).unwrap();
    assert!(!checkpoint_metadata_matches(&pool, &changed).await.unwrap());
}

#[tokio::test]
async fn checkpoint_uses_stored_content_hash_even_when_metadata_matches() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let path = temp.path().join(".codex/sessions/session.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "aaaa").unwrap();

    let validated = test_validated_codex_path(&path);
    let meta = SessionFileMetadata::from_validated_path(&validated).unwrap();
    let old_hash = stream_content_hash(&validated.canonical).await.unwrap();
    record_success(&pool, &meta, Some(&old_hash)).await.unwrap();

    std::fs::write(&path, "bbbb").unwrap();
    let rewritten = SessionFileMetadata::from_validated_path(&validated).unwrap();
    sqlx::query(
        "UPDATE axon_session_watch_checkpoints SET file_size = ?, file_mtime_ms = ? WHERE path_hash = ?",
    )
    .bind(rewritten.file_size as i64)
    .bind(rewritten.file_mtime_ms)
    .bind(&rewritten.path_hash)
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        !checkpoint_metadata_matches(&pool, &rewritten)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn remote_accepted_checkpoint_skips_duplicate_upload_on_rescan() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let path = temp.path().join(".codex/sessions/session.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "unchanged remote session").unwrap();

    let validated = test_validated_codex_path(&path);
    let meta = SessionFileMetadata::from_validated_path(&validated).unwrap();
    let hash = stream_content_hash(&validated.canonical).await.unwrap();
    record_remote_accepted(&pool, &meta, Some(&hash), "remote-job-1")
        .await
        .unwrap();

    // Rescan of the same unchanged file must be treated as already-handled
    // so the watcher does not re-upload it.
    assert!(checkpoint_metadata_matches(&pool, &meta).await.unwrap());
    assert!(checkpoint_record_matches(
        &meta,
        &checkpoint_record_for_path_hash(&pool, &meta.path_hash)
            .await
            .unwrap()
            .unwrap(),
        Some(&hash)
    ));
}

#[tokio::test]
async fn checkpoint_updates_metadata_when_content_hash_is_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let path = temp.path().join(".codex/sessions/session.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "same").unwrap();

    let validated = test_validated_codex_path(&path);
    let meta = SessionFileMetadata::from_validated_path(&validated).unwrap();
    let hash = stream_content_hash(&validated.canonical).await.unwrap();
    record_success(&pool, &meta, Some(&hash)).await.unwrap();
    sqlx::query(
        "UPDATE axon_session_watch_checkpoints SET file_size = 1, file_mtime_ms = 1 WHERE path_hash = ?",
    )
    .bind(&meta.path_hash)
    .execute(&pool)
    .await
    .unwrap();

    assert!(checkpoint_metadata_matches(&pool, &meta).await.unwrap());
    let record = checkpoint_record_for_path_hash(&pool, &meta.path_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.file_size, meta.file_size);
    assert_eq!(record.file_mtime_ms, meta.file_mtime_ms);
}

#[tokio::test]
async fn checkpoint_batch_lookup_returns_records_by_path_hash() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let paths = [root.join("one.jsonl"), root.join("two.jsonl")];
    let mut hashes = Vec::new();
    for path in paths {
        std::fs::write(&path, "same").unwrap();
        let meta =
            SessionFileMetadata::from_validated_path(&test_validated_codex_path(&path)).unwrap();
        hashes.push(meta.path_hash.clone());
        record_success(&pool, &meta, Some("hash")).await.unwrap();
    }

    let records = checkpoint_records_by_path_hash(&pool, &hashes)
        .await
        .unwrap();

    assert_eq!(records.len(), 2);
    assert!(hashes.iter().all(|hash| records.contains_key(hash)));
}

#[tokio::test]
async fn checkpoint_records_and_lists_errors() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let path = temp.path().join(".claude/projects/-tmp-axon/bad.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{bad").unwrap();
    let validated = test_validated_claude_path(&path);
    let meta = SessionFileMetadata::from_validated_path(&validated).unwrap();

    record_error(&pool, &meta, "parse_failed", "parse failed: [REDACTED]")
        .await
        .unwrap();
    let errors = list_recent_errors(&pool, 10).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path_hash, meta.path_hash);
    assert_eq!(errors[0].provider, "claude");
    assert_eq!(errors[0].error_code, "parse_failed");
    assert!(!errors[0].error_redacted.contains("Bearer "));
}

#[tokio::test]
async fn session_watch_status_counts_checkpoints_and_errors() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = test_pool(&db_path).await;
    let good = temp.path().join(".codex/sessions/good.jsonl");
    let bad = temp.path().join(".claude/projects/-tmp-axon/bad.jsonl");
    std::fs::create_dir_all(good.parent().unwrap()).unwrap();
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&good, "good").unwrap();
    std::fs::write(&bad, "bad").unwrap();
    let good_meta =
        SessionFileMetadata::from_validated_path(&test_validated_codex_path(&good)).unwrap();
    let bad_meta =
        SessionFileMetadata::from_validated_path(&test_validated_claude_path(&bad)).unwrap();

    record_success(&pool, &good_meta, Some("hash"))
        .await
        .unwrap();
    record_error(&pool, &bad_meta, "parse_failed", "parse failed")
        .await
        .unwrap();

    let status = watch_status(&pool, 5).await.unwrap();

    assert_eq!(status.checkpoint_count, 2);
    assert_eq!(status.error_count, 1);
    assert_eq!(status.recent_errors.len(), 1);
    assert!(
        checkpoint_success_exists_for_path_hash(&pool, &good_meta.path_hash)
            .await
            .unwrap()
    );
    assert!(
        !checkpoint_success_exists_for_path_hash(&pool, &bad_meta.path_hash)
            .await
            .unwrap()
    );
}

fn test_validated_codex_path(path: &Path) -> ValidatedSessionPath {
    test_validated_path(path, SessionProvider::Codex)
}

fn test_validated_claude_path(path: &Path) -> ValidatedSessionPath {
    test_validated_path(path, SessionProvider::Claude)
}

async fn test_pool(db_path: &Path) -> SqlitePool {
    let pool = open_sqlite_pool(&db_path.to_string_lossy()).await.unwrap();
    apply_test_migrations(&pool).await;
    pool
}

async fn apply_test_migrations(pool: &SqlitePool) {
    for migration in [
        include_str!("../../../axon-jobs/src/migrations/0010_create_session_watch_tables.sql"),
        include_str!(
            "../../../axon-jobs/src/migrations/0011_add_session_watch_checkpoint_state.sql"
        ),
    ] {
        for statement in migration.split(';').map(str::trim) {
            if !statement.is_empty() {
                sqlx::query(statement).execute(pool).await.unwrap();
            }
        }
    }
}

fn test_validated_path(path: &Path, provider: SessionProvider) -> ValidatedSessionPath {
    let canonical = path.canonicalize().unwrap();
    let basename = path.file_name().unwrap().to_string_lossy().to_string();
    let path_hash = format!("test-{}", basename);
    ValidatedSessionPath {
        canonical,
        provider,
        relative: PathBuf::from(&basename),
        basename: basename.clone(),
        redacted_display: format!("{}:{basename}:{path_hash}", provider.as_str()),
        path_hash,
    }
}
