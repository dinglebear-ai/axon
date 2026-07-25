# axon-extract Crate Contract
Last Modified: 2026-07-24

## Purpose

`axon-extract` owns the vertical-extractor implementation catalog: per-site
structured extraction functions (GitHub repo/issue/PR/release, PyPI, npm,
crates.io, docs.rs, Reddit, Hacker News, Stack Overflow, Amazon, eBay,
Shopify, dev.to, Docker Hub, arXiv, Hugging Face models, and the shared
git-source structured-payload shaping they use). It owns only extractor
implementations and their narrow shared context/output types. URL/name
matching order and dispatch policy belong to `axon-adapters` (see
`crates/axon-adapters/src/vertical_registry.rs`); this crate has no pipeline,
acquisition-routing, or ledger ownership.

## Restored-Crate Note

`axon-extract` was removed from `foundation/crate-structure.md`'s workspace
member list on 2026-06-30 and restored to workspace `members` on 2026-07-15,
citing "present intentionally; restored vertical extractor crate." That
restoration was not previously reflected in this contract, which is what this
document fixes. `docs/pipeline-unification/plans/finish-unification-metaplan.md`
still frames `axon-extract` as a **transitional** vertical-extractor catalog
whose modules should eventually be re-homed under `axon-adapters`/`axon-parse`
ownership at final closeout. That is an open follow-up, not a requirement this
contract enforces — this contract describes the crate as it exists today,
consumed one-way by `axon-adapters`.

## Owns

- plain-module vertical extractors: `INFO` capability constant, `matches(url)`
  predicate, and `async extract(url, ctx)` function per site
- `VerticalContext` — the narrowed `ServiceContext` view passed to every
  extractor
- `VerticalError` — the extractor error type (re-exported from
  `axon-core`'s service error taxonomy)
- `ScrapedDoc` / `ExtractorInfo` — extractor output and capability types
- shared git-source structured payload shaping (`git_payload`) consumed by the
  GitHub verticals (repo, issue, PR, release)
- the `auto_dispatch` capability flag distinguishing default-on extractors
  from explicit-only (ToS-risky) ones

## Must Not Own

- URL/name matching order or dispatch policy (owned by
  `axon-adapters::vertical_registry`)
- source routing, canonical source identity, or ledger persistence
- acquisition orchestration, chunking, embedding, or vector writes
- CLI/MCP/REST rendering
- a dependency on `axon-adapters` (dependency flows one-way:
  `axon-adapters -> axon-extract`, enforced by
  `check_adapter_vertical_boundary` in
  `xtask/src/checks/crate_contracts.rs`)

## Public Modules

Only `verticals` is declared `pub mod` in `lib.rs`; `context`, `error`,
`git_payload`, and `types` are private modules whose types are re-exported at
the crate root via `pub use`.

```text
lib.rs
context.rs            (private; VerticalContext re-exported)
error.rs              (private; VerticalError re-exported)
git_payload.rs         (private; internal to the GitHub verticals)
types.rs               (private; ExtractorInfo/ScrapedDoc re-exported)
verticals.rs           (pub mod; declares all vertical sub-modules)
verticals/
  amazon.rs             auto_dispatch: false (ToS-risky)
  arxiv.rs
  crates_io.rs
  dev_to.rs
  docker_hub.rs
  docs_rs.rs
  ebay.rs               auto_dispatch: false (ToS-risky)
  github_issue.rs
  github_pr.rs
  github_release.rs
  github_repo.rs
  hackernews.rs
  huggingface_model.rs
  npm.rs
  pypi.rs
  reddit.rs
  shopify.rs
  stackoverflow.rs
```

## Public API

- `VerticalContext` — defined here
- `VerticalError` — defined here (re-export of
  `axon_core::error::ServiceTaxonomyError`)
- `ExtractorInfo` — defined here
- `ScrapedDoc` — defined here
- `verticals::<name>::{INFO, matches, extract}` — defined here, one module per
  extractor

`ScrapedDoc.extractor_name` and `ScrapedDoc.extractor_version` cross into the
Qdrant payload contract (`axon-api::reset::payload_contract_version`); bumping
`extractor_version` forces re-embedding for that extractor's points on the
next source refresh. `ScrapedDoc` and `ExtractorInfo` are domain-local output
types converted into `axon-api` DTOs by the consuming adapter, not
`axon-api`-defined types themselves.

## Dependencies Allowed

- `axon-core` (HTTP client helpers, error taxonomy, config)
- `axon-api`, `axon-llm` (declared workspace dependencies; current extractor
  implementations do not yet call either directly)
- HTTP, parsing, and archive-handling libraries used behind extractor modules
  (`reqwest`, `flate2`, `base64`, `chrono`, `regex`, `url`, `serde_json`)

## Dependencies Forbidden

- `axon-adapters` (one-way dependency; enforced separately by
  `check_adapter_vertical_boundary`)
- `axon-vectors`, `axon-embedding`, `axon-retrieval`, `axon-ledger`,
  `axon-graph`
- `axon-jobs`, `axon-services`
- transport crates: `axon-cli`, `axon-mcp`, `axon-web`
- retired legacy crates: `axon-vector`, `axon-crawl`, `axon-ingest`,
  `axon-code-index`

## Generated Artifacts

- none — extractor capability metadata (`ExtractorInfo`) is consumed directly
  by `axon-adapters::vertical_registry::list()`, not generated into a
  standalone schema doc

## Fixtures And Fakes

- per-extractor `matches()` truth-table tests (sidecar `_tests.rs` per
  vertical module)
- live-HTTP tests gated behind feature flags or env-driven skips
- no fake/in-memory `VerticalContext` construction helper is currently
  exported; extractor tests build `VerticalContext` directly

## Tests

- every vertical module's `matches()` covers positive and negative URL cases
- extractor output includes `extractor_name` and `extractor_version` used for
  payload/reindex tracking
- `auto_dispatch: false` extractors are excluded from
  `axon-adapters::vertical_registry::dispatch_by_url()` (asserted in
  `axon-adapters`, not here)
- an `axon-adapters` exhaustiveness test asserts every `verticals::list()`
  entry has a corresponding `dispatch_by_name()` arm — that ownership stays in
  `axon-adapters`, not duplicated here

## Acceptance Criteria

- extractor implementations stay plain-module, trait-object-free, and free of
  dispatch/routing logic
- new extractors land here with `INFO`/`matches()`/`extract()` plus a
  `matches()` truth-table test, then get wired into
  `axon-adapters::vertical_registry` per
  `crates/axon-extract/src/CLAUDE.md`'s "Adding a New Extractor" steps
- `axon-extract` never depends on `axon-adapters`; `axon-adapters` remains the
  only consumer that depends on `axon-extract`

See [../README.md](../README.md),
[../../foundation/crate-structure.md](../../foundation/crate-structure.md),
[../axon-adapters/README.md](../axon-adapters/README.md), and
[../../plans/finish-unification-metaplan.md](../../plans/finish-unification-metaplan.md)
for the open re-homing question.
