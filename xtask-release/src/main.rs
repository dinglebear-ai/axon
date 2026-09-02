//! Standalone release-gate binary.
//!
//! Exists so CI jobs that only do release bookkeeping can build this package
//! alone instead of compiling all of Axon through `xtask`. The command surface
//! is the same [`xtask_release::ReleaseCommand`] that `xtask` flattens, so both
//! binaries accept identical arguments.

use clap::Parser;
use xtask_release::ReleaseCommand;

#[derive(Debug, Parser)]
#[command(
    name = "xtask-release",
    about = "Axon component release gating and version bumping"
)]
struct Cli {
    #[command(subcommand)]
    command: ReleaseCommand,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let root = std::env::current_dir()?;
    xtask_release::run(&root, cli.command)?;
    Ok(())
}
