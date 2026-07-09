use super::*;
use crate::context::ServiceContext;
use axon_api::source::{AuthSnapshot, CallerContext, TransportKind, Visibility};
use axon_jobs::backend::JobKind as LegacyJobKind;
use std::sync::Arc;
use std::time::Duration;

async fn test_ctx_with_workers() -> ServiceContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = axon_core::config::Config {
        sqlite_path: dir.path().join("jobs.db"),
        ..axon_core::config::Config::test_default()
    };
    std::mem::forget(dir);
    ServiceContext::new_with_workers(Arc::new(cfg))
        .await
        .expect("service context")
}

#[tokio::test]
async fn embed_start_with_context_enqueues_on_unified_job_store_with_caller_auth() {
    let ctx = test_ctx_with_workers().await;
    let caller = AuthSnapshot::from_caller(
        &CallerContext {
            actor: Some("user_1".to_string()),
            transport: TransportKind::Cli,
            scopes: vec!["axon:read".to_string(), "axon:write".to_string()],
            visibility_ceiling: Visibility::Internal,
        },
        Visibility::Internal,
        "test",
    );
    let outcome = embed_start_with_context(
        ctx.cfg(),
        "not-a-real-embed-target",
        &ctx,
        None,
        None,
        Some(&caller),
    )
    .await
    .expect("embed_start_with_context should enqueue");
    let store = ctx.job_store().expect("unified job store must be attached");
    let job = store
        .get(axon_api::source::JobId(
            uuid::Uuid::parse_str(&outcome.result.job_id).unwrap(),
        ))
        .await
        .unwrap()
        .expect("job row must exist");
    assert_eq!(job.kind, axon_api::source::JobKind::Embed);
}

