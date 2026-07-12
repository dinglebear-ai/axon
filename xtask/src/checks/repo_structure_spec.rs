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
    // axon-vector was deleted outright (issue #298 finale, clean break, not a
    // staged cutover): its last real dependent, `ask --explain`'s legacy
    // reranker, was ported onto `axon-retrieval`'s hybrid RRF hits (see
    // `crates/axon-services/src/query/ask_retrieval/explain.rs`); everything
    // else the crate owned (TEI/Qdrant embed pipeline, code/markdown/text
    // chunkers, ask/evaluate/suggest synthesis) had already moved to
    // `axon-vectors`/`axon-embedding`/`axon-document`/`axon-retrieval`/
    // `axon-services` in earlier #298 slices.
    // axon-extract was deleted outright in Phase 12 (clean break, not a
    // staged cutover) -- see
    // docs/pipeline-unification/plans/2026-07-04-phase-12-old-crate-removal-final-issue-sync.md.
    // Its two genuinely self-contained pieces (extract_sync, generic
    // scrape's HTTP-fetch fallback) moved to axon-services/axon-crawl (the
    // latter itself since relocated -- see the axon-crawl note below).
    // axon-ingest was likewise deleted outright (issue #298 cleanup, not a
    // staged cutover): its still-live pieces (sessions machinery, ingest
    // orchestration/classification) moved to axon-services (sessions_legacy,
    // ingest::{orchestrate,progress,classify_target,target_parse}); the
    // provider-orchestration code it used to own for github/gitlab/gitea/
    // generic_git/reddit/youtube/rss was deleted outright in the earlier
    // Phase 12 clean break.
    // axon-crawl was deleted outright (issue #298 Wave 2a, mechanical move not
    // a rewrite): its Spider-based HTTP/Chrome crawl engine, manifest,
    // sitemap, and screenshot modules relocated verbatim into
    // `crates/axon-adapters/src/web_engine/` (re-exported at
    // `axon_adapters::web_engine::*`), dropping the temporary
    // axon-adapters -> axon-crawl dependency Wave 1a introduced.
    "crates/axon-jobs",
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
