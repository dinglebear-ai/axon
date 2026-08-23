---
date: 2026-08-23 08:44:13 EDT
repo: git@github.com:dinglebear-ai/axon.git
branch: main
head: ad842bd396423e632d0f6281e836dea0d5bcd280
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: "#576 fix(sqlite): prevent scheduler write starvation (https://github.com/dinglebear-ai/axon/pull/576)"
beads: axon_rust-qsbhe, axon_rust-qsbhe.1, axon_rust-qsbhe.2, axon_rust-qsbhe.3, axon_rust-qsbhe.4, axon_rust-qsbhe.5, axon_rust-qsbhe.6, axon_rust-qsbhe.7, axon_rust-qsbhe.8, axon_rust-qsbhe.9, axon_rust-qsbhe.10, axon_rust-qsbhe.11, axon_rust-qsbhe.12
---

# SQLite starvation and source-pipeline performance session

## User Request

Investigate why an Axon crawl of `code.claude.com` was slower and larger than expected, tune cold embedding and the surrounding source pipeline, then stage, push, review, remediate every Vibin and Lavra finding, and land the result on `main`.

## Session Overview

The session moved from artifact-candidate review into a measured source-ingestion performance investigation. It identified SQLite writer starvation, excessive/repeated embedding work, Markdown boundary defects, and serial pipeline stages; implemented shared writer admission, an opt-in content-addressed embedding cache, structural Markdown windowing, bounded web prefetch, and embedding/Qdrant overlap; then completed two review/remediation cycles. PR #576 merged to `main` as `ad842bd396423e632d0f6281e836dea0d5bcd280` after all required CI gates passed.

## Sequence of Events

1. Reviewed PR #569's artifact-candidate integration and remediated runtime wiring, contract, delivery, security, durability, documentation, and test findings.
2. Released and deployed Axon, crawled `code.claude.com`, separated document counts from produced chunks/vectors, and measured cold embedding and end-to-end timings.
3. Tuned TEI and chunking, researched model/runtime alternatives, and found that the largest remaining gains were outside direct GPU compute.
4. Implemented bounded acquisition/vector overlap, structural Markdown chunking, and an opt-in SQLite embedding cache; corrected early cache retention and write-amplification bugs.
5. Ran Vibin review, fixed layering, cache identity, SQLite contention, Markdown fence/frontmatter/complexity, and production-call-site coverage issues.
6. Ran Lavra performance, security, architecture, Rust, simplicity, and goal reviews; tracked and closed every finding through focused tests and closure reviews.
7. Fixed CI-only generated-contract and configuration-boundary failures, reran the transient Hugging Face rate-limited live-RAG job, and allowed PR #576 to squash-merge.
8. Closed the parent Bead after exact-head CI and merge verification; verified local `main` equals `origin/main` and the worktree was clean before creating this note.

## Key Findings

- Eight concurrent workers could occupy all eight SQLite pool connections while waiting for a writer, starving heartbeat and control work. The fix shares one writer-admission gate across schedulers and cache mutations (`crates/axon-jobs/src/scheduler.rs`, `crates/axon-services/src/context/target_runtime.rs`).
- The first cache prototype used a signed subtraction that produced a negative SQLite `LIMIT`, deleting every row when under capacity; it also issued one insert per vector. The final store uses bulk/bind-budgeted writes and trigger-maintained cardinality (`crates/axon-jobs/src/embedding_cache_store.rs`).
- Cache identity must include authority, logical provider, resolved model, dimensions, effective instruction policy, content kind, and exact normalized text; provider identity mismatches otherwise turn every warm lookup into a miss (`crates/axon-embedding/src/cache.rs`).
- Markdown chunking could read ambient user configuration, split fences/tables/lists incorrectly, panic on whitespace-only windows, and become quadratic. The final implementation is injected, fence-aware, semantic, UTF-8 safe, and linear (`crates/axon-document/src/markdown.rs`, `crates/axon-document/src/markdown/windowing.rs`).
- Speculative acquisition must not regress the externally visible phase and must register artifacts before resolving the current batch. Embedding and Qdrant upsert can overlap while preserving checkpoint order (`crates/axon-services/src/source/executor/created_generation/batches.rs`, `crates/axon-services/src/source/executor/vectorize/pipeline.rs`).

## Technical Decisions

