use std::collections::HashSet;
use std::time::Duration;

use axon_api::source::{
    JobEvent, JobId, JobStatusUpdate, LifecycleStatus, PipelinePhase, SourceKind,
    SourceProgressEvent, SourceScope, StageCounts,
};
use axon_services::extract::ExtractProgress;

use crate::commands::job_progress::source_unit;

mod batch;

pub(crate) use batch::BatchWaitViewModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OperatorPhase {
    Resolve,
    Discover,
    Acquire,
    Extract,
    Prepare,
    Embed,
    Publish,
    Clean,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveProgress {
    pub phase: OperatorPhase,
    pub done: u64,
    pub total: Option<u64>,
    pub unit: &'static str,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedMilestone {
    pub phase: OperatorPhase,
    pub summary: String,
    pub elapsed: Duration,
    pub degraded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NoticeCategory {
    PolicyHeld,
    Warning,
    Retry,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NoticeKey {
    pub phase: OperatorPhase,
    pub code: String,
    pub category: NoticeCategory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperatorNotice {
    pub key: NoticeKey,
    pub message: String,
    pub count: u64,
    pub retryable: bool,
}

pub(crate) struct WaitViewModel {
    pub target: String,
    pub scope: Option<SourceScope>,
    pub job_id: Option<JobId>,
    pub milestones: Vec<RenderedMilestone>,
    pub active: Option<ActiveProgress>,
    pub notices: Vec<OperatorNotice>,
    pub terminal: Option<RenderedMilestone>,
    source_kind: Option<SourceKind>,
    seen_event_ids: HashSet<String>,
    phase_started_at: Option<(OperatorPhase, Duration)>,
}

impl WaitViewModel {
    pub(crate) fn source(target: impl Into<String>, scope: Option<SourceScope>) -> Self {
        Self {
            target: target.into(),
            scope,
            job_id: None,
            milestones: Vec::new(),
            active: None,
            notices: Vec::new(),
            terminal: None,
            source_kind: None,
            seen_event_ids: HashSet::new(),
            phase_started_at: None,
        }
    }

    pub(crate) fn apply_snapshot(&mut self, update: JobStatusUpdate) -> bool {
        let previous_job_id = self.job_id.replace(update.job_id);
        let progress = active_progress(
            update.phase,
            update.counts.as_ref(),
            update.current.as_ref(),
            self.source_kind,
        );
        let terminal = is_terminal(update.status)
            .then(|| terminal_milestone(update.status, update.counts.as_ref(), progress.clone()));
        let next = terminal.is_none().then_some(progress);
        let changed =
            previous_job_id != self.job_id || self.active != next || self.terminal != terminal;
        self.active = next;
        self.terminal = terminal;
        changed
    }

    pub(crate) fn apply_extract_progress(&mut self, progress: &ExtractProgress) -> bool {
        let current = progress.last_completed_url.as_ref().map(|url| {
            format!(
                "last completed: {url} · {} {}",
                progress.items_done,
                if progress.items_done == 1 {
                    "item"
                } else {
                    "items"
                }
            )
        });
        let next = ActiveProgress {
            phase: OperatorPhase::Extract,
            done: progress.urls_done,
            total: Some(progress.urls_total),
            unit: if progress.urls_total == 1 {
                "URL"
            } else {
                "URLs"
            },
            current,
        };
        let changed = self.active.as_ref() != Some(&next);
        self.active = Some(next);
        changed
    }

    pub(crate) fn apply_event(&mut self, event: SourceProgressEvent) -> bool {
        if !self.seen_event_ids.insert(event.event_id.clone()) {
            return false;
        }
        self.job_id = Some(event.job_id);
        let mut changed = self.apply_notice(&event);
        if !is_terminal(event.status) {
            let active = active_progress(
                event.phase,
                Some(&event.counts),
                event.current.as_ref(),
                self.source_kind,
            );
            if self.active.as_ref() != Some(&active) {
                self.active = Some(active);
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn apply_persisted_event(&mut self, event: JobEvent) -> bool {
        event
            .details
            .get("source_progress_event")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .is_some_and(|progress| self.apply_event(progress))
    }

    pub(crate) fn start_phase_at(&mut self, phase: PipelinePhase, now: Duration) {
        let phase = operator_phase(phase);
        if self.phase_started_at.map(|started| started.0) != Some(phase) {
            self.phase_started_at = Some((phase, now));
        }
    }

    pub(crate) fn complete_phase_at(&mut self, phase: PipelinePhase, now: Duration) -> bool {
        let phase = operator_phase(phase);
        let Some((started_phase, started_at)) = self.phase_started_at else {
            return false;
        };
        if started_phase != phase {
            return false;
        }
        self.phase_started_at = None;
        let elapsed = now.saturating_sub(started_at);
        if elapsed < Duration::from_secs(1) && !self.phase_is_degraded(phase) {
            return false;
        }
        let summary = self
            .active
            .as_ref()
            .filter(|active| active.phase == phase)
            .map(progress_summary)
            .unwrap_or_else(|| phase.label().to_string());
        self.milestones.push(RenderedMilestone {
            phase,
            summary,
            elapsed,
            degraded: self.phase_is_degraded(phase),
        });
        true
    }

    pub(crate) fn finish(&mut self) -> bool {
        if self.terminal.is_some() {
            return self.active.take().is_some();
        }
        let Some(active) = self.active.take() else {
            return false;
        };
        self.terminal = Some(RenderedMilestone {
            phase: OperatorPhase::Complete,
            summary: progress_summary(&active),
            elapsed: Duration::ZERO,
            degraded: false,
        });
        true
    }

    fn phase_is_degraded(&self, phase: OperatorPhase) -> bool {
        self.notices.iter().any(|notice| notice.key.phase == phase)
    }

    fn apply_notice(&mut self, event: &SourceProgressEvent) -> bool {
        let Some((code, category, retryable, safe_message)) = notice_parts(event) else {
            return false;
        };
        let key = NoticeKey {
            phase: operator_phase(event.phase),
            code,
            category,
        };
        if let Some(notice) = self.notices.iter_mut().find(|notice| notice.key == key) {
            notice.count += 1;
            notice.message = notice_message(category, notice.count, &safe_message);
            notice.retryable |= retryable;
        } else {
            self.notices.push(OperatorNotice {
                key,
                message: notice_message(category, 1, &safe_message),
                count: 1,
                retryable,
            });
        }
        true
    }
}

impl OperatorPhase {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Discover => "discover",
            Self::Acquire => "acquire",
            Self::Extract => "extract",
            Self::Prepare => "prepare",
            Self::Embed => "embed",
            Self::Publish => "publish",
            Self::Clean => "clean",
            Self::Complete => "complete",
        }
    }
}

pub(crate) const fn operator_phase(phase: PipelinePhase) -> OperatorPhase {
    match phase {
        PipelinePhase::Queued
        | PipelinePhase::Requested
        | PipelinePhase::Resolving
        | PipelinePhase::Routing
        | PipelinePhase::Authorizing
        | PipelinePhase::Planning
        | PipelinePhase::Leasing => OperatorPhase::Resolve,
        PipelinePhase::Discovering | PipelinePhase::Diffing => OperatorPhase::Discover,
        PipelinePhase::Fetching | PipelinePhase::Rendering | PipelinePhase::Enriching => {
            OperatorPhase::Acquire
        }
        PipelinePhase::Normalizing
        | PipelinePhase::Parsing
        | PipelinePhase::Graphing
        | PipelinePhase::Preparing
        | PipelinePhase::Retrieving
        | PipelinePhase::Synthesizing
        | PipelinePhase::Evaluating => OperatorPhase::Prepare,
        PipelinePhase::Batching | PipelinePhase::Embedding | PipelinePhase::Vectorizing => {
            OperatorPhase::Embed
        }
        PipelinePhase::Upserting | PipelinePhase::Publishing => OperatorPhase::Publish,
        PipelinePhase::Cleaning => OperatorPhase::Clean,
        PipelinePhase::Complete | PipelinePhase::Canceled => OperatorPhase::Complete,
    }
}

fn active_progress(
    phase: PipelinePhase,
    counts: Option<&StageCounts>,
    current: Option<&axon_api::source::ProgressCurrent>,
    source_kind: Option<SourceKind>,
) -> ActiveProgress {
    let operator = operator_phase(phase);
    let (done, total, unit) = progress_counts(operator, counts, source_kind);
    ActiveProgress {
        phase: operator,
        done,
        total,
        unit,
        current: current.and_then(current_label),
    }
}

fn progress_counts(
    phase: OperatorPhase,
    counts: Option<&StageCounts>,
    source_kind: Option<SourceKind>,
) -> (u64, Option<u64>, &'static str) {
    let Some(counts) = counts else {
        return (0, None, phase.default_unit());
    };
    match phase {
        OperatorPhase::Resolve => (counts.items_done, counts.items_total, "items"),
        OperatorPhase::Discover | OperatorPhase::Acquire => {
            let (_, plural) = source_unit(source_kind);
            (counts.items_done, counts.items_total, plural)
        }
        OperatorPhase::Extract => (counts.items_done, counts.items_total, "URLs"),
        OperatorPhase::Prepare => (counts.documents_done, counts.documents_total, "documents"),
        OperatorPhase::Embed => (counts.chunks_done, counts.chunks_total, "chunks"),
        OperatorPhase::Publish => (counts.chunks_done, counts.chunks_total, "vectors"),
        OperatorPhase::Clean | OperatorPhase::Complete => {
            (counts.items_done, counts.items_total, "items")
        }
    }
}

impl OperatorPhase {
    const fn default_unit(self) -> &'static str {
        match self {
            Self::Discover
            | Self::Acquire
            | Self::Extract
            | Self::Resolve
            | Self::Clean
            | Self::Complete => "items",
            Self::Prepare => "documents",
            Self::Embed => "chunks",
            Self::Publish => "vectors",
        }
    }
}

fn current_label(current: &axon_api::source::ProgressCurrent) -> Option<String> {
    current
        .source_item_key
        .as_ref()
        .map(|value| value.0.clone())
        .or_else(|| current.document_id.as_ref().map(|value| value.0.clone()))
        .or_else(|| current.chunk_id.as_ref().map(|value| value.0.clone()))
        .or_else(|| current.message.clone())
}

fn progress_summary(progress: &ActiveProgress) -> String {
    match progress.total {
        Some(total) if total > 0 => format!("{}/{} {}", progress.done, total, progress.unit),
        _ => format!("{} {}", progress.done, progress.unit),
    }
}

fn notice_parts(event: &SourceProgressEvent) -> Option<(String, NoticeCategory, bool, String)> {
    if let Some(warning) = &event.warning {
        let category = notice_category(&warning.code);
        return Some((
            warning.code.clone(),
            category,
            warning.retryable,
            warning.message.clone(),
        ));
    }
    if let Some(error) = &event.error {
        let code = error.code.0.clone();
        let category = match notice_category(&code) {
            NoticeCategory::PolicyHeld => NoticeCategory::PolicyHeld,
            _ => NoticeCategory::Failure,
        };
        return Some((code, category, error.retryable, error.message.clone()));
    }
    event.retry.as_ref().map(|retry| {
        (
            "provider.retry".to_string(),
            NoticeCategory::Retry,
            true,
            retry.reason.clone(),
        )
    })
}

fn notice_category(code: &str) -> NoticeCategory {
    let code = code.to_ascii_lowercase();
    if ["redact", "secret", "forbidden"]
        .iter()
        .any(|needle| code.contains(needle))
    {
        NoticeCategory::PolicyHeld
    } else {
        NoticeCategory::Warning
    }
}

fn notice_message(category: NoticeCategory, count: u64, safe_message: &str) -> String {
    if category == NoticeCategory::PolicyHeld {
        return format!(
            "secret policy held {count} {}",
            if count == 1 { "chunk" } else { "chunks" }
        );
    }
    if count == 1 {
        safe_message.to_string()
    } else {
        format!("{safe_message} · {count} occurrences")
    }
}

fn is_terminal(status: LifecycleStatus) -> bool {
    matches!(
        status,
        LifecycleStatus::Completed
            | LifecycleStatus::CompletedDegraded
            | LifecycleStatus::Failed
            | LifecycleStatus::Canceled
            | LifecycleStatus::Expired
            | LifecycleStatus::Skipped
    )
}

fn terminal_milestone(
    status: LifecycleStatus,
    counts: Option<&StageCounts>,
    fallback: ActiveProgress,
) -> RenderedMilestone {
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
    RenderedMilestone {
        phase: OperatorPhase::Complete,
        summary,
        elapsed: Duration::ZERO,
        degraded: status != LifecycleStatus::Completed,
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
