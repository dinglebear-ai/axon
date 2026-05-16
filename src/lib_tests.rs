use super::*;

fn cfg(command: CommandKind, positional: &[&str], wait: bool) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = command;
    cfg.positional = positional.iter().map(|value| value.to_string()).collect();
    cfg.wait = wait;
    cfg
}

#[test]
fn job_command_mode_detects_fire_and_forget_submit() {
    assert_eq!(
        job_command_mode(&cfg(CommandKind::Crawl, &["https://example.com"], false)),
        Some(JobCommandMode::Submit {
            fire_and_forget: true
        })
    );
}

#[test]
fn job_command_mode_detects_waiting_submit() {
    assert_eq!(
        job_command_mode(&cfg(CommandKind::Embed, &["./docs"], true)),
        Some(JobCommandMode::Submit {
            fire_and_forget: false
        })
    );
}

#[test]
fn job_command_mode_worker_subcommand_needs_workers() {
    assert_eq!(
        job_command_mode(&cfg(CommandKind::Ingest, &["worker"], false)),
        Some(JobCommandMode::Subcommand {
            name: "worker",
            needs_workers: true,
        })
    );
}

#[test]
fn job_command_mode_read_only_and_recover_subcommands_do_not_spawn_workers() {
    assert_eq!(
        job_command_mode(&cfg(CommandKind::Extract, &["list"], false)),
        Some(JobCommandMode::Subcommand {
            name: "list",
            needs_workers: false,
        })
    );
    assert_eq!(
        job_command_mode(&cfg(CommandKind::Crawl, &["recover"], false)),
        Some(JobCommandMode::Subcommand {
            name: "recover",
            needs_workers: false,
        })
    );
}

#[test]
fn job_command_mode_ignores_non_job_commands() {
    assert_eq!(
        job_command_mode(&cfg(CommandKind::Query, &["worker"], false)),
        None
    );
}