- Keep cache policy/decorator logic in `axon-embedding`, SQLite persistence in `axon-jobs`, and composition in `axon-services`, preserving crate ownership and the layering gate.
- Make embedding caching opt-in and fail-open; cache failures cannot fail ingestion. Warm-hit recency updates are non-blocking, while admitted mutations retain the shared writer gate through commit or rollback.
- Use a fixed, non-refreshing seven-day cache TTL. Because content-addressed rows can be shared by multiple sources, source-scoped deletion is unsafe; weak-lifetime periodic maintenance provides physical reclamation.
- Bound maintenance independently of cache cardinality using exact trigger-maintained state and indexed batches of at most 512 victims; avoid `COUNT`, `OFFSET`, and unbounded deletes.
- Permit only one prefetched acquisition batch and one next embedding batch. Preserve the primary error while attaching/logging secondary concurrent failures.

## Files Changed

All 98 paths below are the complete file set of squash commit `ad842bd396423e632d0f6281e836dea0d5bcd280` (`git diff-tree --no-commit-id --name-status -r ad842bd...`).

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.github/workflows/ci.yml` | — | Path-gate and contract CI work | `ad842bd` |
| modified | `Cargo.lock` | — | Workspace dependency lock updates | `ad842bd` |
| modified | `config.example.toml` | — | Embedding-cache and chunking configuration | `ad842bd` |
| modified | `crates/axon-adapters/src/adapter.rs` | — | Acquisition-prefetch capability contract | `ad842bd` |
| modified | `crates/axon-adapters/src/web.rs` | — | Web prefetch opt-in | `ad842bd` |
| modified | `crates/axon-adapters/src/web_tests.rs` | — | Web capability coverage | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/build_config_tests.rs` | — | Configuration construction coverage | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/env_registry/migration.rs` | — | Cache environment migration registry | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/env_registry_tests.rs` | — | Environment registry tests | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/toml_config.rs` | — | Typed TOML configuration surface | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/toml_config/convert.rs` | — | TOML-to-runtime conversion | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/toml_config/raw.rs` | — | Raw cache/chunking fields | `ad842bd` |
| modified | `crates/axon-core/src/config/parse/tuning.rs` | — | Environment/TOML tuning resolution | `ad842bd` |
| modified | `crates/axon-core/src/config/types/config.rs` | — | Runtime configuration fields | `ad842bd` |
| modified | `crates/axon-core/src/config/types/config_debug.rs` | — | Debug rendering for new fields | `ad842bd` |
| modified | `crates/axon-core/src/config/types/config_impls.rs` | — | Defaults and config behavior | `ad842bd` |
| modified | `crates/axon-core/src/config/types_tests.rs` | — | Boundary/default tests | `ad842bd` |
| modified | `crates/axon-document/src/chunk_router.rs` | — | Injected structural Markdown limits | `ad842bd` |
| modified | `crates/axon-document/src/chunk_router_tests.rs` | — | Router chunk-bound coverage | `ad842bd` |
| modified | `crates/axon-document/src/lib.rs` | — | Document API exports | `ad842bd` |
| modified | `crates/axon-document/src/markdown.rs` | — | Fence-aware Markdown sections | `ad842bd` |
| created | `crates/axon-document/src/markdown/semantics.rs` | — | Semantic block-boundary detection | `ad842bd` |
| created | `crates/axon-document/src/markdown/windowing.rs` | — | Linear bounded Markdown windows | `ad842bd` |
| modified | `crates/axon-document/src/markdown_tests.rs` | — | Fence/frontmatter/table/list regressions | `ad842bd` |
| modified | `crates/axon-document/src/preparer.rs` | — | Configured preparer composition | `ad842bd` |
| modified | `crates/axon-document/src/preparer/chunk_build.rs` | — | Chunk-building policy | `ad842bd` |
| modified | `crates/axon-document/src/preparer_tests.rs` | — | Preparation behavior coverage | `ad842bd` |
| modified | `crates/axon-embedding/Cargo.toml` | — | Cache dependencies | `ad842bd` |
| created | `crates/axon-embedding/src/cache.rs` | — | Content-addressed cache decorator/policy | `ad842bd` |
| created | `crates/axon-embedding/src/cache_tests.rs` | — | Cache identity, failure, and concurrency tests | `ad842bd` |
| modified | `crates/axon-embedding/src/lib.rs` | — | Cache API exports | `ad842bd` |
| modified | `crates/axon-jobs/Cargo.toml` | — | SQLite cache-store dependencies | `ad842bd` |
| created | `crates/axon-jobs/src/embedding_cache_store.rs` | — | SQLite cache persistence and maintenance | `ad842bd` |
| created | `crates/axon-jobs/src/embedding_cache_store_tests.rs` | — | Store, TTL, retention, and gate tests | `ad842bd` |
| modified | `crates/axon-jobs/src/lib.rs` | — | Cache-store exports | `ad842bd` |
| modified | `crates/axon-jobs/src/migration-checksums.txt` | — | Migration integrity pins | `ad842bd` |
| modified | `crates/axon-jobs/src/migrations.rs` | — | Cache migrations registration | `ad842bd` |
| created | `crates/axon-jobs/src/migrations/0007_embedding_vector_cache.sql` | — | Initial embedding cache schema | `ad842bd` |
| created | `crates/axon-jobs/src/migrations/0008_embedding_vector_cache_expiry.sql` | — | TTL index, cardinality state, triggers | `ad842bd` |
| modified | `crates/axon-jobs/src/migrations/identity.rs` | — | Canonical migration identity | `ad842bd` |
| modified | `crates/axon-jobs/src/migrations_tests.rs` | — | Upgrade/reopen/migration tests | `ad842bd` |
| modified | `crates/axon-jobs/src/provider_cooling_tests.rs` | — | Provider cooldown regressions | `ad842bd` |
| modified | `crates/axon-jobs/src/scheduler.rs` | — | Shared SQLite writer gate | `ad842bd` |
| modified | `crates/axon-jobs/src/scheduler/grant.rs` | — | Gate-aware reservation grant | `ad842bd` |
| modified | `crates/axon-jobs/src/scheduler/reconcile.rs` | — | Gate-aware reconciliation | `ad842bd` |
| modified | `crates/axon-jobs/src/scheduler_tests.rs` | — | Deterministic writer admission tests | `ad842bd` |
| modified | `crates/axon-jobs/src/unified_codec.rs` | — | Unified job codec adjustments | `ad842bd` |
| modified | `crates/axon-jobs/src/unified_tests.rs` | — | Unified runtime regressions | `ad842bd` |
| modified | `crates/axon-jobs/src/workers/unified/terminal.rs` | — | Terminal worker SQLite behavior | `ad842bd` |
| modified | `crates/axon-services/src/context.rs` | — | Runtime composition API | `ad842bd` |
| modified | `crates/axon-services/src/context/target_runtime.rs` | — | Cache/provider/shared-gate composition | `ad842bd` |
| modified | `crates/axon-services/src/context/target_runtime/schedulers.rs` | — | One gate across production schedulers | `ad842bd` |
| modified | `crates/axon-services/src/context/target_runtime_tests.rs` | — | Real production-composition tests | `ad842bd` |
| modified | `crates/axon-services/src/reset/sqlite.rs` | — | Cache reset coverage | `ad842bd` |
| modified | `crates/axon-services/src/reset_tests.rs` | — | Reset regression tests | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/created_generation.rs` | — | Bounded generation-batch orchestration | `ad842bd` |
| created | `crates/axon-services/src/source/executor/created_generation/batches.rs` | — | One-batch acquisition prefetch | `ad842bd` |
| created | `crates/axon-services/src/source/executor/created_generation/batches_tests.rs` | — | Actual call-site prefetch/error tests | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/preparation.rs` | — | Configured document preparation | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/preparation_tests.rs` | — | Preparation integration tests | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/progress.rs` | — | Speculative counter-only progress | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/progress_tests.rs` | — | Phase non-regression tests | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/vectorize.rs` | — | Vector pipeline orchestration | `ad842bd` |
| created | `crates/axon-services/src/source/executor/vectorize/batching.rs` | — | Bounded vector batching | `ad842bd` |
| created | `crates/axon-services/src/source/executor/vectorize/pipeline.rs` | — | Embed/upsert overlap | `ad842bd` |
| created | `crates/axon-services/src/source/executor/vectorize/pipeline_tests.rs` | — | Actual overlap/progress/error tests | `ad842bd` |
| modified | `crates/axon-services/src/source/executor/vectorize_tests.rs` | — | Vector orchestration regressions | `ad842bd` |
| modified | `docs/pipeline-unification/sources/chunking-contract.md` | — | Structural chunking contract | `ad842bd` |
| modified | `docs/reference/config/config-toml.md` | — | Generated TOML reference | `ad842bd` |
| modified | `docs/reference/config/config.schema.json` | — | Generated configuration schema | `ad842bd` |
| modified | `docs/reference/config/env.md` | — | Generated environment reference | `ad842bd` |
| modified | `docs/reference/config/env.schema.json` | — | Generated environment schema | `ad842bd` |
| modified | `docs/reference/crate-dependency-graph.md` | — | Generated dependency graph | `ad842bd` |
| modified | `docs/reference/env-matrix.toml` | — | Environment boundary matrix | `ad842bd` |
| modified | `docs/reference/generated/config.md` | — | Generated config projection | `ad842bd` |
| modified | `docs/reference/generated/env.md` | — | Generated env projection | `ad842bd` |
| modified | `docs/reference/generated/memory.md` | — | Generated source manifest projection | `ad842bd` |
| modified | `docs/reference/generated/new-source.md` | — | Generated source guide | `ad842bd` |
| modified | `docs/reference/runtime/database-schema.json` | — | Generated runtime database schema | `ad842bd` |
| modified | `docs/reference/runtime/database-schema.md` | — | Rendered database schema | `ad842bd` |
| modified | `docs/reference/runtime/schema.md` | — | Runtime schema summary | `ad842bd` |
| modified | `docs/reference/source-input-manifest.json` | — | Generated provenance manifest | `ad842bd` |
| modified | `docs/reference/sources/adapter-scopes.json` | — | Generated adapter capability data | `ad842bd` |
| modified | `docs/reference/sources/adapter-scopes.md` | — | Rendered adapter capabilities | `ad842bd` |
| modified | `docs/reference/sources/chunking.md` | — | Generated chunking reference | `ad842bd` |
| modified | `docs/reference/sources/vector-payload.md` | — | Vector payload reference | `ad842bd` |
| modified | `docs/reference/sources/vector-payload.schema.json` | — | Vector payload schema | `ad842bd` |
| modified | `scripts/check-env-config-boundary.py` | — | Cache env/config boundary enforcement | `ad842bd` |
| modified | `src/main.rs` | — | Binary stack-size/runtime entry behavior | `ad842bd` |
| modified | `src/main_tests.rs` | — | Compile-time stack-size contract | `ad842bd` |
| modified | `xtask/src/schemas/config_schema_registry.rs` | — | Generated config registry | `ad842bd` |
| modified | `xtask/src/schemas/config_schema_registry_tests.rs` | — | Config registry tests | `ad842bd` |
| modified | `xtask/src/schemas/database_defs_tests.rs` | — | Database migration namespace tests | `ad842bd` |
| modified | `xtask/tests/fixtures/schemas/adapters/snapshots/adapter-scopes.json` | — | Adapter schema fixture | `ad842bd` |
| modified | `xtask/tests/fixtures/schemas/config/snapshots/config.schema.json` | — | Config schema fixture | `ad842bd` |
| modified | `xtask/tests/fixtures/schemas/config/snapshots/env.schema.json` | — | Env schema fixture | `ad842bd` |
| modified | `xtask/tests/fixtures/schemas/database/snapshots/database-schema.json` | — | Database schema fixture | `ad842bd` |
| modified | `xtask/tests/fixtures/schemas/vector-payload/snapshots/vector-payload.schema.json` | — | Vector payload fixture | `ad842bd` |

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-qsbhe` | Fix default unified-worker SQLite pool starvation | claimed, investigated, commented with four durable lessons, closed after merge | closed | Parent bug and delivery ledger for PR #576 |
| `.1` | Make embedding-cache recency touches non-blocking | created and closed | closed | Prevented warm hits waiting behind writer backlog |
| `.2`, `.3` | Bound embedding-cache retention maintenance cost | duplicate child issues created and closed | closed | Removed full-count/unbounded maintenance work |
| `.4`, `.6` | Align embedding-cache retention with source deletion privacy | duplicate child issues created and closed | closed | Added fixed TTL and physical reclamation |
| `.5`, `.8` | Prevent speculative prefetch from regressing pipeline phase | duplicate child issues created and closed | closed | Preserved externally active progress phase |
| `.7`, `.9` | Track artifacts from successful speculative acquisition on process failure | duplicate child issues created and closed | closed | Prevented leaked speculative artifacts |
| `.10` | Unify serial and overlapped vector progress policy | created and closed | closed | Kept progress/checkpoint semantics consistent |
| `.11` | Make embedding-cache mutation deadlines cancellation-safe | created and closed | closed | Kept admission held through real commit/rollback |
| `.12` | Surface embedding-cache maintenance failures | created and closed | closed | Made retention failures observable |

