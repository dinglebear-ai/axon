//! `cargo xtask docs generate` — rewrite generated-doc headers to cite
//! `cargo xtask docs generate` and (re)write the source-input manifest.

use std::path::Path;

use anyhow::{Result, bail};
use walkdir::WalkDir;

use super::DocsGenerateArgs;
use super::header;
use super::manifest::{self, DocsManifest};

pub fn run(root: &Path, args: &DocsGenerateArgs) -> Result<()> {
    let manifest = manifest::build(root)?;
    let plan = build_plan(root, &manifest, args.family.as_deref())?;

    if args.check {
        return check_plan(root, &plan);
    }
    write_plan(root, &plan)
}

struct Plan {
    manifest_json: String,
    docs: Vec<(std::path::PathBuf, String)>,
}

fn build_plan(root: &Path, manifest: &DocsManifest, family: Option<&str>) -> Result<Plan> {
    if let Some(family) = family {
        if !manifest.families.iter().any(|f| f.family == family) {
            bail!("docs generate: unknown family `{family}` (no generated JSON artifact cites it)");
        }
    }

    let docs_root = root.join("docs/reference");
    let mut docs = Vec::new();
    if docs_root.is_dir() {
        for entry in WalkDir::new(&docs_root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = std::fs::read_to_string(path)?;
            let Some(first_line) = content.lines().next() else {
                continue;
            };
            let Some(slug) = header::family_slug(first_line) else {
                continue;
            };
            if let Some(family) = family {
                if slug != family {
                    continue;
                }
            }
            let Some(fam) = manifest.families.iter().find(|f| f.family == slug) else {
                continue;
            };
            let rewritten = header::rewrite(&content, &slug, &fam.manifest_checksum)?;
            docs.push((path.to_path_buf(), rewritten));
        }
    }
    docs.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(Plan {
        manifest_json: manifest::to_json(manifest)?,
        docs,
    })
}

fn write_plan(root: &Path, plan: &Plan) -> Result<()> {
    let manifest_path = root.join(manifest::MANIFEST_PATH);
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&manifest_path, &plan.manifest_json)?;
    for (path, content) in &plan.docs {
        std::fs::write(path, content)?;
    }
    println!(
        "docs generate: wrote {} manifest and rewrote {} doc header(s).",
        1,
        plan.docs.len()
    );
    Ok(())
}

fn check_plan(root: &Path, plan: &Plan) -> Result<()> {
    let mut drift = Vec::new();
    let manifest_path = root.join(manifest::MANIFEST_PATH);
    match std::fs::read_to_string(&manifest_path) {
        Ok(existing) if existing == plan.manifest_json => {}
        Ok(_) => drift.push(format!(
            "{} differs; run `cargo xtask docs generate`",
            manifest::MANIFEST_PATH
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => drift.push(format!(
            "{} is missing; run `cargo xtask docs generate`",
            manifest::MANIFEST_PATH
        )),
        Err(err) => return Err(err.into()),
    }
    for (path, content) in &plan.docs {
        let existing = std::fs::read_to_string(path)?;
        if &existing != content {
            drift.push(format!(
                "{} header/manifest citation is stale; run `cargo xtask docs generate`",
                path.strip_prefix(root).unwrap_or(path).display()
            ));
        }
    }
    if !drift.is_empty() {
        bail!(
            "docs generate --check: generated docs are stale:\n{}",
            drift.join("\n")
        );
    }
    println!("docs generate --check: up to date.");
    Ok(())
}

#[cfg(test)]
#[path = "generate_tests.rs"]
mod tests;
