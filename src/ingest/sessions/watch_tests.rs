use super::*;
use crate::core::config::Config;
use crate::ingest::sessions::checkpoint::{
    SessionFileMetadata, checkpoint_remote_accepted_exists_for_path_hash,
    checkpoint_success_exists_for_path_hash, list_recent_errors, record_success,
};
use crate::ingest::sessions::watch::validate::SessionWatchRoots;
use crate::ingest::sessions::{IngestSessionsPreparedRequest, PreparedSessionDoc};
use crate::jobs::backend::{BackendResult, JobKind, JobPayload, JobSidecarPayload};
use crate::services::context::ServiceContext;
use crate::services::runtime::ServiceJobRuntime;
use anyhow::anyhow;
use async_trait::async_trait;
use httpmock::Method::POST;
use httpmock::MockServer;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[test]
fn pending_files_debounce_and_coalesce_same_path() {
    let mut pending = PendingFiles::default();
    let now = Instant::now();
    let path = PathBuf::from("/tmp/a.jsonl");

    assert!(pending.push(path.clone(), now));
    assert!(pending.push(path.clone(), now + Duration::from_millis(100)));
    assert_eq!(pending.files.len(), 1);
    assert_eq!(pending.coalesced_events, 1);
    assert!(
        pending
            .debounced_paths(now + Duration::from_millis(849), Duration::from_millis(750))
            .is_empty()
    );
    assert_eq!(
        pending.debounced_paths(now + Duration::from_millis(850), Duration::from_millis(750)),
        vec![path]
    );
}

#[test]
fn pending_files_requeue_resets_stability_and_honors_retry_cap() {
    let mut pending = PendingFiles::default();
    let now = Instant::now();
    let path = PathBuf::from("/tmp/a.jsonl");

    assert!(pending.push(path.clone(), now));
    assert!(pending.requeue(path.clone(), now + Duration::from_secs(1), 2));
    assert!(pending.requeue(path.clone(), now + Duration::from_secs(2), 2));
    assert!(!pending.requeue(path, now + Duration::from_secs(3), 2));
}

#[test]
fn pending_overflow_requests_rescan() {
    let mut pending = PendingFiles::default();
    for i in 0..MAX_PENDING_FILES {
        assert!(pending.push(PathBuf::from(format!("/tmp/{i}.jsonl")), Instant::now()));
    }
    assert!(!pending.push(PathBuf::from("/tmp/overflow.jsonl"), Instant::now()));
}

#[test]
fn overflow_rescan_cooldown_defers_until_due() {
    let now = Instant::now();
    let cooldown = Duration::from_secs(5);

    assert!(rescan_due(now, None, cooldown));
    assert!(!rescan_due(
        now,
        Some(now - Duration::from_secs(4)),
        cooldown
    ));
    assert!(rescan_due(
        now,
        Some(now - Duration::from_secs(5)),
        cooldown
    ));
}

#[test]
fn remove_event_clears_pending_without_claiming_checkpoint_prune() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let nested = root.join("2026/06/11");
    std::fs::create_dir_all(&nested).unwrap();
    let path = root.join("gone.jsonl");
    let roots = SessionWatchRoots::for_home(temp.path());
    let target = WatchTarget::Directory(root.clone());
    let mut pending = PendingFiles::default();

    handle_remove_path(&path, &roots, &[target], &mut pending);
}

#[test]
fn create_directory_event_returns_dirty_subtree_without_overflow_rescan() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let nested = root.join("2026/06/11");
    std::fs::create_dir_all(&nested).unwrap();
    let roots = SessionWatchRoots::for_home(temp.path());
    let target = WatchTarget::Directory(root.canonicalize().unwrap());
    let mut pending = PendingFiles::default();
    let overflow = AtomicBool::new(false);
    let event = notify::Event::new(notify::EventKind::Create(notify::event::CreateKind::Folder))
        .add_path(nested.clone());

    let dirty = handle_event(Ok(event), &roots, &[target], &mut pending, &overflow);

    assert_eq!(dirty, vec![nested]);
    assert!(!overflow.load(Ordering::Relaxed));
}

#[test]
fn dirty_subtree_collection_avoids_full_root_scan() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let old_dir = root.join("2026/06/10");
    let new_dir = root.join("2026/06/11");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();
    std::fs::write(old_dir.join("old.jsonl"), "{}\n").unwrap();
    std::fs::write(new_dir.join("new.jsonl"), "{}\n").unwrap();
    let roots = SessionWatchRoots::for_home(temp.path());

    let files = collect_validated_files_under(&roots, &new_dir);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].basename, "new.jsonl");
}

