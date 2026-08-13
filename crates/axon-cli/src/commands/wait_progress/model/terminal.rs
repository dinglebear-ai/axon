use axon_api::source::{LifecycleStatus, StageCounts};

use super::{ActiveProgress, OperatorPhase, progress_summary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedTerminal {
    pub phase: OperatorPhase,
    pub summary: String,
    pub status: TerminalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalStatus {
    Completed,
    CompletedDegraded,
    Failed,
    Canceled,
    Expired,
    Skipped,
}

impl TerminalStatus {
    pub(super) fn from_lifecycle(status: LifecycleStatus) -> Option<Self> {
        match status {
            LifecycleStatus::Completed => Some(Self::Completed),
            LifecycleStatus::CompletedDegraded => Some(Self::CompletedDegraded),
            LifecycleStatus::Failed => Some(Self::Failed),
            LifecycleStatus::Canceled => Some(Self::Canceled),
            LifecycleStatus::Expired => Some(Self::Expired),
            LifecycleStatus::Skipped => Some(Self::Skipped),
            _ => None,
        }
    }
}

pub(super) fn terminal_milestone(
    status: TerminalStatus,
    counts: Option<&StageCounts>,
    fallback: ActiveProgress,
) -> RenderedTerminal {
    let summary = counts.map_or_else(
        || progress_summary(&fallback),
        |counts| {
            let mut parts = Vec::new();
            if counts.documents_done > 0 {
                parts.push(format!("{} documents", counts.documents_done));
            }
            if counts.chunks_done > 0 {
                parts.push(format!("{} vectors", counts.chunks_done));
            }
            if parts.is_empty() {
                progress_summary(&fallback)
            } else {
                parts.join(" · ")
            }
        },
    );
    RenderedTerminal {
        phase: OperatorPhase::Complete,
        summary,
        status,
    }
}

pub(super) fn status_priority(status: TerminalStatus) -> u8 {
    match status {
        TerminalStatus::Failed => 5,
        TerminalStatus::Canceled | TerminalStatus::Expired => 4,
        TerminalStatus::CompletedDegraded => 3,
        TerminalStatus::Skipped => 2,
        TerminalStatus::Completed => 1,
    }
}
