# Pipeline Unification Completion Implementation Plan

> **For implementing agents:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Use `superpowers:subagent-driven-development` only when Jacob explicitly requests delegated agent work. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining 28 children of `axon_rust-enbmu`, prove that every source family uses one execution pipeline, enforce global provider fairness, close the remaining security gaps, and deploy the resulting Axon release.

**Architecture:** Work proceeds through explicit, dependency-ordered PR waves, each
based on the latest merged `origin/main`. Behavioral tests, executable
documentation drift checks, and live architectural guardrails land before the
source-runner collapse; the canonical runner then becomes family-blind above
`SourceAdapter`; provider capacity moves behind one SQLite-authoritative,
cross-process scheduler; security and transport parity are proved at real
execution boundaries rather than by declaration-only tests.

**Tech Stack:** Rust 2024, Tokio, Axum, rmcp, SQLite, Qdrant, TEI, `cargo xtask`, Beads (`bd`), GitHub Actions, Docker Compose, Incus/systemd.

## Global Constraints

- `CLAUDE.md` is the source of truth; do not edit `AGENTS.md` or `GEMINI.md` directly.
- Use `.worktrees/<branch>` under the repository root for implementation worktrees.
- Create or claim a bead before every non-trivial code change; record evidence
  before push, but close it only after its PR merges and the merged commit is
  verified.
- Preserve `marketplace-no-mcp` as a protected long-lived branch; never merge or delete it as cleanup.
- Never use `foo/mod.rs`; use `foo.rs` plus `foo/*.rs`.
- Tests live in sibling `*_tests.rs` files through `#[path = "..."]`.
- Keep changed production `.rs` files at or below 500 lines and functions below the monolith hard limit.
- Keep transports thin: CLI, MCP, and REST call `axon-services`; they do not own orchestration or call provider internals.
- One `SourceRequest` must retain one job id through route, acquire, prepare, embed, publish, graph, and cleanup.
- Treat `docs/pipeline-unification/` as the target contract. When current code, a dated snapshot, or this plan conflicts with the packet, the target contract wins and the discrepancy must be repaired explicitly.
- Keep the public `SourceAdapter` trait exactly at the contracted boundary: `name`, `version`, `capabilities`, `discover`, `acquire`, and `normalize`. Family-specific preparation and conditional-fetch behavior stay behind those methods or in declarative `SourceAdapterSpec` data; do not retain public `materialize` or `reuse_policy` escape hatches.
- Use the existing `SourceRequest.execution.priority: JobPriority`; do not create a competing transport priority field or enum. Map foreground source work to `High`, interactive query/ask/retrieve to `Interactive`, refresh to `Background`, and maintenance to `Maintenance`.
- Provider reservations and fairness are owned by the unified job scheduler. Providers require a granted reservation but do not own cross-job queues or scheduling policy.
- Redaction failures fail closed before vector, artifact, event, graph, or memory writes.
- Run the smallest targeted test while iterating; run the full repository gate
  once per code PR before push.
- Before the generator-engine wave lands, use existing schema checks and
  hand-update only affected contract text. After it lands, every implementation
  PR must regenerate and check only the documentation families affected by its
  changed surface.
- Do not deploy from a dirty checkout or a pre-merge commit.
- Never merge an ignored/failing contract test. A red characterization commit
  may lead a stacked branch, but the PR that reaches `main` must include the
  corresponding green implementation.
- Do not use job, reservation, source, URL, or caller identifiers as metric
  labels. Provider metrics are bounded to provider kind, bounded provider
  instance id, priority, and job kind.

---

## Confirmed Starting State

- PR `#466` merged as `cc08c978b059f435719833ea6e7e5a9c352362c4`; its tree is byte-identical to the clean `claude/pipeline-unification-review-052d57` worktree.
- The primary checkout is three commits behind `origin/main`.
- The only tracked dirty file before this plan was created is `docker-compose.llama.yaml` (`91` additions, `12` deletions).
- `docker-compose.llama.yaml` has no overlap with the 135 files changed by PR `#466`; it belongs to the separate llama.cpp/TEI tuning session.
- `.full-review/` is ignored review evidence, not uncommitted production work.
- Epic `axon_rust-enbmu` has 28 closed and 28 open children.
- Host and Incus Axon binaries are still `7.1.5`; `origin/main` is `v7.2.0`.

## Contract Authority and Alignment Matrix

The implementation must cite the affected contract sections in each PR body and
record any current-code divergence that the PR removes. A task is not complete
merely because current tests pass if the target contract still says otherwise.

| Concern | Canonical contract | Required invariant |
|---|---|---|
| Pipeline ownership | `README.md`, `foundation/source-pipeline.md` | `axon-services` owns one family-blind execution path; transports never reroute. |
| Adapter boundary | `foundation/types/trait-contract.md`, `sources/new-source-contract.md` | `SourceAdapter` exposes only the six contracted methods and emits normalized `SourceDocument` results. |
| DTOs and enums | `foundation/types/dto-contract.md`, `foundation/types/enum-contract.md` | Reuse `ExecutionPolicy.priority` and canonical `JobPriority::{Interactive, High, Normal, Background, Maintenance}`. |
| Jobs and capacity | `runtime/job-contract.md`, `runtime/provider-contract.md` | The job scheduler owns reservation state and fairness; provider calls require granted reservations. |
| Auth and security | `runtime/auth-contract.md`, `runtime/security-contract.md`, `runtime/redaction-contract.md` | Snapshots never escalate, SSRF/local-path policy is injected and testable, and redaction fails closed. |
| Behavioral proof | `delivery/testing-contract.md` | Fake-boundary tests precede live smokes; family and transport parity compare execution behavior. |
| Generated references | `delivery/documentation-contract.md`, `delivery/docs-generator-contract.md` | All 17 generator families use the contracted layout, interfaces, flags, and drift semantics. |
| Delivery order | `delivery/dependency-order-map.md` | Fake providers and route/adapter boundaries land before reservations and source ports; broad live smokes come last. |

## Engineering Review Decisions

These decisions incorporate the Lavra architecture, simplicity, security, and
performance reviews. They are normative implementation constraints.

### Delivery topology

- The llama/TEI compose change and deployment of already-merged v7.2.0 are an
  independent operational lane. Neither blocks Tasks 2-4.
- Every PR wave uses a new `.worktrees/<branch>` checkout from the latest merged
  `origin/main`; do not accumulate Tasks 2-10 on one branch.
- Large tasks are split into boundary-sized PRs. A bead may receive prerequisite
  work in multiple waves, but it has exactly one closure owner and closes only
  after that owner PR merges.

### Canonical stage and visibility model

Before porting any family, encode one executable stage plan from
`foundation/source-pipeline.md`. Intent-specific operations may declare
contracted skips, but may not reorder stages. Resolve the graph-ordering
contradiction in favor of the target contract: graph candidates are prepared
before publication, and generation commit is the single visibility barrier.

- Vector points, graph evidence, document status, and generated artifacts carry
  the pending generation fence and remain invisible to public reads before
  commit.
- A required-stage failure prevents generation commit and records cleanup debt
  for any external writes that cannot be transactionally rolled back.
- Optional failures may yield `completed_degraded` only when the error policy
  explicitly permits publication.
- Recovery reuses the same `job_id`, creates/continues the contracted attempt,
  and idempotently reconciles every mutating stage.
- Crash-injection tests run after each mutating stage and prove that no vector,
  graph, status, artifact, or event projection exposes a partial generation.

### Global reservation kernel

`axon-jobs` owns the only production reservation kernel. SQLite
`provider_reservations` rows and atomic transactions are authoritative across
server and CLI worker processes. `axon-observe` is read-only projection and
metrics; any existing `axon-observe::ProviderReservationManager` authority is
deleted or reduced to a compatibility facade in the kernel PR. In-process
oneshots/`Notify` may reduce wakeup latency but never decide capacity.

- Grants atomically validate owning job, attempt, stage, provider kind/instance,
  server-computed units, effective priority, cooldown, queue deadline, lease,
  and fencing generation.
- The typed RAII permit owns exactly one `granted -> active -> terminal`
  transition and rejects forged, replayed, cross-job, cross-stage,
  cross-provider, duplicate-activation, and duplicate-release use.
- Real providers have no missing-proof runtime bypass. Fake providers use a
  compile-time fake proof implementation.
- Queue deadline, granted-start deadline, and active lease expiry are distinct.
  Queue length, queued units, per-job entries, and ungrantable requests are
  bounded; `units > capacity` fails immediately.
- Interactive reserve is combined with weighted aging or bounded lane quotas.
  Long-running work releases and reacquires between batches so every lane has a
  tested maximum wait bound.
- Worker admission does not mark a job `running` or hold a general worker permit
  while it waits for a narrower source/provider slot. Capacity wait is
  `blocked`, with accurate heartbeat and starvation metrics.

