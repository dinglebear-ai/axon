//! Data tables mirroring `docs/pipeline-unification/crates/<name>/README.md`.
//!
//! Two categories of workspace crate carry a pipeline-unification contract:
//!
//! - Crates built fresh for issue #298 (`axon-adapters`, `axon-document`,
//!   `axon-embedding`, `axon-error`, `axon-graph`, `axon-ledger`, `axon-llm`,
//!   `axon-memory`, `axon-observe`, `axon-parse`, `axon-prune`,
//!   `axon-retrieval`, `axon-route`, `axon-vectors`) were built to the
//!   contract's minimal module list, so `modules` is non-empty and enforced.
//! - Pre-existing production crates (`axon-api`, `axon-authz`, `axon-cli`,
//!   `axon-core`, `axon-jobs`, `axon-mcp`, `axon-services`, `axon-web`) still
//!   carry their full current-behavior module surface, which is much larger
//!   than the target contract's minimal list. Enforcing the target module
//!   list against them today would flag the *unfinished refactor* as if it
//!   were drift — see `docs/pipeline-unification/README.md`'s "Current
//!   Implementation Snapshot" framing. `modules` is left empty for these; only
//!   the dependency-direction rule is enforced.
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

/// Iterates the full table (both halves) in no particular order.
pub fn all_crate_contracts() -> impl Iterator<Item = &'static CrateContract> {
    CRATE_CONTRACTS
        .iter()
        .chain(super::crate_contracts_spec_cont::CRATE_CONTRACTS_CONT.iter())
}

pub const CRATE_CONTRACTS: &[CrateContract] = &[
    CrateContract {
        name: "axon-adapters",
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
        ],
    },
    CrateContract {
        name: "axon-api",
        modules: &[],
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
        ],
    },
    CrateContract {
        name: "axon-cli",
        modules: &[],
        forbidden_axon_deps: &[],
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
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
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-extract",
            "axon-code-index",
        ],
    },
];
