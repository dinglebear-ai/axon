use super::*;
use crate::context::ServiceContext;
use crate::runtime::ServiceJobRuntime;
use crate::types::{ExecutionMode, StartDisposition};
use async_trait::async_trait;
use axon_api::mcp_schema::{IngestRequest, IngestSourceType};
use axon_core::config::Config;
use axon_ingest as ingest;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload, JobSidecarPayload};
use axon_jobs::config_snapshot::decode_ingest_job_config;
use std::error::Error;
use std::sync::{Arc, Mutex};
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
    ) -> Result<Vec<crate::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<crate::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
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
fn source_from_mcp_request_rejects_remote_sessions_scan() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Sessions),
        ..Default::default()
    };

    let err = source_from_mcp_request(&req, &cfg).expect_err("remote sessions rejected");

    assert!(err.contains("/v1/ingest/sessions/prepared"));
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
        sidecars: Mutex::new(Vec::new()),
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
