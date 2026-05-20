use super::route::{CommandRoute, FallbackPolicy, plan_command_route};
use crate::core::config::{CommandKind, Config};

fn cfg(command: CommandKind) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.command = command;
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg
}

#[test]
fn crawl_start_can_fallback_local() {
    let cfg = cfg(CommandKind::Crawl);
    let plan = plan_command_route(&cfg, &["https://example.com".to_string()]).expect("route plan");

    assert_eq!(plan.route, CommandRoute::PreferServer);
    assert_eq!(plan.fallback_policy, FallbackPolicy::AllowEquivalentLocal);
}

#[test]
fn migrate_remains_local_until_server_mode_plan_support_exists() {
    let cfg = cfg(CommandKind::Migrate);
    let plan = plan_command_route(&cfg, &[]).expect("route plan");

    assert_eq!(plan.route, CommandRoute::LocalOnly);
    assert_eq!(plan.fallback_policy, FallbackPolicy::Disallow);
}

#[test]
fn rest_capable_read_and_llm_commands_prefer_server_mode() {
    for command in [
        CommandKind::Map,
        CommandKind::Query,
        CommandKind::Retrieve,
        CommandKind::Sources,
        CommandKind::Domains,
        CommandKind::Stats,
        CommandKind::Doctor,
        CommandKind::Search,
        CommandKind::Research,
        CommandKind::Ask,
        CommandKind::Evaluate,
        CommandKind::Suggest,
    ] {
        let cfg = cfg(command);
        let plan = plan_command_route(&cfg, &["test".to_string()]).expect("route plan");

        assert_eq!(plan.route, CommandRoute::PreferServer, "{command:?}");
    }
}

#[test]
fn local_flag_forces_local() {
    let mut cfg = cfg(CommandKind::Crawl);
    cfg.local_mode = true;
    let plan = plan_command_route(&cfg, &["https://example.com".to_string()]).expect("route plan");

    assert_eq!(plan.route, CommandRoute::LocalOnly);
}
