use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::ingest;
use crate::crates::jobs::common::{JobTable, mark_job_failed};
use futures::Future;
use futures_util::stream::{FuturesUnordered, StreamExt};
use sqlx::PgPool;
use std::collections::HashSet;
use std::time::Duration;
use tracing::Instrument as _;
use uuid::Uuid;

use super::ops::mark_completed;
use super::types::{IngestJobConfig, IngestSource};

type IngestFuture = std::pin::Pin<Box<dyn Future<Output = (String, Result<usize, String>)> + Send>>;

const TABLE: JobTable = JobTable::Ingest;
const PLAYLIST_CONCURRENCY: usize = 5;
const RETRY_429_BASE_SECS: u64 = 10;
const RETRY_429_MAX_ATTEMPTS: u8 = 3;

/// Load prior playlist progress from `result_json` to support resume on restart.
///
/// Returns `(chunks_embedded, completed_urls)`. If no prior progress exists,
/// returns `(0, HashSet::new())`.
async fn load_playlist_progress_with_pool(pool: &PgPool, job_id: Uuid) -> (usize, HashSet<String>) {
    let row = sqlx::query_scalar::<_, Option<serde_json::Value>>(
        "SELECT result_json FROM axon_ingest_jobs WHERE id=$1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten();

    let Some(v) = row else {
        return (0, HashSet::new());
    };

    let chunks = v
        .get("chunks_embedded")
        .and_then(|c| c.as_u64())
        .unwrap_or(0) as usize;
    let urls = v
        .get("completed_urls")
        .and_then(|u| u.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (chunks, urls)
}

/// Persist ingest progress to the DB. Used by both YouTube playlist and GitHub progress tracking.
/// Logs a warning on failure so errors are observable.
///
/// Safety: each ingest job is processed by a single worker task, so concurrent writes to the
/// same `job_id` do not happen. The JSONB `||` merge is therefore safe from lost-update races.
/// YouTube playlist progress funnels through `drain_playlist_videos_with_pool` (sequential after
/// each `FuturesUnordered::next`). GitHub file progress funnels through a single `mpsc` receiver
/// task (see `process_ingest_job` GitHub branch).
async fn update_ingest_progress(pool: &PgPool, job_id: Uuid, progress: &serde_json::Value) {
    if let Err(e) = sqlx::query(
        "UPDATE axon_ingest_jobs \
         SET result_json=COALESCE(result_json,'{}'::jsonb)||$1, updated_at=NOW() \
         WHERE id=$2",
    )
    .bind(progress)
    .bind(job_id)
    .execute(pool)
    .await
    {
        log_warn(&format!(
            "command=ingest progress_update_failed job_id={job_id} err={e}"
        ));
    }
}

/// Ingest a single video with up to `RETRY_429_MAX_ATTEMPTS` retries on 429 errors.
///
/// Non-429 errors are returned immediately. Backoff: 10s, 20s, 40s.
async fn ingest_video_with_retry(cfg: &Config, video_url: &str) -> Result<usize, String> {
    for attempt in 0..=RETRY_429_MAX_ATTEMPTS {
        // Convert Box<dyn Error> to String immediately so the state machine never
        // holds a non-Send type across an await point.
        let result = ingest::youtube::ingest_youtube(cfg, video_url)
            .await
            .map_err(|e| e.to_string());
        match result {
            Ok(n) => return Ok(n),
            Err(msg) => {
                if (msg.contains("429") || msg.contains("Too Many Requests"))
                    && attempt < RETRY_429_MAX_ATTEMPTS
                {
                    let delay = RETRY_429_BASE_SECS * (1 << attempt);
                    log_warn(&format!(
                        "command=ingest_youtube_playlist 429_retry attempt={} delay={delay}s url={video_url}",
                        attempt + 1
                    ));
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    continue;
                }
                return Err(msg);
            }
        }
    }
    // This point is unreachable: every loop iteration either returns Ok or Err,
    // and the final iteration (attempt == RETRY_429_MAX_ATTEMPTS) always hits the
    // non-retry branch above. The explicit return satisfies the type checker.
    Err("max retries exceeded".to_string())
}

/// Drive the concurrent video ingestion loop for a playlist or channel.
///
/// Drains `pending` videos using `FuturesUnordered` with up to `PLAYLIST_CONCURRENCY`
/// in-flight at once. Progress is persisted to the DB after each completion via
/// `update_ingest_progress`.
///
/// Returns the total number of chunks embedded across all processed videos.
async fn drain_playlist_videos_with_pool(
    cfg: &Config,
    pool: &PgPool,
    job_id: Uuid,
    pending: Vec<String>,
    mut chunks_embedded: usize,
    mut completed_urls: HashSet<String>,
    total: usize,
) -> usize {
    let mut inflight: FuturesUnordered<IngestFuture> = FuturesUnordered::new();
    let mut pending_iter = pending.into_iter();

    // Pre-fill up to PLAYLIST_CONCURRENCY
    for video_url in pending_iter.by_ref().take(PLAYLIST_CONCURRENCY) {
        let cfg_clone = cfg.clone();
        inflight.push(Box::pin(async move {
            let result = ingest_video_with_retry(&cfg_clone, &video_url).await;
            (video_url, result)
        }));
    }

    // Drain results, persist progress, and refill
    while let Some((video_url, result)) = inflight.next().await {
        match result {
            Ok(n) => {
                chunks_embedded += n;
                // Avoid cloning video_url: insert the owned copy and reference it in the log.
                let done = completed_urls.len() + 1;
                log_info(&format!(
                    "command=ingest_youtube_playlist done={done}/{total} chunks={n} total_chunks={chunks_embedded} url={video_url}",
                ));
                completed_urls.insert(video_url);
            }
            Err(e) => log_warn(&format!(
                "command=ingest_youtube_playlist skip video_url={video_url} err={e}"
            )),
        }

        // Persist progress so resume works on restart and status shows live data
        let progress = serde_json::json!({
            "videos_done": completed_urls.len(),
            "videos_total": total,
            "chunks_embedded": chunks_embedded,
            "completed_urls": completed_urls.iter().collect::<Vec<_>>(),
        });
        update_ingest_progress(pool, job_id, &progress).await;

        // Queue next pending video to maintain PLAYLIST_CONCURRENCY
        if let Some(next_url) = pending_iter.next() {
            let cfg_clone = cfg.clone();
            inflight.push(Box::pin(async move {
                let result = ingest_video_with_retry(&cfg_clone, &next_url).await;
                (next_url, result)
            }));
        }
    }

    chunks_embedded
}

/// Enumerate and ingest all videos in a YouTube playlist or channel.
///
/// Resumes from prior progress stored in `result_json` (supports restart after kill).
/// Processes up to `PLAYLIST_CONCURRENCY` videos concurrently.
/// Persists progress after each video so `axon ingest status` shows live progress.
async fn ingest_youtube_playlist_with_pool(
    cfg: &Config,
    pool: &PgPool,
    job_id: Uuid,
    url: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    // Resume: load prior progress (completed_urls enables skipping on restart)
    let (chunks_embedded, completed_urls) = load_playlist_progress_with_pool(pool, job_id).await;

    // Write enumerating placeholder so `axon status` shows activity while yt-dlp lists the channel.
    // Only written on a fresh start — resumed jobs already have result_json with video counts.
    if completed_urls.is_empty() {
        update_ingest_progress(pool, job_id, &serde_json::json!({"enumerating": true})).await;
    }

    // Enumerate all videos via single yt-dlp --flat-playlist call
    let video_urls = ingest::youtube::enumerate_playlist_videos(url).await?;
    if video_urls.is_empty() {
        return Err("yt-dlp found no videos in playlist/channel".into());
    }
    let total = video_urls.len();

    // Filter already-completed videos for resume support
    let pending: Vec<String> = video_urls
        .into_iter()
        .filter(|u| !completed_urls.contains(u))
        .collect();

    log_info(&format!(
        "command=ingest_youtube_playlist total={total} pending={} resumed={} url={url}",
        pending.len(),
        completed_urls.len()
    ));

    if pending.is_empty() {
        return Ok(chunks_embedded);
    }

    // Write initial progress immediately so `axon ingest list/status` shows totals
    // before the first video completes (restores behavior from the sequential implementation).
    let initial_progress = serde_json::json!({
        "videos_done": completed_urls.len(),
        "videos_total": total,
        "chunks_embedded": chunks_embedded,
        "completed_urls": completed_urls.iter().collect::<Vec<_>>(),
    });
    update_ingest_progress(pool, job_id, &initial_progress).await;

    let final_chunks = drain_playlist_videos_with_pool(
        cfg,
        pool,
        job_id,
        pending,
        chunks_embedded,
        completed_urls,
        total,
    )
    .await;

    Ok(final_chunks)
}

// SEC-M-6: `cfg` is captured by value but never serialized into error_text.
// All error paths pass only `e.to_string()` or static messages to `mark_job_failed`,
// so `openai_api_key` and other secrets in `cfg` cannot leak into the database.
pub(crate) async fn process_ingest_job(cfg: Config, pool: PgPool, id: Uuid) {
    let cfg_row = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT config_json FROM axon_ingest_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await;

    let job_cfg: IngestJobConfig = match cfg_row {
        Ok(Some(v)) => match serde_json::from_value(v) {
            Ok(c) => c,
            Err(e) => {
                if let Err(e2) =
                    mark_job_failed(&pool, TABLE, id, &format!("invalid config_json: {e}")).await
                {
                    log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
                }
                return;
            }
        },
        Ok(None) => {
            if let Err(e) = mark_job_failed(&pool, TABLE, id, "job not found in DB").await {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e}"));
            }
            return;
        }
        Err(e) => {
            if let Err(e2) = mark_job_failed(&pool, TABLE, id, &format!("DB read error: {e}")).await
            {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
            return;
        }
    };

    let source_label = match &job_cfg.source {
        IngestSource::Github { .. } => "github",
        IngestSource::Reddit { .. } => "reddit",
        IngestSource::Youtube { .. } => "youtube",
        IngestSource::Sessions { .. } => "sessions",
    };
    let target_label: String = match &job_cfg.source {
        IngestSource::Github { repo, .. } => repo.clone(),
        IngestSource::Reddit { target } | IngestSource::Youtube { target } => target.clone(),
        IngestSource::Sessions { .. } => "sessions".to_string(),
    };
    let _job_span = tracing::info_span!(
        "ingest_job",
        job_id = %id,
        source = source_label,
        target = %target_label,
    )
    .entered();

    let result = match &job_cfg.source {
        IngestSource::Github {
            repo,
            include_source,
        } => {
            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::channel::<serde_json::Value>(256);
            let progress_pool = pool.clone();
            let progress_id = id;
            let progress_task = tokio::spawn(
                async move {
                    while let Some(progress) = progress_rx.recv().await {
                        update_ingest_progress(&progress_pool, progress_id, &progress).await;
                    }
                }
                .instrument(tracing::Span::current()),
            );
            let r =
                ingest::github::ingest_github(&cfg, repo, *include_source, Some(progress_tx)).await;
            // Wait for final DB write to complete before marking done
            let _ = progress_task.await;
            // ingest_github returns anyhow::Result; map to Box<dyn Error> for unified match type.
            r.map_err(|e| -> Box<dyn std::error::Error> { e.into() })
        }
        IngestSource::Reddit { target } => ingest::reddit::ingest_reddit(&cfg, target).await,
        IngestSource::Youtube { target } => {
            if ingest::youtube::is_playlist_or_channel_url(target) {
                ingest_youtube_playlist_with_pool(&cfg, &pool, id, target).await
            } else {
                ingest::youtube::ingest_youtube(&cfg, target).await
            }
        }
        IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => {
            let mut sessions_cfg = cfg.clone();
            sessions_cfg.sessions_claude = *sessions_claude;
            sessions_cfg.sessions_codex = *sessions_codex;
            sessions_cfg.sessions_gemini = *sessions_gemini;
            sessions_cfg.sessions_project = sessions_project.clone();
            ingest::sessions::ingest_sessions(&sessions_cfg).await
        }
    };

    match result {
        Ok(chunks) => {
            mark_completed(&pool, id, chunks).await;
        }
        Err(e) => {
            if let Err(e2) = mark_job_failed(&pool, TABLE, id, &e.to_string()).await {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
        }
    }
}