### Security admission and publication

- Search is read-only for `axon:read` callers. Auto-indexing occurs only when the
  authenticated caller also has write permission; otherwise results are
  returned without indexing. No trusted-system/Admin/Local snapshot is minted.
- Effective priority is server-derived: query/ask/retrieve may be
  `Interactive`; trusted-local foreground source work may be `High`; detached
  source/watch work is `Background`; `Maintenance` is admin/system only.
  Persist requested and effective priority and audit downgrades.
- Adapter output is untrusted. Route safety class, caller snapshot, visibility
  ceiling, local/tool permission, reservation proof, and redaction are checked
  before the first side effect and again at each public publication boundary.
- Security denials emit structured, redacted audit events with job/source/caller
  identity and policy version. URLs, query strings, headers, auth snapshots,
  reservation proofs, provider errors, and local paths are scrubbed from logs.

### Reviewable PR waves

| Wave | Mergeable units |
|---|---|
| O | Independent llama preservation and v7.2 deployment/rollback; does not block code waves. |
| A | Live layering gate; then boundary-violation fixes. |
| B | Docs generator engine/drift core; then `api-dto`, `api-enums`, `adapters`, `events`, `providers`, `schema`. |
| C | Shared observation/conformance harness; adapter metadata; router/classifier; site discovery. |
| D | SQLite reservation kernel/recovery; embedding/query lane; vector-write lane. |
| E | Canonical stage/visibility foundation; web port; local port; remaining adapter ports and deletion. |
| F | Auth/panel/CodeSearch; SSRF/redirects; local paths; redaction/visibility. |
| G | Bounded streaming/SQLite/performance reliability; remaining provider capacity classes. |
| H | Transport parity; adapter goldens; Tier-5 recovery/security cases. |
| I | Remaining documentation families, historical reconciliation, final audit/release/deploy. |

## Delivery Graph

```text
independent ops lane: llama + v7.2 deploy/rollback

live layering gates ---> docs drift engine ---> behavior harness
                                                |
                                                v
                                 adapter/routing normalization
                                                |
                                                v
                         durable scheduler + embedding/vector proof
                                                |
                                                v
                           stage/visibility model + runner ports
                                      |                 |
                                      v                 v
                              security waves    bounded reliability
                                      \                 /
                                       v               v
                               real transport + fixture proof
                                                |
                                                v
                              remaining docs + final audit/release
```

## Task 1: Run the Independent Operational Baseline Lane

**Beads:**
- Existing related bead: `axon_rust-ana1h`
- Create a separate task for landing the compose diff; do not attach it to `axon_rust-enbmu`.

**Files:**
- Modify and commit separately: `docker-compose.llama.yaml`
- Read only: `~/.axon/.env`, Incus `axon:/mnt/axon-data/config.toml`

**Interfaces:**
- Consumes: the current 103-line dirty compose diff.
- Produces: a dedicated llama/TEI PR plus an atomic, rollback-tested v7.2
  deployment. This lane may run before or alongside Tasks 2-4 and does not block
  their implementation.

- [ ] **Step 1: Record the exact dirty surface**

```bash
git status --short --branch
git diff --check
git diff --stat
git diff -- docker-compose.llama.yaml
```

Expected: only `docker-compose.llama.yaml` is modified before this plan file is considered.

- [ ] **Step 2: Create and claim a Beads task**

```bash
bd create \
  --title="Land llama.cpp and TEI coexistence compose tuning" \
  --description="Preserve and review the docker-compose.llama.yaml changes produced by Claude session e552af6f-b0a8-4fbd-9857-6b6dd4cdf924. This is unrelated to #298 and must land separately before the primary checkout is cleaned." \
  --type=task \
  --priority=2
bd update <created-id> --claim
```

- [ ] **Step 3: Copy only the compose diff into a clean worktree**

```bash
git fetch origin
git worktree add .worktrees/llama-tei-compose-tuning \
  -b fix/llama-tei-compose-tuning origin/main
git diff --binary -- docker-compose.llama.yaml |
  git -C .worktrees/llama-tei-compose-tuning apply -
git -C .worktrees/llama-tei-compose-tuning diff --check
```

The primary checkout and the new plan file remain untouched. Verify the copied
diff is byte-equivalent before committing.

- [ ] **Step 4: Validate and commit the standalone compose file**

```bash
cd .worktrees/llama-tei-compose-tuning
docker compose --env-file /home/jmagar/.axon/.env \
  -f docker-compose.llama.yaml config --quiet
git add docker-compose.llama.yaml
git commit -m "fix(compose): preserve llama and TEI GPU coexistence tuning"
git diff HEAD^ --check
```

Expected: Compose renders successfully without changing the running service.

- [ ] **Step 5: Push the isolated branch and open its PR**

```bash
git push -u origin fix/llama-tei-compose-tuning
gh pr create \
  --base main \
  --title "fix(compose): preserve llama and TEI GPU coexistence tuning" \
  --body "Separates the completed llama.cpp/TEI tuning from the #298 pipeline-unification program."
```

- [ ] **Step 6: Establish the v7.2 deployment rollback set**

From a clean worktree at exact merged commit `cc08c978b...`, capture:

```bash
git rev-parse HEAD
sha256sum Cargo.lock ~/.axon/config.toml ~/.axon/.env
incus exec axon -- sha256sum \
  /usr/local/bin/axon /mnt/axon-data/config.toml
```

Back up the exact existing host/container binaries, `~/.axon/config.toml`,
`~/.axon/.env`, the container config, SQLite database, artifact tree, and a
Qdrant snapshot. Record recovery paths and checksums in the deployment bead.
Do not expose secret file contents in logs or the session report.

- [ ] **Step 7: Build and validate both artifacts before replacement**

```bash
cargo build --release --locked --bin axon
./target/release/axon --version
ldd ./target/release/axon
docker run --rm \
  -v "$PWD:/w" \
  -v /home/jmagar/.cargo/registry:/usr/local/cargo/registry \
  -w /w \
  -e CARGO_TARGET_DIR=/w/target-bookworm \
  rust:1-bookworm cargo build --release --locked --bin axon
file target-bookworm/release/axon
sha256sum target/release/axon target-bookworm/release/axon
```

Verify both binaries report `7.2.0`, embed the expected commit/build identity,
and that the Bookworm binary resolves inside the container before touching the
installed paths.

- [ ] **Step 8: Install atomically and enforce a health deadline**

Copy each binary to a temporary sibling, verify checksum and version in place,
then rename atomically over the target. Apply the validated config migration
through a temporary file plus atomic rename. Restart `axon-native.service` and
require service-active, doctor, local `--local` smoke, and deployed REST/MCP
build-identity checks within a bounded timeout.

If any host or container check fails, stop the new service, atomically restore
the old binary and config, restore data only when a migration changed it,
restart, and prove the previous version healthy. A mixed host/container version
is a failed deployment.

## Per-Wave Merge Protocol

Apply this protocol to every implementation unit below:

1. `git fetch origin`, create/reuse a dedicated `.worktrees/<wave-unit>` from
   the latest merged `origin/main`, and claim only the affected bead(s).
2. Run the stated baseline/targeted tests and record pre-existing failures.
3. Implement one boundary-sized unit, regenerate only affected docs families,
   run its full PR gate, commit, push, and open a focused PR.
4. Wait for required checks and merge through normal branch protection.
5. Verify the merged commit on `origin/main`; only then close beads whose full
   acceptance checklist is satisfied.
6. Create the next dependent worktree from that merged `origin/main`. Do not
   stack unrelated waves on a long-lived implementation branch.

## Task 2: Make the Layering and Crate-Contract Gates Observe the Live Workspace

**Beads:** `axon_rust-jc20j`, first half of `axon_rust-a155h`

**Files:**
- Modify: `xtask/src/checks/layering.rs`
- Modify: `xtask/src/checks/layering_tests.rs`
- Modify: `xtask/src/checks/crate_contracts_spec.rs`
- Modify: `xtask/src/checks/crate_contracts_spec_cont.rs`
- Modify: `crates/axon-core/src/paths.rs` or a focused sibling path-helper module
- Modify: `crates/axon-cli/src/commands/screenshot/util.rs`
- Modify: `crates/axon-services/src/scrape.rs`
- Modify: `crates/axon-web/src/server/handlers/chat_stream.rs`
- Modify: the existing `axon-services` chat/synthesis facade
- Test: sidecars adjacent to each moved service/helper

**Interfaces:**
- Consumes: the current 23-crate dependency graph and `axon-services` facade.
- Produces: a gate that rejects transport-to-domain/provider internals and a clean live dependency graph.

- [ ] **Step 1: Write gate regressions for the three known live violations**

Add tests in `xtask/src/checks/layering_tests.rs` whose fixtures contain:

