//! Render sixteen human-reference families from canonical generated JSON.
//! The generator owns complete bodies, provenance headers, and the source-input
//! manifest. Check mode compares without writes and also validates repository
//! links, active contracts, inventory, and marked examples. The aggregate
//! generated-contracts command refreshes schemas and ancillary renderers first.

mod artifact;
mod examples;
mod families;
mod generate;
mod inventory;
mod links;
mod manifest;

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

pub(crate) fn refresh_generated_contracts(root: &Path) -> Result<()> {
    generate::run(root, &DocsGenerateArgs::default())
}

pub(crate) fn check_generated_contracts(root: &Path) -> Result<()> {
    let args = DocsGenerateArgs {
        check: true,
        ..DocsGenerateArgs::default()
    };
    generate::run(root, &args)?;
    check(root)
}

/// `docs check`: repo-wide link check, the existing removed-surface doc
/// contract check, a docs-inventory-vs-Final-Docs-Tree diff, and the
/// marker-annotated example-validation pass. All four run and report; the
/// first failure's message is what propagates, but every check runs so a
/// single invocation surfaces everything.
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

#[cfg(test)]
#[path = "docs_tests.rs"]
mod tests;
