use super::*;
use crate::context::ServiceContext;
use crate::runtime::ServiceJobRuntime;
use crate::types::StartDisposition;
use async_trait::async_trait;
use axon_api::mcp_schema::{IngestRequest, IngestSourceType};
use axon_api::source::{AuthSnapshot, CallerContext, TransportKind, Visibility};
use axon_core::config::Config;
use axon_ingest as ingest;
use axon_jobs::backend::{BackendResult, JobKind as LegacyJobKind, JobPayload, JobSidecarPayload};
use axon_jobs::config_snapshot::decode_ingest_job_config;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

struct CaptureRuntime {
    payloads: Mutex<Vec<JobPayload>>,
    sidecars: Mutex<Vec<JobSidecarPayload>>,
}

fn ingest_req(source_type: IngestSourceType, target: &str) -> IngestRequest {
    IngestRequest {
        source_type: Some(source_type),
        target: Some(target.to_string()),
        ..Default::default()
    }
}

/// Request with NO explicit source_type — exercises the auto-classify fallback.
fn auto_ingest_req(target: &str) -> IngestRequest {
    IngestRequest {
        source_type: None,
        target: Some(target.to_string()),
        ..Default::default()
    }
}

#[async_trait]
impl ServiceJobRuntime for CaptureRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.payloads.lock().expect("lock").push(payload);
        Ok(Uuid::new_v4())
    }

    async fn enqueue_with_sidecar(
        &self,
        payload: JobPayload,
        sidecar: JobSidecarPayload,
    ) -> BackendResult<Uuid> {
        self.payloads.lock().expect("lock").push(payload);
        self.sidecars.lock().expect("lock").push(sidecar);
        Ok(Uuid::new_v4())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: LegacyJobKind) -> BackendResult<String> {
        panic!("--wait false ingest start must enqueue without waiting")
    }

    async fn job_errors(&self, _id: Uuid, _kind: LegacyJobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: LegacyJobKind) -> BackendResult<bool> {
        panic!("--wait false ingest start must not drain the queue")
    }

    async fn list_jobs(
        &self,
        _kind: LegacyJobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<crate::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn job_status(
        &self,
        _kind: LegacyJobKind,
        _id: Uuid,
    ) -> Result<Option<crate::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: LegacyJobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(
        &self,
        _kind: LegacyJobKind,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: LegacyJobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: LegacyJobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: LegacyJobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: LegacyJobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

#[test]
fn source_from_mcp_request_normalizes_github_url() {
    let cfg = Config::test_default();
    let source = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Github,
            "https://github.com/rust-lang/rust/issues/123",
        ),
        &cfg,
    )
    .expect("github url target");

    assert!(matches!(
        source,
        IngestSource::Github {
            repo,
            include_source,
        } if repo == "rust-lang/rust" && include_source == cfg.github_include_source
    ));
}

#[test]
fn source_from_mcp_request_rejects_invalid_github_target() {
    let cfg = Config::test_default();
    let err = source_from_mcp_request(&ingest_req(IngestSourceType::Github, "not-a-target"), &cfg)
        .expect_err("invalid github target");

    assert!(err.contains("invalid GitHub target"));
}

#[test]
fn source_from_mcp_request_auto_classifies_when_source_type_omitted() {
    let cfg = Config::test_default();

    // GitHub owner/repo shorthand.
    let gh = source_from_mcp_request(&auto_ingest_req("unraid/api"), &cfg).expect("auto github");
    assert!(matches!(gh, IngestSource::Github { repo, .. } if repo == "unraid/api"));

    // GitLab URL — the case the palette's TS classifier got wrong (sent as github).
    let gl = source_from_mcp_request(&auto_ingest_req("https://gitlab.com/group/project"), &cfg)
        .expect("auto gitlab");
    assert!(matches!(gl, IngestSource::Gitlab { .. }));

    // Reddit shorthand.
    let rd = source_from_mcp_request(&auto_ingest_req("r/rust"), &cfg).expect("auto reddit");
    assert!(matches!(rd, IngestSource::Reddit { target } if target == "r/rust"));

    // RSS/Atom feed URL — `.rss` extension routes to the feed ingester, not a
    // generic web crawl, when source_type is omitted.
    let rss = source_from_mcp_request(&auto_ingest_req("https://example.com/feed.rss"), &cfg)
        .expect("auto rss");
    assert!(
        matches!(rss, IngestSource::Rss { target } if target == "https://example.com/feed.rss")
    );

    // Explicit `feed:` prefix also classifies as a feed.
    let feed =
        source_from_mcp_request(&auto_ingest_req("feed:https://blog.example.com/atom"), &cfg)
            .expect("auto feed prefix");
    assert!(matches!(feed, IngestSource::Rss { .. }));
}

