use super::{JobCommandMode, command_needs_workers, job_command_mode};
use axon_core::config::{CommandKind, Config};

fn cfg(command: CommandKind, positional: &[&str], wait: bool) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = command;
    cfg.positional = positional.iter().map(|value| value.to_string()).collect();
    cfg.wait = wait;
    cfg
}

#[test]
fn scrape_map_source_projection_needs_workers_without_wait() {
    let cfg = cfg(CommandKind::Scrape, &["https://example.com"], false);
    let command_mode = job_command_mode(&cfg);

    assert_eq!(command_mode, None);
    assert!(command_needs_workers(&cfg, command_mode));
}

#[test]
fn scrape_map_source_projection_keeps_extract_job_detection() {
    let cfg = cfg(CommandKind::Extract, &["worker"], false);
    let command_mode = job_command_mode(&cfg);

    assert_eq!(
        command_mode,
        Some(JobCommandMode::Subcommand {
            name: "worker",
            needs_workers: true,
        })
    );
    assert!(command_needs_workers(&cfg, command_mode));
}
