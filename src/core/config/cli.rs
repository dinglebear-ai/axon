mod config_args;
mod global_args;
mod setup_args;

use super::types::{EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime};
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
    /// Scrape one or more URLs to markdown
    Scrape(ScrapeArgs),
    /// Full site crawl for one or more start URLs
    Crawl(CrawlArgs),
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
    /// Web search via SearXNG/Tavily, auto-queues crawl jobs for results
    Search(TextArg),
    /// Web research via SearXNG/Tavily with LLM synthesis and auto-indexing
    Research(TextArg),
    /// Embed file, directory, or URL into Qdrant
    Embed(EmbedArgs),
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
    /// Remove duplicate points from the Qdrant collection
    Dedupe,
    /// Re-crawl / re-ingest previously indexed origins (full docs refresh)
    Refresh(RefreshArgs),
    /// Ingest external sources (GitHub, GitLab, Gitea/Forgejo, generic Git, Reddit, YouTube)
    Ingest(IngestArgs),
    /// Persistent agent memory: remember, list, search, show, link, supersede, or context memories
    Memory(MemoryArgs),
    /// Index AI session exports (Claude, Codex, Gemini) into Qdrant
    Sessions(SessionsArgs),
    /// Capture a full-page screenshot of one or more URLs
    Screenshot(ScrapeArgs),
    #[command(alias = "completion")]
    /// Generate shell completions (bash, zsh, fish)
    Completions(CompletionArgs),
    /// Start service runtimes
    Serve(ServeArgs),
    /// Check host prerequisites and service readiness
    Preflight,
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
    /// Resolve, launch, and optionally install the axon-palette desktop binary
    Palette(PaletteArgs),
}

#[derive(Debug, Args)]
pub(super) struct DoctorArgs {
    #[command(subcommand)]
    pub(super) action: Option<DoctorSubcommand>,
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
    /// Install, check, remove, or inspect the host-local session watch service.
    #[command(name = "session-watch-service")]
    SessionWatchService {
        #[command(subcommand)]
        action: SessionWatchServiceSubcommand,
    },
}