```rust
use axon_adapters::web_engine::screenshot::url_to_screenshot_filename;
use axon_llm::runtime::complete_streaming;
use axon_adapters::web_engine::scrape::scrape_to_result;
```

Each fixture must produce one finding naming the importing layer and forbidden target.

- [ ] **Step 2: Prove the tests fail against the current allowlist**

```bash
cargo test -p xtask layering -- --nocapture
```

Expected: the new assertions fail because the live prefixes are not enforced.

- [ ] **Step 3: Replace deleted-crate prefixes and deleted-file allowlist rows**

Update `xtask/src/checks/layering.rs` so the forbidden graph names current crates and module roots. Remove every allowlist entry whose file no longer exists. Keep exceptions path-specific and document the bead that removes each temporary exception.

- [ ] **Step 4: Move the pure screenshot filename helper to `axon-core`**

Expose the helper from `axon-core`; update the CLI import; remove the CLI production dependency on `axon-adapters` if no other production import remains.

- [ ] **Step 5: Put streaming completion behind `axon-services`**

Add a typed service method that accepts the existing completion request and delta callback. Make `chat_stream.rs` call that facade and remove its direct provider construction.

- [ ] **Step 6: Separate read-only acquisition from canonical `scrape`**

Name and type the read-only acquisition projection separately and limit it to
summarize/diff-style callers. It constructs a page `SourcePlan`, invokes the web
adapter, and returns normalized content without a ledger generation or vectors.
The retained CLI `axon scrape` continues to submit a canonical page
`SourceRequest`, uses one durable source job, and honors `embed=true`.

Add one end-to-end test for each behavior so the read-only facade cannot
silently capture the CLI command.

- [ ] **Step 6a: Bound streaming completion backpressure**

The `axon-services` streaming facade must propagate cancellation and apply
bounded buffering plus idle/total deadlines. A stalled REST client must not hold
an LLM/provider call indefinitely. Add slow-consumer and disconnect tests.

- [ ] **Step 7: Run focused gates**

```bash
cargo test -p xtask layering -- --nocapture
cargo xtask check-layering
cargo xtask check-crate-contracts
cargo test -p axon-cli screenshot --no-fail-fast
cargo test -p axon-web chat_stream --no-fail-fast
cargo test -p axon-services scrape --no-fail-fast
```

- [ ] **Step 8: Commit and close only the boundary bead**

```bash
git add xtask crates/axon-core crates/axon-cli crates/axon-services crates/axon-web
git commit -m "refactor(architecture): enforce live crate boundaries"
```

After merge, close `axon_rust-jc20j`. Keep `axon_rust-a155h` open until Task 6
removes the temporary source-runner contract exceptions.

## Task 2B: Land Executable Documentation Drift Guardrails

**Beads:** prerequisite portion of `axon_rust-5iglz`

**PR units:**

1. Generator engine, source-input manifests, deterministic rendering, real
   byte comparison, non-empty validation, and negative drift tests.
2. Critical runtime families: `api-dto`, `api-enums`, `adapters`, `events`,
   `providers`, and `schema`.

**Interfaces:**
- Consumes: live runtime registries and schema sources.
- Produces: the contracted `DocsFamilyGenerator`, `DocsArtifactSet`, and
  `GeneratedDocArtifact` engine before runtime contracts begin changing.

- [ ] **Step 1: Build the contracted engine layout**

Create the shared `xtask/src/docs/{mod,args,artifact,check,markdown,manifest,render,examples}.rs`
modules and the family registry. Check mode renders in memory and never writes.
Generation writes only declared outputs.

- [ ] **Step 2: Prove drift detection is executable**

Add tests that mutate a registry fixture and require `--check` to fail, omit a
declared input and require exit code `3`, reject empty/header-only output, and
prove deterministic ordering across randomized map insertion.

- [ ] **Step 3: Add secret/path canaries**

Seed fake auth headers, endpoint userinfo, secret values, and absolute local
paths into test inputs. Generated docs, examples, manifests, diagnostics, and
JSON reports must not contain the canaries.

- [ ] **Step 4: Implement the six critical families**

Add the `api-dto`, `api-enums`, `adapters`, `events`, `providers`, and `schema`
family generators. Their input manifests must enumerate every source registry
so omitting an input makes `--check` fail.

- [ ] **Step 5: Run and merge the guardrail**

```bash
cargo test -p xtask docs -- --nocapture
cargo xtask docs generate
cargo xtask docs generate --check
cargo xtask schemas all --check
```

After this wave merges, Tasks 4-9 must regenerate/check the affected critical
families in their own PRs. Keep `axon_rust-5iglz` open until Task 10 supplies all
17 families.

## Task 3: Land Behavioral Harnesses Before Changing the Runner

**Beads:** `axon_rust-fts94`, `axon_rust-j801x`

**Files:**
- Modify: `crates/axon-services/src/test_support.rs`
- Create: `crates/axon-services/src/source_pipeline_parity_tests.rs`
- Modify: `crates/axon-services/src/lib.rs` or the source module root for the sidecar declaration
- Modify: `crates/axon-authz/src/lib_tests.rs`
- Modify: `crates/axon-jobs/src/workers/unified_tests.rs`
- Modify: `crates/axon-services/src/local_source_tests.rs`

**Interfaces:**
- Consumes: `dispatch_kind`, fake ledger/vector/embedding providers, and `SourceExecutionContext`.
- Produces: one reusable `PipelineObservation` assembled from public results,
  job/events, and existing fake-boundary call logs. Task 9 reuses this same
  harness; it does not invent a second parity model.

- [ ] **Step 1: Extend the fake runtime with traceable counters**

Add a bounded observation collector for:

```rust
pub struct PipelineObservation {
    pub result_job_id: JobId,
    pub observed_job_ids: HashSet<JobId>,
    pub reservation_priorities: Vec<JobPriority>,
    pub prepared_chunk_count: usize,
    pub document_status_writes: usize,
    pub ensure_collection_calls: usize,
    pub phases: Vec<PipelinePhase>,
}
```

The fake ledger, vector store, authz policy, DNS resolver, redactor, vector
provider, and embedding provider append bounded observations rather than
exposing implementation internals. The fake and production compositions use
the same composition constructor. Include allow/deny/visibility, resolver
failure/private-address, and redactor-failure modes; do not use a global
loopback or auth bypass.

- [ ] **Step 2: Add the web/local/git case matrix**

Create web/local/git cases using one identical bounded two-document corpus and
parameterize execution mode. Assertions:

```rust
assert!(observation
    .observed_job_ids
    .iter()
    .all(|id| id == &observation.result_job_id));
assert_eq!(detached.reservation_priorities, vec![JobPriority::Background]);
assert_eq!(foreground.reservation_priorities, vec![JobPriority::High]);
assert_eq!(observation.prepared_chunk_count, expected_chunks);
assert_eq!(observation.document_status_writes, 2);
assert_eq!(observation.ensure_collection_calls, 1);
assert_eq!(observation.phases, expected_pipeline_phases);
```

- [ ] **Step 3: Run the matrix and retain the expected red state**

```bash
cargo test -p axon-services source_pipeline_parity -- --nocapture
```

Expected before Task 6: failures expose the surviving web/local/non-web
divergences. Keep the red characterization commit first in the affected stacked
branch, but merge it only together with the code that makes it green. Never add
`#[ignore]` or weaken the assertions to land a known-red contract.

- [ ] **Step 4: Correct tests that pin defective behavior**

Replace:

- `axon_read_scope_satisfies_write_routes` with explicit read-does-not-satisfy-write coverage.
- the source-only semaphore assertion with a claim-loop progress test covering `Source` plus `Extract`.
- local fixtures defaulting `route: None` and `embedding_reservations: None` with routed, reserved defaults.

- [ ] **Step 5: Run focused tests**

```bash
cargo test -p axon-authz --lib --no-fail-fast
cargo test -p axon-jobs unified --no-fail-fast
cargo test -p axon-services local_source --no-fail-fast
```

- [ ] **Step 6: Commit**

```bash
git add crates/axon-services crates/axon-authz crates/axon-jobs
git commit -m "test(pipeline): add cross-family execution invariants"
```

After merge, close `axon_rust-fts94` only when all three family cases execute.
Close `axon_rust-j801x` only when all three defective tests have been replaced.

## Task 4: Normalize Adapter Output, Routing, and Discovery

**Beads:** `axon_rust-yygrl`, `axon_rust-dkuqo`, `axon_rust-gpuz9`, `axon_rust-igp0i`, `axon_rust-bfsp5`, adapter-owned portion of `axon_rust-upay4`

