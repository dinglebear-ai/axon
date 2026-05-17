//! Translates `clap`-parsed `CliCommand` variants into `(CommandKind, positional)`
//! plus per-command argument accumulators consumed by `into_config()`.
//!
//! Split out of `build_config.rs` (bead axon_rust-2j9.6) to keep the orchestration
//! shim small and the 28-arm match arm in its own module. No behavior change.

use super::super::super::cli::{
    CliCommand, ConfigArgs, ConfigSubcommand, IngestArgs, ServeArgs, ServeSubcommand, SessionsArgs,
    SetupArgs, SetupSubcommand,
};
use super::super::super::types::{
    CommandKind, EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime,
};
use super::super::helpers::{positional_from_job, positional_from_watch_subcommand};
use clap::ValueEnum;
use std::env;

fn env_usize_or(var: &str, default: usize) -> usize {
    env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Per-command arg accumulators populated by the dispatch match.
/// Defaults match the previous in-line `let mut` initializers in `into_config()`.
pub(super) struct DispatchOutput {
    pub command: CommandKind,
    pub positional: Vec<String>,
    pub ask_diagnostics: bool,
    pub ask_explain: bool,
    pub ask_stream: bool,
    pub ask_follow_up: bool,
    pub ask_session: Option<String>,
    pub ask_reset_session: bool,
    pub evaluate_responses_mode: EvaluateResponsesMode,
    pub evaluate_retrieval_ab: bool,
    pub github_include_source: bool,
    pub github_max_issues: usize,
    pub github_max_prs: usize,
    pub reddit_sort: RedditSort,
    pub reddit_time: RedditTime,
    pub reddit_max_posts: usize,
    pub reddit_min_score: i32,
    pub reddit_depth: usize,
    pub reddit_scrape_links: bool,
    pub sessions_claude: bool,
    pub sessions_codex: bool,
    pub sessions_gemini: bool,
    pub sessions_project: Option<String>,
    pub mcp_transport: Option<McpTransport>,
    pub mcp_transport_default: McpTransport,
    pub map_fallback: MapFallback,
    pub retrieve_max_points: Option<usize>,
    pub train_best_rank: Option<usize>,
    pub train_notes: Option<String>,
}

impl DispatchOutput {
    fn defaults() -> Self {
        Self {
            command: CommandKind::Doctor, // overwritten by every match arm
            positional: Vec::new(),
            ask_diagnostics: false,
            ask_explain: false,
            ask_stream: false,
            ask_follow_up: false,
            ask_session: None,
            ask_reset_session: false,
            evaluate_responses_mode: EvaluateResponsesMode::Inline,
            evaluate_retrieval_ab: false,
            github_include_source: true,
            github_max_issues: env_usize_or("GITHUB_MAX_ISSUES", 100),
            github_max_prs: env_usize_or("GITHUB_MAX_PRS", 100),
            reddit_sort: RedditSort::Hot,
            reddit_time: RedditTime::Day,
            reddit_max_posts: 25,
            reddit_min_score: 0,
            reddit_depth: 2,
            reddit_scrape_links: false,
            sessions_claude: false,
            sessions_codex: false,
            sessions_gemini: false,
            sessions_project: None,
            mcp_transport: None,
            mcp_transport_default: McpTransport::Http,
            map_fallback: MapFallback::Structure,
            retrieve_max_points: None,
            train_best_rank: None,
            train_notes: None,
        }
    }
}

/// Splits the `CliCommand` match into its 28 arms, returning per-command
/// accumulators. Pure translation — no env reads beyond the GitHub max
/// counters (which already lived here).
pub(super) fn dispatch(cli_command: CliCommand) -> DispatchOutput {
    let mut out = DispatchOutput::defaults();
    match cli_command {
        CliCommand::Scrape(args) => {
            out.command = CommandKind::Scrape;
            out.positional = args.positional_urls;
        }
        CliCommand::Crawl(args) => {
            out.command = CommandKind::Crawl;
            out.positional = if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.positional_urls
            };
        }
        CliCommand::Watch(args) => {
            out.command = CommandKind::Watch;
            out.positional = if let Some(action) = args.action {
                positional_from_watch_subcommand(action)
            } else {
                vec!["list".to_string()]
            };
        }
        CliCommand::Map(args) => {
            if let Some(fb) = args.map_fallback {
                out.map_fallback = fb;
            }
            out.command = CommandKind::Map;
            out.positional = args.value.into_iter().collect();
        }
        CliCommand::Extract(args) => {
            out.command = CommandKind::Extract;
            out.positional = if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.positional_urls
            };
        }
        CliCommand::Search(args) => set_simple(&mut out, CommandKind::Search, args.value),
        CliCommand::Research(args) => set_simple(&mut out, CommandKind::Research, args.value),
        CliCommand::Embed(args) => {
            out.command = CommandKind::Embed;
            out.positional = if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.input.into_iter().collect()
            };
        }
        CliCommand::Debug(args) => set_simple(&mut out, CommandKind::Debug, args.value),
        CliCommand::Doctor => out.command = CommandKind::Doctor,
        CliCommand::Query(args) => {
            out.ask_diagnostics = args.diagnostics;
            set_simple(&mut out, CommandKind::Query, args.value);
        }
        CliCommand::Retrieve(args) => {
            out.retrieve_max_points = args.max_points;
            set_simple(
                &mut out,
                CommandKind::Retrieve,
                args.value.into_iter().collect(),
            );
        }
        CliCommand::Ask(args) => {
            out.ask_explain = args.explain;
            out.ask_stream = !args.no_stream && !args.explain;
            out.ask_follow_up = args.follow_up;
            out.ask_session = args.session;
            out.ask_reset_session = args.reset_session;
            out.ask_diagnostics = args.diagnostics || args.explain;
            set_simple(&mut out, CommandKind::Ask, args.value);
        }
        CliCommand::Evaluate(args) => {
            out.ask_diagnostics = args.diagnostics;
            out.evaluate_responses_mode = args.responses_mode;
            out.evaluate_retrieval_ab = args.retrieval_ab;
            set_simple(&mut out, CommandKind::Evaluate, args.value);
        }
        CliCommand::Train(args) => {
            out.train_best_rank = args.best_rank;
            out.train_notes = args.notes;
            out.ask_diagnostics = true;
            out.ask_explain = true;
            set_simple(&mut out, CommandKind::Train, args.value);
        }
        CliCommand::Suggest(args) => set_simple(&mut out, CommandKind::Suggest, args.value),
        CliCommand::Sources => out.command = CommandKind::Sources,
        CliCommand::Domains => out.command = CommandKind::Domains,
        CliCommand::Stats => out.command = CommandKind::Stats,
        CliCommand::Status => out.command = CommandKind::Status,
        CliCommand::Dedupe => out.command = CommandKind::Dedupe,
        CliCommand::Ingest(args) => apply_ingest(&mut out, args),
        CliCommand::Sessions(args) => apply_sessions(&mut out, args),
        CliCommand::Screenshot(args) => {
            out.command = CommandKind::Screenshot;
            out.positional = args.positional_urls;
        }
        CliCommand::Completions(args) => {
            out.command = CommandKind::Completions;
            out.positional = vec![
                args.shell
                    .to_possible_value()
                    .expect("shell value")
                    .get_name()
                    .to_string(),
            ];
        }
        CliCommand::Serve(args) => apply_serve(&mut out, args),
        CliCommand::Setup(args) => apply_setup(&mut out, args),
        CliCommand::Mcp(args) => {
            out.mcp_transport = args.transport;
            out.mcp_transport_default = McpTransport::Stdio;
            out.command = CommandKind::Mcp;
        }
        CliCommand::Migrate(args) => {
            out.command = CommandKind::Migrate;
            out.positional = vec![args.from, args.to];
        }
        CliCommand::Config(args) => apply_config(&mut out, args),
    }
    out
}