#[test]
fn dirty_rescan_dirs_coalesce_parent_and_child_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let parent = root.join("2026/06");
    let child = parent.join("11");
    std::fs::create_dir_all(&child).unwrap();

    let mut dirty = DirtyRescanDirs::default();
    assert!(dirty.push(child.clone()));
    assert!(dirty.push(parent.clone()));
    assert_eq!(dirty.len(), 1);

    let dirs = dirty.take();
    assert_eq!(dirs, vec![parent.canonicalize().unwrap()]);
}

#[test]
fn dirty_rescan_dirs_overflow_returns_false_for_full_rescan_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let mut dirty = DirtyRescanDirs::default();

    for i in 0..MAX_DIRTY_RESCAN_DIRS {
        let dir = root.join(format!("dir-{i}"));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(dirty.push(dir));
    }
    let overflow = root.join("overflow");
    std::fs::create_dir_all(&overflow).unwrap();

    assert!(!dirty.push(overflow));
}

#[test]
fn dirty_rescan_routes_discovered_files_through_pending_queue() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let dirty_dir = root.join("2026/06/11");
    std::fs::create_dir_all(&dirty_dir).unwrap();
    let file = dirty_dir.join("session.jsonl");
    write_codex_session(&file, "queued from dirty rescan");
    let roots = SessionWatchRoots::for_home(temp.path());
    let targets = vec![WatchTarget::Directory(root.canonicalize().unwrap())];
    let mut dirty = DirtyRescanDirs::default();
    let mut pending = PendingFiles::default();
    let overflow = AtomicBool::new(false);

    assert!(dirty.push(dirty_dir));
    run_dirty_rescans(
        &Config::default(),
        &roots,
        &targets,
        &mut pending,
        &mut dirty,
        &overflow,
    );

    assert!(!overflow.load(Ordering::Relaxed));
    assert_eq!(pending.files.len(), 1);
    assert!(pending.files.contains_key(&file.canonicalize().unwrap()));
}

#[test]
fn collect_watch_dirs_skips_symlinks_and_includes_nested_dirs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let nested = root.join("2026/06/11");
    std::fs::create_dir_all(&nested).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&nested, root.join("link-to-nested")).unwrap();

    let dirs = collect_watch_dirs(&root).unwrap();

    assert!(dirs.contains(&root.canonicalize().unwrap()));
    assert!(dirs.contains(&root.join("2026").canonicalize().unwrap()));
    assert!(dirs.contains(&root.join("2026/06").canonicalize().unwrap()));
    assert!(dirs.contains(&nested.canonicalize().unwrap()));
    #[cfg(unix)]
    assert!(!dirs.contains(&root.join("link-to-nested")));
}

#[test]
fn watch_targets_accepts_single_file_by_watching_parent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let nested = root.join("2026/06/11");
    std::fs::create_dir_all(&nested).unwrap();
    let file = root.join("one.jsonl");
    std::fs::write(&file, "{}\n").unwrap();

    let options = test_watch_options(Some(file.clone()));
    let cfg = Config::default();
    let roots = SessionWatchRoots::for_home(temp.path());
    let targets = watch_targets(&cfg, &roots, &options).unwrap();

    assert_eq!(targets.len(), 1);
    match &targets[0] {
        WatchTarget::File { path, parent } => {
            assert_eq!(path, &file.canonicalize().unwrap());
            assert_eq!(parent, &root.canonicalize().unwrap());
        }
        other => panic!("expected file target, got {other:?}"),
    }
}

#[test]
fn default_watch_targets_respect_provider_filters() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".claude/projects")).unwrap();
    std::fs::create_dir_all(temp.path().join(".codex/sessions")).unwrap();
    std::fs::create_dir_all(temp.path().join(".gemini/history")).unwrap();

    let cfg = Config {
        sessions_codex: true,
        ..Config::default()
    };
    let roots = SessionWatchRoots::for_home(temp.path());
    let options = test_watch_options(None);
    let targets = watch_targets(&cfg, &roots, &options).unwrap();

    assert_eq!(targets.len(), 1);
    assert!(targets[0].root().ends_with(".codex/sessions"));
}

