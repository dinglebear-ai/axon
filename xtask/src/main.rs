use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Axon repository maintenance checks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run all repository checks.
    Check,
    /// Enforce modern Rust module layout.
    CheckNoModRs,
    /// Verify MCP HTTP transport support.
    CheckMcpHttp,
    /// Reject staged secret env files.
    CheckEnvStaged,
    /// Warn about newly staged unwrap/expect calls.
    CheckUnwraps,
    /// Verify AGENTS.md/GEMINI.md symlinks next to CLAUDE.md files.
    CheckClaudeSymlinks,
    /// Fail if any symlink in the worktree points to a non-existent target.
    CheckBrokenSymlinks,
    /// Scan staged files for secrets and credentials.
    CheckSecrets,
    /// Compatibility check for the CLI component's version-bearing files.
    /// The full multi-component gate is `check-release-versions`.
    CheckVersionSync,
    /// Regenerate and verify all tracked OpenAPI artifacts.
    CheckOpenapiDrift,
    /// Verify all releasable components have valid versions and changed shipping paths have bumps.
    CheckReleaseVersions {
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value = "HEAD")]
        head: String,
        #[arg(long, value_enum, default_value = "pr")]
        mode: checks::release_versions::GateMode,
        #[arg(long)]
        json: bool,
    },
    /// Print the release plan consumed by GitHub Actions.
    ReleasePlan {
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value = "HEAD")]
        head: String,
        #[arg(long, value_enum, default_value = "pr")]
        mode: checks::release_versions::GateMode,
        #[arg(long)]
        json: bool,
    },
    /// Bump all version-bearing files for one component.
    BumpVersion {
        component: String,
        #[arg(value_enum)]
        level: checks::release_versions::BumpLevel,
    },
    /// Benchmark embedding a local corpus through axon, TEI, and Qdrant.
    BenchEmbed {
        /// File or directory to embed.
        corpus: PathBuf,
        /// Axon binary to execute. Defaults to target/debug/axon, then PATH.
        #[arg(long)]
        axon_bin: Option<PathBuf>,
        /// Qdrant collection name. Defaults to a timestamped throwaway collection.
        #[arg(long)]
        collection: Option<String>,
        /// Qdrant base URL. Defaults to QDRANT_URL / AXON_QDRANT_URL from env or ~/.axon/.env.
        #[arg(long)]
        qdrant_url: Option<String>,
        /// TEI base URL for metrics. Defaults to TEI_URL from env or ~/.axon/.env.
        #[arg(long)]
        tei_url: Option<String>,
        /// Keep the benchmark collection instead of deleting it.
        #[arg(long)]
        keep_collection: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = std::env::current_dir()?;
    match cli.command {
        Command::Check => checks::check(&root),
        Command::CheckNoModRs => checks::no_mod_rs::check(&root),
        Command::CheckMcpHttp => checks::mcp_http::check(&root),
        Command::CheckEnvStaged => checks::env_staged::check(&root),
        Command::CheckUnwraps => checks::unwraps::check(&root),
        Command::CheckClaudeSymlinks => checks::claude_symlinks::check(&root),
        Command::CheckBrokenSymlinks => checks::broken_symlinks::check(&root),
        Command::CheckSecrets => checks::secrets::check(&root),
        Command::CheckVersionSync => checks::version_sync::check(&root),
        Command::CheckOpenapiDrift => checks::openapi_drift::check(&root),
        Command::CheckReleaseVersions {
            base,
            head,
            mode,
            json,
        } => Ok(checks::release_versions::check(
            &root,
            base.as_deref(),
            &head,
            mode,
            json,
        )?),
        Command::ReleasePlan {
            base,
            head,
            mode,
            json,
        } => {
            let plans = checks::release_versions::plan(&root, base.as_deref(), &head, mode)?;
            checks::release_versions::print_plans(&plans, json)?;
            Ok(())
        }
        Command::BumpVersion { component, level } => {
            Ok(checks::release_versions::bump(&root, &component, level)?)
        }
        Command::BenchEmbed {
            corpus,
            axon_bin,
            collection,
            qdrant_url,
            tei_url,
            keep_collection,
            json,
        } => bench_embed::run(
            &root,
            bench_embed::BenchEmbedArgs {
                corpus,
                axon_bin,
                collection,
                qdrant_url,
                tei_url,
                keep_collection,
                json,
            },
        ),
    }
}

mod bench_embed;
mod checks;
