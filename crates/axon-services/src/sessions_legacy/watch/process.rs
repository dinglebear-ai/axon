use super::SessionWatchOptions;
use super::queue::{PendingFiles, PendingState};
use super::targets::provider_allowed;
use super::validate::{SessionWatchRoots, ValidatedSessionPath, validate_session_file_path};
use crate::sessions_legacy::checkpoint::{
    SessionFileMetadata, checkpoint_record_matches, checkpoint_record_metadata_matches,
    checkpoint_records_by_path_hash, record_error, record_no_content, record_remote_accepted,
    record_success, refresh_checkpoint_metadata, stream_content_hash,
};
use anyhow::Result;
use async_trait::async_trait;
use axon_core::config::Config;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

mod upload;

use upload::{
    PreparedWatchDoc, TARGET_UPLOAD_BODY_BYTES, remote_upload_body_len,
    upload_prepared_sessions_to_server, watch_upload_chunks,
};
#[cfg(test)]
pub use upload::{redact_remote_prepared_request, upload_prepared_sessions_to_server_with_auth};

#[derive(Debug, Clone, Copy)]
pub(crate) struct WatchOutputMode {
    pub(crate) json: bool,
    pub(crate) verbose_paths: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessOutcome {
    Ingested { chunks_or_job: String },
    RemoteAccepted { job: String },
    SkippedUnchanged,
    SkippedFiltered,
    NoContent,
    RetryableFailure(String),
    TerminalFailure(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWatchProcessEvent {
    pub stage: String,
    pub provider: String,
    pub path_hash: String,
    pub basename: String,
    pub path: String,
    pub detail: Option<String>,
}

pub trait SessionWatchEventSink: Send + Sync {
    fn emit(&self, event: SessionWatchProcessEvent);
}

pub struct NoopSessionWatchEventSink;

impl SessionWatchEventSink for NoopSessionWatchEventSink {
    fn emit(&self, _event: SessionWatchProcessEvent) {}
}

#[async_trait]
pub trait SessionWatchIngestor: Send + Sync {
    async fn ingest_prepared_request_for_watch(
        &self,
        cfg: &Config,
        request: crate::sessions_legacy::IngestSessionsPreparedRequest,
    ) -> Result<WatchIngestResult>;
}

pub(crate) fn validate_event_path(
    roots: &SessionWatchRoots,
    path: &Path,
) -> Option<ValidatedSessionPath> {
    validate_session_file_path(roots, path)
        .map_err(|err| tracing::debug!(error = %err, "ignored unsupported session watch path"))
        .ok()
}

pub async fn process_session_batch_for_watch(
    cfg: &Config,
    ingestor: &dyn SessionWatchIngestor,
    pool: &sqlx::SqlitePool,
    paths: Vec<ValidatedSessionPath>,
    options: &SessionWatchOptions,
    events: &dyn SessionWatchEventSink,
) -> Result<Vec<ProcessOutcome>> {
    let output = WatchOutputMode {
        json: options.json,
        verbose_paths: options.verbose_paths,
    };
    let mut outcomes = vec![None; paths.len()];
    let mut candidates = Vec::new();
    let mut prepared_meta = Vec::new();
    let mut metadata = Vec::new();

    for (index, validated) in paths.into_iter().enumerate() {
        let meta = SessionFileMetadata::from_validated_path(&validated)?;
        metadata.push((index, validated, meta));
    }

    let path_hashes = metadata
        .iter()
        .map(|(_, _, meta)| meta.path_hash.clone())
        .collect::<Vec<_>>();
    let checkpoint_records = checkpoint_records_by_path_hash(pool, &path_hashes).await?;

    for (index, validated, meta) in metadata {
        let record = checkpoint_records.get(&meta.path_hash);
        let current_hash = if record.is_some_and(|record| record.content_hash.is_some()) {
            Some(stream_content_hash(&validated.canonical).await?)
        } else {
            None
        };
        if record
            .is_some_and(|record| checkpoint_record_matches(&meta, record, current_hash.as_deref()))
        {
            let record = record.expect("record checked above");
            if !checkpoint_record_metadata_matches(&meta, record) {
                refresh_checkpoint_metadata(pool, &meta).await?;
            }
            emit_watch_event(events, output, "skipped_unchanged", &validated, None);
            outcomes[index] = Some(ProcessOutcome::SkippedUnchanged);
            continue;
        }
        candidates.push((index, validated, meta, current_hash));
    }

    for parsed in prepare_session_docs_for_watch(cfg, candidates, options).await {
        let (index, validated, meta, known_hash, parsed) = parsed;
        match parsed {
            Ok(Some(session_doc)) => {
                let doc =
                    crate::sessions_legacy::prepared_session_doc_from_session_doc(session_doc)
                        .map_err(anyhow::Error::msg)?;
                let content_hash = match known_hash {
                    Some(hash) => Some(hash),
                    None => stream_content_hash(&validated.canonical).await.ok(),
                };
                prepared_meta.push((index, validated, meta, doc, content_hash));
            }
            Ok(None) => {
                if cfg.sessions_project.is_some() {
                    emit_watch_event(events, output, "skipped_filtered", &validated, None);
                    outcomes[index] = Some(ProcessOutcome::SkippedFiltered);
                } else {
                    record_no_content(pool, &meta).await?;
                    emit_watch_event(events, output, "no_content", &validated, None);
                    outcomes[index] = Some(ProcessOutcome::NoContent);
                }
            }
            Err(error) => {
                let (code, redacted) = classify_and_redact_watch_error(&error);
                record_error(pool, &meta, &code, &redacted).await?;
                emit_watch_event(events, output, "error", &validated, Some(&code));
                outcomes[index] = Some(ProcessOutcome::RetryableFailure(code));
            }
        }
    }

    let ctx = ProcessBatchContext {
        cfg,
        ingestor,
        pool,
        options,
        events,
        output,
    };
    process_prepared_meta(&ctx, prepared_meta, &mut outcomes).await?;

    Ok(outcomes.into_iter().flatten().collect())
}

struct ProcessBatchContext<'a> {
    cfg: &'a Config,
    ingestor: &'a dyn SessionWatchIngestor,
    pool: &'a sqlx::SqlitePool,
    options: &'a SessionWatchOptions,
    events: &'a dyn SessionWatchEventSink,
    output: WatchOutputMode,
}

async fn process_prepared_meta(
    ctx: &ProcessBatchContext<'_>,
    prepared_meta: Vec<PreparedWatchDoc>,
    outcomes: &mut [Option<ProcessOutcome>],
) -> Result<()> {
    let collection = (ctx.cfg.collection != "axon").then(|| ctx.cfg.collection.clone());
    let upload_candidates =
        upload_candidates_below_size_limit(ctx, prepared_meta, collection.as_ref(), outcomes)
            .await?;
    let meta_chunks = watch_upload_chunks(
        &upload_candidates,
        ctx.options,
        ctx.cfg.sessions_project.as_ref(),
        collection.as_ref(),
    )?;
    for meta_chunk in meta_chunks {
        let request = crate::sessions_legacy::IngestSessionsPreparedRequest {
            docs: meta_chunk
                .iter()
                .map(|(_, _, _, doc, _)| (*doc).clone())
                .collect(),
            project: ctx.cfg.sessions_project.clone(),
            collection: collection.clone(),
        };
        match ingest_prepared_request_for_watch(ctx.cfg, ctx.ingestor, request, ctx.options).await {
            Ok(WatchIngestResult::Completed(label)) => {
                for (index, validated, meta, _, content_hash) in meta_chunk {
                    record_success(ctx.pool, meta, content_hash.as_deref()).await?;
                    emit_watch_event(ctx.events, ctx.output, "ingested", validated, Some(&label));
                    outcomes[*index] = Some(ProcessOutcome::Ingested {
                        chunks_or_job: label.clone(),
                    });
                }
            }
            Ok(WatchIngestResult::RemoteAccepted(label)) => {
                for (index, validated, meta, _, content_hash) in meta_chunk {
                    record_remote_accepted(ctx.pool, meta, content_hash.as_deref(), &label).await?;
                    emit_watch_event(
                        ctx.events,
                        ctx.output,
                        "accepted_remote",
                        validated,
                        Some(&label),
                    );
                    outcomes[*index] = Some(ProcessOutcome::RemoteAccepted { job: label.clone() });
                }
            }
            Err(error) => {
                let (code, redacted) = classify_and_redact_watch_error(&error.to_string());
                for (index, validated, meta, _, _) in meta_chunk {
                    record_error(ctx.pool, meta, &code, &redacted).await?;
                    emit_watch_event(ctx.events, ctx.output, "error", validated, Some(&code));
                    outcomes[*index] = Some(ProcessOutcome::RetryableFailure(code.clone()));
                }
            }
        }
    }
    Ok(())
}

async fn upload_candidates_below_size_limit(
    ctx: &ProcessBatchContext<'_>,
    prepared_meta: Vec<PreparedWatchDoc>,
    collection: Option<&String>,
    outcomes: &mut [Option<ProcessOutcome>],
) -> Result<Vec<PreparedWatchDoc>> {
    let mut upload_candidates = Vec::new();
    for item in prepared_meta {
        if ctx.options.upload_to_server {
            let single = [&item];
            let single_len =
                remote_upload_body_len(&single, ctx.cfg.sessions_project.as_ref(), collection)?;
            if single_len > TARGET_UPLOAD_BODY_BYTES {
                record_oversized_upload_doc(ctx, item, single_len, outcomes).await?;
                continue;
            }
        }
        upload_candidates.push(item);
    }
    Ok(upload_candidates)
}

async fn record_oversized_upload_doc(
    ctx: &ProcessBatchContext<'_>,
    item: PreparedWatchDoc,
    single_len: usize,
    outcomes: &mut [Option<ProcessOutcome>],
) -> Result<()> {
    let (index, validated, meta, _, _) = item;
    let code = "upload_too_large".to_string();
    let redacted = redact_error_detail(&format!(
        "single prepared session upload document exceeds size limit: {} bytes > {} bytes ({})",
        single_len, TARGET_UPLOAD_BODY_BYTES, validated.redacted_display
    ));
    record_error(ctx.pool, &meta, &code, &redacted).await?;
    emit_watch_event(ctx.events, ctx.output, "error", &validated, Some(&code));
    outcomes[index] = Some(ProcessOutcome::TerminalFailure(code));
    Ok(())
}

type ParseCandidate = (
    usize,
    ValidatedSessionPath,
    SessionFileMetadata,
    Option<String>,
);
type ParseResult = (
    usize,
    ValidatedSessionPath,
    SessionFileMetadata,
    Option<String>,
    Result<Option<crate::sessions_legacy::SessionDoc>, String>,
);

async fn prepare_session_docs_for_watch(
    cfg: &Config,
    candidates: Vec<ParseCandidate>,
    options: &SessionWatchOptions,
) -> Vec<ParseResult> {
    let permits = effective_processing_concurrency(options);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(permits));
    let mut tasks = FuturesUnordered::new();

    for (index, validated, meta, known_hash) in candidates {
        let cfg = cfg.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.push(async move {
            let _permit = match semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    return (index, validated, meta, known_hash, Err(error.to_string()));
                }
            };
            let parsed = crate::sessions_legacy::collect_session_file_doc(&cfg, &validated)
                .await
                .map_err(|error| error.to_string());
            (index, validated, meta, known_hash, parsed)
        });
    }

    let mut parsed = Vec::new();
    while let Some(result) = tasks.next().await {
        parsed.push(result);
    }
    parsed.sort_by_key(|(index, _, _, _, _)| *index);
    parsed
}