#[test]
fn explicit_directory_outside_session_roots_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".codex/sessions")).unwrap();
    let outside = temp.path().join("workspace");
    std::fs::create_dir_all(&outside).unwrap();

    let cfg = Config::default();
    let roots = SessionWatchRoots::for_home(temp.path());
    let options = test_watch_options(Some(outside));
    let error = watch_targets(&cfg, &roots, &options)
        .unwrap_err()
        .to_string();

    assert!(error.contains("inside a supported AI session root"));
}

#[tokio::test]
async fn process_stable_file_skips_unchanged_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("session.jsonl");
    std::fs::write(
        &path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": "already indexed" }]
            }
        })
        .to_string()
            + "\n",
    )
    .unwrap();
    let validated = test_validated_codex_path(&path);
    let meta = SessionFileMetadata::from_validated_path(&validated).unwrap();
    record_success(&pool, &meta, None).await.unwrap();

    let cfg = Config::default();
    let outcome = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![validated],
        &test_watch_options(None),
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    assert_eq!(outcome, vec![ProcessOutcome::SkippedUnchanged]);
}

#[tokio::test]
async fn process_stable_file_records_parse_error_without_panic() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".claude/projects/-tmp-axon");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("bad.jsonl");
    std::fs::write(&path, "{not-json\n").unwrap();

    let cfg = Config::default();
    let validated = test_validated_claude_path(&path);
    let outcome = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![validated],
        &test_watch_options(None),
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    assert!(matches!(
        outcome.as_slice(),
        [ProcessOutcome::RetryableFailure(code)] if code == "parse_failed"
    ));
    let errors = list_recent_errors(&pool, 10).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_code, "parse_failed");
}

#[tokio::test]
async fn process_pending_honors_max_batch_docs_for_stable_files() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let mut pending = PendingFiles::default();
    for index in 0..3 {
        let path = root.join(format!("pending-{index}.jsonl"));
        write_codex_session(&path, &format!("stable file {index}"));
        pending.push(path, Instant::now() - Duration::from_secs(1));
    }

    let cfg = Config {
        sessions_codex: true,
        ..Config::default()
    };
    let roots = SessionWatchRoots::for_home(temp.path());
    let mut options = test_watch_options(None);
    options.debounce = Duration::ZERO;
    options.settle = Duration::ZERO;
    options.max_batch_docs = 2;

    process_pending(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        &roots,
        &options,
        &mut pending,
        &NoopSessionWatchEventSink,
    )
    .await;
    assert_eq!(pending.files.len(), 3);

    process_pending(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        &roots,
        &options,
        &mut pending,
        &NoopSessionWatchEventSink,
    )
    .await;

    assert_eq!(pending.files.len(), 1);
}

#[tokio::test]
async fn upload_prepared_sessions_requires_accepted_job_response() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/ingest/sessions/prepared")
                .header("authorization", "Bearer test-token");
            then.status(202)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "result": { "job_id": "job-123" }
                }));
        })
        .await;

    let label = upload_prepared_sessions_to_server_with_auth(
        &server.base_url(),
        "test-token",
        IngestSessionsPreparedRequest {
            docs: vec![PreparedSessionDoc {
                url: "file:///tmp/session.jsonl".to_string(),
                title: Some("session".to_string()),
                text: "hello from a session".to_string(),
                session_platform: "codex".to_string(),
                session_project: None,
                session_date: None,
                session_turn_count: Some(1),
                session_file: "session.jsonl".to_string(),
                extra: serde_json::json!({}),
            }],
            project: None,
            collection: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(label, "job-123");
    mock.assert_async().await;
}

#[tokio::test]
async fn upload_prepared_sessions_rejects_success_without_job_id() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/ingest/sessions/prepared")
                .header("authorization", "Bearer test-token");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({ "ok": true }));
        })
        .await;

    let error = upload_prepared_sessions_to_server_with_auth(
        &server.base_url(),
        "test-token",
        IngestSessionsPreparedRequest {
            docs: vec![PreparedSessionDoc {
                url: "file:///tmp/session.jsonl".to_string(),
                title: Some("session".to_string()),
                text: "hello from a session".to_string(),
                session_platform: "codex".to_string(),
                session_project: None,
                session_date: None,
                session_turn_count: Some(1),
                session_file: "session.jsonl".to_string(),
                extra: serde_json::json!({}),
            }],
            project: None,
            collection: None,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("202 Accepted"));
    mock.assert_async().await;
}