No tracker mutation was needed during this save operation: `bd show axon_rust-qsbhe --json` showed the parent and all 12 children closed, with the parent close reason tied to merged commit `ad842bd` and full CI.

## Repository Maintenance

### Plans

- Inspected `docs/plans/`; no plan was unambiguously owned and completed by PR #576, so no file was moved to `docs/plans/complete/`.
- Existing artifact-engine plans were left in place because their broader status could not be inferred safely from this performance/cache session.

### Beads

- Verified the parent and every directly related child are closed. No known implementation or verification remainder justified another follow-up bead.
- Duplicate children `.2/.3`, `.4/.6`, `.5/.8`, and `.7/.9` were retained as historical tracker records rather than destructively rewritten.

### Worktrees and branches

- `git worktree list --porcelain` showed the clean current `main` worktree, an unknown Claude worktree at `1c4e0f461`, and `codex/history-pipeline-unification` at current `main`. Both auxiliary worktrees were left intact because ownership/activity was not proven stale.
- Local branches `codex/live-source-benchmark`, `codex/release-v7.2.20`, and `codex/speed-ci-contracts` have gone upstreams; they were not deleted because squash/ownership safety was not established in this pass.
- `origin/codex/fix-sqlite-starvation` remains at PR head `13674c654`. It was left as merge provenance because PR #576 used a squash merge and the ref is not an ancestor of `main`.

