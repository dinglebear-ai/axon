//! Live workspace crate-contract tables.
//!
//! Each row records the public module surface and forbidden production
//! dependencies for one current workspace crate. Module lists are synchronized
//! from `crates/<name>/src/lib.rs`; ownership intent is documented in the
//! crate's `src/CLAUDE.md` and the living architecture pages under
//! `docs/architecture/`.
//!
//! `forbidden_axon_deps` encodes only explicit production dependency
//! boundaries. Dev and build dependencies remain exempt because tests,
//! fixtures, and code generation legitimately cross runtime boundaries.
//!
//! The table is split across this file and `crate_contracts_spec_cont.rs`
//! solely to stay under the repository's 500-line monolith cap. Use
//! [`all_crate_contracts`] to iterate the combined table.

pub struct CrateContract {
    pub name: &'static str,
    /// Module file stems (without `.rs`) that must exist under `src/` and be
    /// declared `pub mod <name>;` in `lib.rs`. Empty means "not enforced" —
    /// see the module-level doc comment.
    pub modules: &'static [&'static str],
    /// Axon crate names that must not appear in this crate's `[dependencies]`
    /// table (dev/build dependencies are exempt; fixtures/tests legitimately
    /// cross boundaries that runtime code must not).
    pub forbidden_axon_deps: &'static [&'static str],
}

/// Canonical live workspace-crate inventory after the clean-break removal of
/// `axon-vector`, `axon-crawl`, `axon-ingest`, and `axon-code-index`.
pub const LIVE_CRATE_NAMES: &[&str] = &[
    "axon-adapters",
    "axon-api",
    "axon-authz",
    "axon-cli",
    "axon-core",
    "axon-document",
    "axon-embedding",
    "axon-error",
    "axon-extract",
    "axon-graph",
    "axon-jobs",
    "axon-ledger",
    "axon-llm",
    "axon-mcp",
    "axon-memory",
    "axon-observe",
    "axon-parse",
    "axon-prune",
    "axon-retrieval",
    "axon-route",
    "axon-services",
    "axon-vectors",
    "axon-web",
];

/// Iterates the full table (both halves) in no particular order.
pub fn all_crate_contracts() -> impl Iterator<Item = &'static CrateContract> {
    CRATE_CONTRACTS
        .iter()
        .chain(super::crate_contracts_spec_cont::CRATE_CONTRACTS_CONT.iter())
}

