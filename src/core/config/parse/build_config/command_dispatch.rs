//! Translates `clap`-parsed `CliCommand` variants into `(CommandKind, positional)`
//! plus per-command argument accumulators consumed by `into_config()`.
//!
//! Split out of `build_config.rs` (bead axon_rust-2j9.6) to keep the orchestration
//! shim small and the 28-arm match arm in its own module. No behavior change.

use super::super::super::cli::CliCommand;
use super::super::super::types::{
    CommandKind, EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime,
};
use super::super::helpers::{positional_from_job, positional_from_watch_subcommand};
use clap::ValueEnum;
use std::env;

/// Per-command arg accumulators populated by the dispatch match.
/// Defaults match the previous in-line `let mut` initializers in `into_config()`.
pub(super) struct DispatchOutput {
    pub command: CommandKind,
    pub positional: Vec<String>,
    pub ask_diagnostics: bool,
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
}

impl DispatchOutput {
    fn defaults() -> Self {
        Self {
            command: CommandKind::Doctor, // overwritten by every match arm
            positional: Vec::new(),
            ask_diagnostics: false,
            evaluate_responses_mode: EvaluateResponsesMode::Inline,
            evaluate_retrieval_ab: false,
            github_include_source: true,
            github_max_issues: env::var("GITHUB_MAX_ISSUES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            github_max_prs: env::var("GITHUB_MAX_PRS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
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
            out.positional = args.value.into_iter().collect::<Vec<String>>();
        }
        CliCommand::Extract(args) => {
            out.command = CommandKind::Extract;
            out.positional = if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.positional_urls
            };
        }
        CliCommand::Search(args) => {
            out.command = CommandKind::Search;
            out.positional = args.value;
        }
        CliCommand::Research(args) => {
            out.command = CommandKind::Research;
            out.positional = args.value;
        }
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
            set_simple(
                &mut out,
                CommandKind::Retrieve,
                args.value.into_iter().collect(),
            );
        }
        CliCommand::Ask(args) => {
            out.ask_diagnostics = args.diagnostics;
            set_simple(&mut out, CommandKind::Ask, args.value);
        }
        CliCommand::Evaluate(args) => {
            out.ask_diagnostics = args.diagnostics;
            out.evaluate_responses_mode = args.responses_mode;
            out.evaluate_retrieval_ab = args.retrieval_ab;
            set_simple(&mut out, CommandKind::Evaluate, args.value);
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
    }
    out
}

fn set_simple(out: &mut DispatchOutput, kind: CommandKind, positional: Vec<String>) {
    out.command = kind;
    out.positional = positional;
}

fn apply_ingest(out: &mut DispatchOutput, args: super::super::super::cli::IngestArgs) {
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

fn apply_sessions(out: &mut DispatchOutput, args: super::super::super::cli::SessionsArgs) {
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

fn apply_serve(out: &mut DispatchOutput, args: super::super::super::cli::ServeArgs) {
    match args.target {
        Some(super::super::super::cli::ServeSubcommand::Mcp(mcp_args)) => {
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

fn apply_setup(out: &mut DispatchOutput, args: super::super::super::cli::SetupArgs) {
    out.command = CommandKind::Setup;
    match args.action {
        super::super::super::cli::SetupSubcommand::Targets => {
            out.positional = vec!["targets".to_string()];
        }
        super::super::super::cli::SetupSubcommand::Deploy {
            target,
            remote_dir,
            public_exposure,
            accept_new_host_key,
        } => {
            let mut positional = vec![
                "deploy".to_string(),
                target,
                "--remote-dir".to_string(),
                remote_dir,
            ];
            if public_exposure {
                positional.push("--public-exposure".to_string());
            }
            if accept_new_host_key {
                positional.push("--accept-new-host-key".to_string());
            }
            out.positional = positional;
        }
    }
}