### Stale docs

- The implementation commit already refreshed the config, environment, database, dependency, source, chunking, and vector-payload generated references. `cargo xtask generated-contracts check` passed before merge.
- No additional stale documentation contradiction was found during closeout.

## Tools and Skills Used

- **Shell and file tools.** `git`, `gh`, `bd`, Cargo, `xtask`, repository scripts, configuration inspection, and path-limited patching/commits. Shared Cargo build locks occasionally delayed duplicate focused runs.
- **Axon and infrastructure tooling.** Axon source/crawl/job commands, TEI, Qdrant, Incus/container deployment, and GPU/provider inspection were used to obtain live cold-path timing and behavior evidence.
- **Skills.** `vibin:repo-status`, `vibin:review-pr`, `superpowers:systematic-debugging`, Axon ingestion/crawl guidance, Lavra review, and `vibin:save-to-md` structured the status, debugging, review, remediation, and documentation work.
- **Browser/web research.** Internet search and remote browser/device flows were used to research TEI/model alternatives and model availability; Hugging Face rate limiting affected one CI live-RAG attempt.
- **Agents.** Parallel embedding-cache, Vibin code/error/test, and Lavra security/performance/architecture/Rust/simplicity/goal-verification agents reviewed independent surfaces and re-reviewed fixes to closure.

