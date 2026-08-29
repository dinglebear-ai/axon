//! Data tables mirroring `docs/pipeline-unification/crates/<name>/README.md`.
//!
//! Four categories of workspace crate carry a pipeline-unification contract:
//!
//! - Crates built fresh for issue #298 (`axon-adapters`, `axon-document`,
//!   `axon-embedding`, `axon-error`, `axon-graph`, `axon-ledger`, `axon-llm`,
//!   `axon-memory`, `axon-observe`, `axon-parse`, `axon-prune`,
//!   `axon-retrieval`, `axon-route`, `axon-vectors`) were built to the
//!   contract's minimal module list, so `modules` is non-empty and enforced
//!   against that target list.
//! - The production transport/orchestration crates (`axon-api`, `axon-cli`,
//!   `axon-mcp`, `axon-services`, `axon-web`) are checked against their shipped
//!   public module surfaces. The pipeline cutover intentionally did not require
//!   collapsing unrelated transport modules to the illustrative minimal list in
//!   `foundation/crate-structure.md`; this table therefore records the live
//!   contract and fails on unreviewed public-surface drift.
//! - `axon-authz`, `axon-core`, and `axon-jobs` carry `modules: &[]` because
//!   their pipeline contract is expressed through dependency direction and
//!   focused boundary checks rather than a closed public-module allowlist.
//! - `axon-extract` is a restored transitional crate (removed from the
//!   clean-break list, then intentionally restored 2026-07-15 as the
//!   vertical-extractor implementation catalog — see the "Restored-Crate
//!   Note" in `docs/pipeline-unification/crates/axon-extract/README.md`).
//!   Its `modules` entry lists only `verticals`, the sole `pub mod` in its
//!   `lib.rs`; its other files (`context`, `error`, `git_payload`, `types`)
//!   are private modules whose types are re-exported at the crate root, so
//!   listing them would false-positive against the literal `pub mod <name>;`
//!   check.
//!
//! `forbidden_axon_deps` is derived only from each README's explicit
//! "Dependencies Forbidden" text (named crates, or unambiguous category terms
//! like "transport crates" that consistently mean `axon-cli`/`axon-mcp`/
//! `axon-web` throughout the contract packet). It intentionally does not
//! encode the "Dependencies Allowed" list as a closed set — allowed lists are
//! illustrative, not exhaustive, and treating them as exhaustive would flag
//! legitimate utility-crate dependencies as violations.
//!
//! The table is split across this file and `crate_contracts_spec_cont.rs`
//! purely to stay under the repo's 500-line monolith cap — there is no
//! semantic difference between the two halves. Use
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
    "axon-codex",
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
        // README: "all domain crates except `axon-error`" — the only axon
        // dependency this crate may declare is axon-error.
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
        modules: &[],
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
        name: "axon-codex",
        modules: &[
            "api",
            "approval",
            "artifacts",
            "capabilities",
            "control",
            "events",
            "operations",
            "protocol",
            "transport",
        ],
        forbidden_axon_deps: &["axon-services", "axon-cli", "axon-mcp", "axon-web"],
    },
    CrateContract {
        name: "axon-core",
        modules: &[],
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
        // README: "any Axon crate" is forbidden — axon-error is the lowest
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
        // docs/pipeline-unification/crates/axon-extract/README.md). Only
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