fn set_simple(out: &mut DispatchOutput, kind: CommandKind, positional: Vec<String>) {
    out.command = kind;
    out.positional = positional;
}

fn apply_ingest(out: &mut DispatchOutput, args: IngestArgs) {
    // --no-source overrides the default (true). --include-source is now a no-op.
    if args.no_source {
        out.github_include_source = false;
    }
    out.github_max_issues = args.max_issues;
    out.github_max_prs = args.max_prs;
    out.reddit_sort = args.sort;
    out.reddit_time = args.time;
    out.reddit_max_posts = args.max_posts;
    out.reddit_min_score = args.min_score;
    out.reddit_depth = args.depth;
    out.reddit_scrape_links = args.scrape_links;
    out.command = CommandKind::Ingest;
    out.positional = if let Some(job) = args.job {
        positional_from_job(job)
    } else {
        args.target.into_iter().collect()
    };
}

fn apply_sessions(out: &mut DispatchOutput, args: SessionsArgs) {
    out.sessions_claude = args.claude;
    out.sessions_codex = args.codex;
    out.sessions_gemini = args.gemini;
    out.sessions_project = args.project;
    out.command = CommandKind::Sessions;
    out.positional = if let Some(job) = args.job {
        positional_from_job(job)
    } else {
        Vec::new()
    };
}