## Commands Executed

| command | result |
|---|---|
| `cargo test -p axon-embedding ...` | Final cache suite passed 12/12 |
| `cargo test -p axon-jobs embedding_cache_store ...` | Final store suite passed 16/16, including real mid-transaction admission proof |
| `cargo test -p axon-services ...batches...` | Real generation call-site suite passed 8/8 |
| `cargo test -p axon-services ...pipeline...` | Real vector overlap suite passed 6/6 |
| `cargo xtask generated-contracts check` | Generated schemas, docs, manifests, and dependency graph were current |
| `cargo xtask check-layering` | Crate ownership/layering passed |
| `cargo fmt --all -- --check` | Formatting passed |
| `cargo clippy --all-targets --locked -- -D warnings` | Clippy passed in CI |
| `gh pr checks 576 --watch` | All required PR checks eventually passed |
| `gh pr view 576 --json ...` | Confirmed merged PR and squash commit `ad842bd` |
| `git pull --ff-only` | Local `main` synchronized with `origin/main` |

## Errors Encountered

- Cold ingestion stalled because all SQLite connections could wait behind writers; one shared admission gate now prevents pool starvation.
- The early cache retention calculation produced a negative `LIMIT` and erased cache rows; clamping fixed it, and later maintenance moved to exact state plus fixed victim batches.
- Per-vector SQLite inserts added thousands of SQLx round trips; one bind-budgeted bulk upsert per provider result removed that regression.
- A release build began before the retention fix landed, so a newer binary timestamp still contained buggy code; a clean rebuild was required.
- Vibin found cache identity/layering, ambient Markdown config, fence semantics, cancellation, and call-site test defects; each was reproduced or evidenced and remediated.
- CI found a stale generated crate graph and two boundary expectations (cache env keys and jobs migration count); generated artifacts and contract fixtures were corrected.
- The live-RAG job hit a transient Hugging Face HTTP 429 while downloading `BAAI/bge-small-en-v1.5`; its rerun passed without a code workaround.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| SQLite scheduling | Eight workers could exhaust the pool while waiting to write | Writers acquire one shared gate before consuming pool/transaction resources |
| Embedding cache | No production content cache; early prototype was destructive/slow | Opt-in, identity-safe, fail-open cache with bounded writes, TTL, and maintenance |
| Warm cache hits | Could await LRU bookkeeping | Return immediately when the writer gate is busy; recency is best effort |
| Markdown chunking | Ambient config and fragile character/fence behavior | Injected semantic windows preserve structural boundaries and bounds |
| Web acquisition | Current batch completed before next acquisition | Supported web sources prefetch exactly one next batch |
| Vector publication | Embedding and prior upsert were serial | Next embedding overlaps current upsert with deterministic checkpoints/errors |
| Cache retention | Full counts/tail deletion or traffic-dependent cleanup | O(1) cardinality plus periodic indexed 512-row victim batches and 7-day TTL |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| Embedding cache focused suite | Identity, fail-open, bounded concurrency | 12/12 passed | pass |
| SQLite cache-store suite | Migration, TTL, pruning, admission safety | 16/16 passed | pass |
| Generation batch suite | Bounded lookahead, phase, artifacts, errors | 8/8 passed | pass |
| Vector pipeline suite | Real overlap and checkpoint ordering | 6/6 passed | pass |
| Full workspace test CI | No regressions | 5,390 tests passed | pass |
| Rust contracts | Generated/config/database invariants | Passed after two contract fixes | pass |
| Live RAG PR job | Provider-backed runtime smoke | Passed on retry after external 429 | pass |
| PR required checks | Repository Contract, `ci-gate`, `codeql-gate`, `compose-smoke-gate` | All passed | pass |
| Merge state | PR #576 merged to default branch | `main` and `origin/main` at `ad842bd` | pass |

