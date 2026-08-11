use std::collections::{HashMap, HashSet};

use axon_api::source::{JobStatusUpdate, SourceProgressEvent};

use super::{ActiveProgress, active_progress};

pub(crate) struct BatchTarget {
    pub target: String,
    pub progress: Option<ActiveProgress>,
    updated_at: u64,
}

pub(crate) struct BatchWaitViewModel {
    total: usize,
    completed: usize,
    failed: usize,
    active_targets: HashMap<usize, BatchTarget>,
    finished_targets: HashSet<usize>,
    update_sequence: u64,
}

impl BatchWaitViewModel {
    pub(crate) fn new(total: usize) -> Self {
        Self {
            total,
            completed: 0,
            failed: 0,
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
                updated_at: self.update_sequence,
            },
        );
    }

    pub(crate) fn apply_snapshot(&mut self, index: usize, update: JobStatusUpdate) {
        self.touch();
        if let Some(target) = self.active_targets.get_mut(&index) {
            target.progress = Some(active_progress(
                update.phase,
                update.counts.as_ref(),
                update.current.as_ref(),
                None,
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
                None,
            ));
            target.updated_at = self.update_sequence;
        }
    }

    pub(crate) fn completed(&mut self, index: usize) {
        if self.finished_targets.insert(index) {
            self.completed += 1;
            self.active_targets.remove(&index);
        }
    }

    pub(crate) fn failed(&mut self, index: usize) {
        if self.finished_targets.insert(index) {
            self.failed += 1;
            self.active_targets.remove(&index);
        }
    }

    pub(crate) fn summary(&self) -> String {
        let finished = self.completed + self.failed;
        let active = self.active_targets.len();
        let queued = self.total.saturating_sub(finished + active);
        let mut summary = format!(
            "{finished}/{} complete · {active} active · {queued} queued",
            self.total
        );
        if self.failed > 0 {
            summary.push_str(&format!(" · {} failed", self.failed));
        }
        summary
    }

    pub(crate) fn active_detail(&self) -> Option<&BatchTarget> {
        self.active_targets
            .values()
            .max_by_key(|target| target.updated_at)
    }

    fn touch(&mut self) {
        self.update_sequence = self.update_sequence.saturating_add(1);
    }
}
