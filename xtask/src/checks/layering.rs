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
//! Known cutover debt is represented by exact `(file, pattern, bead)` temporary
//! exceptions. The check fails when a **new** file introduces the same reach;
//! exceptions must be deleted by the owning bead, never widened.

use anyhow::{Result, bail};
use std::path::Path;
use walkdir::WalkDir;

/// Domain/provider prefixes that transport crates must not use directly.
const TRANSPORT_FORBIDDEN_REACHES: &[(&str, &str)] = &[
    // axon-adapters flattens acquisition/registry/spec/testing at its crate
    // root (crates/axon-adapters/src/lib.rs) but does NOT re-export
    // `web_engine` — the raw Chrome/CDP render+scrape+screenshot engine.
    ("axon_adapters::web_engine::", "domain internal"),
    // Provider execution belongs behind the axon-services reserved-call
    // facade. Transports must not import axon-llm directly.
    ("axon_llm::", "provider crate"),
    // These are private source-runner modules. Transport code must use a
    // public source service entry point instead of reaching into execution.
    (
        "axon_services::source::execution::",
        "service source internal",
    ),
    ("axon_services::source::events::", "service source internal"),
    (
        "axon_services::source::progress::",
        "service source internal",
    ),
    // axon-vectors re-exports `QdrantVectorStore` at its root
    // (`pub use qdrant::QdrantVectorStore;`) but not the `qdrant` module
    // itself. Cited verbatim in crate-ownership.md as the canonical
    // domain-internal reach to avoid. No live transport violation today —
    // this guards the pattern going forward.
    ("axon_vectors::qdrant::", "domain internal"),
    // axon-prune re-exports `PruneExecutor`/`PruneTarget`/`StepExecution` at
    // its root (crates/axon-prune/src/lib.rs) but not the `executor` module
    // itself. Also cited verbatim in crate-ownership.md. No live transport
    // violation today.
    ("axon_prune::executor::", "domain internal"),
    // axon-extract's per-site vertical extractor implementations
    // (crates/axon-extract/src/lib.rs: "dispatch order and policy belong to
    // axon-adapters::vertical_registry; this crate owns only extractor
    // implementations"). No live violation today.
    ("axon_extract::verticals::", "domain internal"),
];

/// Transport crate `src` roots scanned against `TRANSPORT_FORBIDDEN_REACHES`.
const TRANSPORT_SRC: &[&str] = &[
    "crates/axon-cli/src",
    "crates/axon-web/src",
    "crates/axon-mcp/src",
];

/// `axon-services/src`, scanned against its domain-internal reach list.
const SERVICES_SRC: &[&str] = &["crates/axon-services/src"];

const SERVICES_FORBIDDEN_REACHES: &[(&str, &str)] =
    &[("axon_adapters::web_engine::", "domain internal")];

/// Domain crates whose only sanctioned caller is `axon-services`. A direct
/// Cargo dependency from a transport crate on any of these is a layering
/// violation before a single `use` statement is even written — transports
/// call these through the `axon-services` facade instead.
const TRANSPORT_FORBIDDEN_DEPS: &[&str] = &[
    "axon-adapters",
    "axon-embedding",
    "axon-llm",
    "axon-retrieval",
    "axon-vectors",
];

/// Transport crate manifests checked against `TRANSPORT_FORBIDDEN_DEPS`.
const SURFACE_MANIFESTS: &[&str] = &[
    "crates/axon-cli/Cargo.toml",
    "crates/axon-web/Cargo.toml",
    "crates/axon-mcp/Cargo.toml",
];

/// Exact temporary reach exceptions. Each is deleted by its owning Task 2
/// cutover bead. Matching by `(file, pattern)` prevents a whole file from
/// hiding a new reach.
const TEMPORARY_REACH_EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/axon-cli/src/commands/screenshot/util.rs",
        "axon_adapters::web_engine::",
        "axon_rust-jc20j (Task 2B)",
    ),
    (
        "crates/axon-web/src/server/handlers/chat.rs",
        "axon_llm::",
        "axon_rust-jc20j (Task 2C)",
    ),
    (
        "crates/axon-web/src/server/handlers/chat_stream.rs",
        "axon_llm::",
        "axon_rust-jc20j (Task 2C)",
    ),
    (
        "crates/axon-services/src/scrape.rs",
        "axon_adapters::web_engine::",
        "axon_rust-drahp (Task 7)",
    ),
    (
        "crates/axon-services/src/screenshot.rs",
        "axon_adapters::web_engine::",
        "axon_rust-drahp (Task 7)",
    ),
    (
        "crates/axon-services/src/endpoints/capture.rs",
        "axon_adapters::web_engine::",
        "axon_rust-drahp (Task 7)",
    ),
];

/// Exact manifest exceptions removed by the same focused Task 2 PRs.
const TEMPORARY_MANIFEST_EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/axon-cli/Cargo.toml",
        "axon-adapters",
        "axon_rust-jc20j (Task 2B)",
    ),
    (
        "crates/axon-web/Cargo.toml",
        "axon-llm",
        "axon_rust-jc20j (Task 2C)",
    ),
];

