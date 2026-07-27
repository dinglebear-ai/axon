# Pipeline Unification Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Use `superpowers:subagent-driven-development` only when Jacob explicitly requests delegated agent work. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining 28 implementation findings under
`axon_rust-enbmu`, prove one source execution path and one provider-capacity
authority, reconcile the pipeline-unification contract packet with reality, and
ship the resulting release.

**Architecture:** Live guardrails and behavioral characterization land first.
Canonical stage and attempt identities then become executable data. One
SQLite-authoritative scheduler gates existing provider traits through a single
reserved-call facade. One family-blind runner consumes the existing adapter
registry and publishes through an epoch/watermark protocol spanning Qdrant,
SQLite, graph state, artifacts, and retrieval. Security is enforced at
admission, connect, same-handle local reads, and publication. Real transport and
production-ingress tests close the loop.

**Tech Stack:** Rust 2024, Tokio, Axum, rmcp, SQLite, Qdrant, TEI, Chrome/CDP,
`cargo xtask`, Beads (`bd`), GitHub Actions, Docker Compose, Incus, systemd.

## Confirmed Starting State — 2026-07-26

- Review branch base and `origin/main` are both
  `fd4fac12def85e0b065a6a0035781a29f426dce4`.
- The review worktree is clean before this plan edit.
- PR `#466` merged the first 28 findings and tagged `v7.2.0`.
- PR `#468` merged the llama/TEI compose work. Do not recreate that branch or
  preserve an old dirty compose diff.
- PR `#469` merged the first reviewed version of this plan.
- The host and Incus container both still report `axon 7.1.5`; the operational
  v7.2 deployment remains undone and is independent of the code dependency
  graph.
- Epic `axon_rust-enbmu` still owns 28 open implementation findings. Tracking
  bead `axon_rust-enbmu.2` owns this plan review only.
- `marketplace-no-mcp` remains the intentional protected marketplace variant.

## Execution Status — 2026-07-26

This is an honest in-progress ledger, not a completion claim.

- Task 1 completed locally on 2026-07-26: PR #468 was verified on
  `origin/main`; host and Bookworm container 7.2.2 binaries were backed up,
  built, installed atomically, and smoke-tested through the systemd service,
  REST, and MCP. The existing container config was validated with the
  migration dry-run; it required no file writes.
