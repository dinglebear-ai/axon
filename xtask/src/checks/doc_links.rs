//! `cargo xtask check-doc-links` — relative-link checker for generated reference docs.
//!
//! Walks `docs/reference/**/*.md` and verifies every relative markdown link target
//! resolves to a file that exists on disk. External links (`http(s)://`, `mailto:`,
//! `tel:`, protocol-relative `//`) and pure in-page anchors (`#section`) are skipped;
//! a `#fragment`/`?query` suffix on a relative path is stripped before resolution.
//!
//! This is the `check-doc-links` deliverable from `docs-generator-contract.md`
//! ("link checker fails" drift). It is intentionally standalone (does not require the
//! full `xtask docs` generator) so broken links in already-generated reference docs
//! fail CI today.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// A relative markdown link whose target does not exist on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    /// Repo-relative path of the markdown file containing the link.
    pub source: String,
    /// The raw link target as written in the markdown.
    pub target: String,
}

pub fn check(root: &Path) -> Result<()> {
    let docs_root = root.join("docs/reference");
    if !docs_root.is_dir() {
        // Nothing generated yet — not a failure.
        println!("check-doc-links: docs/reference absent, skipping.");
        return Ok(());
    }
    let mut broken = Vec::new();
    let mut checked = 0usize;
    for entry in WalkDir::new(&docs_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        checked += 1;
        let content = std::fs::read_to_string(path)?;
        let dir = path.parent().unwrap_or(root);
        for target in extract_relative_link_targets(&content) {
            if link_target_exists(dir, &target) {
                continue;
            }
            broken.push(BrokenLink {
                source: rel(root, path),
                target,
            });
        }
    }
    if !broken.is_empty() {
        let mut msg = format!("check-doc-links: {} broken link(s):\n", broken.len());
        for b in &broken {
            msg.push_str(&format!("  {} -> {}\n", b.source, b.target));
        }
        bail!(msg);
    }
    println!("check-doc-links: {checked} markdown file(s), no broken relative links.");
    Ok(())
}

/// Extract the raw target of every inline markdown link/image (`](target)`) that
/// looks like a relative path we should resolve. Skips external schemes and pure
/// anchors. Reference-style links (`[a][b]`) are not resolved.
pub fn extract_relative_link_targets(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            // Read until the matching ')'.
            let start = i + 2;
            let mut j = start;
            let mut depth = 1;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j <= bytes.len() && j > start {
                let raw = content[start..j].trim();
                // A link target may carry a " title" after the URL: `](url "t")`.
                let target = raw.split_whitespace().next().unwrap_or(raw);
                if is_resolvable_relative_target(target) {
                    out.push(target.to_string());
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

fn is_resolvable_relative_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    // Pure in-page anchor.
    if target.starts_with('#') {
        return false;
    }
    // External / non-path schemes and protocol-relative URLs.
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with("//")
        || lower.contains("://")
    {
        return false;
    }
    true
}

fn link_target_exists(dir: &Path, target: &str) -> bool {
    // Strip a `#fragment` and/or `?query` suffix before resolving the path part.
    let path_part = target
        .split('#')
        .next()
        .unwrap_or(target)
        .split('?')
        .next()
        .unwrap_or(target);
    if path_part.is_empty() {
        // Link was a pure fragment/query against the same file.
        return true;
    }
    let candidate: PathBuf = dir.join(path_part);
    candidate.exists()
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "doc_links_tests.rs"]
mod tests;