/// Embed now enqueues onto the unified `JobStore` and runs on the real
/// unified worker (see `EmbedRunner` in `runtime/job_runners.rs`), while
/// `job_service::job_status`/`list_jobs`/`cancel_job`/etc. for
/// `JobKind::Embed` bridge onto the same store (see
/// `runtime/sqlite/embed_bridge.rs`) so existing CLI/MCP/REST callers keep
/// working unchanged. A deliberately-invalid input keeps this test
/// deterministic and network-free — the embed pipeline fails fast in input
/// validation rather than reaching TEI/Qdrant.
#[tokio::test]
async fn embed_job_runs_end_to_end_and_is_claimed_promptly() {
    let ctx = test_ctx_with_workers().await;
    let started = std::time::Instant::now();
    let outcome =
        embed_start_with_context(ctx.cfg(), "not-a-real-embed-target", &ctx, None, None, None)
            .await
            .expect("enqueue");
    let job_id = uuid::Uuid::parse_str(&outcome.result.job_id).expect("job id");

    let mut status = None;
    for _ in 0..100 {
        let job = crate::jobs::job_status(&ctx, LegacyJobKind::Embed, job_id)
            .await
            .expect("job_status")
            .expect("job exists");
        if job.status != "pending" && job.status != "running" {
            status = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let job = status.expect("embed job should reach a terminal status within timeout");
    let unsupported_stage = job
        .error_text
        .as_deref()
        .is_some_and(|text| text.contains("not wired yet"));
    assert!(
        !unsupported_stage,
        "embed must dispatch to the real runner, not the catch-all: {:?}",
        job.error_text
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "embed job took longer than a poll-interval-free path should — notify_unified() regression?"
    );

    let jobs = crate::jobs::list_jobs(&ctx, LegacyJobKind::Embed, 10, 0)
        .await
        .expect("list_jobs");
    assert!(jobs.iter().any(|j| j.id == job_id));
}

#[test]
#[cfg(unix)]
fn validate_server_embed_input_rejects_nested_directory_symlink() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    let input = root.join("docs");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&input).expect("input dir");
    std::fs::create_dir_all(&outside).expect("outside dir");
    std::fs::write(outside.join("secret.md"), "secret").expect("outside file");
    std::os::unix::fs::symlink(outside.join("secret.md"), input.join("linked.md"))
        .expect("symlink");

    let err = validate_server_embed_input_with_roots(
        &input.to_string_lossy(),
        &[root],
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect_err("nested symlink should be rejected");

    assert!(
        err.to_string().contains("must not contain symlinks"),
        "{err}"
    );
}

#[test]
#[cfg(unix)]
fn validate_server_embed_input_allows_symlinks_inside_pruned_dirs() {
    // node_modules/.bin/* is symlinks by design; the reader prunes the whole
    // subtree, so the validator must not reject the embed for files that are
    // never read.
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    let input = root.join("project");
    let bin = input.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).expect("node_modules/.bin");
    std::fs::write(input.join("index.js"), "console.log(1)").expect("source file");
    std::fs::write(bin.join("tool.js"), "#!/usr/bin/env node").expect("tool file");
    std::os::unix::fs::symlink(bin.join("tool.js"), input.join("node_modules").join("tool"))
        .expect("symlink inside pruned dir");

    let validated = validate_server_embed_input_with_roots(
        &input.to_string_lossy(),
        &[root],
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect("symlink inside a pruned dir must not fail validation");

    assert!(validated.ends_with("project"));
}

#[test]
fn validate_server_embed_input_canonicalizes_allowed_local_file() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    let file = root.join("docs").join("page.md");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    std::fs::write(&file, "content").expect("file");

    let validated = validate_server_embed_input_with_roots(
        &file.to_string_lossy(),
        &[root],
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect("allowed local file");

    assert_eq!(
        validated,
        std::fs::canonicalize(file)
            .expect("canonical")
            .to_string_lossy()
            .to_string()
    );
}

#[test]
fn validate_server_embed_input_bounds_directory_depth() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    let nested = root.join("a").join("b");
    std::fs::create_dir_all(&nested).expect("nested dir");
    std::fs::write(nested.join("page.md"), "content").expect("file");

    let err = validate_server_embed_input_with_roots(
        &root.to_string_lossy(),
        std::slice::from_ref(&root),
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 1,
            max_entries: 10_000,
        },
    )
    .expect_err("depth should be bounded");

    assert!(err.to_string().contains("exceeded max depth"), "{err}");
}

#[test]
fn validate_server_embed_input_bounds_directory_entries() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    std::fs::create_dir_all(&root).expect("root dir");
    std::fs::write(root.join("a.md"), "a").expect("file a");
    std::fs::write(root.join("b.md"), "b").expect("file b");

    let err = validate_server_embed_input_with_roots(
        &root.to_string_lossy(),
        std::slice::from_ref(&root),
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 1,
        },
    )
    .expect_err("entry count should be bounded");

    assert!(err.to_string().contains("exceeded max entries"), "{err}");
}

#[test]
fn validate_server_embed_input_uses_configured_roots_and_limits() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    let file = root.join("large.md");
    std::fs::create_dir_all(&root).expect("root dir");
    std::fs::write(&file, "0123456789").expect("file");
    let mut cfg = Config::test_default();
    cfg.mcp_embed_allowed_roots = vec![root];
    cfg.mcp_embed_max_local_bytes = 4;

    let err = validate_server_embed_input_with_config(&cfg, &file.to_string_lossy())
        .expect_err("configured max bytes should reject local file");

    assert!(err.to_string().contains("exceeds 4 byte limit"), "{err}");
}