- Task 2A–2C were merged before this worktree run (PRs #471, #474, and #475).
- Task 2D is implemented locally in `f04f9f66e` plus the artifact-canary
  hardening in this worktree; its focused docs tests, docs drift check, and
  clippy pass (36 tests).
- Task 3 is now in progress locally: the test-only observation harness covers
  web, local, and session fixtures and its shared phase/content/job assertions
  pass. Cancellation and route-error mapping are covered by focused tests;
  the requested integration-target command and the remaining canonical
  characterization cases are still open.
- Task 4 has the router cleanup plus a local registry-validation slice: the
  shared adapter registry now rejects duplicate names, missing matrix
  families, version drift, and capability/spec mismatches. Production
  composition wiring and the remaining adapter-boundary work remain. The
  registry/session metadata projection has also moved into the adapters, and
  the shared runner no longer performs either family rewrite. REST and MCP
  source authorization now consume the canonical route's safety class instead
  of independently classifying raw input.
- Tasks 5–12 have not started.

## Global Constraints

- Treat `docs/pipeline-unification/` as normative. If live code and a current
  contract disagree, repair one or the other explicitly in the same PR.
- `CLAUDE.md` is the agent-memory source; keep sibling `AGENTS.md` and
  `GEMINI.md` symlinked to it.
- Use `.worktrees/<branch>` and a claimed Bead for each non-trivial unit.
- Keep CLI, MCP, REST, and panel thin; orchestration belongs in
  `axon-services`.
- Keep the public `SourceAdapter` boundary to `name`, `version`,
  `capabilities`, `discover`, `acquire`, and `normalize`.
- Reuse `SourceAdapterRegistry`, `JobStagePlan`, `PipelinePhase`,
  `SourceProgressEvent`, `JobPriority`, and current provider traits. Do not add
  parallel factories, phase registries, priority enums, or fake proof types.
- One source request retains one job id across all stages. Crash recovery keeps
  the job id but always creates a new fenced attempt.
- Provider capacity is enforced before the call. Publication visibility is
  enforced before public reads.
- Redaction failure is fail-closed and cannot leak through its audit path.
- No `mod.rs`; tests use sibling `*_tests.rs`; production Rust files remain
  within the monolith policy.
- Run targeted checks while iterating and the full repository gate once for
  each code PR. Documentation-only plan edits use structural validation.
- Merge each unit before branching its dependent. Close a Bead only after its
  acceptance criteria are verified on merged `origin/main`.
- Preserve `marketplace-no-mcp`; do not merge or delete it.

## Contract Alignment

| Concern | Authority | Required outcome |
|---|---|---|
| Source runner | `foundation/source-pipeline.md` | One family-blind executor above the six-method adapter boundary. |
| Adapter types | `foundation/types/trait-contract.md` | Shared registry instances; execution state only in typed requests/results. |
| Jobs and stages | `runtime/job-contract.md` | One job id; new attempt after owner loss; canonical stage ids and skips. |
| Provider capacity | `runtime/provider-contract.md` | One capacity domain authority; queued SQLite grants; no raw service bypass. |
| Authentication | `runtime/auth-contract.md` | Compatibility scope widening remains; mutating operations separately require literal write scope. |
| Security | `runtime/security-contract.md`, `runtime/redaction-contract.md` | Connect-time SSRF, same-handle local containment, fail-closed redaction. |
| Testing | `delivery/testing-contract.md` | Behavioral family and transport parity through real boundaries. |
| Documentation | `delivery/docs-generator-contract.md` | All 17 families render from declared live inputs and drift checks compare bytes. |

## Normative Designs

### Stage and attempt identity

Use the existing `JobStagePlan` and `PipelinePhase` as the only executable
stage registry. Add stable stage ids, applicability, allowed skips, and
transition validation there. A stale/process-crash recovery always increments
the attempt. Stage ids, source leases, publication leases, reservation leases,
and side-effect fences bind to `(job_id, attempt)`. Only an in-process retry
under the same live owner may continue an attempt. Every mutation rejects an
older attempt.

### Provider capacity domain and scheduler authority

Define:

```rust
pub struct ProviderCapacityDomain {
    pub kind: ProviderKind,
    pub instance_id: ProviderInstanceId,
    pub authority_id: SchedulerAuthorityId,
}
```

One authority/database owns each stable `(kind, instance_id)`. Processes may
either share that SQLite file on a filesystem with supported locking or proxy
reservation-bearing operations through the authority service. A local process
configured for a remote authority must reject provider-backed `--local`
execution when it cannot share the authority database. Two distinct databases
must never independently grant against the same provider identity.

The queue algorithm is fixed:

- Base ranks are `Interactive=0`, `High=1`, `Normal=2`, `Background=3`,
  `Maintenance=4`.
- Every configured aging quantum reduces effective rank by one, to a floor of
  zero.
- Candidates order by effective rank, enqueue sequence, then reservation id.
- A configured interactive capacity reserve is unavailable to lower ranks while
  interactive work is queued.
- Global entries, per-job entries, queued units, maximum request units, and
  maximum active duration are bounded.
- A caller holds capacity for at most one configured fairness quantum, bounded
  by units, batches, and wall time, then yields and reacquires.
- The maximum-wait test derives its bound from queue caps, maximum active
  duration, fairness quantum, and aging quantum.

SQLite is the correctness path. Waiters use a predicate loop with bounded
jittered polling until grant, terminal state, or deadline. An in-process
`Notify` may reduce latency but is never required for correctness.

### Reserved provider call

Keep `EmbeddingProvider`, `VectorStore`, fetch/render, graph, artifact, and LLM
traits unchanged. Gate them through one scheduler-owned facade:

```rust
pub async fn call_reserved<K, T, E, F, Fut>(
    scheduler: &ProviderScheduler,
    request: ReservationRequest<K>,
    operation: F,
) -> Result<T, ReservedCallError<E>>
where
    F: FnOnce(ActiveReservationLease<K>) -> Fut,
    Fut: Future<Output = Result<T, E>>;
```

`call_reserved` atomically queues/grants, activates, renews, invokes, and records
one terminal transition. Production `axon-services` code cannot retain raw
provider handles outside this facade; `cargo xtask check-layering` rejects new
raw calls. Tests use the same facade with a fake reservation store.

`Drop` is not an async terminal transition. Callers explicitly
`complete`, `fail`, or `cancel`; a dropped lease records cleanup debt/best-effort
notification, and the reconciler owns terminal cleanup.

An unactivated grant may expire and be regranted. An active lease is renewed by
the owning attempt heartbeat and may be replaced only after its provider future
is aborted and joined, or after owner death plus the provider hard deadline and
conservative grace. If termination cannot be proven, quarantine the units and
enter degraded/cooldown state. Never regrant on lease expiry alone. Side-effect
writes revalidate the fence immediately before commit.

### Publication epoch and visibility

Use one serialized publication lease initially; sharding is deferred. For
reserved epoch `E` and current visible watermark `W`:

1. Bind the publication lease to `(job_id, attempt, source_id, generation, E)`.
2. Write Qdrant points as pending with `born_epoch=E`; mark replaced points with
   `retired_epoch=E`. Public retrieval continues to snapshot `W`.
3. Stage content-addressed artifact bytes without public registry rows.
4. In one SQLite transaction, apply graph mutations, document status, artifact
   registry rows, ledger generation CAS, durable `publication_authorized`, and
   advance the collection watermark to `E`.
5. Public vector reads filter
   `born_epoch <= W AND (retired_epoch IS NULL OR retired_epoch > W)`.
   Service retrieval also batch-verifies `(source_id, generation, fence)`
   against the committed ledger until promotion finalization is complete.
6. Idempotent finalization marks external data promoted. A pre-watermark crash
   leaves data invisible and records cleanup debt; a post-watermark crash
   resumes finalization and never rolls the committed head backward.

Job lifecycle and owner-visible progress events remain immediate. Before
authorization they contain only phase/count/failure data and never content.
They may not claim `Publishing`, `Complete`, or public generation success.
Only vectors, graph evidence/mutations, statuses, artifacts, and other
generation-scoped projections are commit-fenced.

### Authentication and security

Preserve the deliberate compatibility rule that ordinary Axon read/write
scopes satisfy general route checks. Mutating `/v1/search` and `/v1/research`
must additionally call `has_explicit_scope(axon:write)`. A read-only caller is
denied that mutating operation; optional non-indexing search is not introduced
without a separate contract and schema change.

Every durable job stores the exact caller scope set, visibility ceiling, policy
version, and requested/effective priority. Retry, recovery, watch, query, ask,
retrieve, and child work clone that snapshot; none mint Admin, Local, Execute,
or broader visibility.

HTTP acquisition uses an injected resolver, canonical host parsing, and a
pinned reqwest connect target for every redirect. Ambient HTTP(S) proxy
variables are ignored unless an explicitly configured proxy is itself
validated. Browser traffic uses an Axon-controlled forward proxy/network
gateway that applies the same resolve-once/pinned-connect policy to main frames,
redirects, subresources, service workers, and WebSockets. Missing interception
or enforcement fails startup/request closed.

Local reads use one validated handle throughout:

- Linux: `openat2` component walk with containment flags.
- Other Unix: `openat` plus `O_NOFOLLOW` component walk.
- Windows: `CreateFileW` with reparse-point controls, final path/volume
  verification, and reads through that same handle.
- Unsupported platforms fail closed for local, tool, CodeSearch, and artifact
  roots.

Hardlinks outside an explicitly configured trusted root are rejected. The test
matrix covers symlink, rename, hardlink, magic-link/reparse-point, `scope=file`,
and CodeSearch races on every shipped OS.

If redaction fails or panics, emit only a constant-shape emergency event made
from enums, bounded counts, stable policy/detector ids, and an opaque
correlation id. It contains no raw input, URL/path, error `Display`/source
chain, payload hash, or metadata and does not call the failed redactor.

Panel authentication maps to an explicit fixed panel scope set and visibility
ceiling. It does not imply Local or Execute. Background jobs clone that exact
snapshot. `/api/panel/env` returns only compile-time-allowlisted non-secret key
names and `configured: bool`; it never returns values, defaults, lengths,
dynamic prefixes, or inferred metadata.

## Delivery Graph

```text
independent v7.2 deployment

live gates + docs engine
          |
behavior harness + adapter/router normalization
          |
stage/attempt identity
          |
SQLite scheduler + reserved-call gate
          |
publication epoch + canonical runner ports
          |
security + bounded reliability
          |
real transport/ingress proof
          |
17 docs families + final audit/release/deploy
```

## Per-PR Merge Protocol

1. Fetch `origin`; create `.worktrees/<unit>` from merged `origin/main`; claim
   only the listed Bead.
2. Add the focused failing test first and record the expected failure.
3. Implement the smallest unit below; regenerate only affected references.
4. Run the focused commands listed for that unit, then `just precommit`.
5. Commit, push, open a focused PR, wait for required checks, and merge.
6. Verify the merge on `origin/main`, attach implementation/test evidence to the
   Bead, then close it if every subfinding is complete.
7. Run `bd dolt push`; branch the next dependency from the merged commit.

## Task 1: Complete the Independent v7.2 Operational Baseline

**Tracking:** existing operational Bead `axon_rust-ana1h`; not an
`axon_rust-enbmu` code dependency.

**Files:** read-only `~/.axon/.env`,
`axon:/mnt/axon-data/config.toml`; install paths
`/home/jmagar/.local/bin/axon` and `axon:/usr/local/bin/axon`.

- [x] Verify PR `#468` is on `origin/main`; do not create another compose PR.
- [x] Back up both binaries and the validated container config with checksums.
- [x] Build the host binary from merged `main`.
- [x] Build a separate Bookworm-compatible binary for the Incus container.
- [x] Install with temporary files plus atomic rename.
- [x] Validate the already-applied config migration; no file rewrite was
  needed, and the clean-break rewrite command remains dry-run-only.
- [x] Restart and enforce a bounded health deadline.
- [x] Smoke host CLI, container service, REST, and MCP.
- [x] On any mixed version or failed health check, restore both binaries and
  configs and verify `7.1.5` health.

This operation must not block Tasks 2–5.

## Task 2: Repair Live Gates in Small PRs

**Beads:** `axon_rust-jc20j`; prerequisite portions of
`axon_rust-a155h` and `axon_rust-5iglz`.

### Task 2A — Layering gate

**Files:** `xtask/src/checks/layering.rs`,
`xtask/src/checks/layering_tests.rs`,
`xtask/src/checks/crate_contracts_spec.rs`,
`xtask/src/checks/crate_contracts_spec_cont.rs`.

- [x] Add fixtures containing the known transport imports from
  `axon-adapters`, `axon-llm`, and source internals.
- [x] Run `cargo test -p xtask layering -- --nocapture`; record the red result.
- [x] Replace deleted-crate prefixes and remove stale allowlist rows.
- [x] Audit all 23 live crates.
- [x] Add a rule rejecting raw provider calls from transports and
  `axon-services` modules outside the reserved-call facade path.
- [x] Run `cargo xtask check-layering` and
  `cargo xtask check-crate-contracts`.

### Task 2B — Screenshot helper

**Files:** `crates/axon-adapters/src/web_engine/screenshot.rs`,
`crates/axon-core/src/paths.rs`,
`crates/axon-cli/src/commands/screenshot/util.rs` and sidecar tests.

- [x] Move only the pure filename/path helper to `axon-core`.
- [x] Remove the CLI production dependency on `axon-adapters` if unused.
- [x] Run `cargo test -p axon-cli screenshot --no-fail-fast`.

### Task 2C — Chat provider facade

**Files:** `crates/axon-web/src/server/handlers/chat_stream.rs`,
`crates/axon-web/src/server/handlers/chat_stream_tests.rs`, and the existing
`axon-services` chat facade.

- [x] Move direct provider construction behind a typed service method without
  changing response semantics.
- [x] Propagate cancellation when the transport disconnects.
- [x] Run `cargo test -p axon-web chat_stream --no-fail-fast`.
- [x] File bounded slow-consumer/idle-total deadline tuning as a separate
  reliability Bead; it is not part of the layering PR.

### Task 2D — Documentation engine

**Files:** `xtask/src/docs.rs`, existing `xtask/src/docs/*`, and new sibling
modules named by `delivery/docs-generator-contract.md`.

- [x] Add `DocsFamilyGenerator`, `DocsArtifactSet`, and
  `GeneratedDocArtifact`.
- [x] Make `--check` render in memory and byte-compare declared outputs without
  writing.
- [x] Fail missing inputs, empty/header-only outputs, nondeterministic ordering,
  and secret/path canaries.
- [x] Land the six critical families: `api-dto`, `api-enums`, `adapters`,
  `events`, `providers`, and `schema`.
- [x] Run `cargo test -p xtask docs -- --nocapture` and
  `cargo xtask docs generate --check`.

Close `axon_rust-jc20j` after 2A–2C merge. Keep contract exemptions and the docs
Bead open until Tasks 7 and 11.

## Task 3: Characterize Behavior Without a New Runtime Model

**Beads:** `axon_rust-fts94`, `axon_rust-j801x`.

**Files:** integration tests under `tests/`, sidecars adjacent to the shared
source service, `crates/axon-adapters/src/testing.rs`.

Define the harness as a test-only aggregation:

```rust
pub struct PipelineObservation {
    pub request: SourceRequest,
    pub progress: Vec<SourceProgressEvent>,
    pub durable_stages: Vec<JobStageRecord>,
    pub provider_calls: FakeProviderCalls,
    pub result: SourceResult,
}
```

It derives phases from existing events/stage rows and adds fake-only call
counts. It does not add a production phase enum, counter model, or trace format.

- [x] Add equivalent web, local, and non-web fixture requests.
- [ ] Assert identical route, stage order, normalized shape, publication
  semantics, one-job identity, cancellation, and error mapping.
- [x] Assert progress is visible before publication but contains no document
  content.
- [ ] Change the three tests that pin defective family behavior into failing
  canonical expectations.
- [x] Run the harness against production composition with fake providers.
- [x] Run `cargo test --test source_pipeline_differential -- --nocapture`.

The PR reaching `main` includes the green implementation for each changed
characterization; no ignored red test lands.

## Task 4: Normalize the Existing Adapter Registry and Router

**Beads:** `axon_rust-yygrl`, `axon_rust-dkuqo`, `axon_rust-gpuz9`,
`axon_rust-igp0i`, `axon_rust-bfsp5`; prerequisite portion of
`axon_rust-upay4`.

**Files:** `crates/axon-adapters/src/{adapter,registry,spec,family_matrix}.rs`,
`crates/axon-adapters/src/web/site_discovery.rs`,
`crates/axon-adapters/src/providers/http_fetch.rs`, route modules in
`crates/axon-route/src/`, and sidecar tests.

- [ ] Rehabilitate `SourceAdapterRegistry` as the single registry of shared
  `Arc<dyn SourceAdapter>` values.
- [x] Add fail-closed validation for duplicate names, missing families,
  capability/spec mismatch, and family-matrix coverage.
- [ ] Keep per-execution state only in `SourcePlan`, `SourceAcquisition`, and
  normalized results.
- [ ] Add sequential and concurrent same-instance tests proving no ETag,
  request, auth, or output leakage.
- [ ] Delete both family classifiers and route from canonical source
  identity/capabilities.
- [x] Remove `SourceRouter::validate_options`; validate exactly once during
  `RoutePlan` construction and remove its generator/contract references.
- [x] Route CodeSearch refresh through the normal source route instead of
  returning `None`.
- [ ] Inject `FetchProvider` into web discovery and remove direct network calls.
- [ ] Put ledger-trusted ETag/Last-Modified and `CachePolicy::Revalidate` into
  each changed item’s typed `FetchPlan`; reject equivalent untrusted manifest
  metadata.
- [ ] Run adapter, route, discovery, and CodeSearch focused tests.

## Task 5: Land Canonical Stage and Attempt Identity

**Beads:** prerequisite portions of `axon_rust-drahp`,
`axon_rust-nl7au`, and `axon_rust-a155h`.

**Files:** existing `JobStagePlan`/`PipelinePhase` owners in `axon-api`,
`axon-jobs`, `axon-observe`, and source service stage tests.

- [x] Extend the existing phase descriptor with stable id, applicability,
  allowed skip, and transition rules.
- [ ] Make every mutating stage accept `(job_id, attempt, stage_id, fence)`.
- [ ] Make stale/process-crash recovery increment attempt; allow same-attempt
  retry only under a live owner.
- [ ] Reject old-attempt ledger, reservation, event, graph, vector, artifact,
  and document-status mutations.
- [ ] Add a split-brain test in which the old worker resumes after recovery and
  proves every stale mutation rejected or invisible.
- [ ] Run `cargo test -p axon-jobs attempt -- --nocapture` and source stage
  transition tests.

## Task 6: Implement the SQLite Scheduler and Reserved-Call Gate

**Beads:** `axon_rust-nl7au`, `axon_rust-uzy27`; reservation portions of
`axon_rust-er3z7`.

**Files:** `crates/axon-jobs/src/migrations/0002_provider_scheduler.sql`,
`crates/axon-jobs/src/migrations.rs`,
`crates/axon-jobs/src/migration-checksums.txt`,
`crates/axon-jobs/src/unified/heartbeat.rs`, new focused scheduler siblings,
`crates/axon-services/src/` provider composition, generated database schema.

### Schema and upgrade

- [x] Add append-only migration `0002_provider_scheduler.sql`; do not edit
  epoch-1 history.
- [x] Add capacity-domain identity, enqueue sequence, requested/effective
  priority, queue/grant/lease deadlines, lease owner, attempt/stage fence,
  renewal, terminal reason, and quarantine state.
- [x] Add selection and expiry indexes used by the grant/reaper queries.
- [x] Convert current heartbeat reservation writes into read-only projection in
  the same PR so there is one authority.
- [x] Define disposition for old `requested`, `queued`, `granted`, and `active`
  rows: nonterminal epoch-1 rows become terminal `migration_cancelled` and are
  safely retried under the new attempt.
- [ ] Add a real epoch-1 fixture upgrade test and update migration identity and
  generated database-schema references.

### Atomic grant and wait

- [x] Implement queue/grant in one `BEGIN IMMEDIATE` transaction using the
  indexed head candidate from each lane.
- [ ] Implement the fixed aging/FIFO algorithm and interactive reserve exactly
  as specified above.
- [x] Reject requests larger than declared capacity before queueing.
- [x] Enforce global/per-job entry and unit caps.
- [ ] Implement bounded jittered SQLite predicate polling; local notification
  is optional latency optimization only.
- [ ] Add cross-process tests proving a waiter in process B observes a release
  from process A without a shared in-memory signal.
- [ ] Add query-plan assertions preventing full scans and record scheduler SQL
  statements per grant.

### Lease lifecycle

- [x] Implement explicit async activate, renew, complete, fail, cancel, and
  reconcile transitions.
- [ ] Tie renewal to the owning attempt heartbeat and provider hard deadline.
- [ ] Quarantine uncertain active units; regrant only after abort/join or
  owner-death deadline plus grace.
- [ ] Add dropped-lease, killed-process, hung-provider, and stale-completion
  tests.
- [ ] Prove no replacement grant while the old provider future remains alive.

### Call gate and all lanes

- [x] Implement `call_reserved` over unchanged provider traits.
- [ ] Remove/de-authorize
  `axon-observe::ProviderReservationManager`; observation reads durable state.
- [ ] Route source embedding, interactive query/ask/retrieve embedding, vector
  writes, and fetch/render through the gate.
- [ ] Create/reuse query job, attempt, stage, and reservation in one transaction;
  retain them only for a short configured diagnostic window.
- [ ] Persist requested and server-derived effective priority.
- [ ] Release after the bounded fairness quantum and reacquire with one
  continuation cursor and idempotency key.
- [ ] Test two schedulers on one database, and reject two databases declaring
  authority for the same stable provider identity.
- [ ] Reject provider-backed `--local` execution when only the remote authority
  may schedule it.

### Numeric acceptance

- [ ] Under the deterministic mixed-load fixture, assert no capacity
  overcommit, zero lost wakeups, and the derived maximum wait.
- [ ] Assert scheduler p95 grant transaction time under 25 ms, SQLite busy
  failures equal zero, no selection full scan, and no more than the documented
  constant SQL statements per grant/release.
- [ ] Run focused scheduler tests, SQLite migration checks, query/embedding
  tests, then `just precommit`.

## Task 7: Implement Publication Epochs and Collapse the Runner

**Beads:** `axon_rust-drahp`, `axon_rust-2wq1r`; remainder of
`axon_rust-upay4` and `axon_rust-a155h`.

**Files:** `crates/axon-services/src/source.rs`,
`crates/axon-services/src/{source_jobs,web_source}.rs`,
`crates/axon-vectors/src/qdrant/`,
`crates/axon-vectors/src/qdrant/read/retrieve.rs`,
`crates/axon-ledger/src/migrations/`,
`crates/axon-graph/src/migrations/`,
artifact registry/store owners, source crash/parity sidecars.

### Publication foundation

- [x] Add append-only ledger/graph publication-state migrations and exact
  Qdrant payload fields `born_epoch` and `retired_epoch`.
- [x] Add a durable serialized publication lease with bounded takeover rules.
- [ ] Add one SQLite finalization transaction covering graph mutations,
  document status, artifact registry, ledger CAS, authorization, and watermark.
- [ ] Add retrieval watermark filtering and batched committed-ledger fence
  verification.
- [ ] Make artifact staging content-addressed and registry publication
  transactional.
- [ ] Add idempotent finalization and cleanup debt.
- [ ] Inject crashes before authorization, after authorization, mid-Qdrant
  promotion, during artifact promotion, after watermark, and during cleanup.
- [ ] Prove direct Qdrant reads and service retrieval cannot expose pending or
  mismatched-fence data.

### One runner

- [ ] Express the canonical executor with existing `JobStagePlan` and
  `PipelinePhase`; add no second stage registry.
- [ ] Give `JobIntent::Acquire` the canonical route/discover/acquire/normalize
  prefix and declared skips for publish/graph/cleanup.
- [ ] Keep CLI `scrape` as the canonical page indexing projection.
- [ ] Invoke shared `Arc<dyn SourceAdapter>` values; do not construct
  per-execution adapter factories.
- [ ] Slice `SourceManifestDiff` into bounded batches before `acquire` and
  `normalize`; enforce hard discovery item/byte caps.
- [ ] Port web, then local, then every remaining family through that executor.
- [ ] Delete `web_source`, local orphan-job creation, and remaining family
  orchestration only after parity is green.
- [ ] Keep progress events immediate and content-free before publication.
- [ ] Add `Acquire` versus `Index` prefix parity and same-adapter concurrency
  tests.
- [ ] Run the Task 3 differential harness and crash matrix after each family
  port.

## Task 8: Close Authentication and Security Gaps

**Beads:** `axon_rust-9veac`, `axon_rust-cjxfw`,
`axon_rust-4ygmz`, `axon_rust-0sgqz`, `axon_rust-a4t01`,
`axon_rust-hf37r`; security portions of `axon_rust-zb0k1`.

### Auth snapshot and panel

- [x] Preserve `axon_read_scope_satisfies_write_routes` compatibility coverage.
- [x] Preserve/add `explicit_scope_check_rejects_broad_widening`.
- [x] Require literal `axon:write` for mutating search/research in REST and MCP.
- [ ] Persist exact caller snapshots for search, query, ask, retrieve, retry,
  recovery, watch, and child jobs; add no-escalation tests across attempts.
- [ ] Derive effective priority server-side and audit downgrades.
- [ ] Implement the fixed panel scope set and visibility ceiling; exclude Local
  and Execute unless a separate explicit operator control grants them.
- [ ] Test panel tokens against env, local, tool, source, and job endpoints.
- [ ] Restrict `/api/panel/env` to the compile-time key/configured-state allowlist.

### Admission policy

- [ ] Invoke exactly one `SourceAccessDecision` at service admission and retain
  `AffinityPolicy` only as its internal component.
- [ ] Build `RouteSecurityPolicy` from operator config and caller policy.
- [ ] Remove `trusted_tool_execution()` from production routing.
- [ ] Delete dead `enforce_network_source_policy`; connect-time policy belongs
  only at injected fetch/render boundaries.

### HTTP and browser SSRF

- [ ] Canonicalize IDNA, trailing dot, IPv4-mapped IPv6, and alternate numeric
  forms before policy; reject userinfo and unsupported schemes.
- [ ] Pin the validated connect target and revalidate every redirect.
- [ ] Disable ambient proxy bypass; validate explicitly configured proxies.
- [ ] Route all browser traffic through the Axon-controlled pinned-connect
  gateway and require CDP interception as defense in depth.
- [ ] Test DNS rebinding, mixed A/AAAA, redirect, subresource, service worker,
  WebSocket, proxy-env, and interception-loss cases.
- [ ] Remove all release-build loopback test bypasses.

### Local containment

- [ ] Implement Linux, Unix, and Windows same-handle algorithms specified above.
- [ ] Unify denylists for local source, CodeSearch, tool, and artifact roots.
- [ ] Enforce the explicit hardlink policy.
- [ ] Run the per-OS race suite; fail closed on unsupported platforms.
- [x] Remove blocking canonicalization from async worker paths.

### Redaction

- [ ] Redact before every vector, artifact, graph, event, and memory write.
- [ ] Abort publication on detector error/panic.
- [ ] Emit the constant-shape emergency audit event without invoking redaction.
- [ ] Add canaries proving no raw value, hash, URL/path, metadata, or error chain
  reaches logs/events/traces when detector or audit sink fails.

## Task 9: Bound Reliability and Performance

**Beads:** remainder of `axon_rust-er3z7`; remainder of
`axon_rust-zb0k1`.

**Files:** source runner batching, document-status store, manifest merge,
embedding identity cache, Qdrant writer/readers, worker admission, observability
queries.

- [x] Replace per-document status writes with bounded multi-row transactions;
  prove rollback and bind-count limits.
- [x] Replace quadratic status/manifest merge with one indexed map/set pass;
  assert comparison count is linear.
- [ ] Enforce actual encoded bytes after serialization and before every channel,
  artifact, and Qdrant batch; cap sparse entries and status records.
- [ ] Bound manifest items, acquisition items, normalized documents, chunk
  buffers, vectors, statuses, graph candidates, and artifact metadata.
- [ ] Reject an indivisible oversized item with a structured error; prove split
  logic always makes forward progress.
- [x] Make `embed=false` skip collection ensure/create.
- [x] Add TEI identity cache keyed by endpoint, model, dimension, and relevant
  config with per-key singleflight, bounded TTL/negative behavior, and explicit
  invalidation.
- [x] Wire `qdrant-point-buffer` as the only vector item limit and delete
  `UPSERT_BATCH_SIZE`.
- [x] Consume/document source concurrency or delete the dead knob.
- [ ] Limit concurrent DB stages with a semaphore derived from pool size while
  reserving one connection for heartbeat/control work.
- [ ] Use operation-specific Qdrant deadlines, idempotency keys, cancellation,
  and partial-timeout reconciliation.
- [ ] Aggregate observability with bounded SQL queries and bounded labels; no
  job/source/URL/caller labels and no per-provider N+1.
- [ ] Assert high-water counters, SQL statement counts, p95 latency, busy count,
  and bounded metric cardinality under mixed load.

## Task 10: Prove Real Transport and Deployment Parity

**Beads:** `axon_rust-yow0c`, `axon_rust-j5gry`.

**Files:** transport integration tests, status DTO/handlers, `build.rs`,
deployment smoke scripts/reports.

Add:

```rust
pub struct BuildIdentity {
    pub version: String,
    pub git_sha: String,
    pub build_profile: String,
    pub schema_epoch: u32,
}
```

Expose it through the canonical status operation in local CLI, REST, and MCP.
Derive Git SHA in `build.rs`. Replace placeholder config snapshot ids with
content-derived ids for every tested operation.

- [ ] Compare stable request/result behavior, not declarations.
- [ ] Drive real CLI parsing, REST middleware/body limits, MCP schema and
  serialization, and panel auth; do not inject a caller after middleware.
- [ ] Exercise read, write, admin, local, and execute allow/deny matrices.
- [ ] Verify direct service and intended production ingress separately.
- [ ] Assert unauthenticated denial, Authorization/x-api-key forwarding, origin
  policy, and no trust inferred from proxy source IP.
- [ ] Assert commit, version, profile, schema epoch, and config snapshot through
  every ingress before accepting behavior results.
- [ ] Use `--local` for the local binary to prevent proxy false positives.
- [ ] Add retrieval parity with fixed vectors and adapter golden outputs.
- [ ] Add public external-crate tests and the Tier-5 crash/security matrix.
- [ ] Run cross-surface tests and `cargo xtask check-public-api`.

## Task 11: Generate All 17 Contract Families and Reconcile Prose

**Beads:** `axon_rust-5iglz`, `axon_rust-ugvcq`,
`axon_rust-vtdw0`, `axon_rust-beuzs`.

**Files:** `xtask/src/docs.rs`, `xtask/src/docs/*`,
`docs/reference/`, `docs/pipeline-unification/`, affected `CLAUDE.md` files.

- [x] Register exactly `cli`, `cli-help`, `openapi`, `mcp`, `api-dto`,
  `api-enums`, `errors`, `events`, `config`, `env`, `adapters`, `schema`,
  `memory`, `providers`, `presentation`, `schemas`, and `new-source`.
- [x] Give each family declared live inputs, renderer, output paths, and
  deterministic ordering.
- [ ] Make missing renderers/inputs, empty output, secret/path canaries, and byte
  drift fail with contracted exit codes.
- [ ] Render/write one family at a time and assert bounded high-water memory.
- [x] Support aggregate/per-family generate, check, print, JSON, and the
  CI-forbidden snapshot-update mode required by contract.
- [ ] Mark dated plans historical; rewrite live contracts in present tense.
- [x] Remove family-model naming/comment residue and the last `TODO(#298)`.
- [ ] Run docs generation/check, schema drift, layering, crate contracts, and
  public API checks.

## Task 12: Audit, Release, Deploy, and Close

**Beads:** every remaining child and epic `axon_rust-enbmu`.

- [ ] Confirm every grouped subfinding below has a merged implementation and
  focused test reference.
- [ ] Run `just precommit`, all docs/schema checks, layering, crate contracts,
  and public API checks.
- [ ] Exercise web, local, and git sources against live Qdrant, TEI, Chrome, and
  LLM providers.
- [ ] Run cold/warm query plus bulk-ingest contention and record scheduler wait,
  provider latency, end-to-end latency, build identity, and config snapshot.
- [ ] Update the closeout audit with one runner, one job id, one scheduler
  authority, publication epoch proof, transport parity, and all 17 docs.
- [ ] Assess CLI/schema/config/auth compatibility and choose the actual
  patch/minor/major bump; use `cargo xtask bump-version <level> --component cli`.
- [ ] Release through normal branch protection.
- [ ] Build separate host and Bookworm artifacts from merged `main`; back up,
  install atomically, restart with a bounded deadline, and rollback both sides
  on mixed identity or health.
- [ ] Verify direct and production-ingress CLI/REST/MCP identity and behavior.
- [ ] Verify `bd list --parent axon_rust-enbmu --status open --limit 0` is empty.
- [ ] Close the epic, run `bd dolt push`, push Git, and confirm a clean
  up-to-date `main`.

## Bead Closure Matrix

| Final task | Beads |
|---|---|
| 2 | `jc20j`; prerequisites for `a155h`, `5iglz` |
| 3 | `fts94`, `j801x` |
| 4 | `yygrl`, `dkuqo`, `gpuz9`, `igp0i`, `bfsp5`; prerequisite `upay4` |
| 5–7 | `drahp`, `nl7au`, `uzy27`, `2wq1r`, `upay4`, `a155h` |
| 8 | `9veac`, `cjxfw`, `4ygmz`, `0sgqz`, `a4t01`, `hf37r`; security `zb0k1` |
| 9 | `er3z7`; remainder `zb0k1` |
| 10 | `yow0c`, `j5gry` |
| 11 | `5iglz`, `ugvcq`, `vtdw0`, `beuzs` |

All 28 open implementation Beads have one final closure owner. Prerequisite
work never closes a Bead early.

## Grouped-Finding Acceptance

### `axon_rust-er3z7`

- [ ] P10 queued durable reservations and non-zero queue metrics.
- [ ] P11 bounded transactional document-status writes.
- [ ] P12 linear status/manifest merge.
- [ ] P13 hard caps before all corpus-sized materialization.
- [ ] P15 no poisoned in-memory reservation authority.
- [ ] P16 `embed=false` makes zero collection ensure/create calls.

### `axon_rust-hf37r`

- [ ] S-8 Local/Execute issuance and policy are explicit.
- [ ] S-10 panel context and environment response cannot broaden/leak.
- [ ] S-11 synchronous/detached snapshots are identical for equal trust.
- [ ] S-12 duplicate family detectors are removed.

### `axon_rust-a4t01`

- [ ] S-4 route tool policy comes from operator/caller policy.
- [ ] S-5 connect-time SSRF is enforced at real HTTP/browser boundaries.
- [ ] S-9 one admission decision owns local/tool authorization.

### `axon_rust-zb0k1`

- [x] P17 no duplicate TEI identity probes; cache is singleflight and keyed.
- [x] P18 one live Qdrant point-buffer limit.
- [x] P19 concurrency knob is consumed or deleted.
- [ ] P20 DB-stage concurrency preserves heartbeat/control capacity.
- [x] P21 no blocking canonicalization on async worker paths.
- [ ] S-13 redirects have hop, time, byte, scheme, proxy, and host bounds.
- [ ] S-14 browser requests use pinned connect-time enforcement.
- [ ] S-15 release builds cannot enable loopback bypass.
- [ ] S-16 watches never default to test-shaped auth.
- [ ] S-17 CodeSearch never force-stamps public visibility.
- [ ] S-18 dead tool-execution configuration is removed.

## Authoritative Failure-Mode Matrix

| Codepath | Failure | Required rescue | Required proof | Visibility |
|---|---|---|---|---|
| Scheduler authority | Two DBs grant the same provider capacity | Reject duplicate authority/proxy through one authority | Distinct-DB topology test | Startup/execution error and audit |
| Grant wait | Cross-process release has no in-memory wakeup | Durable predicate polling | Two-process lost-wakeup test | Bounded blocked event |
| Active lease | Expired live call overlaps a replacement | Renew, abort/join, then regrant; otherwise quarantine | Hung old future remains alive | Degraded metric/event |
| Permit drop | Rust `Drop` cannot await terminal SQL | Explicit async terminal methods plus reconciler | Dropped permit and killed process | Cleanup-debt event |
| Migration | Epoch-1 table lacks scheduler fields | Append-only migration and old-row disposition | Real epoch-1 fixture upgrade | Startup migration report |
| Recovery | Old worker resumes after new attempt | New attempt and stale fence rejection | Split-brain resume | Redacted rejection event |
| Publication | Qdrant data visible before ledger/graph commit | Epoch/watermark and ledger verification | Crash at every boundary plus direct read | No content before commit |
| Graph/artifact | External or shared state diverges from generation | Same finalization transaction/registry fence | Mid-finalization crash | Cleanup debt; no public partial |
| Acquire intent | Read-only path drifts from indexing prefix | Canonical runner with declared skips | Acquire-vs-Index parity | Same normalized content |
| Adapter reuse | Shared adapter leaks request state | DTO-owned state | Concurrent same-instance test | Structured job failure if violated |
| Auth snapshot | Retry/query/watch gains authority | Exact immutable clone | Multi-attempt/child tests | Auditable denial |
| Browser SSRF | Chrome resolves or connects outside policy | Controlled pinned-connect gateway | Rebind/subresource/WebSocket tests | Redacted denial |
| Local path | Rename/reparse race escapes root | Same-handle per-platform reads | Shipped-OS race suite | Redacted denial |
| Redaction | Detector/audit failure leaks input | Constant-shape emergency event | Panic/error/sink canaries | Opaque correlation only |
| Batching | Oversized encoded data exhausts memory | Actual-byte and item caps | Expansion/indivisible tests | Structured oversize error |
| Qdrant timeout | Partial upsert retries duplicate/diverge | Idempotency and reconciliation | Timeout after partial side effect | Retriable job event |
| TEI identity | Cold herd performs duplicate probes | Per-key singleflight cache | Concurrent cold/warm tests | Bounded latency metric |
| SQLite load | Workers starve heartbeat/control | Reserved pool capacity and numeric thresholds | Mixed-load p95/busy assertions | Bounded metrics |
| Transport | Direct smoke passes stale/broken ingress | Build/config identity through both paths | Direct plus production ingress | Explicit mismatch failure |

No critical row may merge with rescue absent, proof absent, and user-visible
silence.

## Explicit Deferrals

These do not weaken any deterministic acceptance criterion above:

- Publication-epoch sharding/per-source parallel finalization.
- Adaptive capacity autotuning.
- A separate network scheduler service, only while deployment enforces one
  SQLite authority or proxy topology.
- Streaming provider-trait redesign; hard materialization caps and sliced
  adapter calls land now.
- Rich security/scheduler dashboards; bounded redacted events and metrics land
  now.
- Long-running soak, criterion, and external penetration campaigns.
- Cryptographic signing of in-process reservation permits.
- Real-time token revocation; enqueue-time snapshot monotonicity lands now.
- Advanced chat slow-consumer buffering/deadline tuning after cancellation is
  correctly propagated.

## Plan Self-Review

- All 28 open implementation Beads appear in the closure matrix.
- The stale compose landing work is removed; v7.2 deployment is independent.
- The plan chooses one registry, stage model, queue algorithm, scheduler
  authority, provider gate, validation seam, SSRF seam, DB concurrency design,
  and Qdrant buffer.
- Existing provider traits remain object-safe and unchanged.
- Existing auth compatibility behavior is preserved; literal write checks guard
  mutating search/research.
- Progress events remain immediate; only generation-scoped content is fenced.
- Cross-process wakeup, active-lease uncertainty, split-brain recovery,
  epoch-1 migration, and deployed-build identity are implementable and tested.
- Every critical failure mode has both a rescue and an observable test.
- No implementation instruction contains an unresolved “choose one,” “wire or
  remove,” “if retained,” or “where practical” branch.
