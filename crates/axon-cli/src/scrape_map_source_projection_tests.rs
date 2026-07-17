use super::{JobCommandMode, command_needs_workers, job_command_mode};
use crate::commands::source::build_source_request;
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
fn map_projection_needs_source_workers_to_publish_manifest() {
    let cfg = cfg(CommandKind::Map, &["https://example.com"], false);
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

#[test]
fn scrape_projects_to_page_source_request_with_embedding() {
    let cfg = cfg(CommandKind::Scrape, &["https://example.test/intro"], false);

    let request = build_source_request(&cfg, cfg.positional[0].clone()).expect("source request");

    assert_eq!(request.source, "https://example.test/intro");
    assert_eq!(request.scope, Some(axon_api::source::SourceScope::Page));
    assert!(request.embed);
    assert_eq!(request.limits.max_items, Some(1));
    assert_eq!(request.limits.max_pages, Some(1));
    assert_eq!(request.limits.max_depth, Some(0));
}

#[test]
fn scrape_no_embed_is_only_source_embed_false() {
    let mut cfg = cfg(CommandKind::Scrape, &["https://example.test/intro"], false);
    cfg.embed = false;

    let request = build_source_request(&cfg, cfg.positional[0].clone()).expect("source request");

    assert_eq!(request.scope, Some(axon_api::source::SourceScope::Page));
    assert!(!request.embed);
}

#[test]
fn scrape_output_path_requests_clean_content_artifact() {
    let mut cfg = cfg(CommandKind::Scrape, &["https://example.test/intro"], false);
    cfg.output_path = Some("/tmp/axon-scrape-output.md".into());

    let request = build_source_request(&cfg, cfg.positional[0].clone()).expect("source request");

    assert_eq!(request.scope, Some(axon_api::source::SourceScope::Page));
    assert_eq!(
        request.output.artifact_mode,
        axon_api::source::ArtifactMode::Always
    );
}
