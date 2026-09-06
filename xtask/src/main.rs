//! Repository maintenance, contract validation, and schema generation tasks.

#![allow(deprecated)]
#![allow(
    clippy::collapsible_if,
    clippy::redundant_closure,
    clippy::single_char_add_str,
    clippy::too_many_arguments,
    clippy::while_let_on_iterator
)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

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
    /// Enforce crate ownership: transports must not reach into domain internals.
    CheckLayering,
    /// Verify docs/reference/api-parity.md is in sync with the source surfaces.
    CheckApiParity,
    /// Regenerate docs/reference/api-parity.md from the CLI/MCP/REST surfaces.
    GenApiParity,
    /// Verify MCP HTTP transport support.
    CheckMcpHttp,
    /// Verify .cargo/audit.toml and deny.toml advisory ignore lists match.
    CheckAuditIgnoreSync,
    /// Reject staged secret env files.
    CheckEnvStaged,
    /// Warn about newly staged unwrap/expect calls.
    CheckUnwraps,
    /// Enforce that web acquisition goes through the shared fetch ladder.
    CheckFetchDivergence,
    /// Verify AGENTS.md/GEMINI.md symlinks next to CLAUDE.md files.
    CheckClaudeSymlinks,
    /// Verify target pipeline crate skeleton structure.
    CheckRepoStructure,
    /// Audit crate structure/dependencies against docs/pipeline-unification/crates/*/README.md.
    /// Standalone (not part of `check`): failures are genuine contract drift, not false positives.
    CheckCrateContracts,
    /// Fail if any symlink in the worktree points to a non-existent target.
    CheckBrokenSymlinks,
    /// Fail if any relative markdown link in docs/reference points to a missing file.
    CheckDocLinks,
    /// Fail if generated reference docs reference a removed public surface.
    CheckDocContracts,
    /// Verify the crate dependency-graph snapshot is in sync and acyclic.
    CheckDepGraph,
    /// Regenerate docs/reference/crate-dependency-graph.md.
    GenDepGraph,
    /// Verify the per-crate public-API surface snapshot is in sync.
    CheckPublicApi,
    /// Fail when sensitive-looking log call sites bypass redaction.
    CheckRedactionLogs,
    /// Regenerate docs/reference/public-api-surface.md.
    GenPublicApi,
    /// Verify SQLite job migrations are append-only and checksum-pinned.
    CheckSqliteMigrations,
    /// Regenerate the SQLite job migration checksum manifest after adding a migration.
    UpdateSqliteMigrationChecksums,
    /// Scan staged files, or the complete tracked candidate tree, for credentials.
    CheckSecrets {
        /// Scan the checked-out tracked tree instead of the Git index.
        #[arg(long)]
        tree: bool,
    },
    /// Compatibility check for the CLI component's version-bearing files.
    /// The full multi-component gate is `check-release-versions`.
    CheckVersionSync,
    /// Regenerate and verify all tracked OpenAPI artifacts.
    CheckOpenapiDrift,
    /// Verify Android's handwritten /v1 client routes are present in OpenAPI.
    CheckAndroidApiContract,
    /// Run the path-aware local pre-push router.
    PrePush(pre_push::PrePushArgs),
    /// Generate/check clean-break pipeline schema artifacts.
    Schemas(schemas::SchemasArgs),
    /// Generate/check the docs-generator core: header rewrite, source-input
    /// manifest, repo-wide link check, and docs-inventory diff.
    Docs(docs::DocsArgs),
    /// Refresh or verify generated schemas and their dependent documentation
    /// as one ordered contract surface.
    GeneratedContracts(generated_contracts::GeneratedContractsArgs),
    /// Generate/check presentation-token artifacts (colors/typography/spacing/icons).
    Presentation(presentation::PresentationArgs),
    /// Release gating, version bumping, and release-please postprocessing.
    /// Implemented in the `xtask-release` package so CI can run these without
    /// compiling the product; flattened here so `cargo xtask <command>` is
    /// unchanged.
    #[command(flatten)]
    Release(xtask_release::ReleaseCommand),
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
    /// Benchmark the complete source pipeline against a live HTTP site.
    BenchSource {
        /// Live site root to crawl, for example https://code.claude.com/.
        url: String,
        /// Axon binary to execute. Defaults to target/debug/axon, then PATH.
        #[arg(long)]
        axon_bin: Option<PathBuf>,
        /// Run isolated cold crawls, conditional-cache recrawls, or both.
        #[arg(long, value_enum, default_value = "both")]
        scenario: bench_source::ScenarioMode,
        /// Number of measured runs per scenario.
        #[arg(long, default_value_t = 3)]
        runs: usize,
        /// Optional page cap for smoke and tuning runs.
        #[arg(long)]
        max_pages: Option<u64>,
        /// Qdrant base URL. Defaults to QDRANT_URL / AXON_QDRANT_URL.
        #[arg(long)]
        qdrant_url: Option<String>,
        /// TEI base URL used for embedding metrics. Defaults to TEI_URL.
        #[arg(long)]
        tei_url: Option<String>,
        /// JSON artifact path. Defaults under target/bench-source/.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Prior axon-bench-source/v1 artifact to compare against.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Retain isolated SQLite/cache directories and Qdrant collections.
        #[arg(long)]
        keep_state: bool,
        /// Required acknowledgement that the benchmark contacts a live site.
        #[arg(long)]
        allow_live_network: bool,
        /// Emit the complete report to stdout as JSON.
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
        Command::CheckLayering => checks::layering::check(&root),
        Command::CheckApiParity => checks::api_parity::check(&root),
        Command::GenApiParity => checks::api_parity::write(&root),
        Command::CheckMcpHttp => checks::mcp_http::check(&root),
        Command::CheckAuditIgnoreSync => checks::audit_ignore_sync::check(&root),
        Command::CheckEnvStaged => checks::env_staged::check(&root),
        Command::CheckUnwraps => checks::unwraps::check(&root),
        Command::CheckFetchDivergence => checks::fetch_divergence::check(&root),
        Command::CheckClaudeSymlinks => checks::claude_symlinks::check(&root),
        Command::CheckRepoStructure => checks::repo_structure::check(&root),
        Command::CheckCrateContracts => checks::crate_contracts::check(&root),
        Command::CheckBrokenSymlinks => checks::broken_symlinks::check(&root),
        Command::CheckDocLinks => checks::doc_links::check(&root),
        Command::CheckDocContracts => checks::doc_contracts::check(&root),
        Command::CheckDepGraph => checks::dep_graph::check(&root),
        Command::GenDepGraph => checks::dep_graph::write(&root),
        Command::CheckPublicApi => checks::public_api::check(&root),
        Command::CheckRedactionLogs => checks::redaction_logs::check(&root),
        Command::GenPublicApi => checks::public_api::write(&root),
        Command::CheckSqliteMigrations => checks::sqlite_migrations::check(&root),
        Command::UpdateSqliteMigrationChecksums => checks::sqlite_migrations::update(&root),
        Command::CheckSecrets { tree } => {
            if tree {
                checks::secrets::check_tree(&root)
            } else {
                checks::secrets::check(&root)
            }
        }
        Command::CheckVersionSync => checks::version_sync::check(&root),
        Command::CheckOpenapiDrift => checks::openapi_drift::check(&root),
        Command::CheckAndroidApiContract => checks::android_api_contract::check(&root),
        Command::PrePush(args) => pre_push::run(&root, args),
        Command::Schemas(args) => schemas::run(&root, args),
        Command::Docs(args) => docs::run(&root, args),
        Command::GeneratedContracts(args) => generated_contracts::run(&root, args),
        Command::Presentation(args) => presentation::run(&root, args),
        Command::Release(command) => Ok(xtask_release::run(&root, command)?),
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
        command @ Command::BenchSource { .. } => run_bench_source(&root, command),
    }
}

fn run_bench_source(root: &Path, command: Command) -> Result<()> {
    let Command::BenchSource {
        url,
        axon_bin,
        scenario,
        runs,
        max_pages,
        qdrant_url,
        tei_url,
        output,
        baseline,
        keep_state,
        allow_live_network,
        json,
    } = command
    else {
        unreachable!("run_bench_source requires BenchSource");
    };
    bench_source::run(
        root,
        bench_source::BenchSourceArgs {
            url,
            axon_bin,
            scenario,
            runs,
            max_pages,
            qdrant_url,
            tei_url,
            output,
            baseline,
            keep_state,
            allow_live_network,
            json,
        },
    )
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

mod bench_embed;
mod bench_source;
mod checks;
mod docs;
mod generated_contracts;
mod pre_push;
mod presentation;
pub mod schemas;
