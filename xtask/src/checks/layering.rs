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
    "axon_crawl::engine::",
    "axon_extract::registry::",
    "axon_extract::verticals::",
    "axon_ingest::github::",
    "axon_ingest::rss::",
    "axon_ingest::youtube::",
    "axon_vector::ops::",
];

/// Transport crate `src` roots (repo-relative) that the rule applies to.
const TRANSPORT_SRC: &[&str] = &[
    "crates/axon-cli/src",
    "crates/axon-web/src",
    "crates/axon-mcp/src",
];

/// PR9 provider crates are implementation targets only until the source pipeline
/// cutover PR explicitly wires them through services.
const SURFACE_MANIFESTS: &[&str] = &[
    "crates/axon-cli/Cargo.toml",
    "crates/axon-web/Cargo.toml",
    "crates/axon-mcp/Cargo.toml",
];

const PR9_PRIVATE_CRATES: &[&str] = &["axon-embedding", "axon-vectors", "axon-retrieval"];

/// Specific reaches that existed when the rule was introduced. Grandfathered
/// debt — do not add to this list without a deliberate decision. Matching by
/// `(file, prefix)` prevents a whole allowed file from hiding new reaches.
const ALLOWLIST: &[(&str, &str)] = &[
    (
        "crates/axon-cli/src/commands/crawl/audit/sitemap.rs",
        "axon_crawl::engine::",
    ),
    (
        "crates/axon-cli/src/commands/scrape.rs",
        "axon_crawl::engine::",
    ),
    (
        "crates/axon-cli/src/commands/scrape.rs",
        "axon_vector::ops::",
    ),
    (
        "crates/axon-cli/src/commands/sources.rs",
        "axon_vector::ops::",
    ),
    (
        "crates/axon-cli/src/commands/stats.rs",
        "axon_vector::ops::",
    ),
    (
        "crates/axon-mcp/src/server/artifacts/respond.rs",
        "axon_crawl::engine::",
    ),
    (
        "crates/axon-mcp/src/server/artifacts/respond.rs",
        "axon_vector::ops::",
    ),
    (
        "crates/axon-web/src/server/handlers/rest/sync_post.rs",
        "axon_crawl::engine::",
    ),
    (
        "crates/axon-web/src/server/handlers/rest/sync_post.rs",
        "axon_vector::ops::",
    ),
];

fn is_test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.split('/').any(|c| c == "tests")
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
}

fn check_surface_manifests(root: &Path, violations: &mut Vec<String>) {
    for manifest in SURFACE_MANIFESTS {
        let path = root.join(manifest);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<toml::Table>(&text) else {
            continue;
        };
        for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(table) = parsed.get(table_name).and_then(toml::Value::as_table) else {
                continue;
            };
            for krate in PR9_PRIVATE_CRATES {
                if table.contains_key(*krate) {
                    violations.push(format!(
                        "{manifest} declares [{table_name}] dependency on `{krate}` before the public cutover"
                    ));
                }
            }
        }
    }
}

pub fn check(root: &Path) -> Result<()> {
    let mut violations: Vec<String> = Vec::new();

    check_surface_manifests(root, &mut violations);

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
            if is_test_file(&rel) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            for (lineno, line) in text.lines().enumerate() {
                if let Some(pat) = FORBIDDEN.iter().find(|p| line.contains(**p)) {
                    if ALLOWLIST
                        .iter()
                        .any(|(allowed_rel, allowed_pat)| rel == *allowed_rel && pat == allowed_pat)
                    {
                        continue;
                    }
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
         exception, add the exact (file, prefix) reach to ALLOWLIST in\n\
         xtask/src/checks/layering.rs."
    );
    bail!(
        "layering violation: {} reach(es)\n{}",
        violations.len(),
        violations.join("\n")
    );
}