#[test]
fn remote_prepared_upload_redacts_local_paths_before_serializing() {
    let request = redact_remote_prepared_request(IngestSessionsPreparedRequest {
        docs: vec![PreparedSessionDoc {
            url: "file:///home/jmagar/workspace/axon/.codex/session.jsonl".to_string(),
            title: Some("session".to_string()),
            text: "hello from a session".to_string(),
            session_platform: "codex".to_string(),
            session_project: Some("axon".to_string()),
            session_date: None,
            session_turn_count: Some(1),
            session_file: "/home/jmagar/.codex/sessions/2026/06/11/session.jsonl".to_string(),
            extra: serde_json::json!({
                "cwd": "/home/jmagar/workspace/axon",
                "workspace_path": "/home/jmagar/workspace/axon",
                "model": "gpt-5"
            }),
        }],
        project: None,
        collection: None,
    });
    let body = serde_json::to_string(&request).unwrap();

    assert!(!body.contains("/home/jmagar"));
    assert!(!body.contains("workspace_path"));
    assert!(!body.contains("\"cwd\""));
    assert!(request.docs[0].url.starts_with("file:///redacted/codex/"));
    assert_eq!(request.docs[0].session_file, "session.jsonl");
    assert_eq!(request.docs[0].extra["model"], "gpt-5");
}

#[tokio::test]
async fn remote_watch_acceptance_records_remote_checkpoint_only() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/ingest/sessions/prepared")
                .header("authorization", "Bearer remote-token");
            then.status(202)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "result": { "job_id": "remote-job-123" }
                }));
        })
        .await;

    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("remote.jsonl");
    std::fs::write(
        &path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": "remote acceptance is not completion" }]
            }
        })
        .to_string()
            + "\n",
    )
    .unwrap();

    let cfg = Config::default();
    let mut options = test_watch_options(None);
    options.upload_to_server = true;
    options.upload_server_url = Some(server.base_url());
    options.upload_token = Some("remote-token".to_string());

    let validated = test_validated_codex_path(&path);
    let outcomes = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![validated.clone()],
        &options,
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    assert!(matches!(
        outcomes.as_slice(),
        [ProcessOutcome::RemoteAccepted { job }] if job == "remote-job-123"
    ));
    assert!(
        !checkpoint_success_exists_for_path_hash(&pool, &validated.path_hash)
            .await
            .unwrap()
    );
    assert!(
        checkpoint_remote_accepted_exists_for_path_hash(&pool, &validated.path_hash)
            .await
            .unwrap()
    );
    mock.assert_async().await;
}

#[tokio::test]
async fn remote_upload_oversized_doc_records_terminal_error_without_poisoning_batch() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/ingest/sessions/prepared")
                .header("authorization", "Bearer remote-token");
            then.status(202)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "result": { "job_id": "remote-small-job" }
                }));
        })
        .await;

    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let big_path = root.join("too-big.jsonl");
    let small_path = root.join("small.jsonl");
    write_codex_session(&big_path, &"\"".repeat(13 * 1024 * 1024));
    write_codex_session(&small_path, "small valid doc survives oversized neighbor");

    let cfg = Config::default();
    let mut options = test_watch_options(None);
    options.upload_to_server = true;
    options.upload_server_url = Some(server.base_url());
    options.upload_token = Some("remote-token".to_string());
    let big_validated = test_validated_codex_path(&big_path);
    let small_validated = test_validated_codex_path(&small_path);

    let outcomes = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![big_validated.clone(), small_validated.clone()],
        &options,
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    assert!(
        matches!(
            outcomes.as_slice(),
            [ProcessOutcome::TerminalFailure(code), ProcessOutcome::RemoteAccepted { job }]
                if code == "upload_too_large" && job == "remote-small-job"
        ),
        "unexpected outcomes: {outcomes:?}"
    );
    assert!(
        checkpoint_remote_accepted_exists_for_path_hash(&pool, &small_validated.path_hash)
            .await
            .unwrap()
    );
    let errors = list_recent_errors(&pool, 10).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path_hash, big_validated.path_hash);
    assert_eq!(errors[0].error_code, "upload_too_large");
    mock.assert_async().await;
}

