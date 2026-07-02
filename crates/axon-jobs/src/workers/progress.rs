use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::backend::JobKind;
use crate::ops::update_progress_json_for_attempt;
use axon_api::source::{
    ApiError, ErrorStage, JobId, LifecycleStatus, PipelinePhase, ProgressCurrent, Severity,
    SourceItemKey, SourceProgressEvent, StageCounts, Timestamp, Visibility,
};
use axon_crawl::engine::CrawlSummary;
use axon_vector::ops::tei::EmbedProgress;

pub(super) fn spawn_crawl_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    attempt_id: Option<String>,
    output_dir: std::path::PathBuf,
) -> (mpsc::Sender<CrawlSummary>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<CrawlSummary>(32);
    let task = tokio::spawn(async move {
        while let Some(summary) = rx.recv().await {
            let mut progress = serde_json::json!({
                "phase": "crawling",
                "lifecycle_progress": active_ratio(summary.pages_seen as f64, summary.pages_discovered as f64),
                "output_dir": output_dir,
                "output_path": output_dir.join("markdown"),
                "pages_crawled": summary.pages_seen,
                "pages_discovered": summary.pages_discovered,
                "queued": summary.queued(),
                "depth_max": summary.depth_max,
                "md_created": summary.markdown_files,
                "thin_md": summary.thin_pages,
                "error_pages": summary.error_pages,
                "waf_blocked_pages": summary.waf_blocked_pages,
                "reused_pages": summary.reused_pages,
                "diagnostic_count": summary.diagnostics.len(),
                "events": summary.recent_events,
                "rate_limited": summary.rate_limited,
            });
            if let (Some(adaptive), Some(obj)) =
                (summary.adaptive.as_ref(), progress.as_object_mut())
            {
                obj.insert(
                    "adaptive_concurrency".to_string(),
                    serde_json::to_value(adaptive).unwrap_or(serde_json::Value::Null),
                );
            }
            if let Err(e) = update_progress_json_for_attempt(
                &pool,
                JobKind::Crawl,
                id,
                attempt_id.as_deref(),
                &progress,
            )
            .await
            {
                tracing::warn!(job_id = %id, error = %e, "failed to persist crawl progress");
            }
        }
    });
    (tx, task)
}

pub(super) fn spawn_embed_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    attempt_id: Option<String>,
) -> (mpsc::Sender<EmbedProgress>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<EmbedProgress>(32);
    let task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let json = serde_json::json!({
                "phase": "embedding",
                "lifecycle_progress": active_ratio(progress.docs_completed as f64, progress.docs_total as f64),
                "docs_total": progress.docs_total,
                "docs_embedded": progress.docs_completed,
                "chunks_embedded": progress.chunks_embedded,
            });
            if let Err(e) = update_progress_json_for_attempt(
                &pool,
                JobKind::Embed,
                id,
                attempt_id.as_deref(),
                &json,
            )
            .await
            {
                tracing::warn!(job_id = %id, error = %e, "failed to persist embed progress");
            }
        }
    });
    (tx, task)
}

fn active_ratio(done: f64, total: f64) -> f64 {
    if total <= 0.0 {
        return 0.02;
    }
    if done <= 0.0 {
        return 0.0;
    }
    ((done / total).clamp(0.02, 0.98) * 100.0).round() / 100.0
}

