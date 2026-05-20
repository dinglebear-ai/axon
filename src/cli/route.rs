use crate::core::config::{CommandKind, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRoute {
    LocalOnly,
    PreferServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackPolicy {
    AllowEquivalentLocal,
    AllowDegradedLocal,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRoutePlan {
    pub route: CommandRoute,
    pub fallback_policy: FallbackPolicy,
}

pub fn plan_command_route(cfg: &Config, positional: &[String]) -> Result<CommandRoutePlan, String> {
    if cfg.local_mode || cfg.server_url.is_none() {
        return Ok(CommandRoutePlan {
            route: CommandRoute::LocalOnly,
            fallback_policy: FallbackPolicy::AllowEquivalentLocal,
        });
    }

    if !is_server_mode_rest_command(cfg.command) {
        return Ok(CommandRoutePlan {
            route: CommandRoute::LocalOnly,
            fallback_policy: fallback_policy_for(cfg.command, positional),
        });
    }

    Ok(CommandRoutePlan {
        route: CommandRoute::PreferServer,
        fallback_policy: fallback_policy_for(cfg.command, positional),
    })
}

fn is_server_mode_rest_command(command: CommandKind) -> bool {
    matches!(
        command,
        CommandKind::Status
            | CommandKind::Scrape
            | CommandKind::Summarize
            | CommandKind::Crawl
            | CommandKind::Extract
            | CommandKind::Embed
            | CommandKind::Ingest
            | CommandKind::Sessions
    )
}

fn fallback_policy_for(command: CommandKind, positional: &[String]) -> FallbackPolicy {
    match command {
        CommandKind::Crawl | CommandKind::Extract | CommandKind::Embed | CommandKind::Ingest => {
            if is_job_lifecycle_or_worker(positional) {
                FallbackPolicy::Disallow
            } else if command == CommandKind::Ingest {
                FallbackPolicy::AllowDegradedLocal
            } else {
                FallbackPolicy::AllowEquivalentLocal
            }
        }
        CommandKind::Scrape
        | CommandKind::Summarize
        | CommandKind::Map
        | CommandKind::Query
        | CommandKind::Retrieve
        | CommandKind::Sources
        | CommandKind::Domains
        | CommandKind::Stats
        | CommandKind::Sessions
        | CommandKind::Screenshot
        | CommandKind::Doctor
        | CommandKind::Search => FallbackPolicy::AllowEquivalentLocal,
        CommandKind::Research
        | CommandKind::Debug
        | CommandKind::Ask
        | CommandKind::Evaluate
        | CommandKind::Suggest => FallbackPolicy::AllowDegradedLocal,
        CommandKind::Dedupe
        | CommandKind::Migrate
        | CommandKind::Watch
        | CommandKind::Config
        | CommandKind::Sync => FallbackPolicy::Disallow,
        CommandKind::Completions
        | CommandKind::Mcp
        | CommandKind::Serve
        | CommandKind::Setup
        | CommandKind::Preflight
        | CommandKind::Smoke
        | CommandKind::Stack
        | CommandKind::Train
        | CommandKind::Status => FallbackPolicy::Disallow,
    }
}

fn is_job_lifecycle_or_worker(positional: &[String]) -> bool {
    matches!(
        positional.first().map(String::as_str),
        Some("status" | "errors" | "cancel" | "list" | "cleanup" | "clear" | "recover" | "worker")
    )
}
