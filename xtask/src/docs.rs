//! `cargo xtask docs` — generated-reference and living-doc validation.
//!
//! The `generate` verb renders governed reference artifacts. The `check`
//! verb also validates links, removed surfaces, the required living-doc tree,
//! action-page inventory/drift, and marker-annotated examples.
//!
//! `docs generate` renders the governed Markdown families from the generated
//! JSON contracts under `docs/reference/**` and emits a repository-wide
//! source-input manifest from each contract's `x-axon.source_inputs`
//! metadata. `docs check` recomputes both products in memory and compares
//! them byte-for-byte with the tracked artifacts.

mod artifact;
mod examples;
mod families;
mod generate;
mod inventory;
mod links;
mod manifest;

#[cfg(test)]
pub(crate) use families::generated_output_paths;

use std::path::Path;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct DocsArgs {
    #[command(subcommand)]
    command: DocsCommand,
}

#[derive(Debug, Subcommand)]
enum DocsCommand {
    /// Render every implemented documentation family.
    Generate(DocsGenerateArgs),
    /// Validate generated documentation without writing any files.
    Check(DocsGenerateArgs),
    /// Render one critical documentation family.
    ApiDto(DocsGenerateArgs),
    /// Render one critical documentation family.
    ApiEnums(DocsGenerateArgs),
    /// Render one critical documentation family.
    Events(DocsGenerateArgs),
    /// Render one critical documentation family.
    Providers(DocsGenerateArgs),
    /// Render one critical documentation family.
    Schema(DocsGenerateArgs),
}

#[derive(Debug, Args, Clone, Default)]
pub struct DocsGenerateArgs {
    /// Compute the desired output in memory and fail if it differs from
    /// what's on disk, without writing anything.
    #[arg(long)]
    pub check: bool,
    /// Restrict to one family slug (e.g. `cli`, `openapi`, `mcp`).
    #[arg(long)]
    pub family: Option<families::DocsFamily>,
    /// Print generated content rather than writing it.
    #[arg(long)]
    pub print: bool,
    /// Emit a machine-readable generation/check report.
    #[arg(long)]
    pub json: bool,
    /// Reserved for fixture maintenance; never permitted in CI.
    #[arg(long)]
    pub update_snapshots: bool,
}

pub fn run(root: &Path, args: DocsArgs) -> Result<()> {
    match args.command {
        DocsCommand::Generate(gen_args) => generate::run(root, &gen_args),
        DocsCommand::Check(mut args) => {
            args.check = true;
            generate::run(root, &args)?;
            check(root)
        }
        DocsCommand::ApiDto(args) => {
            generate::run_single(root, families::DocsFamily::ApiDto, &args)
        }
        DocsCommand::ApiEnums(args) => {
            generate::run_single(root, families::DocsFamily::ApiEnums, &args)
        }
        DocsCommand::Events(args) => {
            generate::run_single(root, families::DocsFamily::Events, &args)
        }
        DocsCommand::Providers(args) => {
            generate::run_single(root, families::DocsFamily::Providers, &args)
        }
        DocsCommand::Schema(args) => {
            generate::run_single(root, families::DocsFamily::Schema, &args)
        }
    }
}

/// `docs check`: repo-wide links, removed surfaces, required living docs,
/// action-page inventory/drift, and marker-annotated examples. Every check
/// runs so one invocation surfaces the full failure set.
fn check(root: &Path) -> Result<()> {
    let mut failures = Vec::new();

    if let Err(err) = links::check_repo_wide(root) {
        failures.push(err.to_string());
    }
    if let Err(err) = crate::checks::doc_contracts::check(root) {
        failures.push(err.to_string());
    }
    if let Err(err) = inventory::check(root) {
        failures.push(err.to_string());
    }
    if let Err(err) = check_action_pages(root) {
        failures.push(err.to_string());
    }
    if let Err(err) = examples::check(root) {
        failures.push(err.to_string());
    }

    if failures.is_empty() {
        println!("docs check: all checks passed.");
        return Ok(());
    }
    anyhow::bail!(
        "docs check: {} check(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn check_action_pages(root: &Path) -> Result<()> {
    let output = std::process::Command::new("python3")
        .arg("scripts/generate_action_docs.py")
        .arg("--check")
        .current_dir(root)
        .output()?;
    if output.status.success() {
        println!("action docs: current CLI groups and generated surfaces are in sync.");
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    anyhow::bail!("action docs check failed:\n{stdout}{stderr}")
}

#[cfg(test)]
#[path = "docs_tests.rs"]
mod tests;
