mod process;
mod queue;
mod runner;
mod smoke;
mod targets;
pub mod validate;

pub use process::{
    NoopSessionWatchEventSink, SessionWatchEventSink, SessionWatchIngestor,
    SessionWatchProcessEvent, WatchIngestResult,
};
pub use runner::run_session_watch;
pub use smoke::{SessionWatchSmokeReport, smoke_watch};

pub(crate) const WATCH_EVENT_BUFFER: usize = 1024;
pub(crate) const MAX_WATCH_DIRS: usize = 8192;
pub(crate) const MAX_PENDING_FILES: usize = 4096;
pub(crate) const MAX_DIRTY_RESCAN_DIRS: usize = 256;

pub use axon_core::config::SessionWatchConfig as SessionWatchOptions;

#[cfg(test)]
pub use process::{
    ProcessOutcome, effective_processing_concurrency, process_pending,
    process_session_batch_for_watch, redact_error_detail, redact_remote_prepared_request,
    upload_prepared_sessions_to_server_with_auth,
};
#[cfg(test)]
pub use queue::PendingFiles;
#[cfg(test)]
pub use runner::DirtyRescanDirs;
#[cfg(test)]
pub use runner::handle_event;
#[cfg(test)]
pub use runner::rescan_due;
#[cfg(test)]
pub use runner::run_dirty_rescans;
#[cfg(test)]
pub use runner::run_session_watch_with_roots;
#[cfg(test)]
pub use targets::{
    WatchTarget, collect_validated_files_under, collect_watch_dirs, handle_remove_path,
    watch_targets,
};
#[cfg(test)]
pub use validate::ValidatedSessionPath;
