use super::*;
use crate::core::config::Config;
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::jobs::config_snapshot::decode_ingest_job_config;
use crate::mcp::schema::{IngestRequest, IngestSourceType};
use crate::services::context::ServiceContext;
use crate::services::runtime::ServiceJobRuntime;
use crate::services::types::{ExecutionMode, StartDisposition};
use async_trait::async_trait;
use std::error::Error;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct CaptureRuntime {
    payloads: Mutex<Vec<JobPayload>>,
}

fn ingest_req(source_type: IngestSourceType, target: &str) -> IngestRequest {
    IngestRequest {
        source_type: Some(source_type),
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

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        panic!("--wait false ingest start must enqueue without waiting")
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        panic!("--wait false ingest start must not drain the queue")
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
fn source_from_mcp_request_requires_source_type() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        target: Some("owner/repo".to_string()),
        ..Default::default()
    };

    let err = source_from_mcp_request(&req, &cfg).expect_err("missing source type");

    assert!(err.contains("source_type is required"));
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
fn source_from_mcp_request_maps_default_sessions_options() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Sessions),
        ..Default::default()
    };

    let source = source_from_mcp_request(&req, &cfg).expect("default sessions options");

    assert!(matches!(
        source,
        IngestSource::Sessions {
            sessions_claude: false,
            sessions_codex: false,
            sessions_gemini: false,
            sessions_project: None,
        }
    ));
}

#[tokio::test]
async fn ingest_start_with_context_enqueues_sessions_jobs_with_sqlite_backend() {
    let mut cfg = Config::test_default();
    cfg.sessions_claude = true;
    cfg.sessions_codex = false;
    cfg.sessions_gemini = true;
    cfg.sessions_project = Some("axon-rust".to_string());

    let runtime = Arc::new(CaptureRuntime {
        payloads: Mutex::new(Vec::new()),
    });
    let service_context = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());
    let source = IngestSource::Sessions {
        sessions_claude: true,
        sessions_codex: false,
        sessions_gemini: true,
        sessions_project: Some("axon-rust".to_string()),
    };

    let outcome = ingest_start_with_context(&cfg, source.clone(), &service_context)
        .await
        .expect("enqueue sessions");

    assert_eq!(outcome.disposition, StartDisposition::Enqueued);
    assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);

    let payloads = runtime.payloads.lock().expect("lock");
    assert_eq!(payloads.len(), 1);
    let JobPayload::Ingest {
        target,
        source_type,
        config_json,
    } = &payloads[0]
    else {
        panic!("expected ingest payload");
    };

    assert_eq!(source_type, "sessions");
    assert_eq!(target, "claude,gemini:axon-rust");
    let (decoded, effective_cfg) =
        decode_ingest_job_config(&cfg, config_json).expect("decode source config");
    assert!(matches!(
        decoded,
        IngestSource::Sessions {
            sessions_claude: true,
            sessions_codex: false,
            sessions_gemini: true,
            sessions_project: Some(ref project),
        } if project == "axon-rust"
    ));
    assert_eq!(effective_cfg.collection, cfg.collection);
    assert!(effective_cfg.sessions_claude);
    assert!(!effective_cfg.sessions_codex);
    assert!(effective_cfg.sessions_gemini);
    assert_eq!(effective_cfg.sessions_project.as_deref(), Some("axon-rust"));
}
