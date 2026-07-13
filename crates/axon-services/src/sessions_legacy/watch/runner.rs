use super::process::{
    SessionWatchEventSink, SessionWatchIngestor, process_pending, process_session_batch_for_watch,
    redact_error_detail, validate_event_path,
};
use super::queue::PendingFiles;
use super::targets::{
    WatchTarget, canonical_path_allowed, collect_validated_files, collect_validated_files_under,
    collect_watch_dirs, handle_remove_path, provider_allowed, watch_targets,
};
use super::validate::SessionWatchRoots;
use super::{MAX_DIRTY_RESCAN_DIRS, MAX_WATCH_DIRS, SessionWatchOptions, WATCH_EVENT_BUFFER};
use anyhow::{Result, anyhow};
use axon_core::config::Config;
use notify::Watcher;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub async fn run_session_watch(
    cfg: &Config,
    pool: &sqlx::SqlitePool,
    ingestor: &dyn SessionWatchIngestor,
    options: SessionWatchOptions,
    events: &dyn SessionWatchEventSink,
) -> Result<()> {
    let roots = SessionWatchRoots::from_config(cfg)?;
    run_session_watch_with_roots(cfg, pool, ingestor, options, roots, events).await
}

pub async fn run_session_watch_with_roots(
    cfg: &Config,
    pool: &sqlx::SqlitePool,
    ingestor: &dyn SessionWatchIngestor,
    options: SessionWatchOptions,
    roots: SessionWatchRoots,
    events: &dyn SessionWatchEventSink,
) -> Result<()> {
    let targets = watch_targets(cfg, &roots, &options)?;
    if targets.is_empty() {
        return Err(anyhow!("no AI session roots exist to watch"));
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

    let mut watched_dirs = BTreeSet::new();
    for target in &targets {
        watch_directory_tree(&mut watcher, target.root(), &mut watched_dirs)?;
    }
    if watched_dirs.is_empty() {
        return Err(anyhow!(
            "no accessible AI session directories exist to watch"
        ));
    }

    if options.initial_scan {
        run_initial_rescan(cfg, ingestor, pool, &roots, &targets, &options, events).await;
    }

    let tick_duration = options
        .debounce
        .min(options.settle)
        .max(Duration::from_millis(50));
    let mut tick = tokio::time::interval(tick_duration);
    let mut pending = PendingFiles::default();
    let mut last_rescan = None;
    let mut dirty_rescan_dirs = DirtyRescanDirs::default();

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                for dir in handle_event(
                    event,
                    &roots,
                    &targets,
                    &mut pending,
                    overflow_rescan.as_ref(),
                ) {
                    watch_directory_tree(&mut watcher, &dir, &mut watched_dirs)?;
                    if !dirty_rescan_dirs.push(dir) {
                        dirty_rescan_dirs.clear();
                        overflow_rescan.store(true, Ordering::Relaxed);
                    }
                }
            }
            _ = tick.tick() => {
                handle_tick(
                    TickContext {
                        cfg,
                        ingestor,
                        pool,
                        roots: &roots,
                        targets: &targets,
                        options: &options,
                        overflow_rescan: overflow_rescan.as_ref(),
                        events,
                    },
                    &mut pending,
                    &mut last_rescan,
                    &mut dirty_rescan_dirs,
                )
                .await;
            }
            _ = shutdown_signal() => {
                tracing::info!("session watcher stopping");
                return Ok(());
            }
        }
    }
}

fn watch_directory_tree(
    watcher: &mut notify::RecommendedWatcher,
    root: &Path,
    watched_dirs: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    for dir in collect_watch_dirs(root)? {
        if watched_dirs.contains(&dir) {
            continue;
        }
        if watched_dirs.len() >= MAX_WATCH_DIRS {
            return Err(anyhow!(
                "session watcher directory budget exceeded ({MAX_WATCH_DIRS}); use a narrower --path or raise system inotify limits"
            ));
        }
        watcher.watch(&dir, notify::RecursiveMode::NonRecursive)?;
        watched_dirs.insert(dir);
    }
    Ok(())
}

pub fn handle_event(
    event: notify::Result<notify::Event>,
    roots: &SessionWatchRoots,
    targets: &[WatchTarget],
    pending: &mut PendingFiles,
    overflow_rescan: &AtomicBool,
) -> Vec<PathBuf> {
    let mut new_dirs = Vec::new();
    match event {
        Ok(event) => {
            if event.need_rescan() {
                overflow_rescan.store(true, Ordering::Relaxed);
                return new_dirs;
            }
            if event.kind.is_create() || event.kind.is_modify() {
                let now = Instant::now();
                for path in event.paths {
                    if event.kind.is_create()
                        && path.is_dir()
                        && targets.iter().all(|target| target.allowed_file().is_none())
                    {
                        new_dirs.push(path);
                    } else if let Some(validated) = validate_event_path(roots, &path)
                        && canonical_path_allowed(&validated.canonical, targets)
                        && !pending.push(validated.canonical, now)
                    {
                        pending.clear();
                        overflow_rescan.store(true, Ordering::Relaxed);
                    }
                }
            } else if event.kind.is_remove() {
                for path in event.paths {
                    handle_remove_path(&path, roots, targets, pending);
                }
            }
        }
        Err(error) => tracing::warn!(
            detail = %redact_error_detail(&error.to_string()),
            "session watch event failed"
        ),
    }
    new_dirs
}