#[allow(dead_code)]
pub(crate) fn legacy_progress_event(
    id: uuid::Uuid,
    kind: JobKind,
    progress: &serde_json::Value,
    sequence: u64,
) -> SourceProgressEvent {
    let phase = progress
        .get("phase")
        .and_then(serde_json::Value::as_str)
        .map(legacy_phase)
        .unwrap_or_else(|| legacy_kind_phase(kind));
    let status = progress
        .get("status")
        .and_then(serde_json::Value::as_str)
        .map(legacy_status)
        .unwrap_or(LifecycleStatus::Running);
    SourceProgressEvent {
        event_id: format!("legacy-{id}-{sequence}"),
        sequence,
        job_id: JobId::new(id),
        attempt: progress_u32(progress, "attempt").unwrap_or(0),
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase,
        status,
        severity: if progress.get("error").is_some() {
            Severity::Failed
        } else if progress.get("warning").is_some() {
            Severity::Warning
        } else {
            Severity::Info
        },
        visibility: Visibility::Internal,
        message: progress
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| kind.table_name())
            .to_string(),
        timestamp: Timestamp::from(chrono::Utc::now()),
        source_id: None,
        canonical_uri: progress
            .get("canonical_uri")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        adapter: None,
        scope: None,
        generation: None,
        counts: legacy_counts(progress),
        timing: None,
        current: legacy_current(progress, kind),
        throughput: None,
        retry: None,
        warning: progress
            .get("warning")
            .and_then(serde_json::Value::as_str)
            .map(|message| axon_api::source::SourceWarning {
                code: "legacy_worker.warning".to_string(),
                severity: Severity::Warning,
                message: message.to_string(),
                source_item_key: None,
                retryable: true,
            }),
        error: progress
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map(|message| {
                ApiError::new(
                    "legacy_worker.error",
                    ErrorStage::Observing,
                    message.to_string(),
                )
            }),
    }
}

fn legacy_kind_phase(kind: JobKind) -> PipelinePhase {
    match kind {
        JobKind::Crawl => PipelinePhase::Fetching,
        JobKind::Embed => PipelinePhase::Embedding,
        JobKind::Extract => PipelinePhase::Synthesizing,
        JobKind::Ingest => PipelinePhase::Fetching,
    }
}

fn legacy_phase(phase: &str) -> PipelinePhase {
    match phase {
        "crawling" | "fetching" => PipelinePhase::Fetching,
        "embedding" => PipelinePhase::Embedding,
        "extracting" => PipelinePhase::Synthesizing,
        "ingesting" => PipelinePhase::Fetching,
        "complete" => PipelinePhase::Complete,
        _ => PipelinePhase::Publishing,
    }
}

fn legacy_status(status: &str) -> LifecycleStatus {
    match status {
        "completed" | "complete" => LifecycleStatus::Completed,
        "failed" => LifecycleStatus::Failed,
        "canceled" => LifecycleStatus::Canceled,
        "waiting" => LifecycleStatus::Waiting,
        _ => LifecycleStatus::Running,
    }
}

fn legacy_counts(progress: &serde_json::Value) -> StageCounts {
    StageCounts {
        items_total: progress_u64(progress, "pages_discovered")
            .or_else(|| progress_u64(progress, "items_total")),
        items_done: progress_u64(progress, "pages_crawled")
            .or_else(|| progress_u64(progress, "items_done"))
            .unwrap_or(0),
        documents_total: progress_u64(progress, "docs_total"),
        documents_done: progress_u64(progress, "docs_embedded")
            .or_else(|| progress_u64(progress, "documents_done"))
            .unwrap_or(0),
        chunks_total: progress_u64(progress, "chunks_total"),
        chunks_done: progress_u64(progress, "chunks_embedded")
            .or_else(|| progress_u64(progress, "chunks_done"))
            .unwrap_or(0),
        bytes_total: progress_u64(progress, "bytes_total"),
        bytes_done: progress_u64(progress, "bytes_done").unwrap_or(0),
    }
}

fn legacy_current(progress: &serde_json::Value, kind: JobKind) -> Option<ProgressCurrent> {
    let source_item_key = progress
        .get("current_item")
        .or_else(|| progress.get("current_path"))
        .and_then(serde_json::Value::as_str)
        .map(SourceItemKey::new);
    let message = progress
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    (source_item_key.is_some() || message.is_some()).then(|| ProgressCurrent {
        source_item_key,
        document_id: None,
        chunk_id: None,
        adapter: Some(kind.table_name().to_string()),
        provider: None,
        message,
    })
}

fn progress_u64(progress: &serde_json::Value, key: &str) -> Option<u64> {
    progress.get(key).and_then(serde_json::Value::as_u64)
}

fn progress_u32(progress: &serde_json::Value, key: &str) -> Option<u32> {
    progress_u64(progress, key).and_then(|value| u32::try_from(value).ok())
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
