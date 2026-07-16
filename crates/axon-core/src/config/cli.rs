mod config_args;
mod global_args;
mod setup_args;

use super::types::{EvaluateResponsesMode, McpTransport};
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

pub(super) use config_args::{ConfigArgs, ConfigSubcommand, SyncArgs, SyncSubcommand};
pub(super) use global_args::{DEFAULT_OUTPUT_DIR, GlobalArgs};
pub(super) use setup_args::{ComposeArgs, ComposeSubcommand, SetupAuthMode, SetupInitArgs};

#[derive(Debug, Parser)]
#[command(
    name = "axon",
    about = "Web crawl, scrape, extract, embed, and query — self-hosted RAG in one binary",
    version = env!("CARGO_PKG_VERSION")
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub(super) command: CliCommand,

    #[command(flatten)]
    pub(super) global: GlobalArgs,
}

#[derive(Debug, Subcommand)]
pub(super) enum CliCommand {
    /// Manage recurring watch definitions and runs
    Watch(WatchArgs),
    /// Monitor job lifecycle events as a line-oriented stream
    Monitor(MonitorArgs),
    /// Discover all URLs on a site without scraping
    Map(MapArgs),
    /// Discover API endpoints from page HTML and JavaScript bundles
    Endpoints(EndpointArgs),
    /// LLM-powered structured data extraction from URLs
    Extract(ExtractArgs),
    /// Web search via SearXNG/Tavily, auto-queues Source jobs for results
    Search(TextArg),
    /// Web research via SearXNG/Tavily with LLM synthesis and auto-indexing
    Research(TextArg),
    /// Fetch/render/normalize exactly one web page and embed it by default
    Scrape(ScrapeSourceArgs),
    /// Analyze a URL's brand identity: colors, fonts, logos, favicon
    Brand(ScrapeArgs),
    /// Run doctor diagnostics plus LLM-assisted troubleshooting
    Debug(TextArg),
    /// Diff two URLs — show what changed between them
    Diff(DiffArgs),
    /// Check connectivity to all required services
    Doctor(DoctorArgs),
    /// Semantic vector search over the Qdrant index
    Query(QueryArgs),
    /// Fetch stored document chunks from Qdrant by URL
    Retrieve(RetrieveArgs),
    /// RAG: retrieve relevant context, then answer with LLM
    Ask(AskArgs),
    /// Scrape one or more URLs and summarize them with the configured LLM
    Summarize(ScrapeArgs),
    /// RAG vs baseline with independent LLM judge scoring
    Evaluate(EvaluateArgs),
    /// Collect human preference votes for retrieved RAG candidates
    Train(TrainArgs),
    /// Suggest new documentation URLs to crawl
    Suggest(TextArg),
    /// List all indexed source URLs with chunk counts
    Sources(SourcesArgs),
    /// List indexed domains with document statistics
    Domains(DomainsArgs),
    /// Show Qdrant collection and SQLite job statistics
    Stats,
    /// Show async job queue status and recent activity
    Status,
    /// Manage unified durable jobs
    Jobs(JobsArgs),
    /// Persistent agent memory: remember, list, search, show, link, supersede, or context memories
    Memory(MemoryArgs),
    /// Index AI session exports (Claude, Codex, Gemini) into Qdrant
    Sessions(SessionsArgs),
    /// Index a source through the unified pipeline
    Source(SourceArgs),
    /// Capture a full-page screenshot of one or more URLs
    Screenshot(ScrapeArgs),
    #[command(alias = "completion")]
    /// Generate shell completions (bash, zsh, fish)
    Completions(CompletionArgs),
    /// Start service runtimes
    Serve(ServeArgs),
    /// Destructive clean-slate reset of local stores (dry-run by default; requires --yes to mutate)
    Reset(ResetArgs),
    /// Plan or execute a scoped destructive cleanup (target-state replacement for dedupe/purge)
    Prune(PruneArgs),
    /// Check host prerequisites and service readiness
    Preflight(PreflightArgs),
    /// Run crawl/ask smoke checks against the running stack
    Smoke,
    /// Manage the local Docker Compose service stack
    Compose(ComposeArgs),
    /// Initialize and inspect Axon infrastructure
    Setup(SetupArgs),
    /// Start MCP stdio or unified HTTP runtime
    Mcp(McpArgs),
    /// Migrate an unnamed-vector collection to named-mode (enables hybrid RRF search)
    Migrate(MigrateArgs),
    /// Read or write entries in ~/.axon/.env and ~/.axon/config.toml
    Config(ConfigArgs),
    /// Reconcile locally produced server-mode artifacts
    Sync(SyncArgs),
    /// Download and install the latest GitHub Release binary, then sync the local container
    Update(UpdateArgs),
    /// Resolve, launch, and optionally install the axon-palette desktop binary
    Palette(PaletteArgs),
}

