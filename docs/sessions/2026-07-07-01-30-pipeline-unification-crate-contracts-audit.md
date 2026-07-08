# Pipeline-Unification Crate Contracts Audit — PR #381

Date: 2026-07-07

Branch: `claude/pipeline-unification-audit-g5tjxj`

PR: https://github.com/jmagar/axon/pull/381 (merged, squash `e4cc79601bb6a6e7e35e8c9c532691a284503091`)

## Task

Requested: review `docs/pipeline-unification/` and create xtasks that programmatically
audit the code for full alignment to all docs, specs, and contracts in that directory
(~100 files across `foundation/`, `runtime/`, `sources/`, `surfaces/`, `schemas/`,
`configuration/`, `crates/`, `delivery/`, `plans/`).

## Approach

Ran three parallel Explore agents to inventory every doc's machine-checkable claims and
cross-reference them against the existing `xtask` check/schema-generator infrastructure,
to avoid duplicating what was already automated. Findings:

- 12 registry-backed schema families already exist (`cargo xtask schemas <family>` for
  api/cli/openapi/mcp/config/events/errors/database/graph/vector-payload/providers/adapters),
  mapping closely to `docs/pipeline-unification/schemas/*.md`.
- `check_enum_projection_drift`, `REMOVED_SURFACE_RULES`, `check-repo-structure`,
  `check-layering`, `check-doc-links`, `check-doc-contracts` already cover meaningful
  slices of the contract packet.
- The concrete, safely-automatable gap: **`docs/pipeline-unification/crates/<name>/README.md`'s
  "Public Modules" and "Dependencies Forbidden" sections were never checked against the
  real crate tree.** `repo_structure_spec.rs`'s per-crate module enforcement only applied
  while a crate was an unbuilt "PR0 skeleton" (`TARGET_CRATES`), and that list is now empty
  — all target crates have graduated, so nothing enforces their module shape anymore.

## What Shipped

Added `cargo xtask check-crate-contracts` (standalone subcommand, not part of the `check`
aggregate):

- `xtask/src/checks/crate_contracts_spec.rs` + `crate_contracts_spec_cont.rs` (split only
  for the 500-line monolith cap) — a hand-extracted `CrateContract` registry for all 22
  target crates: `modules` (Public Modules list) and `forbidden_axon_deps` (Dependencies
  Forbidden, derived only from explicit README text, never from the "Allowed" list treated
  as closed).
- `xtask/src/checks/crate_contracts.rs` — `check_modules` (documented module ⊆ actual,
  one-directional: extra modules/sidecar tests are fine) and `check_forbidden_deps`
  (`[dependencies]` + `[target.'cfg(...)'.dependencies]`, dev/build deps exempt).
- Module-list enforcement scoped to only the 14 crates built fresh for issue #298
  (`axon-adapters`, `axon-document`, `axon-embedding`, `axon-error`, `axon-graph`,
  `axon-ledger`, `axon-llm`, `axon-memory`, `axon-observe`, `axon-parse`, `axon-prune`,
  `axon-retrieval`, `axon-route`, `axon-vectors`). The other 8 target crates
  (`axon-api`, `axon-authz`, `axon-cli`, `axon-core`, `axon-jobs`, `axon-mcp`,
  `axon-services`, `axon-web`) predate this effort and still carry their full
  current-behavior surface — enforcing the target's minimal module list against them
  would flag the unfinished refactor as drift. Only their dependency-direction rule is
  enforced.
- `xtask/src/checks/layering.rs` — added `axon_code_index::store::` to the forbidden
  transport-reach list, closing a gap where the other four legacy crates
  (`axon_crawl`, `axon_extract`, `axon_ingest`, `axon_vector`) were guarded but
  `axon_code_index` wasn't. Currently a no-op (no transport crate depends on it yet).

## Real Findings (left unfixed — out of scope for a tooling PR)

