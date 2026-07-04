mod event;
mod roots;
use crate::context::ServiceContext;
use crate::query;
use crate::query::CodeSearchRefreshBackend;
use crate::types::CodeSearchCaller;
use anyhow::Result;
use axon_core::config::CodeSearchWatchConfig;
use notify::Watcher;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub use axon_code_index::{ReindexProgress, ReindexProgressSink};
pub use event::{
    CodeSearchWatchDryRunPlan, CodeSearchWatchDryRunRoot, CodeSearchWatchEvent,
    CodeSearchWatchEventSink,
};
use roots::{
    build_code_search_watch_dry_run_plan, code_search_watch_dirs, code_search_watch_dirty_roots,
    discover_code_search_watch_roots_for_dirs,
};

const WATCH_EVENT_BUFFER: usize = 1024;

pub async fn run_code_search_watch(
    ctx: &ServiceContext,
    options: CodeSearchWatchConfig,
    events: &dyn CodeSearchWatchEventSink,
) -> Result<()> {
    let watch_dirs = code_search_watch_dirs(&options)?;
    if options.dry_run {
        let plan = build_code_search_watch_dry_run_plan(&watch_dirs).await?;
        events.emit(CodeSearchWatchEvent::DryRun { plan });
        return Ok(());
    }
    if options.enable {
        anyhow::bail!(
            "persistent code-search-watch service installation was removed; run `axon embed <path> --watch` under your supervisor instead"
        );
    }

    let roots = prepare_watch_roots(&watch_dirs, options.initial_refresh, events)?;
    if options.initial_refresh {
        for root in &roots {
            refresh_code_search_watch_root(ctx, events, root, "initial").await?;
        }
    }

    let overflow_rescan = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<notify::Result<notify::Event>>(WATCH_EVENT_BUFFER);
    let callback_rescan = Arc::clone(&overflow_rescan);
    let mut watcher = notify::RecommendedWatcher::new(
        move |event| {
            if tx.try_send(event).is_err() {
                callback_rescan.store(true, Ordering::Relaxed);
            }
        },
        notify::Config::default().with_follow_symlinks(false),
    )?;
    for root in &roots {
        watcher.watch(root, notify::RecursiveMode::Recursive)?;
    }

    watch_loop(ctx, events, &options, &roots, overflow_rescan, &mut rx).await
}

fn prepare_watch_roots(
    watch_dirs: &[PathBuf],
    initial_refresh: bool,
    events: &dyn CodeSearchWatchEventSink,
) -> Result<Vec<PathBuf>> {
    let mut roots = discover_code_search_watch_roots_for_dirs(watch_dirs)?;
    if roots.is_empty() {
        return Err(anyhow::anyhow!(
            "embed --watch found no Git checkouts to watch"
        ));
    }
    roots.sort();
    roots.dedup();
    events.emit(CodeSearchWatchEvent::Started {
        watch_dirs: watch_dirs.to_vec(),
        roots: roots.clone(),
        initial_refresh,
    });
    Ok(roots)
}

async fn watch_loop(
    ctx: &ServiceContext,
    events: &dyn CodeSearchWatchEventSink,
    options: &CodeSearchWatchConfig,
    roots: &[PathBuf],
    overflow_rescan: Arc<AtomicBool>,
    rx: &mut tokio::sync::mpsc::Receiver<notify::Result<notify::Event>>,
) -> Result<()> {
    let tick_duration = options
        .debounce
        .min(options.settle)
        .max(Duration::from_millis(50));
    let refresh_delay = options.debounce.saturating_add(options.settle);
    let mut tick = tokio::time::interval(tick_duration);
    let mut dirty: BTreeMap<PathBuf, DirtyRoot> = BTreeMap::new();

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                queue_dirty_roots(events, roots, event, overflow_rescan.as_ref(), &mut dirty);
            }
            _ = tick.tick() => {
                refresh_due_roots(ctx, events, roots, overflow_rescan.as_ref(), refresh_delay, &mut dirty).await;
            }
            _ = shutdown_signal() => {
                events.emit(CodeSearchWatchEvent::Stopped);
                return Ok(());
            }
        }
    }
}

fn queue_dirty_roots(
    events: &dyn CodeSearchWatchEventSink,
    roots: &[PathBuf],
    event: notify::Result<notify::Event>,
    overflow_rescan: &AtomicBool,
    dirty: &mut BTreeMap<PathBuf, DirtyRoot>,
) {
    for root in code_search_watch_dirty_roots(roots, event, overflow_rescan) {
        let paths = mark_dirty_root(dirty, root.clone(), Instant::now());
        if paths == 1 || paths.is_multiple_of(100) {
            events.emit(CodeSearchWatchEvent::Pending { root, paths });
        }
    }
}

