//! Ordered refresh and validation for generated schemas and their docs.

use std::path::Path;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct GeneratedContractsArgs {
    #[command(subcommand)]
    command: GeneratedContractsCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
enum GeneratedContractsCommand {
    /// Refresh schema fixtures and artifacts, then render dependent docs.
    Refresh,
    /// Check schema drift first, then dependent docs and docs-wide contracts.
    Check,
}

pub fn run(root: &Path, args: GeneratedContractsArgs) -> Result<()> {
    run_with(
        args.command,
        |check| {
            if check {
                crate::schemas::check_generated_contracts(root)
            } else {
                crate::schemas::refresh_generated_contracts(root)
            }
        },
        |check| {
            if check {
                crate::docs::check_generated_contracts(root)
            } else {
                crate::docs::refresh_generated_contracts(root)
            }
        },
    )
}

#[cfg(test)]
fn refresh_fixture(root: &Path) -> Result<()> {
    run_with(
        GeneratedContractsCommand::Refresh,
        |_| crate::schemas::refresh_generated_contracts_fixture(root),
        |_| crate::docs::refresh_generated_contracts(root),
    )
}

fn run_with<S, D>(command: GeneratedContractsCommand, mut schemas: S, mut docs: D) -> Result<()>
where
    S: FnMut(bool) -> Result<()>,
    D: FnMut(bool) -> Result<()>,
{
    let check = matches!(command, GeneratedContractsCommand::Check);
    schemas(check)?;
    docs(check)
}

#[cfg(test)]
#[path = "generated_contracts_tests.rs"]
mod tests;