- `axon-graph` is missing its documented `query.rs` module entirely.
- `axon-retrieval`'s `testing` module is declared `pub(crate) mod testing;`, not
  `pub mod testing;`, so other crates can't consume its fakes as the contract intends.

Verified all 22 crates' dependency-forbidden rules pass cleanly today (including
confirming `axon-jobs` still doesn't depend on `axon-services`, per
`delivery/contradiction-review.md`'s resolved decision).

## Review Loop

- CodeRabbit posted 2 nitpicks:
  - Fixed: forbidden-dependency check only scanned top-level `[dependencies]`, missing
    `[target.'cfg(...)'.dependencies]`. Verified none of the 22 crates use target-specific
    tables today (coverage-only change, no new violations), added a regression test.
  - Left as-is: exact-match `pub mod {module};` line check has no tolerance for trailing
    comments. Matches the pre-existing `repo_structure.rs` convention, no real `lib.rs` in
    this repo puts a comment on the same line as a `pub mod` declaration, and CodeRabbit
    itself tagged it "Low value".
- CI: all required checks green (`ci-gate`, `fmt`, `clippy` gate, `schema-contract-sync`,
  `monolith`, `toml-fmt`, `codeql-gate`, `compose-smoke-gate`, `image-build-smoke`,
  `ban-skip-validation`, `advisory-lock-policy`, GitGuardian). Many workflow jobs
  correctly skipped via path filters (xtask-only change).

## Coverage Assessment (for future sessions)

Full alignment auditing of all ~100 pipeline-unification docs is **not** achieved by this
PR — it closed one specific gap on top of already-substantial existing infrastructure.
Known remaining gaps, in rough priority order:

1. **`delivery/docs-generator-contract.md`'s entire `cargo xtask docs <family>` namespace
   does not exist** — a separate, large, unbuilt generator system distinct from
   `cargo xtask schemas`.
2. **Removed-surface checks only cover generated docs, not live dispatch** — nothing
   verifies the actual clap parser / MCP action router / REST router reject commands the
   contract says are removed (`embed`, `ingest`, `scrape`, `crawl`, `code-search`,
   `code-search-watch`, `purge`, `dedupe`, etc.).
3. **`runtime/*.md` contracts are almost entirely unchecked at the behavior level** — job
   state machine transitions, ledger lease semantics, memory scoring formula,
   observability metric names, auth scope matrix, redaction detector list, storage
   retention defaults. These belong in `cargo test` in their owning crates, not `xtask`.
4. **Known cross-doc inconsistency, unresolved**: `sources/chunking-contract.md` lists 11
   chunking profiles, but `foundation/types/enum-contract.md`'s `ChunkProfile` enum only
   has 8 variants. No check catches this drift between the two docs.
5. **Legacy-crate-removal readiness is unenforced** — `repo_structure_spec.rs` correctly
   *requires* `axon-vector`/`axon-crawl`/`axon-ingest`/`axon-extract`/`axon-code-index`
   today, but nothing tracks readiness for the day the contract says they must be
   *removed* (gated by `plans/2026-07-04-phase-12-old-crate-removal-final-issue-sync.md`).
6. **`configuration/*.md`'s target key/section lists aren't diffed against the live config
   registry** — only *removed* keys are checked (`REMOVED_SURFACE_RULES`), not the full
   required-key set from `config-contract.md`/`env-contract.md`.
7. **`delivery/testing-contract.md`'s test-tier model and boundary-fake-existence
   requirements** have no `xtask` equivalent — pure `cargo test` convention.

## Follow-ups (not filed as issues yet)

- Fix `axon-graph` missing `query.rs` and `axon-retrieval`'s `testing` module visibility
  (the two real findings above), or update their READMEs if the contract itself changed.
- Reconcile the `ChunkProfile` 11-vs-8 variant mismatch between `chunking-contract.md` and
  `enum-contract.md`.
- Decide whether to wire `check-crate-contracts` into the `check` aggregate once the two
  real findings are resolved (it's currently standalone specifically because it fails
  today for real reasons).
