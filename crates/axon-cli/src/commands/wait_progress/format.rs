use axon_core::ui;

use super::model::{ActiveProgress, NoticeCategory, WaitViewModel};
use super::timing::RateEstimate;

const BAR_WIDTH: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormattedWaitView {
    pub heading: String,
    pub milestones: Vec<String>,
    pub notices: Vec<String>,
    pub active: Vec<String>,
    pub terminal: Option<String>,
}

pub(crate) fn format_wait_view(
    model: &WaitViewModel,
    width: usize,
    timing: Option<RateEstimate>,
    color: bool,
) -> FormattedWaitView {
    FormattedWaitView {
        heading: format_heading(model, width, color),
        milestones: model
            .milestones
            .iter()
            .map(|milestone| {
                let symbol = if milestone.degraded { "⚠" } else { "✓" };
                let plain = format!(
                    "{} {:<10} {:<32} {}",
                    symbol,
                    milestone.phase.label(),
                    sanitize_terminal_text(&milestone.summary),
                    format_duration(milestone.elapsed)
                );
                style_status_line(&plain, symbol, milestone.degraded, color)
            })
            .collect(),
        notices: model
            .notices
            .iter()
            .map(|notice| {
                let label = if notice.key.category == NoticeCategory::PolicyHeld {
                    "policy"
                } else {
                    notice.key.phase.label()
                };
                let plain = format!(
                    "⚠ {:<10} {}",
                    label,
                    sanitize_terminal_text(&notice.message)
                );
                ui::warning_when(color, &plain)
            })
            .collect(),
        active: model
            .active
            .as_ref()
            .map(|active| format_active(active, width, timing, color))
            .unwrap_or_default(),
        terminal: model
            .terminal
            .as_ref()
            .map(|terminal| format_terminal(terminal, color)),
    }
}

fn format_terminal(terminal: &super::model::RenderedTerminal, color: bool) -> String {
    use super::model::TerminalStatus;

    let (symbol, label) = match terminal.status {
        TerminalStatus::Completed if terminal.phase == super::model::OperatorPhase::Extract => {
            ("✓", "extracted")
        }
        TerminalStatus::Completed => ("✓", "indexed"),
        TerminalStatus::CompletedDegraded => ("⚠", "degraded"),
        TerminalStatus::Failed => ("✗", "failed"),
        TerminalStatus::Canceled => ("⚠", "canceled"),
        TerminalStatus::Expired => ("⚠", "expired"),
        TerminalStatus::Skipped => ("↷", "skipped"),
    };
    let plain = format!("{symbol} {label:<10} {}", terminal.summary);
    match terminal.status {
        TerminalStatus::Completed => ui::success_when(color, &plain),
        TerminalStatus::Failed => ui::error_when(color, &plain),
        TerminalStatus::Skipped => ui::muted_when(color, &plain),
        _ => ui::warning_when(color, &plain),
    }
}

fn format_heading(model: &WaitViewModel, width: usize, color: bool) -> String {
    let scope = model.scope.map(scope_label);
    let job = model
        .job_id
        .map(|job_id| job_id.0.simple().to_string())
        .map(|job_id| job_id[..8].to_string());
    let scope_suffix = scope.map(|scope| format!(" · {scope}")).unwrap_or_default();
    let job_suffix = job.map(|job| format!("  job {job}")).unwrap_or_default();
    let reserved = 15 + scope_suffix.chars().count() + job_suffix.chars().count();
    let target = middle_truncate(
        &sanitize_terminal_text(&model.target),
        width.saturating_sub(reserved).max(8),
    );
    let plain = format!("  axon source  {target}{scope_suffix}{job_suffix}");
    let identity = ui::primary_when(color, "axon");
    plain.replacen("axon", &identity, 1)
}

