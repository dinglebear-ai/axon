use anyhow::Result;
use clap::{Parser, Subcommand};

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
    /// Verify the CLI component's version-bearing files (Cargo.toml, README.md,
    /// CHANGELOG.md, apps/web/package.json, apps/web/openapi/axon.json) carry the
    /// same version, and that plugin.json carries none.
    CheckVersionSync,
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
    }
}

mod checks;