**Files:**
- Modify: `crates/axon-adapters/src/registry_sources.rs`
- Modify: `crates/axon-adapters/src/sessions.rs`
- Modify: their existing sidecar tests
- Delete family branches from: `crates/axon-services/src/source/non_web/metadata.rs`
- Modify: `crates/axon-services/src/query/code_search_refresh.rs`
- Modify: `crates/axon-services/src/source/dispatch_kind.rs`
- Modify: `crates/axon-route` option validation files and sidecars
- Modify: `crates/axon-adapters/src/web/site_discovery.rs`
- Modify: `crates/axon-adapters/src/web/site_discovery_tests.rs`
- Modify: `crates/axon-adapters/src/adapter.rs`

**Interfaces:**
- Consumes: `SourceRequest`, `RoutePlan`, `SourceAdapter`, and injected `FetchProvider`.
- Produces: adapter-normalized metadata, a single router classification, a
  validated production adapter composition root, and provider-accounted site
  discovery.

**PR units:** adapter metadata; router/classifier plus CodeSearch safety; site
discovery. Each unit merges before the next starts.

- [ ] **Step 1: Add adapter-output tests**

Assert that registry normalization emits `package_*` fields and `source_family = "package"` directly, and that session normalization emits only its allowlisted metadata fields.

- [ ] **Step 2: Delete shared-pipeline family rewrites**

Remove `sanitize_documents` and session chunk sanitization from `source/non_web`. The shared runner must accept adapter output without inspecting `SourceKind`.

- [ ] **Step 3: Route code-search refresh through `index_source`**

In the same PR, first remove CodeSearch’s forced `visibility="public"` stamp,
enforce the caller’s local/visibility policy, and add a zero-public-write
negative test. Then build a canonical local `SourceRequest` with
`LocalSourceSelectionPolicy::CodeSearch` represented in validated route options.
Remove the random private job id and `route: None`. Do not reactivate the route
in an intermediate commit that can publish local code as public.

- [ ] **Step 4: Make option validation one pure source of truth**

Extract `validate_options_against_spec(request, options, SourceAdapterSpec)` and
call it while constructing `RoutePlan`. `ValidatedOptions` is immutable/opaque.
If the trait contract retains `validate_options`, it calls that same pure
function with the exact compiled spec/policy identity; it never reconstructs
rules from a lossy capability summary. If the target contract is amended to
drop redundant revalidation, remove the trait method and generator references
in the same PR.

- [ ] **Step 5: Remove the duplicate string family classifier**

Use `axon-route`’s resolved `SourceKind` for authorization and dispatch classification. Assert that every `source_family_matrix()` entry resolves identically through route, auth, and adapter selection.

- [ ] **Step 6: Inject `FetchProvider` into site discovery**

Change `manifest_items` and downstream sitemap/robots/llms.txt discovery
functions to accept the adapter’s injected fetch provider. Delete the private
reqwest client and retry loop from discovery. Bound discovered URLs, concurrent
fetches, response bytes, retries, and total deadline. Apply reservations and
cooldown at the actual fetch-call granularity so a 512-item sitemap cannot
amplify 429 retries outside scheduler accounting.

- [ ] **Step 6a: Add the production adapter composition root**

Construct one registry of `SourceAdapterSpec` plus an internal constructor for
a fresh `Arc<dyn SourceAdapter>` per execution. Fail startup on duplicate
name/version, missing implementation, unsupported kind, or spec/capability
mismatch. Require exact coverage of `source_family_matrix()`; router output must
resolve to exactly one registered adapter with no fallback.

- [ ] **Step 7: Run focused tests**

```bash
cargo test -p axon-adapters registry_sources --no-fail-fast
cargo test -p axon-adapters sessions --no-fail-fast
cargo test -p axon-adapters site_discovery --no-fail-fast
cargo test -p axon-route --lib --no-fail-fast
cargo test -p axon-services code_search_refresh --no-fail-fast
```

- [ ] **Step 8: Commit**

```bash
git add crates/axon-adapters crates/axon-route crates/axon-services
git commit -m "refactor(source): make routing and adapter output canonical"
```

## Task 5: Implement Real Global Provider Fairness

**Beads:** `axon_rust-nl7au`, `axon_rust-uzy27`, provider-reservation portion of `axon_rust-er3z7`

**Interfaces:**
- Consumes: durable job/attempt/stage ownership, provider capacity registry,
  cancellation, cooldown, and server-derived `JobPriority`.
- Produces: one SQLite-authoritative reservation kernel whose
  `requested -> queued -> granted -> active -> released/canceled/expired/failed`
  lifecycle remains correct across processes and restarts.

**PR units:**

1. SQLite kernel, migration away from `axon-observe` ownership, recovery, and
   worker admission.
2. Durable interactive query/ask/retrieve ownership plus embedding integration.
3. Vector-read/write integration. Remaining provider classes wait until the
   canonical runner exists and land in Task 8.

- [ ] **Step 1: Write durable cross-process kernel tests**

Use two independently constructed stores/schedulers against one temporary
SQLite database. Prove aggregate granted/active units never exceed capacity,
including concurrent grants. Cover restart recovery for queued, granted, and
active rows; fencing-token rejection; cancellation/grant, expiry/grant,
cooldown/release, lost-wakeup, thundering-herd, and duplicate-release races.

- [ ] **Step 2: Make SQLite grant transactions authoritative**

Create the one `axon-jobs` reservation registry keyed by stable provider
instance id plus `ProviderKind`. The grant transaction validates job, attempt,
stage, cooldown, effective priority, queue deadline, requested units, current
grants, and lease/fencing generation before atomically changing
`queued -> granted`. It must not hold a database transaction or connection
across a provider await.

Use per-waiter oneshots where practical. Any `Notify` optimization waits in a
predicate loop against durable state so a release between check and sleep cannot
strand a waiter, and one free unit does not wake the entire queue.

`axon-observe` reads durable state and exports bounded metrics only. Delete or
reduce the current `axon-observe::ProviderReservationManager` and
`axon-embedding` compatibility manager in the same PR so there is no second
capacity truth.

- [ ] **Step 3: Define liveness and overload policy**

Implement interactive reserve plus weighted aging/bounded lane quotas. A bulk
job yields between batches and cannot repeatedly jump older waiters. Reject
`units > capacity`; bound queue entries/units globally and per job; distinguish
queue deadline, granted-start deadline, and active lease expiry; return
structured overload/timeout errors. Cooldown cancels queued background work,
preserves safe active cleanup, and health probes may end cooldown.

Sustained-arrival tests assert a maximum wait bound for `Interactive`, `High`,
`Normal`, `Background`, and `Maintenance`, not merely one preferred grant.

- [ ] **Step 4: Fix worker admission semantics**

Do not claim a job as `running` or hold a general worker permit while it waits
for source/provider admission. Use `blocked` plus an explicit reason and
heartbeat while waiting, then acquire admission and transition to `running`
atomically. Test more source jobs than both limits followed by interactive and
non-source jobs; neither may starve behind source semaphore waiters.

- [ ] **Step 5: Add a bound, replay-proof RAII permit**

The permit atomically binds reservation id, job/attempt/stage, provider
kind/instance, effective priority, server-computed units, lease, and fencing
generation. It owns exactly one `granted -> active -> terminal` lifecycle.
Test forged ids, cross-job/stage/provider reuse, duplicate activation/release,
expiry during a call, and cancellation races. Real providers cannot compile a
call without a real proof; fake providers receive a fake proof type, not a
runtime bypass.

- [ ] **Step 6: Give interactive operations durable ownership**

Query, ask, and retrieve create/reuse canonical job, attempt, and stage
identities for provider-backed work. Persist requested and effective priority
in the immutable snapshot. Server policy derives the effective value; MCP/REST
callers cannot self-promote. Foreground trusted-local source work maps to
`High`, detached source/watch to `Background`, and maintenance to
admin/system-only `Maintenance`.

- [ ] **Step 7: Integrate embedding and vector capacity**

The service obtains a grant before invoking the real embedding/vector provider
and passes the typed permit. Providers activate/release the permit but do not
schedule. Query/ask/retrieve use `Interactive`. Embedding and vector-write use
separate capacity classes and declared units, batch limits, and deadlines.

- [ ] **Step 8: Run focused and concurrency tests**

```bash
cargo test -p axon-jobs reservation --no-fail-fast
cargo test -p axon-jobs worker_admission --no-fail-fast
cargo test -p axon-observe reservation --no-fail-fast
cargo test -p axon-embedding reservation --no-fail-fast
cargo test -p axon-retrieval --lib --no-fail-fast
cargo test -p axon-services provider --no-fail-fast
```

Merge each PR unit separately and close `axon_rust-nl7au`/`axon_rust-uzy27`
only after all three units and their merged-commit tests are verified.

## Task 6: Collapse Web, Local, and Non-Web onto One Canonical Runner

**Beads:** `axon_rust-drahp`, `axon_rust-2wq1r`, remaining `axon_rust-upay4`, remaining `axon_rust-a155h`

