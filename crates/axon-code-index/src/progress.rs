#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ReindexProgress {
    Started {
        generation: i64,
        total_files: usize,
        added_files: usize,
        modified_files: usize,
        removed_files: usize,
        total_batches: usize,
    },
    BatchFinished {
        generation: i64,
        batch_number: usize,
        total_batches: usize,
        processed_files: usize,
        total_files: usize,
        batch_files: usize,
        embedded_docs: usize,
    },
    CleanupStarted {
        generation: i64,
        cleanup_paths: usize,
    },
    CommitStarted {
        generation: i64,
    },
    Finished {
        generation: i64,
    },
}

pub trait ReindexProgressSink: Sync {
    fn emit(&self, progress: ReindexProgress);
}

pub(crate) struct ReindexRunOptions<'a> {
    pub generation: i64,
    pub progress: Option<&'a dyn ReindexProgressSink>,
}

pub(crate) fn emit_progress(progress: Option<&dyn ReindexProgressSink>, event: ReindexProgress) {
    if let Some(progress) = progress {
        progress.emit(event);
    }
}
