//! Layering guardrail: transport crates must not reach into a domain crate's
//! internal modules. See `docs/architecture/crate-ownership.md` and
//! `docs/architecture/crate-structure.md` for the current 23-crate layout.
//!
//! Transports (`axon-cli`, `axon-web`, `axon-mcp`) call a typed entry point
//! (`axon-services`, or a domain crate's public `pub fn`/root re-export),
//! never a domain crate's private implementation modules (the pattern that
//! motivated this check: the CLI once imported the pre-unification
//! `axon_vector::ops::qdrant`).
//!
//! `axon-services` is not a transport — it is the composition facade and is
//! expected to import domain crates freely for wiring — but it owes the same
//! "public entry, not internal module" discipline for domain crates it
//! doesn't otherwise compose concrete implementations of. It is scanned
//! separately, against a narrower pattern list (see `SERVICES_FORBIDDEN`),
//! so that deliberate composition-root patterns (e.g. `axon-services`
//! constructing `axon_vectors::qdrant::QdrantVectorStore` directly to wire a
//! `VectorStore` implementation) are not swept in as "debt" alongside real
//! layering violations.
//!
//! Enforcement is allowlist-based: the files below already contain a reach and
//! are grandfathered (pre-existing debt). The check fails when a **new** file
//! introduces one — pay the debt down, don't extend it.

use anyhow::{Result, bail};
use std::path::Path;
use walkdir::WalkDir;

/// Domain-crate internal import prefixes that transport crates must not use
/// directly. Each names a real, live module in a crate that exists in the
/// current workspace (`ls crates/`) — verified against each crate's
/// `src/lib.rs` module list and root `pub use` re-exports, not guessed.
const FORBIDDEN: &[&str] = &[
    // axon-adapters flattens acquisition/registry/spec/testing at its crate
    // root (crates/axon-adapters/src/lib.rs) but does NOT re-export
    // `web_engine` — the raw Chrome/CDP render+scrape+screenshot engine.
    // Live violation today: crates/axon-cli/src/commands/screenshot/util.rs.
    "axon_adapters::web_engine::",
    // axon-vectors re-exports `QdrantVectorStore` at its root
    // (`pub use qdrant::QdrantVectorStore;`) but not the `qdrant` module
    // itself. Cited verbatim in crate-ownership.md as the canonical
    // domain-internal reach to avoid. No live transport violation today —
    // this guards the pattern going forward.
    "axon_vectors::qdrant::",
    // axon-prune re-exports `PruneExecutor`/`PruneTarget`/`StepExecution` at
    // its root (crates/axon-prune/src/lib.rs) but not the `executor` module
    // itself. Also cited verbatim in crate-ownership.md. No live transport
    // violation today.
    "axon_prune::executor::",
    // axon-extract's per-site vertical extractor implementations
    // (crates/axon-extract/src/lib.rs: "dispatch order and policy belong to
    // axon-adapters::vertical_registry; this crate owns only extractor
    // implementations"). No live violation today.
    "axon_extract::verticals::",
];

/// Transport crate `src` roots (repo-relative) scanned against `FORBIDDEN`.
const TRANSPORT_SRC: &[&str] = &[
    "crates/axon-cli/src",
    "crates/axon-web/src",
    "crates/axon-mcp/src",
];

/// `axon-services/src`, scanned against the narrower `SERVICES_FORBIDDEN`
/// list (see the module doc comment for why this isn't just `FORBIDDEN`).
const SERVICES_SRC: &[&str] = &["crates/axon-services/src"];

/// Reaches that are forbidden for `axon-services` specifically. Currently
/// just the `web_engine` reach — services should call through whatever public
/// scrape/screenshot/capture entry axon-adapters intends (none is flattened
/// today), not its raw render-engine internals.
const SERVICES_FORBIDDEN: &[&str] = &["axon_adapters::web_engine::"];

