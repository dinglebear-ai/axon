//! Translates `clap`-parsed `CliCommand` variants into `(CommandKind, positional)`
//! plus per-command argument accumulators consumed by `into_config()`.
//!
//! Split out of `build_config.rs` (bead axon_rust-2j9.6) to keep the orchestration
//! shim small and the 28-arm match arm in its own module. No behavior change.

use super::super::super::cli::{
    CliCommand, ComposeArgs, ComposeSubcommand, ConfigArgs, ConfigSubcommand, DoctorSubcommand,
    FreshSubcommand, MemoryCliSubcommand, MonitorSubcommand, PaletteArgs, ServeArgs,
    ServeSubcommand, SessionWatchServiceSubcommand, SessionsArgs, SessionsSubcommand, SetupArgs,
    SetupAuthMode, SetupInitArgs, SetupSubcommand, SourceArgs, SyncSubcommand, UpdateArgs,
};
use super::super::super::types::{
    CommandKind, EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime,
    SessionWatchConfig, SessionWatchServiceAction, SessionsRuntimeAction,
};
use super::super::super::types::{FreshAction, FreshnessRequest};
use super::super::helpers::{positional_from_job, positional_from_watch_subcommand};
use clap::ValueEnum;
use std::env;
use std::time::Duration;

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
    pub ask_new_session: bool,
    pub ask_list_sessions: bool,
    pub freshness: Option<FreshnessRequest>,
    pub fresh_action: Option<FreshAction>,
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
    pub sessions_watch: Option<SessionWatchConfig>,
    pub sessions_action: Option<SessionsRuntimeAction>,
    pub setup_session_watch_action: Option<SessionWatchServiceAction>,
    pub mcp_transport: Option<McpTransport>,
    pub mcp_transport_default: McpTransport,
    pub map_fallback: MapFallback,
    pub endpoints_include_bundles: bool,
    pub endpoints_first_party_only: bool,
    pub endpoints_unique_only: bool,
    pub endpoints_max_scripts: usize,
    pub endpoints_max_scan_bytes: usize,
    pub endpoints_verify: bool,
    pub endpoints_capture_network: bool,
    pub endpoints_probe_rpc: bool,
    pub endpoints_probe_rpc_subdomains: bool,
    pub retrieve_max_points: Option<usize>,
    pub train_best_rank: Option<usize>,
    pub train_notes: Option<String>,
    pub doctor_diagnose: bool,
    pub sources_domain: Option<String>,
    pub sources_domain_all: bool,
    pub domains_domain: Option<String>,
    pub purge_prefix: bool,
    pub purge_dry_run: bool,
    /// Binary acquisition method passed in by install.sh via `axon setup --method pull|build`
    pub setup_method: Option<String>,
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
            ask_new_session: false,
            ask_list_sessions: false,
            freshness: None,
            fresh_action: None,
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
            sessions_watch: None,
            sessions_action: None,
            setup_session_watch_action: None,
            mcp_transport: None,
            mcp_transport_default: McpTransport::Http,
            map_fallback: MapFallback::Structure,
            endpoints_include_bundles: true,
            endpoints_first_party_only: false,
            endpoints_unique_only: true,
            endpoints_max_scripts: 40,
            endpoints_max_scan_bytes: 8 * 1024 * 1024,
            endpoints_verify: false,
            endpoints_capture_network: false,
            endpoints_probe_rpc: false,
            endpoints_probe_rpc_subdomains: false,
            retrieve_max_points: None,
            train_best_rank: None,
            train_notes: None,
            doctor_diagnose: false,
            sources_domain: None,
            sources_domain_all: false,
            domains_domain: None,
            purge_prefix: false,
            purge_dry_run: false,
            setup_method: None,
        }
    }
}