**Files:**
- Promote/refactor: `crates/axon-services/src/source/non_web.rs`
- Rename its focused children to family-neutral names under `crates/axon-services/src/source/`
- Modify: `crates/axon-services/src/source/dispatch_kind.rs`
- Modify: `crates/axon-adapters/src/adapter.rs`
- Delete: `crates/axon-services/src/web_source.rs`
- Delete: `crates/axon-services/src/web_source/`
- Delete: `crates/axon-services/src/local_source.rs`
- Delete: `crates/axon-services/src/local_source/`
- Delete: `crates/axon-services/src/local_source_vectorize.rs`
- Delete obsolete web/local sidecars after moving their assertions to the parity harness
- Modify: `xtask/src/checks/crate_contracts_spec.rs`
- Modify: `docs/pipeline-unification/foundation/crate-structure.md`
- Modify: `crates/axon-services/src/CLAUDE.md`

**Interfaces:**
- Consumes: the exact six-method `SourceAdapter` contract, declarative `SourceAdapterSpec`, one `SourceExecutionContext`, and the Task 3 parity harness.
- Produces: one family-neutral source executor, one executable stage plan, and
  one generation visibility/recovery model.

**PR units:** stage/visibility foundation; web port; local port; remaining
adapter ports plus deletion. Each port is green and mergeable independently,
then the final unit deletes all parallel executors.

- [ ] **Step 1: Encode the canonical stage and commit model**

Represent the target contract’s stage order and intent-specific allowed skips as
data shared by execution and tests. Graph preparation occurs before publish.
Generation commit is the sole public visibility barrier for vectors, graph
evidence, document status, and generated artifacts. Required failures prevent
commit; optional degradation follows explicit error policy; external leftovers
create cleanup debt.

Add crash injection after every mutating stage. Recovery under the same
`job_id`/attempt contract must be idempotent and prove no partial generation is
vector-searchable, graph-visible, artifact-visible, or reported published.

- [ ] **Step 2: Rename the canonical input and entry point**

Replace `NonWebPipelineInput` with `SourcePipelineInput` and `index_materialized_source` with a family-neutral `execute_source_pipeline`. The input retains adapter, plan, collection, owner, auth snapshot, and execution context.

- [ ] **Step 3: Remove transitional methods and hidden adapter state**

Delete public `SourceAdapter::materialize` and `SourceAdapter::reuse_policy`.
Fold family-specific preparation behind `discover`, `acquire`, and `normalize`,
so every adapter returns the contracted normalized `SourceDocument` stage
result. Keep declarative capabilities and option validation in
`SourceAdapterSpec`; do not move family branching into `axon-services`.

Construct a fresh adapter instance per source execution. Move inherent
materialization into the earliest contracted phase that needs it, return all
acquired state through `SourceAcquisition`, and prohibit cross-request adapter
state except explicit provider caches. Add a test proving acquire/normalize
cannot observe residue from a prior request.

- [ ] **Step 4: Keep conditional HTTP reuse behind the web adapter**

The web adapter may use prior ETag/Last-Modified metadata internally during
`acquire`. Pass immutable prior-item metadata through a typed contract DTO; the
adapter never reaches into the ledger. The shared runner supplies generic
plan/manifest state and never
branches on `ReusePolicy`, adapter name, or `SourceKind`. If the behavior must
be advertised, declare it in capability/spec data rather than adding an
execution method to the trait.

Add a 304 case proving no body fetch, prior document/content preservation, no
empty-generation publication, and identical phase/status semantics.

- [ ] **Step 5: Port web, local, and remaining adapters in separate PRs**

Each family resolves through the production registry and calls
`execute_source_pipeline` with the worker’s existing `SourceExecutionContext`.
Map scope becomes a documented discovery-only stage skip, not a separate
executor. Before the first adapter call, assert route safety class, caller
snapshot, visibility ceiling, local/tool permission, and reservation proof.

- [ ] **Step 6: Unify phase events and document status writes**

Every family emits the same ordered phases and uses the same batch writer. Family-specific events may originate inside adapters, but the service-level lifecycle is identical.

- [ ] **Step 7: Delete the parallel implementations**

Remove all web/local vectorize, publish, progress, job-creation, and reuse modules made redundant by the canonical runner. Keep only adapter-owned web/local acquisition code in `axon-adapters`.
Collapse `dispatch_kind` to the validated registry lookup; do not retain
`dispatch_web` or `dispatch_local` as durable production branches.

- [ ] **Step 8: Turn the Task 3 parity matrix green**

```bash
cargo test -p axon-services source_pipeline_parity -- --nocapture
```

All web/local/git assertions must pass without family-specific expected values.

- [ ] **Step 9: Remove crate-contract exemptions**

Update the crate contract to describe the resulting `axon-services` module surface. Remove “until the #298 cutover” exemptions for crates whose target layout now holds. The gate must audit all 23 production crates.

- [ ] **Step 10: Run structural and service gates**

```bash
cargo xtask check-layering
cargo xtask check-crate-contracts
cargo test -p axon-services --lib --no-fail-fast
cargo xtask check-file-size
```

- [ ] **Step 11: Merge and close**

Merge each family-port PR before beginning its dependent deletion unit. Close
`axon_rust-drahp`, `axon_rust-2wq1r`, `axon_rust-upay4`, and
`axon_rust-a155h` only after the deletion PR merges and the merged tree contains
one runner and zero contract exemptions.

## Task 7: Close Authorization, SSRF, Local-Path, and Redaction Gaps

**Beads:** `axon_rust-9veac`, `axon_rust-cjxfw`, `axon_rust-4ygmz`, `axon_rust-0sgqz`, `axon_rust-a4t01`, `axon_rust-hf37r`, security portion of `axon_rust-zb0k1`

**Interfaces:**
- Consumes: caller `AuthSnapshot`, injectable DNS/address policy, canonical local containment policy, public-write redaction gate.
- Produces: no scope escalation, testable connect-time SSRF enforcement, race-safe local reads, explicit panel policy, and fail-closed writes/outputs.

**PR units:** auth/panel/CodeSearch; SSRF/redirects; local paths; redaction and
visibility. No unit combines unrelated security boundaries.

- [ ] **Step 1: Fix search auto-index authorization**

Search remains readable with `axon:read`, but auto-indexing occurs only when the
same caller has `axon:write`; otherwise it returns search results without
creating a job. Persist the caller id and exact snapshot for allowed auto-index
jobs. Test read-only, read+write, and denied inputs through real REST auth
middleware. Never mint trusted-system/Admin/Local authority.

- [ ] **Step 2: Make the fine-grained scope model reachable and symmetric**

Decide and implement production issuance/configuration for `axon:local` and
`axon:execute`; include them in OAuth/static-token capability metadata only
under explicit operator policy. Use one trusted-local default for synchronous
and detached execution, preserving caller/delegator provenance. Remove
`auth_snapshot=None` opposite defaults. A production watch missing its stored
snapshot fails closed or records an explicit migrated system snapshot; it never
uses `AuthSnapshot::default()`/`AuthMode::Test`.

- [ ] **Step 3: Put panel routes inside an explicit security boundary**

Map the panel token to an explicit admin-equivalent `CallerContext` and
visibility policy. Replace `/api/panel/env` raw content with an allowlisted key
and configured/unconfigured status response; values are always redacted. Add
response/log canaries proving API keys, passwords, auth headers, and local
secret paths never leave REST or logs.

- [ ] **Step 4: Wire or remove every dormant enforcement mechanism**

Wire router tool gating, shared network policy, `AffinityPolicy`,
`SourceAccessDecision`, visibility policy, and the real `allow_tool_execution`
config at their service boundaries. If a mechanism is rejected, delete it and
amend the target contract in the same PR. Remove adapter-specific secret
detector lists and complete the canonical Gitea/Reddit detectors.

- [ ] **Step 5: Make SSRF enforcement connect-time and race-free**

Compile the resolver in tests and inject resolver/policy separately from the
test server address. For every request and redirect, resolve once, reject if any
A/AAAA answer is forbidden, pin the actual connection to the validated address,
and avoid a second validation/connect lookup. Apply the same policy to fetch,
discovery, Chrome, and screenshot; fail closed if Chrome interception cannot be
installed.

Cap redirect hops, total duration, response bytes, and retries here—not in the
later performance task. Reject scheme downgrade and non-HTTP schemes. Test
private/metadata/link-local targets, mixed allowed+denied answers, IPv4-mapped
IPv6, supported alternate numeric forms, userinfo, DNS rebinding, resolver
failure, redirect loops, and sitemap/robots/llms/screenshot paths. The
`test-util` feature must not expose a release-build loopback bypass; only the
injected test resolver may target the local harness.

- [ ] **Step 6: Make local reads handle-based and race-safe**

