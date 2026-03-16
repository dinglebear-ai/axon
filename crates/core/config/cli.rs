mod global_args;

use super::types::{EvaluateResponsesMode, McpTransport, RedditSort, RedditTime};
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

pub(super) use global_args::GlobalArgs;

#[derive(Debug, Parser)]
#[command(name = "axon", about = "Axon CLI (Rust + Spider.rs)")]
pub(super) struct Cli {
    #[command(subcommand)]
    pub(super) command: CliCommand,

    #[command(flatten)]
    pub(super) global: GlobalArgs,
}

#[derive(Debug, Subcommand)]
pub(super) enum CliCommand {
    Scrape(ScrapeArgs),
    Crawl(CrawlArgs),
    Refresh(RefreshArgs),
    Watch(WatchArgs),
    Map(UrlArg),
    Extract(ExtractArgs),
    Search(TextArg),
    Research(TextArg),
    Embed(EmbedArgs),
    Debug(TextArg),
    Doctor,
    Query(TextArg),
    Retrieve(UrlArg),
    Ask(AskArgs),
    Evaluate(EvaluateArgs),
    Suggest(TextArg),
    Sources,
    Domains,
    Stats,
    Status,
    Dedupe,
    Ingest(IngestArgs),
    Sessions(SessionsArgs),
    Screenshot(ScrapeArgs),
    /// Knowledge graph operations
    Graph(GraphArgs),
    #[command(alias = "completion")]
    Completions(CompletionArgs),
    Mcp(McpArgs),
    Serve(ServeArgs),
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
    /// Port to bind the web UI server on
    #[arg(long, env = "AXON_SERVE_PORT", default_value_t = 49000)]
    pub(super) port: u16,
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
#[command(args_conflicts_with_subcommands = true)]
pub(super) struct GraphArgs {
    #[command(subcommand)]
    pub(super) action: Option<GraphSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum GraphSubcommand {
    Build {
        /// Target URL (positional form: `axon graph build <url>`)
        #[arg(value_name = "URL")]
        positional_url: Option<String>,
        /// Target URL (flag form: `axon graph build --url <url>`)
        #[arg(long = "url", conflicts_with = "positional_url")]
        url_flag: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long, action = ArgAction::SetTrue)]
        all: bool,
    },
    Status,
    Explore {
        entity: String,
    },
    Stats,
    Worker,
}

#[derive(Debug, Args)]
pub(super) struct UrlArg {
    #[arg(value_name = "URL")]
    pub(super) value: Option<String>,
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
    #[arg(value_name = "TEXT")]
    pub(super) value: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct EvaluateArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) diagnostics: bool,
    #[arg(long = "responses-mode", value_enum, default_value_t = EvaluateResponsesMode::SideBySide)]
    pub(super) responses_mode: EvaluateResponsesMode,
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
pub(super) struct RefreshArgs {
    #[command(subcommand)]
    pub(super) action: Option<RefreshSubcommand>,
    #[arg(value_name = "URL")]
    pub(super) positional_urls: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub(super) enum RefreshSubcommand {
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
    Schedule {
        #[command(subcommand)]
        action: RefreshScheduleSubcommand,
    },
}

#[derive(Debug, Subcommand)]
pub(super) enum RefreshScheduleSubcommand {
    Add {
        name: String,
        seed_url: Option<String>,
        #[arg(long = "every-seconds")]
        every_seconds: Option<i64>,
        #[arg(long, value_parser = ["high", "medium", "low"])]
        tier: Option<String>,
        #[arg(long)]
        urls: Option<String>,
    },
    List,
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Delete {
        name: String,
    },
    Worker,
    #[command(name = "run-due")]
    RunDue {
        #[arg(long, default_value_t = 25)]
        batch: usize,
    },
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
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parse_mcp_transport_stdio_flag() {
        let result = Cli::try_parse_from(["axon", "mcp", "--transport", "stdio"]);
        assert!(
            result.is_ok(),
            "mcp --transport stdio should parse: {result:?}"
        );
    }

    #[test]
    fn parse_mcp_transport_both_flag() {
        let result = Cli::try_parse_from(["axon", "mcp", "--transport", "both"]);
        assert!(
            result.is_ok(),
            "mcp --transport both should parse: {result:?}"
        );
    }

    #[test]
    fn parse_migrate_flags() {
        let result =
            Cli::try_parse_from(["axon", "migrate", "--from", "cortex", "--to", "cortex_v2"]);
        assert!(
            result.is_ok(),
            "migrate --from --to should parse: {result:?}"
        );
    }
}