fn format_active(
    active: &ActiveProgress,
    width: usize,
    timing: Option<RateEstimate>,
    color: bool,
) -> Vec<String> {
    let percentage = active
        .total
        .filter(|total| *total > 0)
        .map(|total| ((active.done as f64 / total as f64) * 100.0).clamp(0.0, 100.0));
    let counts = count_label(active);
    if width < 46 {
        let percent = percentage
            .map(|value| format!(" · {value:.1}%"))
            .unwrap_or_default();
        return vec![style_active(
            &format!("◐ {:<8} {counts}{percent}", active.phase.label()),
            color,
        )];
    }

    let mut lines = Vec::new();
    if let Some(value) = percentage {
        let bar = progress_bar(value, BAR_WIDTH.min(width.saturating_sub(22)));
        lines.push(style_active(
            &format!("◐ {:<10} {bar}  {value:.1}%", active.phase.label()),
            color,
        ));
    } else {
        lines.push(style_active(
            &format!("◐ {:<10} {counts}", active.phase.label()),
            color,
        ));
    }
    if width >= 50 {
        let timing = timing
            .map(|estimate| {
                format!(
                    " · {}/s · ETA {}",
                    format_rate(estimate.per_second),
                    format_duration(estimate.remaining)
                )
            })
            .unwrap_or_default();
        lines.push(style_supporting(&format!("   {counts}{timing}"), color));
    }
    if width >= 60
        && let Some(current) = active.current.as_deref()
    {
        let current = middle_truncate(&sanitize_terminal_text(current), width.saturating_sub(3));
        lines.push(style_supporting(&format!("   {current}"), color));
    }
    lines
}

fn count_label(active: &ActiveProgress) -> String {
    match active.total {
        Some(total) if total > 0 => format!("{}/{} {}", active.done, total, active.unit),
        _ => format!("{} {}", active.done, active.unit),
    }
}

fn progress_bar(percent: f64, width: usize) -> String {
    let width = width.max(4);
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    format!(
        "{}{}",
        "━".repeat(filled.min(width)),
        "─".repeat(width.saturating_sub(filled))
    )
}

fn style_active(plain: &str, color: bool) -> String {
    ui::accent_when(color, plain)
}

fn style_supporting(plain: &str, color: bool) -> String {
    ui::info_when(color, plain)
}

fn style_status_line(plain: &str, symbol: &str, degraded: bool, color: bool) -> String {
    let styled = if degraded {
        ui::warning_when(color, symbol)
    } else {
        ui::success_when(color, symbol)
    };
    plain.replacen(symbol, &styled, 1)
}

fn scope_label(scope: axon_api::source::SourceScope) -> &'static str {
    match scope {
        axon_api::source::SourceScope::Page => "page",
        axon_api::source::SourceScope::Site => "site",
        axon_api::source::SourceScope::Docs => "docs",
        axon_api::source::SourceScope::Repo => "repo",
        axon_api::source::SourceScope::Workspace => "workspace",
        axon_api::source::SourceScope::Branch => "branch",
        axon_api::source::SourceScope::Org => "org",
        axon_api::source::SourceScope::Package => "package",
        axon_api::source::SourceScope::Version => "version",
        axon_api::source::SourceScope::Feed => "feed",
        axon_api::source::SourceScope::Subreddit => "subreddit",
        axon_api::source::SourceScope::Thread => "thread",
        axon_api::source::SourceScope::Comment => "comment",
        axon_api::source::SourceScope::Video => "video",
        axon_api::source::SourceScope::Playlist => "playlist",
        axon_api::source::SourceScope::Channel => "channel",
        axon_api::source::SourceScope::Issue => "issue",
        axon_api::source::SourceScope::PullRequest => "pull request",
        axon_api::source::SourceScope::MergeRequest => "merge request",
        axon_api::source::SourceScope::Release => "release",
        axon_api::source::SourceScope::Wiki => "wiki",
        axon_api::source::SourceScope::File => "file",
        axon_api::source::SourceScope::Directory => "directory",
        axon_api::source::SourceScope::Map => "map",
        axon_api::source::SourceScope::Tool => "tool",
        axon_api::source::SourceScope::Script => "script",
        axon_api::source::SourceScope::Api => "api",
    }
}

fn format_rate(rate: f64) -> String {
    if rate < 10.0 {
        format!("{rate:.1}")
    } else {
        format!("{rate:.0}")
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds < 10.0 {
        format!("{seconds:.1}s")
    } else {
        format!("{:.0}s", seconds)
    }
}

pub(crate) fn sanitize_terminal_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect()
}

pub(crate) fn middle_truncate(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 1 {
        return "…".chars().take(max_chars).collect();
    }
    let prefix = (max_chars - 1) / 2;
    let suffix = max_chars - prefix - 1;
    format!(
        "{}…{}",
        chars[..prefix].iter().collect::<String>(),
        chars[chars.len() - suffix..].iter().collect::<String>()
    )
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