#[derive(Debug, Args)]
pub(super) struct DoctorArgs {
    #[command(subcommand)]
    pub(super) action: Option<DoctorSubcommand>,
}

#[derive(Debug, Args)]
pub(super) struct PreflightArgs {
    /// Validate config cutover keys only; do not run service readiness probes.
    #[arg(long = "config", action = ArgAction::SetTrue)]
    pub(super) config: bool,
}

#[derive(Debug, Args)]
pub(super) struct SourcesArgs {
    /// Filter source URLs by exact indexed domain/host.
    #[arg(long)]
    pub(super) domain: Option<String>,

    /// Export every matching URL for --domain instead of the default bounded page.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) all: bool,
}

#[derive(Debug, Args)]
pub(super) struct DomainsArgs {
    /// Check whether this exact indexed domain/host has any stored URLs.
    #[arg(long)]
    pub(super) domain: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(super) enum DoctorSubcommand {
    /// Print doctor output plus LLM diagnosis when configured
    Diagnose,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Args)]
pub(super) struct CompletionArgs {
    #[arg(value_enum)]
    pub(super) shell: CompletionShell,
}

#[derive(Debug, Args)]
pub(super) struct McpArgs {
    /// MCP transport: stdio, http, or both.
    #[arg(long, value_enum)]
    pub(super) transport: Option<McpTransport>,
}

