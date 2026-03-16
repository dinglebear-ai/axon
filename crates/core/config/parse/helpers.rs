use super::super::cli::{
    GraphSubcommand, JobSubcommand, RefreshScheduleSubcommand, RefreshSubcommand, WatchSubcommand,
};

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

pub(super) fn positional_from_graph_subcommand(action: GraphSubcommand) -> Vec<String> {
    match action {
        GraphSubcommand::Build {
            url,
            url_flag,
            domain,
            all,
        } => {
            let mut positional = vec!["build".to_string()];
            if let Some(url) = url_flag.or(url) {
                positional.push("--url".to_string());
                positional.push(url);
            }
            if let Some(domain) = domain {
                positional.push("--domain".to_string());
                positional.push(domain);
            }
            if all {
                positional.push("--all".to_string());
            }
            positional
        }
        GraphSubcommand::Status => vec!["status".to_string()],
        GraphSubcommand::Explore { entity } => vec!["explore".to_string(), entity],
        GraphSubcommand::Stats => vec!["stats".to_string()],
        GraphSubcommand::Worker => vec!["worker".to_string()],
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

pub(super) fn positional_from_refresh_subcommand(action: RefreshSubcommand) -> Vec<String> {
    match action {
        RefreshSubcommand::Status { job_id } => vec!["status".to_string(), job_id],
        RefreshSubcommand::Cancel { job_id } => vec!["cancel".to_string(), job_id],
        RefreshSubcommand::Errors { job_id } => vec!["errors".to_string(), job_id],
        RefreshSubcommand::List => vec!["list".to_string()],
        RefreshSubcommand::Cleanup => vec!["cleanup".to_string()],
        RefreshSubcommand::Clear => vec!["clear".to_string()],
        RefreshSubcommand::Worker => vec!["worker".to_string()],
        RefreshSubcommand::Recover => vec!["recover".to_string()],
        RefreshSubcommand::Schedule { action } => positional_from_refresh_schedule(action),
    }
}

fn positional_from_refresh_schedule(action: RefreshScheduleSubcommand) -> Vec<String> {
    match action {
        RefreshScheduleSubcommand::Add {
            name,
            seed_url,
            every_seconds,
            tier,
            urls,
        } => {
            let mut positional = vec!["schedule".to_string(), "add".to_string(), name];
            if let Some(every_seconds) = every_seconds {
                positional.push("--every-seconds".to_string());
                positional.push(every_seconds.to_string());
            }
            if let Some(tier) = tier {
                positional.push("--tier".to_string());
                positional.push(tier);
            }
            if let Some(seed_url) = seed_url {
                positional.push(seed_url);
            }
            if let Some(urls) = urls {
                positional.push("--urls".to_string());
                positional.push(urls);
            }
            positional
        }
        RefreshScheduleSubcommand::List => vec!["schedule".to_string(), "list".to_string()],
        RefreshScheduleSubcommand::Enable { name } => {
            vec!["schedule".to_string(), "enable".to_string(), name]
        }
        RefreshScheduleSubcommand::Disable { name } => {
            vec!["schedule".to_string(), "disable".to_string(), name]
        }
        RefreshScheduleSubcommand::Delete { name } => {
            vec!["schedule".to_string(), "delete".to_string(), name]
        }
        RefreshScheduleSubcommand::Worker => vec!["schedule".to_string(), "worker".to_string()],
        RefreshScheduleSubcommand::RunDue { batch } => vec![
            "schedule".to_string(),
            "run-due".to_string(),
            "--batch".to_string(),
            batch.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_schedule_worker_maps_to_positional_worker() {
        let positional = positional_from_refresh_subcommand(RefreshSubcommand::Schedule {
            action: RefreshScheduleSubcommand::Worker,
        });
        assert_eq!(positional, vec!["schedule", "worker"]);
    }
}
