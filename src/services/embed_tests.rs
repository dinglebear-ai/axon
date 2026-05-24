use super::*;
use crate::core::config::Config;
use crate::jobs::backend::BackendResult;
use crate::services::context::ServiceContext;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

struct CaptureRuntime {
    payloads: Mutex<Vec<JobPayload>>,
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
        panic!("--wait false embed start must enqueue without waiting")
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        panic!("--wait false embed start must not drain the queue")
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

#[tokio::test]
async fn embed_start_with_context_enqueues_without_blocking_when_wait_false()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let mut cfg = Config::test_default();
    cfg.wait = false;
    let runtime = Arc::new(CaptureRuntime {
        payloads: Mutex::new(Vec::new()),
    });
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

    let outcome = embed_start_with_context(&cfg, "./README.md", &ctx, None, None)
        .await
        .map_err(|e| e.to_string())?;

    assert_eq!(outcome.disposition, StartDisposition::Enqueued);
    assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
    assert_eq!(runtime.payloads.lock().expect("lock").len(), 1);
    Ok(())
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