#[derive(Debug, Args)]
pub(super) struct ServeArgs {
    #[command(subcommand)]
    pub(super) target: Option<ServeSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum ServeSubcommand {
    /// Start unified web + MCP HTTP runtime
    Mcp(McpArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(super) enum SetupMethod {
    /// Download the axon binary from GitHub releases (default)
    Pull,
    /// Build the axon binary from source with cargo
    Build,
}

impl SetupMethod {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Pull => "pull",
            Self::Build => "build",
        }
    }
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct SetupArgs {
    #[command(subcommand)]
    pub(super) action: Option<SetupSubcommand>,
    /// Binary acquisition method passed through from install.sh (pull = GitHub release, build = cargo)
    #[arg(long, value_enum)]
    pub(super) method: Option<SetupMethod>,
}

#[derive(Debug, Subcommand)]
pub(super) enum SetupSubcommand {
    /// Hook-safe preflight/setup entrypoint for Claude Code plugin SessionStart
    #[command(name = "plugin-hook", alias = "hook")]
    PluginHook {
        /// Run preflight only; do not run the setup wrapper if preflight fails.
        #[arg(long = "no-setup")]
        no_setup: bool,
    },
    /// Initialize local Axon config, env, and compose assets
    Init(Box<SetupInitArgs>),
    /// Check local prerequisites without mutating files or services
    Check,
    /// List SSH host aliases discovered from ~/.ssh/config (informational).
    Targets,
    /// Copy the axon binary into ~/.local/bin for terminal use.
    Install,
    /// Inspect or rewrite local config files.
    Config {
        #[command(subcommand)]
        action: SetupConfigSubcommand,
    },
}

#[derive(Debug, Subcommand)]
pub(super) enum SetupConfigSubcommand {
    /// Preview or apply clean-break config key rewrites.
    Rewrite {
        /// Print proposed .env/config.toml edits without writing files.
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
}

#[derive(Debug, Args)]
pub(super) struct ResetArgs {
    /// Comma-separated stores to reset (jobs, ledger, graph, memory, vectors,
    /// artifacts). Omit to select every store.
    #[arg(long, value_name = "STORES", value_delimiter = ',')]
    pub(super) stores: Vec<String>,

    /// Preview the reset plan without deleting anything. Reset is dry-run by
    /// default; pass this to keep it a dry-run even alongside --yes.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) dry_run: bool,

    /// Execute a previously reviewed reset plan id. Required with --yes.
    #[arg(long = "plan-id")]
    pub(super) plan_id: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct PruneArgs {
    #[command(subcommand)]
    pub(super) action: PruneCliSubcommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum PruneCliSubcommand {
    /// Resolve a prune target into a reviewable dry-run plan (mutates nothing)
    Plan(PruneTargetArgs),
    /// Execute a prune target's plan (destructive; requires --confirm and admin)
    Exec(PruneExecArgs),
}

#[derive(Debug, Args)]
pub(super) struct PruneTargetArgs {
    /// Plan target for `plan`; reviewed plan id for `exec`.
    pub(super) target: String,

    /// Scope the prune to one generation of `target` instead of the whole source
    #[arg(long)]
    pub(super) generation: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct PruneExecArgs {
    #[command(flatten)]
    pub(super) target: PruneTargetArgs,

    /// Explicit confirmation required to actually delete (destructive prune
    /// also requires local admin trust — see `axon prune exec --help`)
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) confirm: bool,
}

#[derive(Debug, Args)]
pub(super) struct MigrateArgs {
    /// Source collection to migrate from (must use unnamed dense vectors)
    #[arg(long)]
    pub(super) from: String,
    /// Destination collection to create with named dense + bm42 sparse vectors
    #[arg(long)]
    pub(super) to: String,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct PaletteArgs {
    /// Subcommand: launch (default), install, desktop, autostart
    #[arg(value_name = "SUBCOMMAND")]
    pub(super) action: Option<String>,
    /// Binary acquisition method when the palette binary is missing or during install
    #[arg(long, value_enum)]
    pub(super) method: Option<SetupMethod>,
}

#[derive(Debug, Args)]
pub(super) struct UpdateArgs {
    /// GitHub repository in owner/name form.
    #[arg(long, default_value = "jmagar/axon")]
    pub(super) repo: String,

    /// Release tag to install. Defaults to the latest GitHub Release.
    #[arg(long)]
    pub(super) version: Option<String>,

    /// Install even when the destination already reports the target version.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) force: bool,

    /// Do not restart/sync the local Axon container after installing.
    #[arg(long = "no-container", action = ArgAction::SetTrue)]
    pub(super) no_container: bool,
}

#[derive(Debug, Args)]
pub(super) struct ScrapeArgs {
    #[arg(value_name = "URL")]
    pub(super) positional_urls: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct SourceArgs {
    /// Source to index: a local path, git URL, feed URL, youtube target, reddit
    /// target, web URL, session selector, or registry target.
    #[arg(value_name = "SOURCE")]
    pub(super) path: Option<String>,

    /// Acquisition scope override (e.g. `page`, `site`). Adapter-specific; when
    /// omitted the adapter's default scope is used.
    #[arg(long, value_name = "SCOPE")]
    pub(super) scope: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct ScrapeSourceArgs {
    /// URL to scrape as exactly one page.
    #[arg(value_name = "URL")]
    pub(super) url: String,
    /// Skip vector embedding while still returning or saving clean content.
    #[arg(long = "no-embed", action = ArgAction::SetTrue)]
    pub(super) no_embed: bool,
    /// Return the cleaned page body inline when it fits the output policy.
    #[arg(long = "inline", action = ArgAction::SetTrue)]
    pub(super) inline: bool,
}

#[derive(Debug, Args)]
pub(super) struct MemoryArgs {
    #[command(subcommand)]
    pub(super) action: MemoryCliSubcommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum MemoryCliSubcommand {
    /// Store a memory in the dedicated memory collection
    Remember {
        #[arg(value_name = "BODY")]
        body: Vec<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        confidence: Option<f64>,
    },
    /// List memory metadata without semantic search
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Search active memories
    Search {
        #[arg(value_name = "QUERY")]
        query: Vec<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show one memory by id
    Show { id: String },
    /// Link two memories in the SQLite graph
    Link {
        source_id: String,
        target_id: String,
        #[arg(long = "type")]
        edge_type: Option<String>,
    },
    /// Mark an old memory as superseded by a replacement memory
    Supersede {
        replacement_id: String,
        old_id: String,
    },
    /// Build an inline, defanged context block from memories
    Context {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        token_budget: Option<usize>,
    },
}

#[derive(Debug, Args)]
pub(super) struct DiffArgs {
    /// First URL (baseline)
    #[arg(value_name = "URL_A")]
    pub(super) url_a: String,
    /// Second URL (comparison)
    #[arg(value_name = "URL_B")]
    pub(super) url_b: String,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct WatchArgs {
    /// Source URI/path to watch. Equivalent to `watch create <SOURCE>`.
    #[arg(value_name = "SOURCE")]
    pub(super) source: Option<String>,
    /// Watch interval in seconds for source watch creation.
    #[arg(long = "every-seconds")]
    pub(super) every_seconds: Option<i64>,
    /// Target collection for source watch runs.
    #[arg(long = "collection")]
    pub(super) collection: Option<String>,
    #[command(subcommand)]
    pub(super) action: Option<WatchSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum WatchSubcommand {
    Create {
        source: String,
        #[arg(long = "every-seconds", default_value_t = 3600)]
        every_seconds: i64,
        #[arg(long = "collection")]
        collection: Option<String>,
    },
    List,
    Get {
        id: String,
    },
    Status {
        id: String,
    },
    Update {
        id: String,
        #[arg(long = "every-seconds")]
        every_seconds: Option<i64>,
        #[arg(long = "collection")]
        collection: Option<String>,
    },
    #[command(name = "exec")]
    Exec {
        id: String,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Delete {
        id: String,
    },
    History {
        id: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
}

#[derive(Debug, Args)]
pub(super) struct MonitorArgs {
    #[command(subcommand)]
    pub(super) action: MonitorSubcommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum MonitorSubcommand {
    /// Emit crawl/extract/embed/ingest start, completion, failure, and cancel events
    Jobs(MonitorJobsArgs),
}

#[derive(Debug, Args)]
pub(super) struct MonitorJobsArgs {
    /// Keep polling instead of emitting one batch and exiting.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) watch: bool,

    /// Emit one compact JSON object per event.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) jsonl: bool,

    /// Poll interval while --watch is active.
    #[arg(long, default_value_t = 5)]
    pub(super) interval_secs: u64,

    /// State file used to suppress duplicate events.
    #[arg(long)]
    pub(super) state_file: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct UrlArg {
    #[arg(value_name = "URL")]
    pub(super) value: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct RetrieveArgs {
    #[arg(value_name = "URL")]
    pub(super) value: Option<String>,
    /// Maximum chunks to fetch for this document before reconstruction.
    #[arg(long, value_name = "N")]
    pub(super) max_points: Option<usize>,
}

#[derive(Debug, Args)]
pub(super) struct MapArgs {
    #[arg(value_name = "URL")]
    pub(super) value: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct EndpointArgs {
    #[arg(value_name = "URL")]
    pub(super) url: String,
    /// Fetch and scan first-party JavaScript bundles.
    #[arg(long = "include-bundles", action = ArgAction::Set, default_value_t = true)]
    pub(super) include_bundles: bool,
    /// Return only endpoints on the target page's host.
    #[arg(long = "first-party-only", action = ArgAction::Set, default_value_t = false)]
    pub(super) first_party_only: bool,
    /// Deduplicate by normalized endpoint URL.
    #[arg(long = "unique-only", action = ArgAction::Set, default_value_t = true)]
    pub(super) unique_only: bool,
    /// Maximum script bundle URLs to fetch and scan.
    #[arg(long = "max-scripts", default_value_t = 40)]
    pub(super) max_scripts: usize,
    /// Maximum HTML plus JavaScript bytes to scan.
    #[arg(long = "max-scan-bytes", default_value_t = 8 * 1024 * 1024)]
    pub(super) max_scan_bytes: usize,
    /// Probe discovered HTTP endpoints without credentials.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) verify: bool,
    /// Capture browser network requests. Executes page code and requires Chrome.
    #[arg(long = "capture-network", action = ArgAction::SetTrue)]
    pub(super) capture_network: bool,
    /// Probe discovered endpoints for JSON-RPC 2.0 / MCP / ACP protocol support.
    #[arg(long = "probe-rpc", action = ArgAction::SetTrue)]
    pub(super) probe_rpc: bool,
    /// Also probe `mcp.<apex>` subdomain candidates for MCP/JSON-RPC. No-op without --probe-rpc.
    #[arg(long = "probe-rpc-subdomains", action = ArgAction::SetTrue)]
    pub(super) probe_rpc_subdomains: bool,
}

#[derive(Debug, Args)]
pub(super) struct TextArg {
    #[arg(value_name = "TEXT")]
    pub(super) value: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct AskArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) diagnostics: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) explain: bool,
    /// Stream answer tokens as they arrive for interactive use. This is the default.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) stream: bool,
    /// Disable answer token streaming and render only the final response.
    #[arg(long = "no-stream", action = ArgAction::SetTrue)]
    pub(super) no_stream: bool,
    /// Treat this question as a follow-up to recent turns in the ask session.
    /// `--continue` is accepted as an alias and `-c` is the short form.
    #[arg(
        long = "follow-up",
        alias = "continue",
        short = 'c',
        action = ArgAction::SetTrue,
        conflicts_with_all = ["new_session", "resume"],
    )]
    pub(super) follow_up: bool,
    /// Name of the local ask session used for follow-up context.
    #[arg(long = "session", value_name = "NAME", conflicts_with = "resume")]
    pub(super) session: Option<String>,
    /// Clear the selected ask session before running this question.
    #[arg(
        long = "reset-session",
        action = ArgAction::SetTrue,
        conflicts_with = "new_session",
    )]
    pub(super) reset_session: bool,
    /// Force a fresh ask session, overwriting any selected one (mutually exclusive with --follow-up).
    #[arg(long = "new-session", action = ArgAction::SetTrue)]
    pub(super) new_session: bool,
    /// List local ask sessions and exit. Cannot be combined with a query argument.
    #[arg(long = "list-sessions", action = ArgAction::SetTrue)]
    pub(super) list_sessions: bool,
    /// Resume a named ask session (shorthand for `--follow-up --session NAME`).
    #[arg(
        long = "resume",
        value_name = "NAME",
        conflicts_with_all = ["new_session", "reset_session", "session"],
    )]
    pub(super) resume: Option<String>,
    #[arg(value_name = "TEXT")]
    pub(super) value: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct QueryArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) diagnostics: bool,
    #[arg(value_name = "TEXT")]
    pub(super) value: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct EvaluateArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) diagnostics: bool,
    #[arg(long = "responses-mode", value_enum, default_value_t = EvaluateResponsesMode::SideBySide)]
    pub(super) responses_mode: EvaluateResponsesMode,
    /// Replace the no-context baseline with a second RAG run that has hybrid retrieval
    /// disabled. The judge then compares hybrid-RAG vs dense-only-RAG.
    #[arg(long = "retrieval-ab", action = ArgAction::SetTrue)]
    pub(super) retrieval_ab: bool,
    #[arg(value_name = "TEXT")]
    pub(super) value: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct TrainArgs {
    /// Record this 1-based candidate rank without prompting.
    #[arg(long = "best", value_name = "RANK")]
    pub(super) best_rank: Option<usize>,
    /// Optional note stored with the preference event.
    #[arg(long)]
    pub(super) notes: Option<String>,
    #[arg(value_name = "TEXT")]
    pub(super) value: Vec<String>,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct ExtractArgs {
    #[command(subcommand)]
    pub(super) job: Option<JobSubcommand>,
    #[arg(value_name = "URL")]
    pub(super) positional_urls: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct SessionsArgs {
    /// Only scan Claude session exports.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) claude: bool,
    /// Only scan Codex session exports.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) codex: bool,
    /// Only scan Gemini session exports.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) gemini: bool,
    /// Filter session projects by substring.
    #[arg(long, value_name = "NAME")]
    pub(super) project: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(super) enum JobSubcommand {
    Status { job_id: String },
    Cancel { job_id: String },
    Errors { job_id: String },
    List,
    Cleanup,
    Clear,
    Worker,
    Recover,
}

#[derive(Debug, Args)]
pub(super) struct JobsArgs {
    #[command(subcommand)]
    pub(super) action: Option<JobsSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum JobsSubcommand {
    /// List unified durable jobs.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Show one unified durable job.
    Get { job_id: String },
    /// Show one job's event page.
    Events {
        job_id: String,
        #[arg(long = "after-sequence")]
        after_sequence: Option<u64>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Fetch an event page for stream consumers.
    Stream {
        job_id: String,
        #[arg(long = "after-sequence")]
        after_sequence: Option<u64>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Request cancellation for a unified durable job.
    Cancel {
        job_id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Retry a unified durable job.
    Retry {
        job_id: String,
        #[arg(long, default_value = "same_config")]
        mode: String,
    },
    /// Recover stale unified durable jobs.
    Recover {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "stale-before")]
        stale_before: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Remove old terminal unified durable jobs.
    Cleanup {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "older-than")]
        older_than: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
    /// Clear all unified durable job rows.
    Clear {
        #[arg(long, action = ArgAction::SetTrue)]
        confirm: bool,
    },
}
#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