#[test]
fn source_from_mcp_request_auto_classify_requires_a_target() {
    let cfg = Config::test_default();
    let err = source_from_mcp_request(
        &IngestRequest {
            source_type: None,
            target: None,
            ..Default::default()
        },
        &cfg,
    )
    .expect_err("auto-classify with no target");
    assert!(err.contains("target"));
}

#[tokio::test]
async fn preflight_skips_non_github_sources() {
    // Non-GitHub sources are never probed, so this is offline-safe and must
    // succeed without touching the network.
    let cfg = Config::test_default();
    let source = IngestSource::Reddit {
        target: "r/rust".to_string(),
    };
    preflight_ingest_source(&cfg, &source)
        .await
        .expect("non-github preflight is a no-op");
}

#[test]
fn source_from_mcp_request_rejects_wrong_source_target_pair() {
    let cfg = Config::test_default();
    let err = source_from_mcp_request(
        &ingest_req(IngestSourceType::Reddit, "https://example.com/not/reddit"),
        &cfg,
    )
    .expect_err("invalid reddit target");

    assert!(err.contains("Reddit") || err.contains("reddit"));
}

#[test]
fn source_from_mcp_request_rejects_invalid_youtube_target() {
    let cfg = Config::test_default();
    let err = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Youtube,
            "https://example.com/watch?v=nope",
        ),
        &cfg,
    )
    .expect_err("invalid youtube target");

    assert!(err.contains("YouTube"));
}

#[test]
fn ingest_payload_uses_chunks_embedded_as_canonical_count() {
    let payload = ingest_payload("github", Some(("repo", "nexu-io/open-design")), 43598);

    assert_eq!(payload["source"], "github");
    assert_eq!(payload["repo"], "nexu-io/open-design");
    assert_eq!(payload["chunks_embedded"], 43598);
    assert_eq!(payload["chunks"], 43598);
}

#[test]
fn source_from_mcp_request_normalizes_supported_git_targets() {
    let cfg = Config::test_default();

    let gitlab = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Gitlab,
            "https://gitlab.com/group/subgroup/project/-/issues/1",
        ),
        &cfg,
    )
    .expect("valid gitlab target");
    assert!(matches!(
        gitlab,
        IngestSource::Gitlab {
            target,
            include_source,
        } if target == "gitlab.com/group/subgroup/project"
            && include_source == cfg.github_include_source
    ));

    let gitea = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Gitea,
            "gitea:gitea.example.com/org/repo.git",
        ),
        &cfg,
    )
    .expect("valid gitea target");
    assert!(matches!(
        gitea,
        IngestSource::Gitea {
            target,
            include_source,
        } if target == "gitea.example.com/org/repo"
            && include_source == cfg.github_include_source
    ));

    let generic = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Git,
            "git:https://example.com/org/repo.git",
        ),
        &cfg,
    )
    .expect("valid generic git target");
    assert!(matches!(
        generic,
        IngestSource::GenericGit {
            target,
            include_source,
        } if target == "https://example.com/org/repo.git"
            && include_source == cfg.github_include_source
    ));
}

#[test]
fn source_from_mcp_request_respects_include_source_override() {
    let mut cfg = Config::test_default();
    cfg.github_include_source = true;
    let mut req = ingest_req(
        IngestSourceType::Github,
        "https://github.com/owner/repo.git",
    );
    req.include_source = Some(false);

    let source = source_from_mcp_request(&req, &cfg).expect("valid github target");

    assert!(matches!(
        source,
        IngestSource::Github {
            repo,
            include_source: false,
        } if repo == "owner/repo"
    ));
}

#[test]
fn source_from_mcp_request_accepts_youtube_handle() {
    let cfg = Config::test_default();
    let source = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Youtube,
            "https://www.youtube.com/@SpaceinvaderOne",
        ),
        &cfg,
    )
    .expect("valid youtube channel target");

    assert!(
        matches!(source, IngestSource::Youtube { target } if target.contains("@SpaceinvaderOne"))
    );
}