Use Linux `openat2` with `RESOLVE_BENEATH|RESOLVE_NO_MAGICLINKS` or a
documented safe fallback. Validate the opened handle’s identity and read from
that handle; never canonicalize, validate, then reopen by path. Move any
remaining blocking filesystem work to `tokio::fs` or `spawn_blocking`. Apply one
policy to local sources, scope=file, CodeSearch, uploads, artifacts, and tool
paths. Test symlink swap, hardlink/rename race, `..`, magic links, denylisted
`.env`/SSH/cloud/Codex/Gemini/browser paths, and artifact-root traversal. Redact
absolute paths from identities, events, and errors.

- [ ] **Step 7: Enforce redaction at every public boundary**

Create a table-driven boundary suite covering vectors, graph evidence, memory,
artifacts, job events, CLI JSON, MCP, REST, metrics/traces, and provider
`last_error`. Unknown metadata defaults internal. A detector error or forbidden
value produces zero public writes/response leakage, emits a structured redacted
audit event, and prevents partial generation publication. CodeSearch never
force-stamps public visibility.

- [ ] **Step 8: Prove audit-event completeness**

Auth, SSRF, local-path, tool, redaction, artifact, and priority denials record
job/source/caller/policy-version context with a redacted reason. Add canaries
showing URL query strings, headers, snapshots, reservation proofs, provider
errors, and raw local paths never appear in events or logs.

- [ ] **Step 9: Run focused security tests**

```bash
cargo test -p axon-authz --lib --no-fail-fast
cargo test -p axon-core ssrf --no-fail-fast
cargo test -p axon-route local --no-fail-fast
cargo test -p axon-vectors redaction --no-fail-fast
cargo test -p axon-services authorize --no-fail-fast
cargo test -p axon-web panel --no-fail-fast
cargo test -p axon-services security_boundary --no-fail-fast
cargo xtask check-redaction-logs
```

- [ ] **Step 10: Verify the grouped security checklist before closure**

Before closing any grouped bead, match every numbered item in `hf37r`, `a4t01`,
and the security portion of `zb0k1` to a merged implementation and focused
negative test. Generic umbrella tests do not satisfy closure.

## Task 8: Close Remaining Performance and Reliability Defects

**Beads:** remaining `axon_rust-er3z7`, remaining `axon_rust-zb0k1`

**Interfaces:**
- Consumes: the canonical runner from Task 6 and queued reservations from Task 5.
- Produces: a bounded streaming pipeline, batched transactional status writes,
  linear merge behavior, measured SQLite concurrency, cached provider identity,
  and remaining provider-class gates.

**PR units:** streaming/vector/status; SQLite/runtime contention; provider
identity/config cleanup; fetch/render; parse/graph/artifact.

- [ ] **Step 1: Add high-water and complexity regressions**

```rust
assert!(max_embedding_batch_items <= configured_limit);
assert!(max_in_flight_serialized_bytes <= configured_byte_limit);
assert!(max_retained_vector_points <= configured_point_limit);
assert!(max_retained_sparse_entries <= configured_sparse_limit);
assert!(max_retained_statuses <= configured_status_limit);
assert_eq!(document_status_store_calls, expected_batch_calls);
assert!(manifest_merge_comparisons <= linear_bound);
```

Counters are deterministic regressions, not benchmarks. Add separate ignored
criterion/soak benchmarks only if useful after correctness gates are green.

- [ ] **Step 2: Stream end-to-end with bounded memory**

Do not construct a whole `VectorPointBatch` or all document statuses before
chunking. Stream bounded prepared chunks through embedding, vector point
construction, sparse maps, upsert, and status writes with limits on items,
estimated tokens, serialized bytes, and in-flight batches. A single oversized
payload fails or splits deterministically; many ordinary payloads never exceed
the high-water bounds. Every batch releases/reacquires its reservation so older
waiters can run.

- [ ] **Step 3: Replace document-status N+1 safely**

Add a typed `axon-ledger` batch method. Validate source existence once, validate
source items set-wise, chunk statements below SQLite bind limits, preserve
`updated_at` conflict semantics, and use bounded transactions. Test statement
counts and rollback when one item is missing mid-batch. Never hold a SQL
connection/transaction across provider awaits.

- [ ] **Step 4: Replace quadratic merge and unnecessary calls**

Index status/manifest items by canonical key, preserve deterministic output
ordering, and assert a linear comparison bound. Gate `ensure_collection` on
`embed=true` for every family. Remove obsolete poisoned reservation state along
with the Task 5 manager migration; no `expect("... mutex poisoned")` remains.

- [ ] **Step 5: Eliminate repeated TEI identity probes**

Cache embedding identity in the provider/runtime snapshot and invalidate it
when endpoint, model, dimensions, or config snapshot changes. An interactive
query performs the actual embedding call without two preliminary live probes.
Test cache reuse and invalidation; a stale identity must not survive a provider
change.

- [ ] **Step 6: Resolve worker/SQLite concurrency explicitly**

Measure pool acquire time, busy time, and transaction duration. Either size the
pool from active DB-stage concurrency with a safe ceiling, cap DB stages to the
pool, or introduce a bounded writer path. Add a load regression covering eight
workers, a four-connection baseline, heartbeats/status writes, and later
interactive work. Move blocking canonicalization/error-path filesystem work to
`tokio::fs` or `spawn_blocking`.

- [ ] **Step 7: Resolve dead knobs and Qdrant request policy**

Choose one disposition for `qdrant-point-buffer`: wire it as the authoritative
item/byte vector batch limit or delete it and the docs; do not retain it beside
hard-coded `UPSERT_BATCH_SIZE`. Remove/document the dead crawl concurrency knob
and other unconsumed config, then regenerate affected config references.

Use operation-specific Qdrant connect/request/end-to-end deadlines rather than
one shared 30-second budget. Add cold and warm query smokes that record provider
time, scheduler wait, and total elapsed time.

- [ ] **Step 8: Add remaining provider capacity classes**

Using the one Task 5 registry, add fetch/render, then parse/graph/artifact in
separate PRs at canonical service choke points. Define units, item/token/byte
limits, deadlines, cooldown, and cancellation for each class. Do not create
per-service or per-transport managers.

- [ ] **Step 9: Run focused regression tests**

```bash
cargo test -p axon-ledger --lib --no-fail-fast
cargo test -p axon-services source --no-fail-fast
cargo test -p axon-services bounded_pipeline --no-fail-fast
cargo test -p axon-jobs sqlite_contention --no-fail-fast
cargo test -p axon-retrieval provider_identity --no-fail-fast
cargo xtask schemas config --check
```

- [ ] **Step 10: Verify every grouped performance item**

Map P10-P16 from `er3z7` and P17-P21 from `zb0k1` to a merged implementation,
focused regression, and metric where applicable. Do not close either grouped
bead on the strength of generic source tests.

## Task 9: Replace Declaration-Only Tests with Execution Proof

**Beads:** `axon_rust-yow0c`, `axon_rust-j5gry`

**Files:**
- Modify: `tests/cross_surface_operation_matrix.rs`
- Modify: `tests/cross_surface_scope_matrix.rs`
- Create focused root integration tests for source and retrieval parity
- Modify: `crates/axon-adapters/src/fixture_tests.rs`
- Add recorded adapter inputs beside existing fixture packs
- Add public-API integration tests under selected `crates/*/tests/*.rs`
- Modify CI path classifier/gates for the new integration targets

**Interfaces:**
- Consumes: fake service composition and canonical transport-neutral DTOs.
- Produces: one reusable conformance harness proving logical request mapping,
  stable result semantics, real auth middleware, serialization, and deployed
  build identity across CLI, MCP, REST, and panel where applicable.

**PR units:** request/result transport conformance; adapter goldens; Tier-5
recovery/security matrix.

- [ ] **Step 1: Define stable versus generated fields**

For independent submissions, compare canonical `SourceRequest`, source
identity, counts, warnings, visibility, redacted fields, phase sequence, and
terminal semantics. Job ids, attempt ids, trace ids, timestamps, and generated
artifact ids are expected to differ but must each remain internally consistent.
If testing dedupe, submit one explicit idempotency key and separately assert all
surfaces resolve to the same existing job.

- [ ] **Step 2: Reuse the Task 3 observation harness**

First prove CLI, MCP, and REST parsing map the same logical input to the same
transport-neutral DTO. Then invoke the shared service and compare stable
semantics using `PipelineObservation`; do not build a second trace structure or
normalize away real mismatches.

- [ ] **Step 3: Exercise real transport boundaries**

Run requests through CLI parsing, MCP action schema/serialization, REST auth
middleware/body limits/response filtering, and panel auth where applicable.
Cover read, write, admin, execute, and local callers plus denial paths. Do not
inject a prebuilt `CallerContext` after middleware.

Add deployed REST and MCP smokes after restarting the actual service. Record and
assert expected commit, version, config snapshot, and returned job/build
identity. Use `--local` for the separately tested local binary so CLI proxying
cannot create a false pass.