pub const CRATE_CONTRACTS: &[CrateContract] = &[
    CrateContract {
        name: "axon-adapters",
        // Adapter-owned vertical routing intentionally consumes both the
        // transitional extractor implementations and parser artifacts. The
        // reverse edges are rejected by `check_adapter_vertical_boundary`.
        modules: &[
            "adapter",
            "registry",
            "capability",
            "acquisition",
            "manifest",
            "web",
            "local",
            "git",
            "registry_sources",
            "feed",
            "youtube",
            "reddit",
            "sessions",
            "cli_tool",
            "mcp_tool",
            "testing",
        ],
        forbidden_axon_deps: &[
            "axon-vectors",
            "axon-embedding",
            "axon-retrieval",
            "axon-services",
            "axon-cli",
            "axon-mcp",
            "axon-web",
        ],
    },
    CrateContract {
        name: "axon-api",
        // Actual shipped `pub mod` surface (crates/axon-api/src/lib.rs), not
        // the target-contract minimal list — see module doc comment above.
        modules: &[
            "diff",
            "explain",
            "job_dto",
            "job_progress",
            "job_status",
            "mcp_schema",
            "migration",
            "reset",
            "result",
            "schema_registry",
            "service_job",
            "source",
        ],
        // axon-api may depend only on axon-error among Axon crates.
        forbidden_axon_deps: &[
            "axon-adapters",
            "axon-authz",
            "axon-cli",
            "axon-core",
            "axon-document",
            "axon-embedding",
            "axon-graph",
            "axon-jobs",
            "axon-ledger",
            "axon-llm",
            "axon-mcp",
            "axon-memory",
            "axon-observe",
            "axon-parse",
            "axon-prune",
            "axon-retrieval",
            "axon-route",
            "axon-services",
            "axon-vectors",
            "axon-web",
            "axon-extract",
        ],
    },
    CrateContract {
        name: "axon-authz",
        modules: &[
            "affinity",
            "caller",
            "decision",
            "http",
            "policy",
            "visibility",
        ],
        forbidden_axon_deps: &[
            "axon-services",
            "axon-jobs",
            "axon-cli",
            "axon-mcp",
            "axon-web",
            "axon-extract",
        ],
    },
    CrateContract {
        name: "axon-cli",
        // Actual shipped `pub mod` surface (crates/axon-cli/src/lib.rs), not
        // the target-contract minimal list — see module doc comment above.
        modules: &["commands", "json", "schema_registry", "ui"],
        forbidden_axon_deps: &[],
    },
    CrateContract {
        name: "axon-core",
        modules: &[
            "artifacts",
            "ask_explain",
            "binary_status",
            "boundary",
            "config",
            "content",
            "endpoints",
            "env",
            "error",
            "events",
            "hardening",
            "health",
            "http",
            "llm",
            "logging",
            "paths",
            "redact",
            "sqlite",
            "structured",
            "ui",
        ],
        forbidden_axon_deps: &[
            "axon-services",
            "axon-jobs",
            "axon-cli",
            "axon-mcp",
            "axon-web",
            "axon-extract",
        ],
    },
    CrateContract {
        name: "axon-document",
        modules: &[
            "preparer",
            "chunk_router",
            "profile",
            "prepared",
            "chunk",
            "metadata",
            "code",
            "markdown",
            "transcript",
            "session",
            "schema",
            "text",
            "testing",
        ],
        forbidden_axon_deps: &[
            "axon-embedding",
            "axon-vectors",
            "axon-llm",
            "axon-jobs",
            "axon-adapters",
            "axon-cli",
            "axon-mcp",
            "axon-web",
            "axon-extract",
        ],
    },
    CrateContract {
        name: "axon-embedding",
        modules: &[
            "provider",
            "batch",
            "capability",
            "reservation",
            "tei",
            "openai_compat",
            "fake",
            "testing",
        ],
        forbidden_axon_deps: &[
            "axon-vectors",
            "axon-retrieval",
            "axon-services",
            "axon-cli",
            "axon-mcp",
            "axon-web",
            "axon-extract",
        ],
    },
    CrateContract {
        name: "axon-error",
        modules: &[
            "api_error",
            "code",
            "stage",
            "severity",
            "retry",
            "degradation",
            "cooling",
            "context",
            "conversion",
            "testing",
        ],
        // axon-error is the lowest-level Axon crate, so all Axon dependencies
        // are forbidden.
        // layer and may declare zero axon-* dependencies.
        forbidden_axon_deps: &[
            "axon-adapters",
            "axon-api",
            "axon-authz",
            "axon-cli",
            "axon-core",
            "axon-document",
            "axon-embedding",
            "axon-graph",
            "axon-jobs",
            "axon-ledger",
            "axon-llm",
            "axon-mcp",
            "axon-memory",
            "axon-observe",
            "axon-parse",
            "axon-prune",
            "axon-retrieval",
            "axon-route",
            "axon-services",
            "axon-vectors",
            "axon-web",
            "axon-extract",
        ],
    },
    CrateContract {
        name: "axon-extract",
        // Restored transitional vertical-extractor catalog (see the
        // "Restored-Crate Note" in
        // crates/axon-extract/src/CLAUDE.md). Only
        // `verticals` is `pub mod` in lib.rs — `context`, `error`,
        // `git_payload`, and `types` are private modules whose types are
        // re-exported at the crate root via `pub use`, so they are not
        // listed here (the check requires a literal `pub mod <name>;`).
        modules: &["verticals"],
        forbidden_axon_deps: &[
            "axon-adapters",
            "axon-vectors",
            "axon-embedding",
            "axon-retrieval",
            "axon-ledger",
            "axon-graph",
            "axon-jobs",
            "axon-services",
            "axon-cli",
            "axon-mcp",
            "axon-web",
        ],
    },
    CrateContract {
        name: "axon-graph",
        modules: &[
            "store",
            "sqlite",
            "migration",
            "node",
            "edge",
            "evidence",
            "candidate",
            "authority",
            "merge",
            "testing",
        ],
        forbidden_axon_deps: &[
            "axon-parse",
            "axon-vectors",
            "axon-embedding",
            "axon-llm",
            "axon-cli",
            "axon-mcp",
            "axon-web",
            "axon-extract",
        ],
    },
];
