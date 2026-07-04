pub const REQUIRED_WORKSPACE_MEMBERS: &[&str] = &[
    "xtask",
    // axon-error graduated from the PR0 skeleton in Phase 1 (issue #298): it now
    // carries real dependencies and sidecar tests, so it is a required member
    // rather than a `TARGET_CRATES` skeleton entry.
    "crates/axon-error",
    "crates/axon-api",
    "crates/axon-authz",
    "crates/axon-core",
    // Phase 3 / PR4 observability crate graduated from PR0 skeleton status:
    // it now owns shared provider reservation/cooling state and sidecar tests.
    "crates/axon-observe",
    // Phase 3 / PR4 boundary crates graduated from PR0 skeleton status:
    // they now own store/provider traits, fakes, and sidecar tests.
    "crates/axon-ledger",
    "crates/axon-graph",
    "crates/axon-memory",
    "crates/axon-embedding",
    "crates/axon-vectors",
    // Phase 9 / PR9 retrieval crate graduated from PR0 skeleton status: it now
    // owns retrieval boundary DTOs, fakes, ranking/context/citation helpers, and
    // sidecar tests.
    "crates/axon-retrieval",
    "crates/axon-llm",
    // Phase 21 / prune crate graduated from PR0 skeleton status: it now owns
    // prune plans, cleanup debt execution, receipts, safety checks, and
    // sidecar tests.
    "crates/axon-prune",
    "crates/axon-adapters",
    // Phase 4 / PR5 route crate graduated from PR0 skeleton status:
    // it now owns source resolving, canonicalization, routing, adapter
    // capability metadata, authority aliases, stable source IDs, and sidecar
    // tests.
    "crates/axon-route",
    // Phase 8 / PR8 parse crate graduated from PR0 skeleton status: it now owns
    // parser traits, parser registry selection, no-op degradation, API DTO
    // re-exports, and fake parser test implementations.
    "crates/axon-parse",
    // Phase 8 / PR8 document crate graduated from PR0 skeleton status: it now
    // owns document preparation DTO adapters, chunk routing profiles, fake
    // preparers, and sidecar tests.
    "crates/axon-document",
    "crates/axon-crawl",
    "crates/axon-vector",
    "crates/axon-ingest",
    "crates/axon-extract",
    "crates/axon-jobs",
    "crates/axon-code-index",
    "crates/axon-services",
    "crates/axon-mcp",
    "crates/axon-web",
    "crates/axon-cli",
];

pub struct TargetCrate {
    pub name: &'static str,
    pub modules: &'static [&'static str],
}

// NOTE: crates filled in during issue #298 implementation PRs are intentionally
// NOT listed here once they own real dependencies, sidecar tests, and public
// API. They move to `REQUIRED_WORKSPACE_MEMBERS`; this list only contains
// remaining PR0 skeleton crates.
pub const TARGET_CRATES: &[TargetCrate] = &[];