fn apply_serve(out: &mut DispatchOutput, args: ServeArgs) {
    match args.target {
        Some(ServeSubcommand::Mcp(mcp_args)) => {
            out.mcp_transport = mcp_args.transport;
            out.mcp_transport_default = McpTransport::Http;
            out.command = CommandKind::Mcp;
        }
        None => {
            out.mcp_transport = Some(McpTransport::Both);
            out.mcp_transport_default = McpTransport::Both;
            out.command = CommandKind::Serve;
        }
    }
}

fn apply_config(out: &mut DispatchOutput, args: ConfigArgs) {
    out.command = CommandKind::Config;
    match args.action {
        None => out.positional = vec!["list".to_string()],
        Some(ConfigSubcommand::List { env, toml, reveal }) => {
            out.positional = vec!["list".to_string()];
            if env {
                out.positional.push("--env".to_string());
            }
            if toml {
                out.positional.push("--toml".to_string());
            }
            if reveal {
                out.positional.push("--reveal".to_string());
            }
        }
        Some(ConfigSubcommand::Get {
            key,
            env,
            toml,
            reveal,
        }) => {
            out.positional = vec!["get".to_string(), key];
            if env {
                out.positional.push("--env".to_string());
            }
            if toml {
                out.positional.push("--toml".to_string());
            }
            if reveal {
                out.positional.push("--reveal".to_string());
            }
        }
        Some(ConfigSubcommand::Set {
            key,
            value,
            env,
            toml,
        }) => {
            out.positional = vec!["set".to_string(), key, value];
            if env {
                out.positional.push("--env".to_string());
            }
            if toml {
                out.positional.push("--toml".to_string());
            }
        }
        Some(ConfigSubcommand::Unset { key, env, toml }) => {
            out.positional = vec!["unset".to_string(), key];
            if env {
                out.positional.push("--env".to_string());
            }
            if toml {
                out.positional.push("--toml".to_string());
            }
        }
        Some(ConfigSubcommand::Path) => out.positional = vec!["path".to_string()],
    }
}

fn apply_setup(out: &mut DispatchOutput, args: SetupArgs) {
    out.command = CommandKind::Setup;
    match args.action {
        None => {}
        Some(SetupSubcommand::PluginHook { no_repair }) => {
            out.positional = vec!["plugin-hook".to_string()];
            if no_repair {
                out.positional.push("--no-repair".to_string());
            }
        }
        Some(SetupSubcommand::Check) => {
            out.positional = vec!["check".to_string()];
        }
        Some(SetupSubcommand::Repair { migrate_env }) => {
            out.positional = vec!["repair".to_string()];
            if migrate_env {
                out.positional.push("--migrate-env".to_string());
            }
        }
        Some(SetupSubcommand::Targets) => {
            out.positional = vec!["targets".to_string()];
        }
    }
}
