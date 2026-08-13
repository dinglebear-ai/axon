use std::collections::{HashMap, HashSet};

use axon_api::source::{JobStatusUpdate, LifecycleStatus, SourceKind, SourceProgressEvent};

use super::{ActiveProgress, active_progress};

pub(crate) struct BatchTarget {
    pub target: String,
    pub progress: Option<ActiveProgress>,
    source_kind: Option<SourceKind>,
    updated_at: u64,
}

pub(crate) struct BatchWaitViewModel {
    total: usize,
    completed: usize,
    degraded: usize,
    failed: usize,
    canceled: usize,
    expired: usize,
    skipped: usize,
    active_targets: HashMap<usize, BatchTarget>,
    finished_targets: HashSet<usize>,
    update_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchTerminalOutcome {
    Completed,
    Degraded,
    Failed,
    Canceled,
    Expired,
    Skipped,
}

impl BatchTerminalOutcome {
    pub(crate) fn from_status(status: LifecycleStatus) -> Self {
        match status {
            LifecycleStatus::Completed => Self::Completed,
            LifecycleStatus::CompletedDegraded => Self::Degraded,
            LifecycleStatus::Canceled => Self::Canceled,
            LifecycleStatus::Expired => Self::Expired,
            LifecycleStatus::Skipped => Self::Skipped,
            LifecycleStatus::Failed
            | LifecycleStatus::Queued
            | LifecycleStatus::Pending
            | LifecycleStatus::Running
            | LifecycleStatus::Waiting
            | LifecycleStatus::Blocked
            | LifecycleStatus::Canceling => Self::Failed,
        }
    }
}

impl BatchWaitViewModel {
    pub(crate) fn new(total: usize) -> Self {
        Self {
            total,
            completed: 0,
            degraded: 0,
            failed: 0,
            canceled: 0,
            expired: 0,
            skipped: 0,
            active_targets: HashMap::new(),
            finished_targets: HashSet::new(),
            update_sequence: 0,
        }
    }

    pub(crate) fn running(&mut self, index: usize, target: impl Into<String>) {
        self.touch();
        self.active_targets.insert(
            index,
            BatchTarget {
                target: target.into(),
                progress: None,
                source_kind: None,
                updated_at: self.update_sequence,
            },
        );
    }

    pub(crate) fn routed(&mut self, index: usize, source_kind: SourceKind) {
        self.touch();
        if let Some(target) = self.active_targets.get_mut(&index) {
            target.source_kind = Some(source_kind);
            target.updated_at = self.update_sequence;
        }
    }

    pub(crate) fn apply_snapshot(&mut self, index: usize, update: JobStatusUpdate) {
        self.touch();
        if let Some(target) = self.active_targets.get_mut(&index) {
            target.progress = Some(active_progress(
                update.phase,
                update.counts.as_ref(),
                update.current.as_ref(),
                target.source_kind,
            ));
            target.updated_at = self.update_sequence;
        }
    }

    pub(crate) fn apply_event(&mut self, index: usize, event: SourceProgressEvent) {
        self.touch();
        if let Some(target) = self.active_targets.get_mut(&index) {
            target.progress = Some(active_progress(
                event.phase,
                Some(&event.counts),
                event.current.as_ref(),
                target.source_kind,
            ));
            target.updated_at = self.update_sequence;
        }
    }

    pub(crate) fn finish(&mut self, index: usize, outcome: BatchTerminalOutcome) {
        if self.finished_targets.insert(index) {
            match outcome {
                BatchTerminalOutcome::Completed => self.completed += 1,
                BatchTerminalOutcome::Degraded => self.degraded += 1,
                BatchTerminalOutcome::Failed => self.failed += 1,
                BatchTerminalOutcome::Canceled => self.canceled += 1,
                BatchTerminalOutcome::Expired => self.expired += 1,
                BatchTerminalOutcome::Skipped => self.skipped += 1,
            }
            self.active_targets.remove(&index);
        }
    }

    pub(crate) fn summary(&self) -> String {
        let finished = self.finished_count();
        let active = self.active_targets.len();
        let queued = self.total.saturating_sub(finished + active);
        let mut summary = format!(
            "{finished}/{} complete · {active} active · {queued} queued",
            self.total
        );
        if self.failed > 0 {
            summary.push_str(&format!(" · {} failed", self.failed));
        }
        if self.canceled > 0 {
            summary.push_str(&format!(" · {} canceled", self.canceled));
        }
        if self.expired > 0 {
            summary.push_str(&format!(" · {} expired", self.expired));
        }
        if self.degraded > 0 {
            summary.push_str(&format!(" · {} degraded", self.degraded));
        }
        if self.skipped > 0 {
            summary.push_str(&format!(" · {} skipped", self.skipped));
        }
        summary
    }

    pub(crate) fn active_detail(&self) -> Option<&BatchTarget> {
        self.active_targets
            .values()
            .max_by_key(|target| target.updated_at)
    }

    pub(crate) fn successful_count(&self) -> usize {
        self.completed + self.skipped
    }

    pub(crate) fn hard_failure_count(&self) -> usize {
        self.failed + self.canceled + self.expired
    }

    pub(crate) fn degraded_count(&self) -> usize {
        self.degraded
    }

    fn finished_count(&self) -> usize {
        self.completed + self.degraded + self.failed + self.canceled + self.expired + self.skipped
    }

    fn touch(&mut self) {
        self.update_sequence = self.update_sequence.saturating_add(1);
    }
}