/// Splits the `CliCommand` match into its 28 arms, returning per-command
/// accumulators. Pure translation — no env reads beyond the GitHub max
/// counters (which already lived here).
pub(super) fn dispatch(cli_command: CliCommand) -> DispatchOutput {
    let mut out = DispatchOutput::defaults();
    match cli_command {
        CliCommand::Watch(args) => {
            out.command = CommandKind::Watch;
            out.positional = if let Some(action) = args.action {
                positional_from_watch_subcommand(action)
            } else {
                vec!["list".to_string()]
            };
        }
        CliCommand::Monitor(args) => apply_monitor(&mut out, args.action),
        CliCommand::Map(args) => {
            if let Some(fb) = args.map_fallback {
                out.map_fallback = fb;
            }
            out.command = CommandKind::Map;
            out.positional = args.value.into_iter().collect();
        }
        CliCommand::Endpoints(args) => {
            out.command = CommandKind::Endpoints;
            out.positional = vec![args.url];
            out.endpoints_include_bundles = args.include_bundles;
            out.endpoints_first_party_only = args.first_party_only;
            out.endpoints_unique_only = args.unique_only;
            out.endpoints_max_scripts = args.max_scripts;
            out.endpoints_max_scan_bytes = args.max_scan_bytes;
            out.endpoints_verify = args.verify;
            out.endpoints_capture_network = args.capture_network;
            out.endpoints_probe_rpc = args.probe_rpc;
            out.endpoints_probe_rpc_subdomains = args.probe_rpc_subdomains;
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
        CliCommand::Brand(args) => {
            out.command = CommandKind::Brand;
            out.positional = args.positional_urls;
        }
        CliCommand::Debug(args) => set_simple(&mut out, CommandKind::Debug, args.value),
        CliCommand::Diff(args) => {
            out.command = CommandKind::Diff;
            out.positional = vec![args.url_a, args.url_b];
        }
        CliCommand::Doctor(args) => {
            out.command = CommandKind::Doctor;
            out.doctor_diagnose = matches!(args.action, Some(DoctorSubcommand::Diagnose));
            if out.doctor_diagnose {
                out.positional = vec!["diagnose".to_string()];
            }
        }
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
            // `--resume NAME` is a thin alias for `--follow-up --session NAME`.
            let (follow_up, session) = match args.resume {
                Some(name) => (true, Some(name)),
                None => (args.follow_up, args.session),
            };
            out.ask_follow_up = follow_up;
            out.ask_session = session;
            out.ask_reset_session = args.reset_session;
            out.ask_new_session = args.new_session;
            out.ask_list_sessions = args.list_sessions;
            out.ask_diagnostics = args.diagnostics || args.explain;
            set_simple(&mut out, CommandKind::Ask, args.value);
        }
        CliCommand::Summarize(args) => {
            out.command = CommandKind::Summarize;
            out.positional = args.positional_urls;
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
        CliCommand::Sources(args) => {
            out.command = CommandKind::Sources;
            out.sources_domain = args.domain;
            out.sources_domain_all = args.all;
        }
        CliCommand::Domains(args) => {
            out.command = CommandKind::Domains;
            out.domains_domain = args.domain;
        }
        CliCommand::Stats => out.command = CommandKind::Stats,
        CliCommand::Status => out.command = CommandKind::Status,
        CliCommand::Dedupe => out.command = CommandKind::Dedupe,
        CliCommand::Purge(args) => {
            out.command = CommandKind::Purge;
            out.positional = vec![args.url];
            out.purge_prefix = args.prefix;
            out.purge_dry_run = args.dry_run;
        }
        CliCommand::Refresh(args) => {
            out.command = CommandKind::Refresh;
            out.positional = args.filter.into_iter().collect();
        }
        CliCommand::Fresh(args) => {
            out.command = CommandKind::Fresh;
            out.fresh_action = Some(fresh_action_from_subcommand(args.action));
        }
        CliCommand::Memory(args) => apply_memory(&mut out, args.action),
        CliCommand::Sessions(args) => apply_sessions(&mut out, args),
        CliCommand::Source(args) => apply_source(&mut out, args),
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
        CliCommand::Preflight => out.command = CommandKind::Preflight,
        CliCommand::Smoke => out.command = CommandKind::Smoke,
        CliCommand::Compose(args) => apply_compose(&mut out, args),
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
        CliCommand::Sync(args) => {
            out.command = CommandKind::Sync;
            out.positional = match args.action.unwrap_or(SyncSubcommand::Pending) {
                SyncSubcommand::Pending => vec!["pending".to_string()],
            };
        }
        CliCommand::Update(args) => apply_update(&mut out, args),
        CliCommand::Palette(args) => apply_palette(&mut out, args),
    }
    out
}

fn set_simple(out: &mut DispatchOutput, kind: CommandKind, positional: Vec<String>) {
    out.command = kind;
    out.positional = positional;
}

fn apply_monitor(out: &mut DispatchOutput, action: MonitorSubcommand) {
    out.command = CommandKind::Monitor;
    match action {
        MonitorSubcommand::Jobs(args) => {
            let mut positional = vec!["jobs".to_string()];
            if args.watch {
                positional.push("--watch".to_string());
            }
            if args.jsonl {
                positional.push("--jsonl".to_string());
            }
            positional.push("--interval-secs".to_string());
            positional.push(args.interval_secs.to_string());
            if let Some(state_file) = args.state_file {
                positional.push("--state-file".to_string());
                positional.push(state_file);
            }
            out.positional = positional;
        }
    }
}

fn apply_update(out: &mut DispatchOutput, args: UpdateArgs) {
    out.command = CommandKind::Update;
    if let Some(version) = args.version {
        out.positional.push("--version".to_string());
        out.positional.push(version);
    }
    if args.repo != "jmagar/axon" {
        out.positional.push("--repo".to_string());
        out.positional.push(args.repo);
    }
    if args.no_container {
        out.positional.push("--no-container".to_string());
    }
    if args.force {
        out.positional.push("--force".to_string());
    }
}

fn apply_memory(out: &mut DispatchOutput, action: MemoryCliSubcommand) {
    out.command = CommandKind::Memory;
    match action {
        MemoryCliSubcommand::Remember {
            body,
            title,
            memory_type,
            project,
            repo,
            file,
            confidence,
        } => {
            out.positional = vec!["remember".to_string(), body.join(" ")];
            push_opt(&mut out.positional, "--title", title);
            push_opt(&mut out.positional, "--type", memory_type);
            push_opt(&mut out.positional, "--project", project);
            push_opt(&mut out.positional, "--repo", repo);
            push_opt(&mut out.positional, "--file", file);
            if let Some(confidence) = confidence {
                out.positional.push("--confidence".to_string());
                out.positional.push(confidence.to_string());
            }
        }
        MemoryCliSubcommand::List {
            project,
            repo,
            file,
            memory_type,
            status,
            limit,
        } => {
            out.positional = vec!["list".to_string()];
            push_opt(&mut out.positional, "--project", project);
            push_opt(&mut out.positional, "--repo", repo);
            push_opt(&mut out.positional, "--file", file);
            push_opt(&mut out.positional, "--type", memory_type);
            push_opt(&mut out.positional, "--status", status);
            if let Some(limit) = limit {
                out.positional.push("--limit".to_string());
                out.positional.push(limit.to_string());
            }
        }
        MemoryCliSubcommand::Search {
            query,
            project,
            repo,
            file,
            limit,
        } => {
            out.positional = vec!["search".to_string(), query.join(" ")];
            push_opt(&mut out.positional, "--project", project);
            push_opt(&mut out.positional, "--repo", repo);
            push_opt(&mut out.positional, "--file", file);
            if let Some(limit) = limit {
                out.positional.push("--limit".to_string());
                out.positional.push(limit.to_string());
            }
        }
        MemoryCliSubcommand::Show { id } => {
            out.positional = vec!["show".to_string(), id];
        }
        MemoryCliSubcommand::Link {
            source_id,
            target_id,
            edge_type,
        } => {
            out.positional = vec!["link".to_string(), source_id, target_id];
            push_opt(&mut out.positional, "--type", edge_type);
        }
        MemoryCliSubcommand::Supersede {
            replacement_id,
            old_id,
        } => {
            out.positional = vec!["supersede".to_string(), replacement_id, old_id];
        }
        MemoryCliSubcommand::Context {
            query,
            project,
            repo,
            file,
            limit,
            token_budget,
        } => {
            out.positional = vec!["context".to_string()];
            push_opt(&mut out.positional, "--query", query);
            push_opt(&mut out.positional, "--project", project);
            push_opt(&mut out.positional, "--repo", repo);
            push_opt(&mut out.positional, "--file", file);
            if let Some(limit) = limit {
                out.positional.push("--limit".to_string());
                out.positional.push(limit.to_string());
            }
            if let Some(token_budget) = token_budget {
                out.positional.push("--token-budget".to_string());
                out.positional.push(token_budget.to_string());
            }
        }
    }
}

fn apply_source(out: &mut DispatchOutput, args: SourceArgs) {
    out.command = CommandKind::Source;
    out.positional = args.path.into_iter().collect();
}

fn apply_sessions(out: &mut DispatchOutput, args: SessionsArgs) {
    out.command = CommandKind::Sessions;
    out.sessions_claude = args.claude;
    out.sessions_codex = args.codex;
    out.sessions_gemini = args.gemini;
    out.sessions_project = args.project;
    match args.action {
        Some(SessionsSubcommand::Watch(watch)) => {
            out.sessions_claude = watch.claude;
            out.sessions_codex = watch.codex;
            out.sessions_gemini = watch.gemini;
            out.sessions_project = watch.project;
            out.sessions_watch = Some(SessionWatchConfig {
                path: watch.path,
                debounce: Duration::from_millis(watch.debounce_ms),
                settle: Duration::from_millis(watch.settle_ms),
                max_retries: watch.max_retries,
                max_batch_docs: watch.max_batch_docs,
                max_processing_concurrency: watch.max_processing_concurrency,
                rescan_cooldown: Duration::from_millis(watch.rescan_cooldown_ms),
                initial_scan: !watch.no_initial_scan,
                upload_to_server: watch.upload_to_server,
                upload_server_url: None,
                upload_token: None,
                verbose_paths: watch.verbose_paths,
                json: watch.json,
            });
        }
        Some(SessionsSubcommand::WatchStatus { limit }) => {
            out.sessions_action = Some(SessionsRuntimeAction::WatchStatus { limit });
        }
        Some(SessionsSubcommand::SmokeWatch { timeout_secs }) => {
            out.sessions_action = Some(SessionsRuntimeAction::SmokeWatch { timeout_secs });
        }
        Some(job) => {
            if let Some(positional) = sessions_job_positionals(job) {
                out.positional = positional;
            }
        }
        None => {}
    }
}

fn sessions_job_positionals(job: SessionsSubcommand) -> Option<Vec<String>> {
    match job {
        SessionsSubcommand::Status { job_id } => Some(vec!["status".to_string(), job_id]),
        SessionsSubcommand::Cancel { job_id } => Some(vec!["cancel".to_string(), job_id]),
        SessionsSubcommand::Errors { job_id } => Some(vec!["errors".to_string(), job_id]),
        SessionsSubcommand::List => Some(vec!["list".to_string()]),
        SessionsSubcommand::Cleanup => Some(vec!["cleanup".to_string()]),
        SessionsSubcommand::Clear => Some(vec!["clear".to_string()]),
        SessionsSubcommand::Worker => Some(vec!["worker".to_string()]),
        SessionsSubcommand::Recover => Some(vec!["recover".to_string()]),
        SessionsSubcommand::Watch(_)
        | SessionsSubcommand::WatchStatus { .. }
        | SessionsSubcommand::SmokeWatch { .. } => None,
    }
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
    if let Some(method) = args.method {
        out.setup_method = Some(method.as_str().to_string());
    }
    match args.action {
        None => {}
        Some(SetupSubcommand::PluginHook { no_setup }) => {
            out.positional = vec!["plugin-hook".to_string()];
            if no_setup {
                out.positional.push("--no-setup".to_string());
            }
        }
        Some(SetupSubcommand::Check) => {
            out.positional = vec!["preflight".to_string()];
        }
        Some(SetupSubcommand::Init(init)) => {
            let init = *init;
            out.positional = setup_init_positionals(init);
        }
        Some(SetupSubcommand::Install) => {
            out.positional = vec!["install".to_string()];
        }
        Some(SetupSubcommand::Targets) => {
            out.positional = vec!["targets".to_string()];
        }
        Some(SetupSubcommand::SessionWatchService { action }) => {
            out.setup_session_watch_action = Some(match action {
                SessionWatchServiceSubcommand::Install => SessionWatchServiceAction::Install,
                SessionWatchServiceSubcommand::Check => SessionWatchServiceAction::Check,
                SessionWatchServiceSubcommand::Remove => SessionWatchServiceAction::Remove,
                SessionWatchServiceSubcommand::Status => SessionWatchServiceAction::Status,
            });
        }
    }
}

fn apply_compose(out: &mut DispatchOutput, args: ComposeArgs) {
    out.command = CommandKind::Compose;
    out.positional = vec![
        match args.action {
            ComposeSubcommand::Up => "up",
            ComposeSubcommand::Down => "down",
            ComposeSubcommand::Restart => "restart",
            ComposeSubcommand::Rebuild => "rebuild",
        }
        .to_string(),
    ];
}

fn fresh_action_from_subcommand(action: FreshSubcommand) -> FreshAction {
    match action {
        FreshSubcommand::List { json } => FreshAction::List { json },
        FreshSubcommand::RunNow { id, json } => FreshAction::RunNow { id, json },
        FreshSubcommand::History { id, limit, json } => FreshAction::History { id, limit, json },
    }
}

fn setup_init_positionals(init: SetupInitArgs) -> Vec<String> {
    let mut out = vec!["init".to_string()];
    push_opt(&mut out, "--mcp-host", init.mcp_host);
    if let Some(port) = init.mcp_port {
        push_opt(&mut out, "--mcp-port", Some(port.to_string()));
    }
    if let Some(mode) = init.auth_mode {
        push_opt(
            &mut out,
            "--auth-mode",
            Some(
                match mode {
                    SetupAuthMode::Bearer => "bearer",
                    SetupAuthMode::Oauth => "oauth",
                }
                .to_string(),
            ),
        );
    }
    push_opt(&mut out, "--mcp-token", init.mcp_token);
    push_opt(&mut out, "--oauth-public-url", init.oauth_public_url);
    push_opt(&mut out, "--google-client-id", init.google_client_id);
    push_opt(
        &mut out,
        "--google-client-secret",
        init.google_client_secret,
    );
    push_opt(&mut out, "--auth-admin-email", init.auth_admin_email);
    push_opt(&mut out, "--tavily-api-key", init.tavily_api_key);
    push_opt(&mut out, "--github-token", init.github_token);
    push_opt(&mut out, "--reddit-client-id", init.reddit_client_id);
    push_opt(
        &mut out,
        "--reddit-client-secret",
        init.reddit_client_secret,
    );
    out
}

fn apply_palette(out: &mut DispatchOutput, args: PaletteArgs) {
    out.command = CommandKind::Palette;
    out.positional = args.action.into_iter().collect();
    if let Some(method) = args.method {
        out.setup_method = Some(method.as_str().to_string());
    }
}

fn push_opt(out: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value);
    }
}