## Risks and Rollback

- Embedding caching remains opt-in, limiting behavior change. Disable it with the typed configuration/environment switch if cache behavior is suspect.
- Overlap is bounded to one acquisition batch and one next embedding batch. A rollback can revert squash commit `ad842bd`, but generated schemas and SQLite migrations must be handled with the repository's forward-migration policy rather than deleting migration history.
- Periodic maintenance is fail-open and observable; persistent SQLite errors may delay physical reclamation but no longer fail source ingestion silently.

## Decisions Not Taken

- Did not replace Qwen3-Embedding-0.6B after alternate model/runtime benchmarking did not demonstrate a clear quality-compatible win.
- Did not use source-scoped cache deletion because content-addressed rows can be shared by multiple sources; fixed TTL was safer without provenance joins.
- Did not allowlist cache code in the layering or monolith gates; responsibilities and oversized functions were split instead.
- Did not make speculative prefetch generic and unbounded; only adapters that explicitly opt in receive one-batch lookahead.

## References

- PR #576: https://github.com/dinglebear-ai/axon/pull/576
- Merge commit: `ad842bd396423e632d0f6281e836dea0d5bcd280`
- Chunking contract: `docs/pipeline-unification/sources/chunking-contract.md`
- Generated database schema: `docs/reference/runtime/database-schema.json`
- Configuration reference: `docs/reference/config/config-toml.md`

## Open Questions

- The auxiliary worktrees and gone-upstream local branches have unclear ownership; review them separately before cleanup.
- The squash-merged remote feature branch remains for provenance; delete it only after confirming no follow-on work depends on its individual commits.
- The session transcript path was not available from the injected context, so this note reconstructs the full session from retained conversation context, Git, GitHub, Beads, and verification evidence.

## Next Steps

- No implementation, review, CI, merge, or Bead work remains for PR #576.
- Run a fresh production cold and warm `code.claude.com` crawl after deploying `ad842bd` if a post-merge operational benchmark is desired; record provider timing, document count, chunk/vector count, and cache hit/miss metrics separately.
- Audit the three auxiliary/gone-upstream branches and two registered auxiliary worktrees in a dedicated cleanup pass with owner confirmation.