#[test]
fn source_from_mcp_request_accepts_youtube_playlist_url() {
    let cfg = Config::test_default();
    let source = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Youtube,
            "https://www.youtube.com/playlist?list=PL1234567890abcdef",
        ),
        &cfg,
    )
    .expect("valid youtube playlist target");

    assert!(matches!(source, IngestSource::Youtube { target } if target.contains("playlist")));
}

#[test]
fn source_from_mcp_request_rejects_non_reddit_comments_url() {
    let cfg = Config::test_default();
    let err = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Reddit,
            "https://example.com/r/rust/comments/abc/title",
        ),
        &cfg,
    )
    .expect_err("non-reddit thread URL should fail");

    assert!(err.contains("Reddit") || err.contains("reddit"));
}

#[test]
fn source_from_mcp_request_auto_classifies_omitted_source_type() {
    // Previously this errored ("source_type is required"); now an omitted
    // source_type auto-detects from the target via the shared classifier.
    let cfg = Config::test_default();
    let req = IngestRequest {
        target: Some("owner/repo".to_string()),
        ..Default::default()
    };

    let source = source_from_mcp_request(&req, &cfg).expect("auto-classified github");
    assert!(matches!(source, IngestSource::Github { repo, .. } if repo == "owner/repo"));
}

#[test]
fn source_from_mcp_request_requires_target_for_github() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Github),
        ..Default::default()
    };

    let err = source_from_mcp_request(&req, &cfg).expect_err("missing github target");

    assert!(err.contains("target repo is required"));
}

#[test]
fn source_from_mcp_request_rejects_remote_sessions_scan() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Sessions),
        ..Default::default()
    };

    let err = source_from_mcp_request(&req, &cfg).expect_err("remote sessions rejected");

    assert!(err.contains("/v1/ingest/sessions/prepared"));
}

async fn test_ctx_with_workers() -> ServiceContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        sqlite_path: dir.path().join("jobs.db"),
        ..Config::test_default()
    };
    std::mem::forget(dir);
    ServiceContext::new_with_workers(Arc::new(cfg))
        .await
        .expect("service context")
}

#[tokio::test]
async fn ingest_start_with_context_enqueues_sessions_source_on_unified_job_store_with_caller_auth()
{
    let ctx = test_ctx_with_workers().await;
    let mut cfg = ctx.cfg().clone();
    cfg.sessions_claude = true;
    cfg.sessions_codex = false;
    cfg.sessions_gemini = true;
    cfg.sessions_project = Some("axon-rust".to_string());
    let source = IngestSource::Sessions {
        sessions_claude: true,
        sessions_codex: false,
        sessions_gemini: true,
        sessions_project: Some("axon-rust".to_string()),
    };
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

    let outcome = ingest_start_with_context(&cfg, source, &ctx, Some(&caller))
        .await
        .expect("ingest_start_with_context should enqueue");

    let store = ctx.job_store().expect("unified job store must be attached");
    let job = store
        .get(axon_api::source::JobId(
            uuid::Uuid::parse_str(&outcome.result.job_id).unwrap(),
        ))
        .await
        .unwrap()
        .expect("job row must exist");
    assert_eq!(job.kind, axon_api::source::JobKind::Ingest);
}

