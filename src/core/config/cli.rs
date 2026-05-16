mod global_args;

use super::types::{EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime};
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

pub(super) use global_args::{DEFAULT_OUTPUT_DIR, GlobalArgs};

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
    /// Discover all URLs on a site without scraping
    Map(MapArgs),
    /// LLM-powered structured data extraction from URLs
    Extract(ExtractArgs),
    /// Web search via Tavily, auto-queues crawl jobs for results
    Search(TextArg),
    /// Web research via Tavily AI search with LLM synthesis
    Research(TextArg),
    /// Embed file, directory, or URL into Qdrant
    Embed(EmbedArgs),
    /// Run doctor diagnostics plus LLM-assisted troubleshooting
    Debug(TextArg),
    /// Check connectivity to all required services
    Doctor,
    /// Semantic vector search over the Qdrant index
    Query(QueryArgs),
    /// Fetch stored document chunks from Qdrant by URL
    Retrieve(RetrieveArgs),
    /// RAG: retrieve relevant context, then answer with LLM
    Ask(AskArgs),
    /// RAG vs baseline with independent LLM judge scoring
    Evaluate(EvaluateArgs),
    /// Collect human preference votes for retrieved RAG candidates
    Train(TrainArgs),
    /// Suggest new documentation URLs to crawl
    Suggest(TextArg),
    /// List all indexed source URLs with chunk counts
    Sources,
    /// List indexed domains with document statistics
    Domains,
    /// Show Qdrant collection and Postgres job statistics
    Stats,
    /// Show async job queue status and recent activity
    Status,
    /// Remove duplicate points from the Qdrant collection
    Dedupe,
    /// Ingest external sources (GitHub repo, Reddit, YouTube)
    Ingest(IngestArgs),
    /// Index AI session exports (Claude, Codex, Gemini) into Qdrant
    Sessions(SessionsArgs),
    /// Capture a full-page screenshot of one or more URLs
    Screenshot(ScrapeArgs),
    #[command(alias = "completion")]
    /// Generate shell completions (bash, zsh, fish)
    Completions(CompletionArgs),
    /// Start service runtimes
    Serve(ServeArgs),
    /// Setup and deploy Axon infrastructure
    Setup(SetupArgs),
    /// Start MCP server (stdio, HTTP, or both)
    Mcp(McpArgs),
    /// Migrate an unnamed-vector collection to named-mode (enables hybrid RRF search)
    Migrate(MigrateArgs),
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

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct SetupArgs {
    #[command(subcommand)]
    pub(super) action: Option<SetupSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum SetupSubcommand {
    /// Hook-safe check/repair entrypoint for Claude Code plugin SessionStart
    #[command(name = "plugin-hook", alias = "hook")]
    PluginHook {
        /// Inspect only; do not repair config/assets or restart services.
        #[arg(long)]
        no_repair: bool,
    },
    /// Check local Docker setup prerequisites without mutating files or services
    Check,
    /// Repair local Axon config, compose assets, and Docker stack
    Repair {
        /// Backup and migrate ~/.axon/.env to the minimal env/config boundary.
        #[arg(long)]
        migrate_env: bool,
    },
    /// List SSH host aliases discovered from ~/.ssh/config (informational).
    Targets,
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
pub(super) struct ScrapeArgs {
    #[arg(value_name = "URL")]
    pub(super) positional_urls: Vec<String>,
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
    #[arg(long = "follow-up", action = ArgAction::SetTrue)]
    pub(super) follow_up: bool,
    /// Name of the local ask session used for follow-up context.
    #[arg(long = "session", value_name = "NAME")]
    pub(super) session: Option<String>,
    /// Clear the selected ask session before running this question.
    #[arg(long = "reset-session", action = ArgAction::SetTrue)]
    pub(super) reset_session: bool,
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

    /// Ingest target: GitHub slug (owner/repo), YouTube URL/@handle, or Reddit subreddit (r/name)
    #[arg(value_name = "TARGET")]
    pub(super) target: Option<String>,

    /// (GitHub only) Also index source code files in addition to markdown, issues, and PRs
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) include_source: bool,

    /// (GitHub only) Skip source code files when ingesting a GitHub repository (default: include source).
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
    pub(super) job: Option<JobSubcommand>,
    /// Index Claude Code sessions
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) claude: bool,
    /// Index Codex sessions
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) codex: bool,
    /// Index Gemini sessions
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) gemini: bool,
    /// Filter sessions by project name (substring match)
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
#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