pub fn effective_processing_concurrency(options: &SessionWatchOptions) -> usize {
    options.max_processing_concurrency.max(1)
}

pub enum WatchIngestResult {
    Completed(String),
    RemoteAccepted(String),
}

async fn ingest_prepared_request_for_watch(
    cfg: &Config,
    ingestor: &dyn SessionWatchIngestor,
    request: crate::sessions_legacy::IngestSessionsPreparedRequest,
    options: &SessionWatchOptions,
) -> Result<WatchIngestResult> {
    if options.upload_to_server {
        return upload_prepared_sessions_to_server(request, options)
            .await
            .map(WatchIngestResult::RemoteAccepted);
    }

    ingestor
        .ingest_prepared_request_for_watch(cfg, request)
        .await
}

pub async fn process_pending(
    cfg: &Config,
    ingestor: &dyn SessionWatchIngestor,
    pool: &sqlx::SqlitePool,
    roots: &SessionWatchRoots,
    options: &SessionWatchOptions,
    pending: &mut PendingFiles,
    events: &dyn SessionWatchEventSink,
) {
    let now = Instant::now();
    let mut stable = Vec::new();
    let mut stable_paths = Vec::new();

    let max_batch_docs = options.max_batch_docs.max(1);
    for path in pending.debounced_paths(now, options.debounce) {
        if stable.len() >= max_batch_docs {
            break;
        }
        match pending.stable(&path, now, options.settle) {
            Ok(PendingState::Stable) => {
                if let Some(validated) = validate_event_path(roots, &path)
                    && provider_allowed(cfg, validated.provider)
                {
                    stable.push(validated);
                    stable_paths.push(path);
                } else {
                    pending.remove(&path);
                }
            }
            Ok(PendingState::NotReady) => {}
            Ok(PendingState::Terminal) => pending.remove(&path),
            Err(error) => {
                let detail = redact_error_detail(&error.to_string());
                tracing::warn!(detail = %detail, "session watch stability check failed");
                if !pending.requeue(path.clone(), Instant::now(), options.max_retries) {
                    pending.remove(&path);
                }
            }
        }
    }

    if stable.is_empty() {
        return;
    }

    match process_session_batch_for_watch(cfg, ingestor, pool, stable, options, events).await {
        Ok(outcomes) => {
            for (path, outcome) in stable_paths.into_iter().zip(outcomes) {
                match outcome {
                    ProcessOutcome::RetryableFailure(detail) => {
                        if !pending.requeue(path.clone(), Instant::now(), options.max_retries) {
                            tracing::warn!(
                                detail = %redact_error_detail(&detail),
                                "session watch retry cap reached"
                            );
                            pending.remove(&path);
                        }
                    }
                    _ => pending.remove(&path),
                }
            }
        }
        Err(error) => {
            let detail = redact_error_detail(&error.to_string());
            for path in stable_paths {
                if !pending.requeue(path.clone(), Instant::now(), options.max_retries) {
                    tracing::warn!(
                        detail = %detail,
                        "session watch batch processing failed at retry cap"
                    );
                    pending.remove(&path);
                }
            }
        }
    }
}