#[derive(Debug, Default)]
pub struct DirtyRescanDirs {
    dirs: BTreeSet<PathBuf>,
    coalesced_events: u64,
}

impl DirtyRescanDirs {
    pub(crate) fn push(&mut self, dir: PathBuf) -> bool {
        let dir = dir.canonicalize().unwrap_or(dir);
        if self.dirs.iter().any(|existing| dir.starts_with(existing)) {
            self.coalesced_events += 1;
            return true;
        }

        let children = self
            .dirs
            .iter()
            .filter(|existing| existing.starts_with(&dir))
            .cloned()
            .collect::<Vec<_>>();
        for child in children {
            self.dirs.remove(&child);
            self.coalesced_events += 1;
        }

        if self.dirs.len() >= MAX_DIRTY_RESCAN_DIRS {
            return false;
        }
        self.dirs.insert(dir);
        true
    }

    pub(crate) fn take(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.dirs).into_iter().collect()
    }

    pub(crate) fn clear(&mut self) {
        self.dirs.clear();
    }
}

async fn run_initial_rescan(
    cfg: &Config,
    ingestor: &dyn SessionWatchIngestor,
    pool: &sqlx::SqlitePool,
    roots: &SessionWatchRoots,
    targets: &[WatchTarget],
    options: &SessionWatchOptions,
    events: &dyn SessionWatchEventSink,
) {
    for target in targets {
        let files = collect_validated_files(roots, target);
        process_rescan_batches(cfg, ingestor, pool, files, options, events, "initial").await;
    }
}

async fn process_rescan_batches(
    cfg: &Config,
    ingestor: &dyn SessionWatchIngestor,
    pool: &sqlx::SqlitePool,
    files: Vec<super::validate::ValidatedSessionPath>,
    options: &SessionWatchOptions,
    events: &dyn SessionWatchEventSink,
    phase: &'static str,
) {
    for batch in files.chunks(options.max_batch_docs.max(1)) {
        if let Err(error) =
            process_session_batch_for_watch(cfg, ingestor, pool, batch.to_vec(), options, events)
                .await
        {
            tracing::warn!(
                detail = %redact_error_detail(&error.to_string()),
                phase,
                "session watch rescan batch failed"
            );
        }
    }
}

struct TickContext<'a> {
    cfg: &'a Config,
    ingestor: &'a dyn SessionWatchIngestor,
    pool: &'a sqlx::SqlitePool,
    roots: &'a SessionWatchRoots,
    targets: &'a [WatchTarget],
    options: &'a SessionWatchOptions,
    overflow_rescan: &'a AtomicBool,
    events: &'a dyn SessionWatchEventSink,
}

async fn handle_tick(
    ctx: TickContext<'_>,
    pending: &mut PendingFiles,
    last_rescan: &mut Option<Instant>,
    dirty_rescan_dirs: &mut DirtyRescanDirs,
) {
    run_dirty_rescans(
        ctx.cfg,
        ctx.roots,
        ctx.targets,
        pending,
        dirty_rescan_dirs,
        ctx.overflow_rescan,
    );
    if ctx.overflow_rescan.load(Ordering::Relaxed)
        && rescan_due(Instant::now(), *last_rescan, ctx.options.rescan_cooldown)
        && ctx
            .overflow_rescan
            .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        run_initial_rescan_with_cooldown(&ctx, last_rescan).await;
    }
    process_pending(
        ctx.cfg,
        ctx.ingestor,
        ctx.pool,
        ctx.roots,
        ctx.options,
        pending,
        ctx.events,
    )
    .await;
}

pub fn run_dirty_rescans(
    cfg: &Config,
    roots: &SessionWatchRoots,
    targets: &[WatchTarget],
    pending: &mut PendingFiles,
    dirty_rescan_dirs: &mut DirtyRescanDirs,
    overflow_rescan: &AtomicBool,
) {
    let now = Instant::now();
    for dir in dirty_rescan_dirs.take() {
        for validated in collect_validated_files_under(roots, &dir) {
            if !provider_allowed(cfg, validated.provider)
                || !canonical_path_allowed(&validated.canonical, targets)
            {
                continue;
            }
            if !pending.push(validated.canonical, now) {
                pending.clear();
                overflow_rescan.store(true, Ordering::Relaxed);
                return;
            }
        }
    }
}

async fn run_initial_rescan_with_cooldown(
    ctx: &TickContext<'_>,
    last_rescan: &mut Option<Instant>,
) -> bool {
    let now = Instant::now();
    if !rescan_due(now, *last_rescan, ctx.options.rescan_cooldown) {
        return false;
    }
    *last_rescan = Some(now);
    run_initial_rescan(
        ctx.cfg,
        ctx.ingestor,
        ctx.pool,
        ctx.roots,
        ctx.targets,
        ctx.options,
        ctx.events,
    )
    .await;
    true
}

pub fn rescan_due(now: Instant, last_rescan: Option<Instant>, cooldown: Duration) -> bool {
    last_rescan.is_none_or(|last| now.duration_since(last) >= cooldown)
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
                tracing::warn!(
                    detail = %redact_error_detail(&error.to_string()),
                    "failed to install SIGTERM handler for session watcher"
                );
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