/// Raw provider-call spellings in production service/transport code. The
/// whitespace-compacted scan also catches chained calls split across lines.
const RAW_PROVIDER_CALLS: &[&str] = &[
    "embedding_provider.embed(",
    "vector_store.upsert(",
    "llm::complete_streaming(",
    "llm::complete_text(",
];

/// The future Task 6 facade is the sole permanent location allowed to invoke
/// raw providers. Its path is fixed here so "facade" cannot become a broad
/// directory exemption.
const RESERVED_CALL_FACADE: &str = "crates/axon-services/src/reserved_call.rs";

/// Existing raw calls awaiting the durable scheduler facade. These exceptions
/// are deliberately exact and owned by Task 6's scheduler bead.
const TEMPORARY_RAW_CALL_EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/axon-services/src/source/non_web/vectorize.rs",
        "embedding_provider.embed(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/source/non_web/vectorize.rs",
        "vector_store.upsert(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/web_source/vectorize.rs",
        "embedding_provider.embed(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/web_source/vectorize.rs",
        "vector_store.upsert(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/local_source/local_source_vectorize.rs",
        "embedding_provider.embed(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/local_source/local_source_vectorize.rs",
        "vector_store.upsert(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/query/code_search.rs",
        "embedding_provider.embed(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/search/synthesis.rs",
        "llm::complete_streaming(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/query/synthesis/completion.rs",
        "llm::complete_streaming(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/query/synthesis/completion.rs",
        "llm::complete_text(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/debug.rs",
        "llm::complete_text(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/summarize.rs",
        "llm::complete_streaming(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/service_traits/ask_service.rs",
        "llm::complete_text(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-services/src/memory/store.rs",
        "llm::complete_text(",
        "axon_rust-nl7au (Task 6)",
    ),
    (
        "crates/axon-web/src/server/handlers/chat_stream.rs",
        "llm::complete_streaming(",
        "axon_rust-jc20j (Task 2C)",
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
                    if TEMPORARY_MANIFEST_EXCEPTIONS.iter().any(
                        |(allowed_manifest, allowed_crate, _)| {
                            manifest == allowed_manifest && krate == allowed_crate
                        },
                    ) {
                        continue;
                    }
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
fn scan_reaches(
    root: &Path,
    src_roots: &[&str],
    forbidden: &[(&str, &str)],
    violations: &mut Vec<String>,
) {
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
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if let Some((pat, reach_kind)) =
                    forbidden.iter().find(|(pat, _)| line.contains(*pat))
                {
                    if TEMPORARY_REACH_EXCEPTIONS
                        .iter()
                        .any(|(allowed_rel, allowed_pat, _)| {
                            rel == *allowed_rel && pat == allowed_pat
                        })
                    {
                        continue;
                    }
                    violations.push(format!(
                        "{rel}:{}  reaches {reach_kind} `{pat}`",
                        lineno + 1
                    ));
                }
            }
        }
    }
}

fn compact_non_comment_source(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn scan_raw_provider_calls(root: &Path, violations: &mut Vec<String>) {
    for src in TRANSPORT_SRC.iter().chain(SERVICES_SRC) {
        let dir = root.join(src);
        for entry in WalkDir::new(&dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file()
                || entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs")
            {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if is_test_file(&rel) || rel == RESERVED_CALL_FACADE {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let compact = compact_non_comment_source(&text);
            for pattern in RAW_PROVIDER_CALLS {
                let pattern = *pattern;
                if !compact.contains(pattern) {
                    continue;
                }
                if TEMPORARY_RAW_CALL_EXCEPTIONS
                    .iter()
                    .any(|(allowed_rel, allowed_pattern, _)| {
                        rel == *allowed_rel && pattern == *allowed_pattern
                    })
                {
                    continue;
                }
                let line = text
                    .lines()
                    .position(|source_line| {
                        source_line
                            .chars()
                            .filter(|ch| !ch.is_whitespace())
                            .collect::<String>()
                            .contains(pattern)
                    })
                    .map_or(1, |index| index + 1);
                violations.push(format!("{rel}:{line}  calls raw provider via `{pattern}`"));
            }
        }
    }
}

pub fn check(root: &Path) -> Result<()> {
    let mut violations: Vec<String> = Vec::new();

    check_surface_manifests(root, &mut violations);
    scan_reaches(
        root,
        TRANSPORT_SRC,
        TRANSPORT_FORBIDDEN_REACHES,
        &mut violations,
    );
    scan_reaches(
        root,
        SERVICES_SRC,
        SERVICES_FORBIDDEN_REACHES,
        &mut violations,
    );
    scan_raw_provider_calls(root, &mut violations);

    if violations.is_empty() {
        println!("OK: live transport/domain layering and reserved-call gate pass.");
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
         Temporary exceptions must name the exact file, exact pattern, and owning bead.\n\
         Raw providers may be called only by crates/axon-services/src/reserved_call.rs."
    );
    bail!(
        "layering violation: {} reach(es)\n{}",
        violations.len(),
        violations.join("\n")
    );
}