fn classify_and_redact_watch_error(error: &str) -> (String, String) {
    let detail = redact_error_detail(error);
    let lower = detail.to_ascii_lowercase();
    let code = if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("http") || lower.contains("status=") {
        "upload_failed"
    } else if lower.contains("parse") || lower.contains("json") {
        "parse_failed"
    } else {
        "watch_failed"
    };
    (code.to_string(), detail)
}

pub fn redact_error_detail(raw: &str) -> String {
    let mut redacted = redact_local_paths(raw)
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("bearer")
                || lower.starts_with("sk-")
                || lower.starts_with("ghp_")
                || lower.contains("token")
                || lower.contains("api_key")
            {
                "[REDACTED]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    const MAX_ERROR_DETAIL_CHARS: usize = 300;
    if redacted.len() > MAX_ERROR_DETAIL_CHARS {
        redacted.truncate(MAX_ERROR_DETAIL_CHARS);
        redacted.push_str("...");
    }
    redacted
}

fn redact_local_paths(raw: &str) -> String {
    let mut redacted = raw.to_string();
    if let Some(home) = std::env::var_os("HOME").and_then(|value| value.into_string().ok())
        && home.len() > 1
    {
        redacted = redacted.replace(&home, "[REDACTED-HOME]");
    }
    for root in [
        crate::sessions_legacy::expand_home("~/.claude/projects"),
        crate::sessions_legacy::expand_home("~/.codex/sessions"),
        crate::sessions_legacy::expand_home("~/.gemini/history"),
        crate::sessions_legacy::expand_home("~/.gemini/tmp"),
    ] {
        let root = root.display().to_string();
        if root.len() > 1 {
            redacted = redacted.replace(&root, "[REDACTED-SESSION-ROOT]");
        }
    }
    redacted
}

fn emit_watch_event(
    events: &dyn SessionWatchEventSink,
    output: WatchOutputMode,
    stage: &str,
    path: &ValidatedSessionPath,
    detail: Option<&str>,
) {
    if output.json {
        let path_value = if output.verbose_paths {
            path.canonical.display().to_string()
        } else {
            path.redacted_display.clone()
        };
        events.emit(SessionWatchProcessEvent {
            stage: stage.to_string(),
            provider: path.provider.as_str().to_string(),
            path_hash: path.path_hash.clone(),
            basename: path.basename.clone(),
            path: path_value,
            detail: detail.map(str::to_string),
        });
    }
}