#[tokio::test]
async fn remote_accepted_unchanged_rescan_skips_without_reupload() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/ingest/sessions/prepared")
                .header("authorization", "Bearer remote-token");
            then.status(202)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "result": { "job_id": "remote-job-once" }
                }));
        })
        .await;

    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("remote.jsonl");
    write_codex_session(&path, "remote acceptance is idempotent");

    let cfg = Config::default();
    let mut options = test_watch_options(None);
    options.upload_to_server = true;
    options.upload_server_url = Some(server.base_url());
    options.upload_token = Some("remote-token".to_string());
    let validated = test_validated_codex_path(&path);

    let first = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![validated.clone()],
        &options,
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();
    let second = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![validated],
        &options,
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    // First scan uploads and records a remote_accepted checkpoint.
    assert!(matches!(
        first.as_slice(),
        [ProcessOutcome::RemoteAccepted { job }] if job == "remote-job-once"
    ));
    // An unchanged rescan treats the 202-accepted upload as already handled:
    // it is skipped (no duplicate remote job), so the mock is hit only once.
    assert!(matches!(
        second.as_slice(),
        [ProcessOutcome::SkippedUnchanged]
    ));
    assert_eq!(mock.calls_async().await, 1);
}

#[tokio::test]
async fn watcher_loop_processes_filesystem_create_event() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/ingest/sessions/prepared")
                .header("authorization", "Bearer loop-token");
            then.status(202)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "result": { "job_id": "loop-job" }
                }));
        })
        .await;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let nested = root.join("2026/06/11");
    std::fs::create_dir_all(&nested).unwrap();
    let db_path = temp.path().join("jobs.db");
    let cfg = Config {
        sqlite_path: db_path,
        sessions_codex: true,
        ..Config::default()
    };
    let service_context = ServiceContext::new(Arc::new(cfg.clone())).await.unwrap();
    let mut options = test_watch_options(Some(root.clone()));
    options.initial_scan = false;
    options.debounce = Duration::from_millis(50);
    options.settle = Duration::from_millis(50);
    options.upload_to_server = true;
    options.upload_server_url = Some(server.base_url());
    options.upload_token = Some("loop-token".to_string());
    let watcher_context = service_context.clone();
    let watcher_cfg = cfg.clone();
    let roots = SessionWatchRoots::for_home(temp.path());
    let watcher = tokio::spawn(async move {
        let pool = watcher_context.jobs.sqlite_pool().unwrap();
        let _ = run_session_watch_with_roots(
            &watcher_cfg,
            pool.as_ref(),
            &SuccessfulWatchIngestor,
            options,
            roots,
            &NoopSessionWatchEventSink,
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(250)).await;
    let path = nested.join("loop.jsonl");
    write_codex_session(&path, "created after watcher startup");
    let validated =
        validate::validate_session_file_path(&SessionWatchRoots::for_home(temp.path()), &path)
            .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if checkpoint_remote_accepted_exists_for_path_hash(
            service_context.jobs.sqlite_pool().unwrap().as_ref(),
            &validated.path_hash,
        )
        .await
        .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for watcher checkpoint"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    watcher.abort();
    mock.assert_async().await;
}

#[tokio::test]
async fn project_filtered_watch_skip_does_not_record_success_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions/2026/06/11");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("filtered.jsonl");
    std::fs::write(
        &path,
        serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": "/tmp/other-project", "model": "gpt-5" }
        })
        .to_string()
            + "\n"
            + &serde_json::json!({
                "type": "response_item",
                "payload": {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "this should be filtered" }]
                }
            })
            .to_string()
            + "\n",
    )
    .unwrap();

    let cfg = Config {
        sessions_project: Some("axon".to_string()),
        ..Config::default()
    };
    let options = test_watch_options(None);
    let validated = test_validated_codex_path(&path);

    let outcomes = process_session_batch_for_watch(
        &cfg,
        &SuccessfulWatchIngestor,
        &pool,
        vec![validated.clone()],
        &options,
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    assert_eq!(outcomes, vec![ProcessOutcome::SkippedFiltered]);
    assert!(
        !checkpoint_success_exists_for_path_hash(&pool, &validated.path_hash)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn local_watch_sync_failure_records_error_without_enqueue_success() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.db");
    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
        .await
        .unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("sync-fail.jsonl");
    std::fs::write(
        &path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": "must not checkpoint on enqueue" }]
            }
        })
        .to_string()
            + "\n",
    )
    .unwrap();
    let cfg = Config {
        collection: "invalid/collection".to_string(),
        ..Config::default()
    };
    let runtime = Arc::new(AckingRuntime::default());
    let options = test_watch_options(None);
    let validated = test_validated_codex_path(&path);

    let outcomes = process_session_batch_for_watch(
        &cfg,
        &FailingWatchIngestor,
        &pool,
        vec![validated.clone()],
        &options,
        &NoopSessionWatchEventSink,
    )
    .await
    .unwrap();

    assert!(matches!(
        outcomes.as_slice(),
        [ProcessOutcome::RetryableFailure(_)]
    ));
    assert_eq!(runtime.enqueued.load(Ordering::Relaxed), 0);
    assert!(
        !checkpoint_success_exists_for_path_hash(&pool, &validated.path_hash)
            .await
            .unwrap()
    );
    let errors = list_recent_errors(&pool, 10).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path_hash, validated.path_hash);
}