- [ ] **Step 4: Add executable retrieval parity**

Run query/retrieve with one fixed vector fixture through all three transports
and assert equivalent citations, visibility filtering, content omission,
requested/effective priority, and redacted errors. Generated trace ids may
differ but must correlate within each response/event chain.

- [ ] **Step 5: Convert adapter fixtures into golden outputs**

For each `source_family_matrix()` entry, feed a recorded acquisition into the adapter and compare emitted `SourceDocument` metadata/payload to the committed fixture. The fixture may be regenerated only through an explicit xtask command.

- [ ] **Step 6: Add public-caller integration targets**

Create integration tests that import `axon-services`, `axon-adapters`, `axon-jobs`, and `axon-vectors` as external crates. They must use public APIs only and reproduce the canonical source lifecycle.

- [ ] **Step 7: Implement the missing contract cases**

Cover:

- fresh-schema/reset/reindex cutover;
- stalled interactive-lane watchdog;
- reservation cancellation and expiry;
- duplicate watch coalescing;
- vector writes bounded separately from embedding;
- committed-generation recovery idempotence;
- interrupted pre-publish generation hidden after restart;
- canonical stage-registry ordering plus intent-specific allowed skips;
- bulk embedding cannot starve interactive query embedding.
- crash injection after every mutating source stage with no pre-commit vector,
  graph, status, artifact, or event visibility;
- real auth/SSRF/redaction denial modes in the shared harness;
- fake composition is offline-only and cannot reach live TEI/Qdrant.

- [ ] **Step 8: Run parity and integration suites**

```bash
cargo test --test cross_surface_operation_matrix -- --nocapture
cargo test --test cross_surface_scope_matrix -- --nocapture
cargo test --workspace --tests --no-fail-fast
cargo xtask check-public-api
```

- [ ] **Step 9: Merge in three units**

Merge transport conformance, goldens, and Tier-5 cases separately. Close
`axon_rust-yow0c` only after real parsing/auth/serialization parity merges, and
`axon_rust-j5gry` only after every required integration case is present.

## Task 10: Build the Contracted Documentation Generators and Reconcile the Packet

**Beads:** `axon_rust-5iglz`, `axon_rust-ugvcq`, `axon_rust-vtdw0`, `axon_rust-beuzs`

**Files:**
- Refactor to the contracted roots: `xtask/src/docs/mod.rs`, `args.rs`, `artifact.rs`, `check.rs`, `markdown.rs`, `manifest.rs`, `render.rs`, and `examples.rs`
- Add the 17 modules under `xtask/src/docs/families/`
- Modify: `xtask/src/util/{diff,fs,markdown,paths}.rs` as required by the contract
- Modify: generated references under `docs/reference/`
- Modify: `docs/pipeline-unification/`
- Modify: affected crate `CLAUDE.md` files

**Interfaces:**
- Consumes: the merged Task 2B generator engine plus live registries already
  used by `cargo xtask schemas`.
- Produces: `DocsFamilyGenerator`, `DocsArtifactSet`, and `GeneratedDocArtifact`
  implementations with deterministic markdown/JSON whose checksums derive from
  declared runtime inputs, not self-referential headers.

- [ ] **Step 1: Reconcile the existing six families and enumerate all 17**

Encode the exact contracted families—`cli`, `cli-help`, `openapi`, `mcp`,
`api-dto`, `api-enums`, `errors`, `events`, `config`, `env`, `adapters`,
`schema`, `memory`, `providers`, `presentation`, `schemas`, and `new-source`—as
a typed registry with output path, renderer, and source-input manifest. Reuse
the six Task 2B implementations; do not replace the engine. A family without a
renderer must fail generation rather than stamp an empty file.

- [ ] **Step 2: Add failing content-generation tests**

For each formerly empty authoritative reference, assert required headings, at least one live registry row, and a source checksum that changes when the registry fixture changes.

- [ ] **Step 3: Implement renderers by reusing schema registries**

Render CLI, REST, MCP, config, database, jobs/events, adapters, vector payload, graph, memory, auth/security/redaction, observability, providers, pruning, presentation, and inventory references from their owning registries.
Render/write artifacts one family at a time with bounded memory. Add a
high-water test so aggregate generation does not retain all 17 documents and
source manifests simultaneously.

- [ ] **Step 4: Make `--check` compare regenerated content**

Generate into memory or a temporary directory and byte-compare every expected output. Header-only differences cannot make an empty document pass.

- [ ] **Step 5: Implement the exact CLI and validation contract**

Support aggregate `generate`, `generate --check`, and `generate --print`, every
per-family command, and the `--family`, `--json`, and CI-forbidden
`--update-snapshots` flags with exit codes `0` through `4` as specified.

- [ ] **Step 6: Reconcile historical/future-tense contracts**

Mark dated delivery plans historical, update live contracts to present tense,
correct module maps, remove residue naming, and ensure the closeout audit links
the still-open epic until all children close. Reconcile the stale
`runtime/job-contract.md` priority prose (`low/normal/high/interactive`) with
the canonical five-value enum contract; the enum contract wins.

- [ ] **Step 7: Run generator drift checks**

```bash
cargo xtask docs generate
cargo xtask docs generate --check
cargo xtask schemas all --check
cargo xtask docs check
cargo xtask check-public-api
```

- [ ] **Step 8: Commit**

```bash
git add xtask docs crates/*/src/CLAUDE.md
git commit -m "docs(pipeline): generate live unification references"
```

Split this work into the generator engine already merged in Task 2B, small
family batches, and a final historical-prose reconciliation PR. Close
`axon_rust-5iglz` only after all 17 family commands and aggregate check pass on
the merged tree.

## Task 11: Final Closure, Release, and Deployment

**Beads:** `axon_rust-enbmu` and every remaining child

**Files:**
- Modify generated release/version files only through the repository’s supported commands
- Modify the closeout audit and metaplan completion state
- Add a final session/verification report under `docs/sessions/`

**Interfaces:**
- Consumes: all previous tasks merged to `main`.
- Produces: zero open epic children, green gates, a released version, and matching host/container runtime.

- [ ] **Step 1: Audit all 28 children against live code**

```bash
bd list --parent axon_rust-enbmu --status open --limit 0
rg -n 'TODO\\(#298\\)|FIXME|WIP|XXX' crates xtask tests docs/pipeline-unification
```

Do not close a grouped bead until every sub-finding in its description has a test and implementation reference.
For every completed task, verify its implementation against the alignment matrix
above and cite the exact target-contract sections in the closeout audit.

- [ ] **Step 2: Run the repository’s full pre-PR gate**

```bash
just precommit
cargo xtask docs generate --check
cargo xtask schemas all --check
cargo xtask check-layering
cargo xtask check-crate-contracts
cargo xtask check-public-api
```

- [ ] **Step 3: Run live-provider smoke tests**

Use the configured external Qdrant, TEI, Chrome, and LLM endpoints. Exercise one web, local, and git source; query while a bulk source job holds embedding capacity; verify the interactive request waits and completes rather than failing or starving.
Run cold and warm Qdrant cases separately and record scheduler wait, provider
latency, and end-to-end latency. Validate the local artifact with `--local`;
then separately validate deployed REST and MCP after restart. Each result must
report the expected merged commit/build identity, version, config snapshot, and
job id.

- [ ] **Step 4: Update the final audit**

Record:

- one executor path for all 12 families;
- one job id through every phase;
- real queued provider metrics;
- all 23 crate contracts audited;
- behavioral CLI/MCP/REST parity;
- all contracted docs generated;
- no open #298 TODO or bead.

- [ ] **Step 5: Merge through normal branch protection**

```bash
git pull --rebase
bd dolt push
git push
gh pr checks --watch
```

- [ ] **Step 6: Assess compatibility, then bump and release**

Produce a compatibility checklist covering CLI syntax, REST/MCP schemas,
configuration, job/database schema, auth/scope behavior, and source-result
semantics. Select patch/minor/major from the actual merged changes and repository
release policy; do not default to minor merely because transport/config files
changed. Use the supported manual CLI command with the assessed level:

```bash
cargo xtask bump-version cli <patch|minor|major>
```

Run version, artifact, and release smoke gates before tagging.

- [ ] **Step 7: Deploy from merged `main`**

Repeat Task 1’s checksum, backup, atomic temp+rename, bounded health deadline,
and rollback protocol from the exact merged commit. Build separate host and
Bookworm-compatible artifacts; never copy the host build into Bookworm. Verify:

```bash
/home/jmagar/.local/bin/axon --version
/home/jmagar/.local/bin/axon doctor
incus exec axon -- /usr/local/bin/axon --version
incus exec axon -- systemctl is-active axon-native.service
# Then exercise deployed REST and MCP and assert build identity.
```