/// Domain crates whose only sanctioned caller is `axon-services`. A direct
/// Cargo dependency from a transport crate on any of these is a layering
/// violation before a single `use` statement is even written — transports
/// call these through the `axon-services` facade instead.
const TRANSPORT_FORBIDDEN_DEPS: &[&str] = &["axon-embedding", "axon-vectors", "axon-retrieval"];

/// Transport crate manifests checked against `TRANSPORT_FORBIDDEN_DEPS`.
const SURFACE_MANIFESTS: &[&str] = &[
    "crates/axon-cli/Cargo.toml",
    "crates/axon-web/Cargo.toml",
    "crates/axon-mcp/Cargo.toml",
];

/// Specific reaches that exist today. Grandfathered debt — do not add to this
/// list without a deliberate decision, and always attach a TODO/bead in the
/// nearby code, not just here. Matching by `(file, prefix)` prevents a whole
/// allowed file from hiding new, unrelated reaches.
const ALLOWLIST: &[(&str, &str)] = &[
    // Test-only re-export (`#[cfg(test)]`); see the TODO in the source file.
    // TODO(axon-realacq-replatform): fold into a public axon-adapters
    // screenshot-filename helper instead of reaching into `web_engine`.
    (
        "crates/axon-cli/src/commands/screenshot/util.rs",
        "axon_adapters::web_engine::",
    ),
    // TODO(axon-realacq-replatform): route through a public axon-adapters
    // scrape entry point instead of `web_engine::scrape::*` directly.
    (
        "crates/axon-services/src/scrape.rs",
        "axon_adapters::web_engine::",
    ),
    // TODO(axon-realacq-replatform): route through a public axon-adapters
    // screenshot entry point instead of `web_engine::screenshot::*` directly.
    (
        "crates/axon-services/src/screenshot.rs",
        "axon_adapters::web_engine::",
    ),
    // TODO(axon-realacq-replatform): route through a public axon-adapters
    // CDP-resolution entry point instead of `web_engine::engine::*` directly.
    (
        "crates/axon-services/src/endpoints/capture.rs",
        "axon_adapters::web_engine::",
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
            for krate in TRANSPORT_FORBIDDEN_DEPS {
                if table.contains_key(*krate) {
                    violations.push(format!(
                        "{manifest} declares [{table_name}] dependency on `{krate}` — \
                         transports must go through the axon-services facade, not depend on \
                         this domain crate directly"
                    ));
                }
            }
        }
    }
}

/// Scan every `.rs` file under each of `src_roots` for the first matching
/// entry in `forbidden`, skipping test files and allowlisted `(file, prefix)`
/// pairs, appending human-readable violation strings to `violations`.
fn scan_reaches(root: &Path, src_roots: &[&str], forbidden: &[&str], violations: &mut Vec<String>) {
    for src in src_roots {
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
                if let Some(pat) = forbidden.iter().find(|p| line.contains(**p)) {
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
}

pub fn check(root: &Path) -> Result<()> {
    let mut violations: Vec<String> = Vec::new();

    check_surface_manifests(root, &mut violations);
    scan_reaches(root, TRANSPORT_SRC, FORBIDDEN, &mut violations);
    scan_reaches(root, SERVICES_SRC, SERVICES_FORBIDDEN, &mut violations);

    if violations.is_empty() {
        println!("OK: no new transport→domain-internal reaches.");
        return Ok(());
    }

    eprintln!("ERROR: transport/services crates reach into domain-crate internals:");
    for v in &violations {
        eprintln!("  {v}");
    }
    eprintln!(
        "\nTransports and axon-services must call a typed entry point (the axon-services\n\
         facade or a domain crate's public `pub fn` / root re-export), not a domain\n\
         crate's private implementation modules. See docs/architecture/crate-ownership.md.\n\
         If this is a deliberate, reviewed exception, add the exact (file, prefix) reach\n\
         to ALLOWLIST in xtask/src/checks/layering.rs with a TODO naming the follow-up."
    );
    bail!(
        "layering violation: {} reach(es)\n{}",
        violations.len(),
        violations.join("\n")
    );
}
