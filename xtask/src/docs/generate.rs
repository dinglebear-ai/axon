//! In-memory docs rendering and byte-for-byte drift checking.

use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;

use super::DocsGenerateArgs;
use super::artifact::DocsArtifactSet;
use super::families::{self, DocsFamily};

#[derive(Debug, Serialize)]
struct FamilyReport {
    family: String,
    artifacts: usize,
    status: &'static str,
}

pub fn run(root: &Path, args: &DocsGenerateArgs) -> Result<()> {
    if args.update_snapshots && std::env::var_os("CI").is_some() {
        bail!("docs generate: --update-snapshots is forbidden in CI");
    }
    if args.print && args.json {
        bail!("docs generate: --print and --json are mutually exclusive");
    }
    let selected = args
        .family
        .map_or_else(|| DocsFamily::ALL.to_vec(), |family| vec![family]);
    let sets = selected
        .into_iter()
        .map(|family| {
            let generator = families::generator_for(family);
            debug_assert_eq!(generator.family(), family);
            generator.generate(root)
        })
        .collect::<Result<Vec<_>>>()?;
    let rerendered = sets
        .iter()
        .map(|set| {
            let generator = families::generator_for(set.family);
            generator.generate(root)
        })
        .collect::<Result<Vec<_>>>()?;
    if sets != rerendered {
        bail!("docs generate: renderer output is not deterministic");
    }
    if args.print {
        for set in &sets {
            for artifact in &set.artifacts {
                println!("--- {}", artifact.path.display());
                print!("{}", artifact.content);
            }
        }
        return Ok(());
    }
    if args.check {
        check(root, &sets)?;
    } else {
        write(root, &sets)?;
    }
    if args.family.is_none() {
        if args.check {
            super::manifest::check(root)?;
        } else {
            super::manifest::refresh(root)?;
        }
    }
    if args.json {
        let reports = sets
            .iter()
            .map(|set| FamilyReport {
                family: set.family.slug().to_owned(),
                artifacts: set.artifacts.len(),
                status: if args.check { "checked" } else { "written" },
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&reports)?);
    }
    Ok(())
}

pub fn run_single(root: &Path, family: DocsFamily, args: &DocsGenerateArgs) -> Result<()> {
    if args.family.is_some() {
        bail!(
            "docs {}: --family is only valid with `docs generate`",
            family.slug()
        );
    }
    let mut args = args.clone();
    args.family = Some(family);
    run(root, &args)
}

fn write(root: &Path, sets: &[DocsArtifactSet]) -> Result<()> {
    let mut count = 0;
    for set in sets {
        for artifact in &set.artifacts {
            let path = root.join(&artifact.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &artifact.content)?;
            count += 1;
        }
    }
    println!("docs generate: wrote {count} artifact(s).");
    Ok(())
}

fn check(root: &Path, sets: &[DocsArtifactSet]) -> Result<()> {
    let mut drift = Vec::new();
    for set in sets {
        for artifact in &set.artifacts {
            let path = root.join(&artifact.path);
            match std::fs::read_to_string(&path) {
                Ok(existing) if existing == artifact.content => {}
                Ok(_) => drift.push(format!(
                    "{} differs; run `cargo xtask docs generate --family {}`",
                    artifact.path.display(),
                    set.family.slug()
                )),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => drift.push(format!(
                    "{} is missing; run `cargo xtask docs generate --family {}`",
                    artifact.path.display(),
                    set.family.slug()
                )),
                Err(err) => return Err(err.into()),
            }
        }
    }
    if drift.is_empty() {
        println!("docs generate --check: up to date.");
        Ok(())
    } else {
        bail!(
            "docs generate --check: generated docs are stale:\n{}",
            drift.join("\n")
        );
    }
}

#[cfg(test)]
#[path = "generate_tests.rs"]
mod tests;