Any mixed-version or partial-health result triggers automatic restoration of
both prior binaries/configs and verification of the previous runtime.

- [ ] **Step 8: Close the epic**

```bash
bd list --parent axon_rust-enbmu --status open --limit 0
bd close axon_rust-enbmu
bd dolt push
git status
```

Expected: no open children, no uncommitted files, branch up to date with origin, and host/container versions match the release.

## Bead Coverage Matrix

| Task | Beads closed |
|---|---|
| 2 | `jc20j`; partial `a155h` |
| 2B | prerequisite generator-engine portion of `5iglz` |
| 3 | `fts94`, `j801x` |
| 4 | `yygrl`, `dkuqo`, `gpuz9`, `igp0i`, `bfsp5`; partial `upay4` |
| 5 | `nl7au`, `uzy27`; reservation portions of `er3z7` |
| 6 | `drahp`, `2wq1r`, remainder of `upay4`, remainder of `a155h` |
| 7 | `9veac`, `cjxfw`, `4ygmz`, `0sgqz`, `a4t01`, `hf37r`; security portions of `zb0k1` |
| 8 | remainder of `er3z7`, remainder of `zb0k1` |
| 9 | `yow0c`, `j5gry` |
| 10 | `5iglz`, `ugvcq`, `vtdw0`, `beuzs` |

Every one of the 28 open children has exactly one final closure owner; partial
rows identify prerequisite contributions without early closure.

## Grouped-Finding Closure Checklists

Each row requires a merged implementation reference and focused test reference
in the bead before closure.

### `axon_rust-er3z7`

- [ ] P10: queued durable reservations and non-zero queue metrics.
- [ ] P11: document-status N+1 replaced by bounded transactional batches.
- [ ] P12: status/manifest merge reduced from quadratic to linear.
- [ ] P13: oversized documents use the same bounded streaming path for every family.
- [ ] P15: poisoned reservation mutex authority removed; panic cannot brick capacity.
- [ ] P16: `embed=false` performs no collection ensure/create call.

### `axon_rust-hf37r`

- [ ] S-8: production issuance/policy for `axon:local` and `axon:execute` is explicit and tested.
- [ ] S-10: panel has a caller context and `/api/panel/env` cannot return secrets.
- [ ] S-11: synchronous and detached auth defaults are identical for the same trust context.
- [ ] S-12: adapter detector lists are deleted and canonical Gitea/Reddit detection is complete.

### `axon_rust-a4t01`

- [ ] S-4: router tool-execution policy is wired from operator/caller policy.
- [ ] S-5: the shared SSRF policy is the real fetch/render boundary or is removed with a contract amendment.
- [ ] S-9: `AffinityPolicy`/`SourceAccessDecision` is enforced for inline local/tool work or removed with a contract amendment.

### `axon_rust-zb0k1`

- [ ] P17: interactive CLI does not issue two TEI identity probes per request.
- [ ] P18: `qdrant-point-buffer` is wired as the sole batch limit or deleted.
- [ ] P19: crawl/source worker concurrency is consumed and documented or removed.
- [ ] P20: worker/SQLite pool concurrency has a measured bounded design.
- [ ] P21: blocking filesystem canonicalization is absent from async worker paths.
- [ ] S-13: shared redirects have hop, time, byte, and scheme bounds.
- [ ] S-14: screenshot uses pinned connect-time DNS/SSRF enforcement.
- [ ] S-15: release builds cannot enable a loopback test bypass.
- [ ] S-16: watches never default to test-shaped auth.
- [ ] S-17: CodeSearch never force-stamps public visibility.
- [ ] S-18: `allow_tool_execution` is live or removed from config/docs.

## Engineering Review Record

Lavra engineering review completed on 2026-07-25 with architecture,
simplicity, security, and performance specialists. Raw review counts were:

| Discipline | Critical | Important/high | Medium/minor | Critical silent gaps before revision |
|---|---:|---:|---:|---:|
| Architecture | 4 | 7 | 5 | 6 |
| Simplicity | 2 | 8 | 4 | 2 |
| Security | 5 | 8 | 5 | 6 |
| Performance | 6 | 12 | 4 | 2 |

After deduplication, 32 actionable recommendations were applied to this plan.

### Consolidated strengths

- The target contract, six-method adapter boundary, orchestration ownership, and
  separate embedding/vector capacity classes were already correct.
- Behavioral proof precedes deletion, and live-provider smokes remain outside
  default CI.
- All 28 children have closure owners.

### Consolidated concerns resolved by this revision

- Cross-process capacity now has one SQLite authority and multi-process proof.
- Delivery now uses mergeable PR waves and post-merge bead closure.
- Runner migration now has a stage, visibility, graph-order, and crash-recovery
  contract.
- Executable docs drift protection lands before contract-changing runtime work.
- Auth, panel, SSRF, local containment, redaction, and priority admission have
  explicit negative tests.
- Performance acceptance includes worker admission, bounded streaming, SQLite
  contention, provider identity, and cold/warm Qdrant behavior.

### Revised failure-mode matrix

| Codepath | Production failure | Rescue now specified? | Test now specified? | User-visible/logged? |
|---|---|---:|---:|---|
| Operational deployment | Mixed binaries or failed migration | yes, atomic rollback | yes, bounded health/build checks | outage prevented; journal/report |
| Layering/facades | Alias bypass or stalled stream | yes, gate + bounded stream | yes | structured timeout/gate failure |
| Observation harness | Fake wiring diverges from production | yes, shared constructor | yes, security modes | CI-visible |
| Adapter registry | Missing/duplicate adapter | yes, startup fail | yes, full matrix | startup error |
| Reservation kernel | Cross-process over-grant/replay/restart leak | yes, SQLite/fencing | yes, two schedulers + races | structured error + bounded metrics |
| Worker admission | Source waiters consume all workers | yes, `blocked` admission | yes, mixed-job regression | dashboard/event |
| Canonical runner | Partial vector/graph/status visibility | yes, commit fence/debt | yes, crash each mutating stage | job failure/degraded event |
| Conditional reuse | 304 publishes empty content | yes, typed prior metadata | yes | job/event |
| Security boundaries | scope escalation, panel leak, SSRF/path race | yes | yes, black-box negative suites | redacted denial/audit |
| Bounded pipeline | Whole-run point/status materialization OOM | yes, streaming limits | yes, high-water counters | structured oversize/overload |
| SQLite/status batch | bind overflow or partial batch | yes, bounded transactions | yes, rollback/count/load | job error + metrics |
| Transport parity | Internal harness bypasses middleware/stale server | yes, black-box/deployed proof | yes, build identity + `--local` separation | report/event |
| Docs generation | stale/secret/nondeterministic output | yes, early engine/manifests | yes, negative drift + canaries | CI-visible |
| Final query smoke | cold Qdrant or stale proxy false pass | yes, operation budgets | yes, cold/warm local/deployed | timed report |

No revised row remains both unrescued and untested while silent.

### Not in the epic’s implementation critical path

- Adaptive capacity autotuning: static explicit limits are sufficient for the
  cutover.
- A separate distributed scheduler service: SQLite transactions/fencing provide
  host-local cross-process authority without another service.
- Long-running criterion/soak benchmarks: deterministic liveness, complexity,
  and high-water gates land first; soak work may receive a future performance
  bead if those gates reveal a need.
- Cache optimization beyond eliminating repeated TEI identity probes: no current
  finding requires it.
- Rich security dashboards: required redacted events/metrics land here; new UI
  work is separate product scope.
- Llama tuning and the v7.2 baseline deployment: retained as Wave O, independent
  of the architectural dependency chain.

## Self-Review Results

- **Spec coverage:** all 28 open children appear in the coverage matrix; deployment of the already-merged v7.2.0 is Task 1; final release/deployment is Task 11.
- **Placeholder scan:** no implementation step contains `TBD`, `TODO`, “implement later,” or an unspecified “write tests” instruction.
- **Type consistency:** the plan keeps the canonical `SourceAdapter`,
  `SourceExecutionContext`, `ProviderReservationContext`, `JobPriority`, and
  `PipelinePhase` contracts. Current `ReusePolicy` and `MaterializedSource`
  adapter escape hatches are treated as transitional implementation details and
  removed from the public adapter boundary in Task 6.
- **Isolation:** the unrelated llama compose diff and v7.2 deployment are an
  independent operational lane and do not block architectural work.
- **Ordering:** live gates and the docs drift engine precede contract-changing
  runtime work; the observation harness and adapter/router normalization precede
  the durable scheduler; the scheduler and visibility model precede family
  ports; security/reliability and black-box parity follow; remaining docs, live
  smokes, and release proof come last.
- **Engineering review:** all 32 consolidated actionable recommendations are
  represented in normative decisions, task steps, closure checklists, or
  explicit out-of-scope decisions.