#[test]
fn max_processing_concurrency_is_clamped_for_batch_preparation() {
    let mut options = test_watch_options(None);
    options.max_processing_concurrency = 0;
    assert_eq!(effective_processing_concurrency(&options), 1);
    options.max_processing_concurrency = 7;
    assert_eq!(effective_processing_concurrency(&options), 7);
}

#[test]
fn redact_error_detail_removes_home_paths_and_tokens() {
    let home = std::env::var("HOME").unwrap();
    let detail = redact_error_detail(&format!(
        "failed to read {home}/.codex/sessions/private.jsonl bearer secret-token"
    ));

    assert!(!detail.contains(&home));
    assert!(!detail.contains("secret-token"));
    assert!(detail.contains("[REDACTED-HOME]") || detail.contains("[REDACTED-SESSION-ROOT]"));
}

fn test_watch_options(path: Option<PathBuf>) -> SessionWatchOptions {
    SessionWatchOptions {
        path,
        debounce: Duration::from_millis(750),
        settle: Duration::from_millis(500),
        max_retries: 5,
        max_batch_docs: 50,
        max_processing_concurrency: 2,
        rescan_cooldown: Duration::from_secs(5),
        initial_scan: false,
        upload_to_server: false,
        upload_server_url: None,
        upload_token: None,
        verbose_paths: false,
        json: false,
    }
}

fn write_codex_session(path: &Path, text: &str) {
    std::fs::write(
        path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": text }]
            }
        })
        .to_string()
            + "\n",
    )
    .unwrap();
}

struct SuccessfulWatchIngestor;

#[async_trait]
impl SessionWatchIngestor for SuccessfulWatchIngestor {
    async fn ingest_prepared_request_for_watch(
        &self,
        _cfg: &Config,
        request: IngestSessionsPreparedRequest,
    ) -> anyhow::Result<WatchIngestResult> {
        Ok(WatchIngestResult::Completed(format!(
            "prepared-session-chunks={}",
            request.docs.len()
        )))
    }
}

struct FailingWatchIngestor;

#[async_trait]
impl SessionWatchIngestor for FailingWatchIngestor {
    async fn ingest_prepared_request_for_watch(
        &self,
        _cfg: &Config,
        _request: IngestSessionsPreparedRequest,
    ) -> anyhow::Result<WatchIngestResult> {
        Err(anyhow!("prepared session exports ingest failed"))
    }
}

#[derive(Default)]
struct AckingRuntime {
    enqueued: AtomicBoolCounter,
}

#[derive(Default)]
struct AtomicBoolCounter(std::sync::atomic::AtomicUsize);

impl AtomicBoolCounter {
    fn fetch_add(&self, value: usize, ordering: Ordering) {
        self.0.fetch_add(value, ordering);
    }

    fn load(&self, ordering: Ordering) -> usize {
        self.0.load(ordering)
    }
}

#[async_trait]
impl ServiceJobRuntime for AckingRuntime {
    fn mode_name(&self) -> &'static str {
        "acking-test"
    }

    async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        Ok(Uuid::new_v4())
    }

    async fn enqueue_with_sidecar(
        &self,
        _payload: JobPayload,
        _sidecar: JobSidecarPayload,
    ) -> BackendResult<Uuid> {
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        Ok(Uuid::new_v4())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Ok("completed".to_string())
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<crate::jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

fn test_validated_codex_path(path: &Path) -> ValidatedSessionPath {
    test_validated_path(path, validate::SessionProvider::Codex)
}

fn test_validated_claude_path(path: &Path) -> ValidatedSessionPath {
    test_validated_path(path, validate::SessionProvider::Claude)
}

fn test_validated_path(path: &Path, provider: validate::SessionProvider) -> ValidatedSessionPath {
    let canonical = path.canonicalize().unwrap();
    let basename = path.file_name().unwrap().to_string_lossy().to_string();
    let path_hash = format!("watch-test-{basename}");
    ValidatedSessionPath {
        canonical,
        provider,
        relative: PathBuf::from(&basename),
        basename: basename.clone(),
        redacted_display: format!("{}:{basename}:{}", provider.as_str(), &path_hash[..12]),
        path_hash,
    }
}