#[derive(Debug, Subcommand, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionWatchServiceSubcommand {
    /// Write service files, run initial ingest, and enable the user service.
    Install,
    /// Verify generated files and systemd state without mutating service files.
    Check,
    /// Disable the user service and remove generated service files.
    Remove,
    /// Print current user systemd status for the service.
    Status,
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
pub(super) struct ScrapeArgs {
    #[arg(value_name = "URL")]
    pub(super) positional_urls: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct RefreshArgs {
    /// Optional filter: a source_type (crawl/embed/scrape/github/gitlab/gitea/
    /// git/reddit/youtube) or a domain/substring matched against indexed origins.
    /// Omit to refresh every indexed origin.
    #[arg(value_name = "FILTER")]
    pub(super) filter: Option<String>,
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
    #[command(subcommand)]
    pub(super) action: Option<WatchSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum WatchSubcommand {
    Create {
        name: String,
        #[arg(long = "task-type")]
        task_type: String,
        #[arg(long = "every-seconds")]
        every_seconds: i64,
        #[arg(long = "task-payload")]
        task_payload: Option<String>,
    },
    List,
    Get {
        id: String,
    },
    Update {
        id: String,
        #[arg(long = "every-seconds")]
        every_seconds: Option<i64>,
    },
    #[command(name = "run-now")]
    RunNow {
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
    Artifacts {
        run_id: String,
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
    /// Fallback strategy when no sitemap documents are found.
    /// `structure`: fetch root page and extract anchor hrefs (default, fast).
    /// `crawl`: run a full Spider.rs crawl (slow, legacy — explicit opt-in).
    #[arg(long, value_enum)]
    pub(super) map_fallback: Option<MapFallback>,
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
pub(super) struct CrawlArgs {
    #[command(subcommand)]
    pub(super) job: Option<JobSubcommand>,
    #[arg(value_name = "URL")]
    pub(super) positional_urls: Vec<String>,
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
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct EmbedArgs {
    #[command(subcommand)]
    pub(super) job: Option<JobSubcommand>,
    #[arg(value_name = "INPUT")]
    pub(super) input: Option<String>,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct IngestArgs {
    #[command(subcommand)]
    pub(super) job: Option<JobSubcommand>,

    /// Ingest target: GitHub slug, GitLab/Gitea URL, git:https URL, YouTube URL/@handle, or Reddit target
    #[arg(value_name = "TARGET")]
    pub(super) target: Option<String>,

    /// Skip source code files when ingesting a Git repository (GitHub, GitLab, Gitea, or generic git).
    /// By default source code is included. Has no effect on Reddit or YouTube targets.
    #[arg(long = "no-source")]
    pub(super) no_source: bool,
    // ── GitHub-specific limits (ignored for Reddit / YouTube) ────────────
    /// Maximum issues to fetch per repository (0 = unlimited, default 100)
    #[arg(long = "max-issues", default_value_t = 100)]
    pub(super) max_issues: usize,
    /// Maximum pull requests to fetch per repository (0 = unlimited, default 100)
    #[arg(long = "max-prs", default_value_t = 100)]
    pub(super) max_prs: usize,

    // ── Reddit-specific filters (ignored for GitHub / YouTube) ────────────
    /// Subreddit sorting (hot, top, new, rising)
    #[arg(long, value_enum, default_value_t = RedditSort::Hot)]
    pub(super) sort: RedditSort,
    /// Time range for top sort (hour, day, week, month, year, all)
    #[arg(long, value_enum, default_value_t = RedditTime::Day)]
    pub(super) time: RedditTime,
    /// Maximum posts to fetch (0 for unlimited)
    #[arg(long, default_value_t = 25)]
    pub(super) max_posts: usize,
    /// Minimum score threshold for posts and comments
    #[arg(long, default_value_t = 0)]
    pub(super) min_score: i32,
    /// Comment traversal depth
    #[arg(long, default_value_t = 2)]
    pub(super) depth: usize,
    /// Scrape content of linked URLs in link posts
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) scrape_links: bool,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct SessionsArgs {
    #[command(subcommand)]
    pub(super) action: Option<SessionsSubcommand>,
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
pub(super) enum SessionsSubcommand {
    Status {
        job_id: String,
    },
    Cancel {
        job_id: String,
    },
    Errors {
        job_id: String,
    },
    List,
    Cleanup,
    Clear,
    Worker,
    Recover,
    /// Watch local AI session exports and ingest stable changes.
    Watch(SessionsWatchArgs),
    /// Summarize session watch checkpoints and recent errors.
    #[command(name = "watch-status")]
    WatchStatus {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Write a probe transcript and wait for the watcher to ingest it.
    #[command(name = "smoke-watch")]
    SmokeWatch {
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
    },
}

#[derive(Debug, Args, Clone)]
pub(super) struct SessionsWatchArgs {
    /// Only watch Claude session exports.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) claude: bool,
    /// Only watch Codex session exports.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) codex: bool,
    /// Only watch Gemini session exports.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) gemini: bool,
    /// Filter watched session projects by substring.
    #[arg(long, value_name = "NAME")]
    pub(super) project: Option<String>,
    /// Watch one transcript file or directory instead of all default provider roots.
    #[arg(long)]
    pub(super) path: Option<std::path::PathBuf>,
    /// Debounce file events by this many milliseconds before checking stability.
    #[arg(long = "debounce-ms", default_value_t = 750)]
    pub(super) debounce_ms: u64,
    /// Require unchanged size and mtime for this many milliseconds before ingest.
    #[arg(long = "settle-ms", default_value_t = 500)]
    pub(super) settle_ms: u64,
    /// Retry parse, upload, or storage failures this many times before recording an error.
    #[arg(long = "max-retries", default_value_t = 5)]
    pub(super) max_retries: u8,
    /// Maximum prepared session docs to submit in one batch.
    #[arg(long = "max-batch-docs", default_value_t = 50)]
    pub(super) max_batch_docs: usize,
    /// Maximum files parsed concurrently while preparing a batch.
    #[arg(long = "max-processing-concurrency", default_value_t = 2)]
    pub(super) max_processing_concurrency: usize,
    /// Minimum cooldown before another overflow/root rescan can run.
    #[arg(long = "rescan-cooldown-ms", default_value_t = 5000)]
    pub(super) rescan_cooldown_ms: u64,
    /// Skip the startup full scan. The setup service uses this after its one-time initial ingest.
    #[arg(long = "no-initial-scan")]
    pub(super) no_initial_scan: bool,
    /// Explicitly upload prepared docs to AXON_SERVER_URL instead of local ingest.
    #[arg(long = "upload-to-server")]
    pub(super) upload_to_server: bool,
    /// Include raw local paths in JSON/log output. Default output is redacted.
    #[arg(long = "verbose-paths")]
    pub(super) verbose_paths: bool,
    /// Emit newline-delimited JSON events suitable for systemd logs.
    #[arg(long)]
    pub(super) json: bool,
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
#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
