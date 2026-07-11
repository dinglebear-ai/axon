//! `cargo xtask presentation` — the presentation-token generator described
//! by `docs/pipeline-unification/surfaces/presentation-contract.md`.
//!
//! Reads the single canonical token source (`presentation/source.json`) and
//! emits the contract-named platform projections (web/Palette/extension CSS,
//! Android Kotlin, CLI Rust consts) plus reference docs. Idempotent: running
//! `generate` twice with an unchanged source produces byte-identical files.
//! Consumption by the apps themselves is an intentional follow-up — see the
//! generated `docs/reference/presentation/README.md`.

mod emit_css;
mod emit_docs;
mod emit_kotlin;
mod emit_rust;
mod header;
mod model;
mod readme;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use model::TokenSource;

#[derive(Debug, Args)]
pub struct PresentationArgs {
    #[command(subcommand)]
    command: PresentationCommand,
}

#[derive(Debug, Subcommand)]
enum PresentationCommand {
    /// Write all generated presentation-token artifacts.
    Generate(GenerateArgs),
    /// Verify generated artifacts match the canonical source without writing.
    Check,
}

#[derive(Debug, Args, Clone, Default)]
struct GenerateArgs {
    /// Compute output in memory and fail on drift instead of writing files.
    #[arg(long)]
    check: bool,
}

pub fn run(root: &Path, args: PresentationArgs) -> Result<()> {
    match args.command {
        PresentationCommand::Generate(g) => generate(root, g.check),
        PresentationCommand::Check => generate(root, true),
    }
}

/// One (path, contents) artifact this generator owns.
fn artifacts(root: &Path, src: &TokenSource) -> Vec<(PathBuf, String)> {
    let css = emit_css::render(src);
    vec![
        (
            root.join("docs/reference/presentation/tokens.md"),
            emit_docs::render_markdown(src),
        ),
        (
            root.join("docs/reference/presentation/tokens.schema.json"),
            emit_docs::render_schema(src),
        ),
        (
            root.join("docs/reference/presentation/README.md"),
            readme::render(src),
        ),
        (
            root.join("apps/web/src/styles/axon-tokens.css"),
            css.clone(),
        ),
        (
            root.join("apps/palette-tauri/src/styles/axon-tokens.css"),
            css.clone(),
        ),
        (
            root.join("apps/chrome-extension/src/styles/axon-tokens.css"),
            css,
        ),
        (
            root.join(
                "apps/android/app/src/main/java/com/axon/app/ui/theme/generated/AxonTokens.kt",
            ),
            emit_kotlin::render(src),
        ),
        (
            root.join("crates/axon-cli/src/ui/tokens.rs"),
            emit_rust::render(src),
        ),
    ]
}

fn generate(root: &Path, check: bool) -> Result<()> {
    let src = TokenSource::load()?;
    let items = artifacts(root, &src);

    if !check {
        for (path, contents) in &items {
            write_if_changed(path, contents)?;
        }
        println!(
            "presentation generate: wrote {} artifact(s) (contract {}, hash {})",
            items.len(),
            src.contract_version,
            src.source_hash()
        );
        return Ok(());
    }

    let mut drift = Vec::new();
    for (path, expected) in &items {
        let actual = fs::read_to_string(path).unwrap_or_default();
        if &actual != expected {
            drift.push(path.clone());
        }
    }
    if drift.is_empty() {
        println!("presentation check: {} artifact(s) up to date", items.len());
        Ok(())
    } else {
        for path in &drift {
            eprintln!("presentation check: drift in {}", path.display());
        }
        bail!(
            "presentation check: {} artifact(s) out of date; run `cargo xtask presentation generate`",
            drift.len()
        );
    }
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing == contents {
        return Ok(());
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
