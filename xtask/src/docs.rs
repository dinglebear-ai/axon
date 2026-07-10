//! `cargo xtask docs` — the docs-generator command family described by
//! `docs-generator-contract.md`.
//!
//! This is the CORE slice only: `generate` and `check` verbs. The full
//! contract describes 17 per-family subcommands, an example-validation
//! harness, presentation-token generation, and CI wiring — those are
//! intentionally deferred (see the wave summary in the delivering PR).
//!
//! `docs generate` does not re-render markdown from scratch (that job
//! belongs to the frozen `schemas` generator). It post-processes the
//! already-generated docs under `docs/reference/**` that `schemas generate`
//! produces: it rewrites their header comment to cite `cargo xtask docs
//! generate` (per the contract's "Generated Header" section) and emits a
//! repo-wide source-input manifest built from the `x-axon.source_inputs`
//! metadata already embedded in each family's generated JSON schema
//! artifact.

mod generate;
mod header;
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
    /// Rewrite generated-doc headers and (re)write the source-input manifest.
    Generate(DocsGenerateArgs),
    /// Run docs drift/link/inventory checks without writing any files.
    Check,
}

#[derive(Debug, Args, Clone, Default)]
pub struct DocsGenerateArgs {
    /// Compute the desired output in memory and fail if it differs from
    /// what's on disk, without writing anything.
    #[arg(long)]
    pub check: bool,
    /// Restrict to one family slug (e.g. `cli`, `openapi`, `mcp`).
    #[arg(long)]
    pub family: Option<String>,
}

pub fn run(root: &Path, args: DocsArgs) -> Result<()> {
    match args.command {
        DocsCommand::Generate(gen_args) => generate::run(root, &gen_args),
        DocsCommand::Check => check(root),
    }
}

/// `docs check`: repo-wide link check, the existing removed-surface doc
/// contract check, and a docs-inventory-vs-Final-Docs-Tree diff. All three
/// run and report; the first failure's message is what propagates, but every
/// check runs so a single invocation surfaces everything.
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
