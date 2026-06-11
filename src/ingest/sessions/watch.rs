mod process;
mod queue;
mod runner;
mod smoke;
mod targets;
pub mod validate;

pub(crate) use process::{
    NoopSessionWatchEventSink, SessionWatchEventSink, SessionWatchIngestor,
    SessionWatchProcessEvent, WatchIngestResult,
};
pub(crate) use runner::run_session_watch;
pub use smoke::{SessionWatchSmokeReport, smoke_watch};

pub(crate) const WATCH_EVENT_BUFFER: usize = 1024;
pub(crate) const MAX_WATCH_DIRS: usize = 8192;
pub(crate) const MAX_PENDING_FILES: usize = 4096;
pub(crate) const MAX_DIRTY_RESCAN_DIRS: usize = 256;

pub use crate::core::config::SessionWatchConfig as SessionWatchOptions;

#[cfg(test)]
pub(crate) use process::{
    ProcessOutcome, effective_processing_concurrency, process_pending,
    process_session_batch_for_watch, redact_error_detail, redact_remote_prepared_request,
    upload_prepared_sessions_to_server_with_auth,
};
#[cfg(test)]
pub(crate) use queue::PendingFiles;
#[cfg(test)]
pub(crate) use runner::DirtyRescanDirs;
#[cfg(test)]
pub(crate) use runner::handle_event;
#[cfg(test)]
pub(crate) use runner::rescan_due;
#[cfg(test)]
pub(crate) use runner::run_dirty_rescans;
#[cfg(test)]
pub(crate) use runner::run_session_watch_with_roots;
#[cfg(test)]
pub(crate) use targets::{
    WatchTarget, collect_validated_files_under, collect_watch_dirs, handle_remove_path,
    watch_targets,
};
#[cfg(test)]
pub(crate) use validate::ValidatedSessionPath;

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
