use super::super::cli::{JobSubcommand, WatchSubcommand};
use std::env;

/// Read an environment variable as a boolean flag with a default fallback.
///
/// Recognized truthy values: `"1"`, `"true"`, `"yes"`, `"y"`, `"on"`.
/// Recognized falsy values: `"0"`, `"false"`, `"no"`, `"n"`, `"off"`.
/// Unset, empty, or unrecognized values return `default`.
///
/// Single source of truth for boolean env-var parsing — do not duplicate.
pub(crate) fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key).ok().as_deref().map(|v| v.trim()) {
        None | Some("") => default,
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => true,
            "0" | "false" | "no" | "n" | "off" => false,
            _ => default,
        },
    }
}

pub(super) fn positional_from_job(job: JobSubcommand) -> Vec<String> {
    match job {
        JobSubcommand::Status { job_id } => vec!["status".to_string(), job_id],
        JobSubcommand::Cancel { job_id } => vec!["cancel".to_string(), job_id],
        JobSubcommand::Errors { job_id } => vec!["errors".to_string(), job_id],
        JobSubcommand::List => vec!["list".to_string()],
        JobSubcommand::Cleanup => vec!["cleanup".to_string()],
        JobSubcommand::Clear => vec!["clear".to_string()],
        JobSubcommand::Worker => vec!["worker".to_string()],
        JobSubcommand::Recover => vec!["recover".to_string()],
    }
}

pub(super) fn positional_from_watch_subcommand(action: WatchSubcommand) -> Vec<String> {
    match action {
        WatchSubcommand::Create {
            name,
            task_type,
            every_seconds,
            task_payload,
        } => {
            let mut positional = vec![
                "create".to_string(),
                name,
                "--task-type".to_string(),
                task_type,
                "--every-seconds".to_string(),
                every_seconds.to_string(),
            ];
            if let Some(payload) = task_payload {
                positional.push("--task-payload".to_string());
                positional.push(payload);
            }
            positional
        }
        WatchSubcommand::List => vec!["list".to_string()],
        WatchSubcommand::Get { id } => vec!["get".to_string(), id],
        WatchSubcommand::Update { id, every_seconds } => {
            let mut positional = vec!["update".to_string(), id];
            if let Some(v) = every_seconds {
                positional.push("--every-seconds".to_string());
                positional.push(v.to_string());
            }
            positional
        }
        WatchSubcommand::RunNow { id } => vec!["run-now".to_string(), id],
        WatchSubcommand::Pause { id } => vec!["pause".to_string(), id],
        WatchSubcommand::Resume { id } => vec!["resume".to_string(), id],
        WatchSubcommand::Delete { id } => vec!["delete".to_string(), id],
        WatchSubcommand::History { id, limit } => vec![
            "history".to_string(),
            id,
            "--limit".to_string(),
            limit.to_string(),
        ],
        WatchSubcommand::Artifacts { run_id, limit } => vec![
            "artifacts".to_string(),
            run_id,
            "--limit".to_string(),
            limit.to_string(),
        ],
    }
}

/// Parse a viewport string like "1920x1080" into (width, height).
/// Falls back to (1920, 1080) on any parse failure.
pub(super) fn parse_viewport(s: &str) -> (u32, u32) {
    const DEFAULT: (u32, u32) = (1920, 1080);
    let Some((w, h)) = s.split_once('x') else {
        return DEFAULT;
    };
    match (w.trim().parse::<u32>(), h.trim().parse::<u32>()) {
        (Ok(w), Ok(h)) if w > 0 && h > 0 => (w, h),
        _ => DEFAULT,
    }
}
