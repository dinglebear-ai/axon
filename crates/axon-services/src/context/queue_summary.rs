use super::*;

pub(super) fn spawn_queue_summary_logger(
    jobs: Arc<dyn ServiceJobRuntime>,
    secs: u64,
) -> std::io::Result<Option<Arc<QueueSummaryTask>>> {
    spawn_queue_summary_logger_with_runtime(
        jobs,
        secs,
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build(),
        "axon-queue-summary",
    )
}

pub(super) fn spawn_queue_summary_logger_with_runtime(
    jobs: Arc<dyn ServiceJobRuntime>,
    secs: u64,
    runtime: std::io::Result<tokio::runtime::Runtime>,
    thread_name: &str,
) -> std::io::Result<Option<Arc<QueueSummaryTask>>> {
    if secs == 0 {
        return Ok(None);
    }
    if thread_name.as_bytes().contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "background worker thread name contains a null byte",
        ));
    }
    let (stop, stopped) = std::sync::mpsc::channel();
    let (ready, startup) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let runtime = match runtime {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                }
            };
            if ready.send(Ok(())).is_err() {
                return;
            }
            loop {
                match stopped.recv_timeout(Duration::from_secs(secs)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
                runtime.block_on(log_queue_depths(&jobs, secs));
            }
        })?;
    match startup.recv() {
        Ok(Ok(())) => Ok(Some(Arc::new(QueueSummaryTask::new(stop, thread)))),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(error) => {
            let _ = thread.join();
            Err(std::io::Error::other(format!(
                "queue summary startup handshake failed: {error}"
            )))
        }
    }
}

async fn log_queue_depths(jobs: &Arc<dyn ServiceJobRuntime>, secs: u64) {
    let Some(source) = queue_depth(jobs, JobKind::Source).await else {
        return;
    };
    let Some(extract) = queue_depth(jobs, JobKind::Extract).await else {
        return;
    };
    let Some(watch) = queue_depth(jobs, JobKind::Watch).await else {
        return;
    };
    let Some(prune) = queue_depth(jobs, JobKind::Prune).await else {
        return;
    };
    tracing::info!(
        source,
        extract,
        watch,
        prune,
        interval_secs = secs,
        "job queue summary"
    );
}

async fn queue_depth(jobs: &Arc<dyn ServiceJobRuntime>, kind: JobKind) -> Option<i64> {
    match jobs.count_jobs(kind).await {
        Ok(count) => Some(count),
        Err(err) => {
            tracing::warn!(?kind, error = %err, "failed to read job queue depth");
            None
        }
    }
}
