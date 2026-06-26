//! Layering guardrail: transport crates must not reach into a domain crate's
//! internal modules. See `docs/architecture/crate-ownership.md`.
//!
//! Transports (`axon-cli`, `axon-web`, `axon-mcp`) call a typed entry point
//! (`axon-services`, or a domain crate's public `pub fn`), never a domain
//! crate's `::ops::` / engine / extractor internals. This is exactly the bug the
//! `purge` retrofit fixed (the CLI imported `axon_vector::ops::qdrant`).
//!
//! Enforcement is allowlist-based: the files below already contain a reach and
//! are grandfathered (pre-existing debt). The check fails when a **new** file in
//! a transport crate introduces one — pay the debt down, don't extend it.

use anyhow::{Result, bail};
use std::path::Path;
use walkdir::WalkDir;

/// Domain-crate internal import prefixes that transports must not use directly.
const FORBIDDEN: &[&str] = &[
    "axon_vector::ops::",
    "axon_crawl::engine::",
    "axon_extract::verticals::",
    "axon_extract::registry::",
];

/// Transport crate `src` roots (repo-relative) that the rule applies to.
const TRANSPORT_SRC: &[&str] = &[
    "crates/axon-cli/src",
    "crates/axon-web/src",
    "crates/axon-mcp/src",
];

/// Files that already contain a reach when the rule was introduced. Grandfathered
/// debt — do not add to this list without a deliberate decision (each entry is a
/// candidate to push down to a domain-crate service entry).
const ALLOWLIST: &[&str] = &[
    "crates/axon-cli/src/commands/crawl/audit/sitemap.rs",
    "crates/axon-cli/src/commands/scrape.rs",
    "crates/axon-cli/src/commands/sources.rs",
    "crates/axon-cli/src/commands/stats.rs",
    "crates/axon-mcp/src/server/artifacts/respond.rs",
    "crates/axon-web/src/server/handlers/rest/sync_post.rs",
];

fn is_test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.split('/').any(|c| c == "tests")
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
}

pub fn check(root: &Path) -> Result<()> {
    let mut violations: Vec<String> = Vec::new();

    for src in TRANSPORT_SRC {
        let dir = root.join(src);
        for entry in WalkDir::new(&dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if is_test_file(&rel) || ALLOWLIST.contains(&rel.as_str()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            for (lineno, line) in text.lines().enumerate() {
                if let Some(pat) = FORBIDDEN.iter().find(|p| line.contains(**p)) {
                    violations.push(format!("{rel}:{}  reaches `{pat}`", lineno + 1));
                }
            }
        }
    }

    if violations.is_empty() {
        println!("OK: no new transport→domain-internal reaches.");
        return Ok(());
    }

    eprintln!("ERROR: transport crates reach into domain-crate internals:");
    for v in &violations {
        eprintln!("  {v}");
    }
    eprintln!(
        "\nTransports must call a typed entry point (axon-services facade or a domain\n\
         crate's public `pub fn`), not `::ops::`/engine internals. See\n\
         docs/architecture/crate-ownership.md. If this is a deliberate, reviewed\n\
         exception, add the file to ALLOWLIST in xtask/src/checks/layering.rs."
    );
    bail!("layering violation: {} reach(es)", violations.len());
}
