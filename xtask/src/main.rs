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
    /// Enforce crate ownership: transports must not reach into domain internals.
    CheckLayering,
    /// Verify docs/reference/api-parity.md is in sync with the source surfaces.
    CheckApiParity,
    /// Regenerate docs/reference/api-parity.md from the CLI/MCP/REST surfaces.
    GenApiParity,
    /// Verify MCP HTTP transport support.
    CheckMcpHttp,
    /// Reject staged secret env files.
    CheckEnvStaged,
    /// Warn about newly staged unwrap/expect calls.
    CheckUnwraps,
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
    /// Regenerate docs/reference/public-api-surface.md.
    GenPublicApi,
    /// Verify SQLite job migrations are append-only and checksum-pinned.
    CheckSqliteMigrations,
    /// Regenerate the SQLite job migration checksum manifest after adding a migration.
    UpdateSqliteMigrationChecksums,
    /// Scan staged files for secrets and credentials.
    CheckSecrets,
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
    /// Manually bump one component's version-bearing files. Only `cli` is
    /// expected to need this — see the doc comment on
    /// `checks::release_versions::bump_component_version`.
    BumpVersion {
        #[arg(long, default_value = "cli")]
        component: String,
        #[arg(value_enum)]
        level: checks::release_versions::BumpLevel,
    },
    /// Apply release-please postprocessing for files it cannot update directly.
    ReleasePleaseFixups {
        #[arg(long)]
        component: String,
        #[arg(long)]
        version: String,
    },
    /// Print release-please postprocessing needed for a release PR file list.
    ReleasePleaseFixupPlan {
        #[arg(long)]
        files: String,
        #[arg(long)]
        json: bool,
    },
    /// Print the artifact workflow dispatch plan from release-please outputs.
    ReleasePleaseDispatchPlan {
        #[arg(long)]
        release_outputs: String,
        #[arg(long)]
        json: bool,
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
        Command::CheckLayering => checks::layering::check(&root),
        Command::CheckApiParity => checks::api_parity::check(&root),
        Command::GenApiParity => checks::api_parity::write(&root),
        Command::CheckMcpHttp => checks::mcp_http::check(&root),
        Command::CheckEnvStaged => checks::env_staged::check(&root),
        Command::CheckUnwraps => checks::unwraps::check(&root),
        Command::CheckClaudeSymlinks => checks::claude_symlinks::check(&root),
        Command::CheckRepoStructure => checks::repo_structure::check(&root),
        Command::CheckCrateContracts => checks::crate_contracts::check(&root),
        Command::CheckBrokenSymlinks => checks::broken_symlinks::check(&root),
        Command::CheckDocLinks => checks::doc_links::check(&root),
        Command::CheckDocContracts => checks::doc_contracts::check(&root),
        Command::CheckDepGraph => checks::dep_graph::check(&root),
        Command::GenDepGraph => checks::dep_graph::write(&root),
        Command::CheckPublicApi => checks::public_api::check(&root),
        Command::GenPublicApi => checks::public_api::write(&root),
        Command::CheckSqliteMigrations => checks::sqlite_migrations::check(&root),
        Command::UpdateSqliteMigrationChecksums => checks::sqlite_migrations::update(&root),
        Command::CheckSecrets => checks::secrets::check(&root),
        Command::CheckVersionSync => checks::version_sync::check(&root),
        Command::CheckOpenapiDrift => checks::openapi_drift::check(&root),
        Command::CheckAndroidApiContract => checks::android_api_contract::check(&root),
        Command::PrePush(args) => pre_push::run(&root, args),
        Command::Schemas(args) => schemas::run(&root, args),
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
        Command::BumpVersion { component, level } => Ok(
            checks::release_versions::bump_component_version(&root, &component, level)?,
        ),
        Command::ReleasePleaseFixups { component, version } => Ok(
            checks::release_versions::release_please_fixups(&root, &component, &version)?,
        ),
        Command::ReleasePleaseFixupPlan { files, json } => {
            let items = checks::release_versions::release_please_fixup_plan(&root, &files)?;
            checks::release_versions::print_release_please_fixup_plan(&items, json)?;
            Ok(())
        }
        Command::ReleasePleaseDispatchPlan {
            release_outputs,
            json,
        } => {
            let items =
                checks::release_versions::release_please_dispatch_plan(&root, &release_outputs)?;
            checks::release_versions::print_release_please_dispatch_plan(&items, json)?;
            Ok(())
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
mod pre_push;
pub mod schemas;