fn mark_dirty_root(
    dirty: &mut BTreeMap<PathBuf, DirtyRoot>,
    root: PathBuf,
    since: Instant,
) -> usize {
    let entry = dirty.entry(root).or_insert(DirtyRoot { since, paths: 0 });
    entry.since = since;
    entry.paths = entry.paths.saturating_add(1);
    entry.paths
}

async fn refresh_due_roots(
    ctx: &ServiceContext,
    events: &dyn CodeSearchWatchEventSink,
    roots: &[PathBuf],
    overflow_rescan: &AtomicBool,
    refresh_delay: Duration,
    dirty: &mut BTreeMap<PathBuf, DirtyRoot>,
) {
    if overflow_rescan.swap(false, Ordering::Relaxed) {
        let now = Instant::now();
        for root in roots {
            dirty.entry(root.clone()).or_insert(DirtyRoot {
                since: now,
                paths: 1,
            });
        }
    }
    let due = due_dirty_roots(dirty, refresh_delay);
    for root in due {
        if refresh_code_search_watch_root(ctx, events, &root, "file_change")
            .await
            .is_err()
        {
            if let Some(state) = dirty.get_mut(&root) {
                state.since = Instant::now();
            }
        } else {
            dirty.remove(&root);
        }
    }
}

fn due_dirty_roots(dirty: &BTreeMap<PathBuf, DirtyRoot>, refresh_delay: Duration) -> Vec<PathBuf> {
    dirty
        .iter()
        .filter_map(|(root, state)| {
            (state.since.elapsed() >= refresh_delay).then_some(root.clone())
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct DirtyRoot {
    since: Instant,
    paths: usize,
}

async fn refresh_code_search_watch_root(
    ctx: &ServiceContext,
    events: &dyn CodeSearchWatchEventSink,
    root: &Path,
    reason: &'static str,
) -> Result<()> {
    events.emit(CodeSearchWatchEvent::RefreshStarted {
        root: root.to_path_buf(),
        reason,
    });
    let progress = WatchProgressSink { events };
    let backend = query::default_code_search_refresh_backend(ctx);
    let target_refresh = matches!(backend, CodeSearchRefreshBackend::TargetLocalSource);
    let refresh = query::refresh_code_search_index_with_backend(
        ctx,
        Some(root),
        CodeSearchCaller::Cli,
        backend,
        Some(&progress),
    )
    .await;
    match refresh {
        Ok(result) => {
            let status = result.freshness.status;
            let warning = result.freshness.warning;
            let indexed_files = result.freshness.indexed_files;
            let removed_files = result.freshness.removed_files;
            let generation = result.legacy_code_index_generation;
            let target_source_generation = result.target_source_generation;
            let failed_refresh = status != "fresh" || warning.is_some();
            let failed_initial = reason == "initial" && failed_refresh;
            let failed_target_refresh = target_refresh && failed_refresh;
            let warning_message = warning.clone();
            if failed_initial || failed_target_refresh {
                let error = format!(
                    "local code index refresh failed for {}: {}",
                    root.display(),
                    warning_message
                        .unwrap_or_else(|| "refresh did not produce a fresh index".to_string())
                );
                events.emit(CodeSearchWatchEvent::RefreshFailed {
                    root: root.to_path_buf(),
                    error: error.clone(),
                });
                return Err(anyhow::anyhow!(error));
            }
            events.emit(CodeSearchWatchEvent::RefreshFinished {
                root: root.to_path_buf(),
                status,
                warning,
                indexed_files,
                removed_files,
                generation,
                target_source_generation,
            });
            Ok(())
        }
        Err(error) => {
            events.emit(CodeSearchWatchEvent::RefreshFailed {
                root: root.to_path_buf(),
                error: error.to_string(),
            });
            Err(anyhow::anyhow!("{error}"))
        }
    }
}

struct WatchProgressSink<'a> {
    events: &'a dyn CodeSearchWatchEventSink,
}

impl ReindexProgressSink for WatchProgressSink<'_> {
    fn emit(&self, progress: ReindexProgress) {
        self.events
            .emit(CodeSearchWatchEvent::RefreshProgress { progress });
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to install SIGTERM handler for code-search watcher");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
#[path = "code_search_watch_tests.rs"]
mod tests;