/// Ingest now enqueues onto the unified `JobStore` and runs on the real
/// unified worker (see `IngestRunner` in `runtime/job_runners/
/// ingest_runner.rs`), while `job_service::job_status`/`list_jobs`/
/// `cancel_job`/etc. for `JobKind::Ingest` bridge onto the same store (see
/// `runtime/sqlite/ingest_bridge.rs`) so existing CLI/MCP/REST callers keep
/// working unchanged. Session-export scanning always reads real `~/...`
/// paths (`expand_home` in `axon-ingest`), and — importantly — passing
/// `sessions_claude`/`codex`/`gemini` all `false` does NOT mean "scan
/// nothing": `axon-ingest`'s `all_platforms = !claude && !codex && !gemini`
/// treats that as "no filter" and scans every platform's real home
/// directory, which is slow and non-deterministic in CI. `HOME` is
/// redirected to an empty tempdir for the duration of this test (guarded by
/// `#[serial_test::serial]` + a scope guard restoring the original value on
/// drop, matching `crates/axon-core/src/paths_tests.rs`'s established
/// pattern) so the scan is real but instant and filesystem-light — this
/// still exercises the full unified enqueue → claim → run → terminal-status
/// path.
#[allow(unsafe_code)]
#[serial_test::serial]
#[tokio::test]
async fn ingest_job_runs_end_to_end_and_is_claimed_promptly() {
    struct HomeGuard(Option<String>);
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => unsafe { std::env::set_var("HOME", v) },
                None => unsafe { std::env::remove_var("HOME") },
            }
        }
    }
    let empty_home = tempfile::tempdir().expect("tempdir");
    let saved_home = std::env::var("HOME").ok();
    unsafe { std::env::set_var("HOME", empty_home.path()) };
    let _home_guard = HomeGuard(saved_home);

    let ctx = test_ctx_with_workers().await;
    let cfg = ctx.cfg().clone();
    let source = IngestSource::Sessions {
        sessions_claude: false,
        sessions_codex: false,
        sessions_gemini: false,
        sessions_project: None,
    };
    let started = std::time::Instant::now();
    let outcome = ingest_start_with_context(&cfg, source, &ctx, None)
        .await
        .expect("enqueue");
    let job_id = uuid::Uuid::parse_str(&outcome.result.job_id).expect("job id");

    let mut status = None;
    for _ in 0..100 {
        let job = crate::jobs::job_status(&ctx, LegacyJobKind::Ingest, job_id)
            .await
            .expect("job_status")
            .expect("job exists");
        if job.status != "pending" && job.status != "running" {
            status = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let job = status.expect("ingest job should reach a terminal status within timeout");
    let unsupported_stage = job
        .error_text
        .as_deref()
        .is_some_and(|text| text.contains("not wired yet"));
    assert!(
        !unsupported_stage,
        "ingest must dispatch to the real runner, not the catch-all: {:?}",
        job.error_text
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "ingest job took longer than a poll-interval-free path should — notify_unified() regression?"
    );

    let jobs = crate::jobs::list_jobs(&ctx, LegacyJobKind::Ingest, 10, 0)
        .await
        .expect("list_jobs");
    assert!(jobs.iter().any(|j| j.id == job_id));
}

#[tokio::test]
async fn prepared_sessions_start_enqueues_ingest_job_with_sidecar_payload() {
    let cfg = Config::test_default();
    let runtime = Arc::new(CaptureRuntime {
        payloads: Mutex::new(Vec::new()),
        sidecars: Mutex::new(Vec::new()),
    });
    let service_context = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());
    let request = ingest::sessions::IngestSessionsPreparedRequest {
        docs: vec![ingest::sessions::PreparedSessionDoc {
            url: "file:///tmp/session.jsonl".to_string(),
            title: None,
            text: "### USER:\nhello".to_string(),
            session_platform: "codex".to_string(),
            session_project: Some("axon_rust".to_string()),
            session_date: None,
            session_turn_count: Some(1),
            session_file: "/tmp/session.jsonl".to_string(),
            extra: serde_json::json!({}),
        }],
        project: Some("axon_rust".to_string()),
        collection: Some("axon_sessions".to_string()),
    };

    let outcome = ingest_sessions_prepared_start_with_context(&cfg, request, &service_context)
        .await
        .expect("enqueue prepared sessions");

    assert_eq!(outcome.disposition, StartDisposition::Enqueued);
    let payloads = runtime.payloads.lock().expect("lock");
    let sidecars = runtime.sidecars.lock().expect("lock");
    assert_eq!(payloads.len(), 1);
    assert_eq!(sidecars.len(), 1);

    let JobPayload::Ingest {
        target,
        source_type,
        config_json,
    } = &payloads[0]
    else {
        panic!("expected ingest payload");
    };
    assert_eq!(target, "prepared_sessions");
    assert_eq!(source_type, "prepared_sessions");
    let (decoded, _) = decode_ingest_job_config(&cfg, config_json).expect("decode config");
    assert!(matches!(decoded, IngestSource::PreparedSessions {}));
    assert!(matches!(
        &sidecars[0],
        JobSidecarPayload::IngestPreparedSessions { payload_json }
            if payload_json.contains("session.jsonl")
    ));
}
