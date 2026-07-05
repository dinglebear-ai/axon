use axon_api::source::SourceGenerationId;
use serde::Serialize;
use std::path::PathBuf;

use super::ReindexProgress;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CodeSearchWatchEvent {
    Started {
        watch_dirs: Vec<PathBuf>,
        roots: Vec<PathBuf>,
        initial_refresh: bool,
    },
    RefreshStarted {
        root: PathBuf,
        reason: &'static str,
    },
    RefreshFinished {
        root: PathBuf,
        status: String,
        warning: Option<String>,
        indexed_files: usize,
        removed_files: usize,
        generation: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_source_generation: Option<SourceGenerationId>,
    },
    RefreshProgress {
        progress: ReindexProgress,
    },
    RefreshFailed {
        root: PathBuf,
        error: String,
    },
    Pending {
        root: PathBuf,
        paths: usize,
    },
    DryRun {
        plan: CodeSearchWatchDryRunPlan,
    },
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeSearchWatchDryRunPlan {
    pub roots: Vec<CodeSearchWatchDryRunRoot>,
    pub total_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeSearchWatchDryRunRoot {
    pub root: PathBuf,
    pub files: Vec<String>,
}

pub trait CodeSearchWatchEventSink: Sync {
    fn emit(&self, event: CodeSearchWatchEvent);
}