#[test]
fn validate_server_embed_input_rejects_missing_path_like_input() {
    let err = validate_server_embed_input_with_roots(
        "./missing/docs.md",
        &[],
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect_err("missing path-like input should not be treated as free text");

    assert!(err.to_string().contains("does not exist"), "{err}");
}

#[test]
fn validate_server_embed_input_prunes_junk_dirs_before_security_checks() {
    // A dotfile buried in node_modules/ would trip the dotfile rejection if the
    // validator descended into it — but the reader never reads node_modules, so
    // the validator must prune it first and accept the directory.
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    let junk = root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&junk).expect("junk dir");
    std::fs::write(junk.join("tool"), "#!/bin/sh\n").expect("junk file");
    std::fs::write(root.join("main.rs"), "fn main() {}").expect("real file");

    let validated = validate_server_embed_input_with_roots(
        &root.to_string_lossy(),
        std::slice::from_ref(&root),
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect("pruned junk dir must not fail validation");

    assert_eq!(
        validated,
        std::fs::canonicalize(&root)
            .expect("canonical")
            .to_string_lossy()
            .to_string()
    );
}

#[test]
fn validate_server_embed_input_skips_binary_files() {
    // A binary-extension file is skipped by the reader, so the validator must
    // not subject it to (e.g.) the size limit. A large .png passes even when a
    // same-size .md would be rejected.
    let temp = tempfile::TempDir::new().expect("tempdir");
    let root = temp.path().join("allowed");
    std::fs::create_dir_all(&root).expect("root dir");
    std::fs::write(root.join("big.png"), vec![0u8; 64]).expect("png");
    std::fs::write(root.join("ok.md"), "hi").expect("md");

    let validated = validate_server_embed_input_with_roots(
        &root.to_string_lossy(),
        std::slice::from_ref(&root),
        EmbedValidationLimits {
            max_file_bytes: 8,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect("binary file should be skipped, not size-checked");

    assert_eq!(
        validated,
        std::fs::canonicalize(&root)
            .expect("canonical")
            .to_string_lossy()
            .to_string()
    );
}

#[test]
fn validate_server_embed_input_allows_free_text_with_slashes() {
    let validated = validate_server_embed_input_with_roots(
        "a/b testing plan",
        &[],
        EmbedValidationLimits {
            max_file_bytes: 1024,
            max_depth: 16,
            max_entries: 10_000,
        },
    )
    .expect("slash-containing prose should remain valid free text");

    assert_eq!(validated, "a/b testing plan");
}

#[test]
fn embed_input_is_local_path_classifies_paths_urls_and_free_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = dir.path().to_string_lossy().to_string();
    assert!(
        embed_input_is_local_path(&dir_path),
        "an existing directory is a local path"
    );

    let file = dir.path().join("note.txt");
    std::fs::write(&file, "hi").expect("write file");
    assert!(
        embed_input_is_local_path(&file.to_string_lossy()),
        "an existing file is a local path"
    );

    // URLs are never local paths.
    assert!(
        !embed_input_is_local_path("https://example.com/doc"),
        "https URL is not a local path"
    );
    assert!(
        !embed_input_is_local_path("http://example.com/doc"),
        "http URL is not a local path"
    );

    // Free text and non-existent paths are not local paths. (A path-like input
    // that does not exist is rejected upstream by validation, never reaching the
    // in-process guard.)
    assert!(
        !embed_input_is_local_path("just some free text"),
        "free text is not a local path"
    );
    assert!(
        !embed_input_is_local_path("/nonexistent/path/should/not/exist/xyz"),
        "a non-existent path is not a local path"
    );

    // Surrounding whitespace is trimmed before the existence check.
    assert!(
        embed_input_is_local_path(&format!("  {dir_path}  ")),
        "leading/trailing whitespace is trimmed before classification"
    );

    // The URL short-circuit is anchored (starts_with), so a scheme-like substring
    // that is not a prefix is classified by existence, not the URL guard.
    assert!(
        !embed_input_is_local_path("notes/http://draft"),
        "non-anchored scheme substring (nonexistent) is not a local path"
    );

    // A trailing slash on an existing directory still classifies as local.
    assert!(
        embed_input_is_local_path(&format!("{dir_path}/")),
        "trailing-slash directory path is a local path"
    );
}
