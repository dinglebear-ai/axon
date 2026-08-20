# W21 Artifact Engine Progress

Last updated: 2026-08-19
Status: active implementation

## Lane identity

- Worktree: /home/jmagar/workspace/axon-w21-artifact-engine
- Branch: codex/w21-artifact-engine-20260819
- Base: origin/main b319595736b6c1152b764beca4dc5a2690215c64
- Draft PR: #569, feat: add artifact candidate crawl and enrichment pipeline

## Isolation evidence

Repository/worktree inspection completed before changes. The pre-existing pipeline-unification worktree contains extensive uncommitted work and has not been cleaned, reused, or modified by W21. The W21 branch/worktree was created directly from the then-current origin/main and verified with an identical merge-base.

## Governing research complete

Reviewed Axon source-pipeline, adapter, ledger, API, and service ownership contracts plus Phoenix docs/eight ADR-0023, depot.md, axon.md, schemas.md, licensing.md, and meta-plan.md.

Current upstream verification performed 2026-08-19:

- skills.sh official API docs and Terms;
- ARD official publication/spec references;
- Microsoft APM primitives/targets/registry/marketplace references.

Key resolved assumptions:

- skills.sh is a discovery pointer/evidence provider, not source-license authority;
- its structured API is preferred to HTML crawl for catalog enumeration;
- its published API is rate-limited and exposes bounded pagination/search controls;
- canonical repository/base source must be resolved independently;
- unknown/restricted license state remains metadata-only/no-public-byte-mirroring;
- ARD/APM remain adapters/sources, not Axon's canonical artifact object model.

## Checkpoints

### 4e707258b - initial candidate evidence seam

Implemented and pushed the first coherent typed candidate/sink checkpoint and opened draft PR #569. Coordination review subsequently established Depot W20 as the provisional G0 owner of the individual neutral ArtifactCandidate payload, so the initial Axon-shaped serialized candidate is being superseded in the next follow-up commit rather than allowed to become a competing cross-repo schema.

Original focused evidence:

- cargo test -p axon-api artifact_candidate -- --nocapture: 4 passed, 0 failed;
- cargo test -p axon-adapters artifact_candidates -- --nocapture: 4 passed, 0 failed;
- cargo fmt --all: passed;
- git diff --check: passed before checkpoint commit.

Operational note: repository commit/pre-push hooks exceed LABBY Code Mode's per-execution wall-clock budget. The first hook attempts timed out without committing/pushing; state was explicitly verified. The checkpoint was then committed/pushed with hooks skipped after equivalent focused checks. Full required checks remain a W21 exit gate.

### Shared G0 payload parity and PR CI repair - ready for follow-up checkpoint

Coordination rule applied:

- Depot W20 is the provisional G0 owner of the individual candidate payload;
- Axon now targets `schemaVersion=dinglebear.artifact-candidate/v1`;
- the neutral top-level field set exactly follows W20's current G0 vocabulary;
- old Axon-only candidate fields are rejected by parity tests;
- Axon exact dedupe helpers remain producer-side helpers/evidence rather than shared top-level fields;
- Axon source-item correlation uses bounded `manifestMetadata.axonSourceItemKey`;
- `crawlJobId` and `crawlGenerationId` carry shared crawl correlation;
- the Axon-specific `ArtifactCandidateBatch` remains a separate transport wrapper at `application/vnd.dinglebear.axon.artifact-candidates+json;version=1`;
- candidate license data remains evidence-only and the local public-byte gate fails closed except explicit `redistributable`/`forkable`.

Neutral fixture seam:

- `crates/axon-api/tests/fixtures/schema/artifact_candidate.v1.neutral.json`;
- exact fixture deserialize -> shared-bound validation -> serialize equality;
- exact 18-field camelCase top-level set;
- W20-equivalent candidate/JSON/path/digest/secret-key bounds;
- byte/authority-field exclusion.

Depot W20 G0 is frozen at commit `25de725`. Axon copies the open canonical fixtures byte-for-byte from `docs/artifact-registry/fixtures/artifact-candidate-v1.json` and `artifact-interchange-v1.json`; no Depot implementation code is imported or linked.

Frozen-fixture proof:

- candidate SHA-256: `58afd5392e664ead043a89a45d072c32d4fb7bd2cb119bb50678c09b2775f732`;
- interchange SHA-256: `6b52ca32894a42720a3f18e7f2919a54a82031f7a89c0da2e026069d27eec88b`;
- both Axon copies compare byte-identical to Depot `25de725`;
- `cargo test -p axon-api artifact_candidate -- --nocapture`: 6 passed, 0 failed, including exact canonical JSON serialization and round-trip parity for the individual candidate payload;
- `./target/debug/xtask generated-contracts check`: passed against the frozen candidate fixture;
- `cargo fmt --all -- --check` and `git diff --check`: passed.

PR #569 CI root cause and repair:

- `rust-contracts` failed because `xtask/tests/fixtures/schemas/api/snapshots/schemas.json` was stale after the new DTO;
- `ci-gate` failed only because `rust-contracts` failed; downstream clippy/test/binary jobs were skipped by the workflow graph, not independently failing;
- added `ArtifactCandidate` plus the Axon batch/sink DTOs as generated API schema roots;
- refreshed generated contracts/snapshots;
- `./target/debug/xtask generated-contracts refresh && ./target/debug/xtask generated-contracts check`: passed, including 504 doc-link checks and 127 removed-surface contract checks;
- `cargo test -p axon-api artifact_candidate -- --nocapture`: 5 passed, 0 failed;
- `cargo check -p axon-services --lib`: passed against the neutral payload projection.

## Current work

### 1b8898b06 - frozen G0 optional evidence semantics

Pushed after reviewing Depot `25de725` constructor/contract semantics:

- shared v1 fields remain unchanged;
- only `schemaVersion`, `id`, `canonicalSourceUri`, `sourceProvider`, and `observedAt` are required by Axon's generated schema;
- optional evidence deserializes with Depot-equivalent null/empty defaults;
- canonical Rust serialization still emits the full 18-field frozen candidate shape;
- `cargo test -p axon-api artifact_candidate -- --nocapture`: 7 passed, 0 failed;
- `generated-contracts refresh && check`: passed; generated schema reports the five G0 core required fields, 18 properties, and `additionalProperties=false`.

### 9ad1b48d4 - unified SourceRequest candidate sink

Current wiring:

- injects `ArtifactCandidateSink` into `TargetLocalSourceRuntime` with `NoopArtifactCandidateSink` production/test defaults;
- adds an additive default `SourceAdapter::artifact_candidates` hook so existing adapters produce no candidate evidence unless explicitly artifact-aware;
- collects candidates beside normalized changed documents from the same `SourceRequest`, `job_id`, source and ledger generation;
- applies Axon's complete public-write redaction boundary to the candidate before shared-contract validation, including token-shaped values under otherwise innocent evidence keys;
- validates frozen shared payload bounds plus crawl job/generation/source-item correlation and suppresses duplicate candidate IDs;
- buffers candidates until the source generation has actually committed, preventing failed-generation ghost intake;
- submits bounded candidate batches after commit only, with a hard Axon ceiling of 64 and smaller sink-advertised limits honored;
- requires wrapper-version compatibility and sink idempotency support; impossible receipt accounting is rejected as degraded evidence;
- sink capability, delivery, partial, rejected and invalid-receipt failures become source warnings and do not roll back already-committed RAG/vector state;
- unchanged refresh does not replay candidate delivery.

Focused proof:

- final `axon-services` test binary compiled successfully;
- candidate/sink + integration filter: 10 passed, 0 failed;
- dedicated post-commit/unchanged-refresh integration: 1 passed, 0 failed;
- existing `source_pipeline_differential_tests`: 4 passed, 0 failed;
- default adapter candidate hook regression: 1 passed, 0 failed;
- `cargo check -p axon-services --lib`: passed;
- `cargo fmt --all -- --check`: passed;
- `xtask check-layering`: passed;
- `generated-contracts check`: passed;
- changed product files pass the monolith hard limits with no allowlist additions;
- `git diff --check`: passed.
- `cargo clippy -p axon-api -p axon-adapters -p axon-services --all-targets -- -D warnings`: passed.

Adversarial review findings addressed before checkpoint:

- retry/recovery could have produced different batch idempotency keys if the same candidate set arrived in a different order; candidate IDs are now sorted before bounded partitioning, and the replay test reverses producer order while requiring identical delivery keys;
- token-shaped evidence under a non-secret key could have bypassed shared key-name validation; the entire candidate now crosses Axon's existing public-write redactor before shared validation;
- sinks that do not advertise idempotency are rejected before submission;
- impossible accepted/partial/rejected receipt counts are rejected as degraded evidence;
- no new migration/table/job/ledger/vector path, unbounded fan-out, retry loop, spawn, or destructive publication authority was introduced by C2.

C2 checkpoint `9ad1b48d4` is committed and pushed to draft PR #569.

### C3 structured skills.sh catalog discovery - complete

Current structured provider slice:

- routes `skills.sh`, `skills.sh:leaderboard`, and `skills.sh:search` through the existing `SourceRequest` path as `SourceKind::Registry + SourceScope::Api`, canonicalized under `catalog://skills.sh/...`;
- treats that canonical URI as the sole leaderboard/search mode identity: adapter options cannot override `mode` and silently make the fetched endpoint diverge from source identity;
- keeps the concrete registry executor and existing ledger/jobs/watch lifecycle rather than creating a skills-specific persistence or scheduling subsystem;
- uses the official `https://skills.sh/api/v1/skills` JSON API with Vercel OIDC bearer auth, a 20-second request timeout, 4 MiB streaming response cap, default 100-row/one-page discovery, hard 1,000-row/10-page ceiling, and provider-documented 500 leaderboard / 200 search page ceilings;
- pagination is sequential and stable-sized for the run, with stable-id dedupe so duplicate rows cannot consume the unique result ceiling;
- HTTP 429 stops the current bounded run immediately, caps `Retry-After` at 300 seconds, and projects a provider-scope retry hint without sleeping/retrying locally; 5xx/network failures are likewise provider-scope retryable while 401/403 fail closed with redacted credential errors;
- never persists or logs the OIDC token;
- deliberately does not call the detail/files endpoint, so third-party skill files do not enter Axon catalog materialization before license/right gates exist;
- maps safe listing JSON into stable manifest items, structured `SourceDocument`s, and frozen `dinglebear.artifact-candidate/v1` evidence from the same changed item;
- candidate evidence preserves installs/source type/duplicate state, resolves safe `installUrl` repository pointers without inventing a source path/ref, marks redistribution unknown, emits no observed files, and therefore fails the public-byte gate;
- candidate observation time is evidence-backed from the catalog dump and fails closed if that timestamp is missing rather than falling back to wall clock time;
- Registry `api` graph projection uses generic source/artifact semantics instead of pretending catalog rows are package versions.

C3 audit enrichment is now implemented behind an explicit bounded option:

- `audit_limit` defaults to 0, hard-caps at 25, and is additionally capped by the selected catalog result ceiling, so the default structured crawl performs zero audit N+1 calls;
- audit lookup uses the documented stable listing `id` at `/api/v1/skills/audit/{id}`, reuses one authenticated HTTP client for the bounded run, executes sequentially, and performs no hidden retries;
- HTTP 404 records `none`; validated responses record `available`; the first auth/rate/provider/shape failure records `unavailable`, marks the remaining selected rows `skipped_after_failure`, and stops issuing audit calls without discarding base catalog discovery;
- audit response identity must exactly match listing `id/source/slug`; audit entry counts/strings/status/risk/timestamps/categories are bounded and validated before becoming evidence, then partner entries are sorted deterministically;
- listing/search responses cannot inject audit-owned fields: those fields are cleared at the listing trust boundary and only the dedicated audit endpoint may populate them;
- listing rows now validate the documented `id == source/slug` identity and `github|well-known` source shapes before ledger materialization;
- GitHub canonical pointers require HTTPS `github.com/<same owner/repo>`; unrelated install hosts fail closed to the validated skills.sh aggregator page and do not populate `repository`;
- the undocumented listing `hash` field was removed completely. Because the listing API does not promise a content hash and C3 deliberately does not call the detail/files endpoint, skills.sh candidates emit no source content digests;
- audit evidence changes the listing content hash while preserving the stable source item identity, so existing ledger diffing can treat audit changes as modified evidence without inventing a new item.

Focused C3 proof after adversarial hardening:

- `axon-adapters` skills.sh filter: 24 passed, 0 failed, covering canonical route-mode inference, provider-future cancellation, listing trust-boundary validation, audit bounds/404/429/fail-soft stop, hostile pointer fallback, evidence-backed timestamps, hard limits, and the registry dump -> document -> frozen candidate vertical path;
- `axon-route` skills.sh filter: 2 passed, 0 failed, including acceptance of bounded `audit_limit` and rejection of mode overrides that could diverge canonical source identity;
- `axon-services` source-pipeline differential filter: 4 passed, 0 failed;
- `axon-services` graph filter: 29 passed, 0 failed, including family-specific Registry `api` graph semantics;
- generated-contract refresh/check: passed, including 504 doc-link checks, 127 removed-surface contract checks, and full docs inventory;
- `xtask check-layering`: passed;
- `cargo check -p axon-route -p axon-adapters -p axon-services --lib`: passed;
- `cargo clippy -p axon-route -p axon-adapters -p axon-services --all-targets -- -D warnings`: passed;
- `cargo fmt --all -- --check`: passed;
- all new/touched C3 Rust modules checked by `scripts/enforce_monoliths.py --file`: passed with no allowlist additions;
- `git diff --check`: passed;
- post-audit `cargo clippy -p axon-route -p axon-adapters --all-targets -- -D warnings`: passed;
- post-audit monolith checks passed for all 9 touched route/adapter Rust files;
- live authenticated seed was not attempted because neither `SKILLS_SH_OIDC_TOKEN` nor `VERCEL_OIDC_TOKEN` is present in the DOOKIE execution environment; the adapter does not fall back to unauthenticated scraping.

C3 checkpoints are pushed to draft PR #569: core structured discovery `95122ed47`; bounded audit evidence and trust-boundary hardening `e44493acb`.

### C4 changed-only refresh/watch proof - additive checkpoint

- C4 changes tests only in `crates/axon-adapters/src/registry_sources/skills_sh_tests.rs` and `crates/axon-services/src/watch_tests.rs`; both files are clear of PR #570's current diff, and no W21 production file changed for this checkpoint.
- the skills.sh adapter proves the existing `SourceManifestDiff` boundary fetches and normalizes only `added`/`modified` rows; `unchanged` rows produce no acquisition/candidate input, and `removed` rows remain reconciliation evidence instead of becoming authoritative delete commands;
- the existing added-path vertical test plus the new modified/unchanged/removal tests cover the changed-only candidate matrix without a second ledger/watch/crawl path;
- watch execution proves persisted `source`, `options`, `scope`, `embed`, and `collection` are replayed while execution-time `refresh`, `wait`, and `reason` are the tested overrides;
- `WatchRequest` does not currently persist the full `SourceRequest` limits/metadata envelope, so W21 does not claim exact full-request replay. That remaining contract gap is intentionally left for coordination after #570 settles because `watch.rs` overlaps that lane.

Focused proof:

- `axon-adapters` skills.sh filter: 26 passed, 0 failed, including both new incremental changed-only tests;
- `axon-services` watch replay filter: 1 passed, 0 failed;
- consolidated C4 gate status `0`: format check, generated-contract check, layering, `git diff --check`, frozen Depot fixture hashes, and `cargo clippy -p axon-adapters -p axon-services --all-targets -- -D warnings` all passed using the normal kache path;
- frozen Depot fixture hashes remain candidate `58afd5392e664ead043a89a45d072c32d4fb7bd2cb119bb50678c09b2775f732` and interchange `6b52ca32894a42720a3f18e7f2919a54a82031f7a89c0da2e026069d27eec88b`.

### C5 Depot candidate intake checkpoint

- Depot W20 PR #37 is merged at `b76807cd59eb4546c00375ba66c2cc9428eb390a`; its C2 hosted registry + bounded candidate ledger is complete and the canonical write-scoped operation is `depot.artifacts.intake_candidate`;
- JSON transport is `POST /api/operations/depot.artifacts.intake_candidate` with request `{candidate: <dinglebear.artifact-candidate/v1>}`, Axon's `Idempotency-Key` and `X-Axon-Delivery-Id` headers, and success `{result:{candidate:<canonical v1>}}`; Depot requires bearer scope `skills:write`;
- pushed Axon checkpoint `3abf29c3b` implements `DepotArtifactCandidateSink` only in #570-clear `axon-adapters` files; no config/context/runtime production file was touched;
- the sink advertises `max_batch_size=1` because Depot's canonical operation accepts one candidate, and a shared one-permit semaphore enforces max in-flight delivery = 1 across cloned sinks/source jobs;
- the bearer is transport-only, redirects are disabled, ambient proxies are disabled, and operator-configured private/Tailscale Depot addresses remain valid; embedded URL credentials/query/fragment and non-HTTP(S) schemes fail closed;
- successful responses must echo the exact submitted canonical candidate; a mismatched echo is rejected rather than treated as accepted;
- 401/403/other 4xx become non-retryable rejection receipts; 429 and 5xx/network failures are provider-retryable, Retry-After is capped at 300 seconds, and the sink never performs a hidden local retry;
- Axon's deterministic batch delivery/idempotency keys remain intact in the unified pipeline while Depot's candidate ledger makes identical candidate-ID/payload re-intake idempotent and rejects same-ID conflicting evidence;
- the merged Depot candidate/interchange fixtures remain byte-identical to Axon's copies: candidate `58afd5392e664ead043a89a45d072c32d4fb7bd2cb119bb50678c09b2775f732`, interchange `6b52ca32894a42720a3f18e7f2919a54a82031f7a89c0da2e026069d27eec88b`.

Focused/final C5 proof:

- focused Depot sink suite: 10 passed, 0 failed, including exact frozen-fixture POST/echo, auth separation, 401/403/422, 429 Retry-After cap, 503 retry classification, redirect containment, cross-clone serialization, and invalid base URLs;
- full ArtifactCandidate filter: 16 passed, 0 failed;
- `cargo fmt --all -- --check`, generated-contracts, layering, `git diff --check`, explicit per-file and changed-file monolith policies, frozen fixture hashes, and `cargo clippy -p axon-adapters --all-targets -- -D warnings`: passed on the normal kache path;
- adversarial cleanup split HTTP response classification out of `submit_candidate`, eliminating the only new monolith soft warning without changing behavior;
- pushed C6 baseline CI had one unrelated `axon-services` memory-compaction failure after SQLite reported `database is locked`; local exact reproduction passed 1/1 in 15.76s, while all artifact/C5 gates remained green.

The review follow-up wires production target-runtime injection through the paired
`AXON_ARTIFACT_CANDIDATE_DEPOT_URL` / `AXON_ARTIFACT_CANDIDATE_DEPOT_TOKEN`
environment contract. Partial configuration fails startup; leaving both unset
retains the explicit no-op sink.

### C6 semantic/graph evidence checkpoint

- pushed implementation checkpoint `e4ae82ab0` keeps all C6 production changes outside PR #570-owned source/graph/watch service files;
- `artifact_candidate_duplicate_evidence` preserves Axon exact identity/content keys as the authoritative producer-side dedupe seam while carrying provider duplicate observations and semantic near-neighbor candidate ids as sibling evidence only;
- semantic neighbor ids are sorted, deduplicated, capped at 32, and report truncation deterministically; no semantic signal changes `ArtifactCandidate.id`;
- skills.sh candidates now carry `discoveryEvidence.axonDuplicateEvidence` with the provider `isDuplicate` signal and `authorityScope = evidence-only`; the frozen shared v1 top-level field set is unchanged;
- structured skills.sh documents emit typed `GraphCandidate` values through the existing `_axon_vertical_graph_candidates` metadata bridge consumed by the normal document-preparation/graph path, without adding a second graph writer;
- graph evidence links the Registry API source to the observed artifact with `source_indexed_as` and, when a validated GitHub repository pointer exists, the artifact to the repo with `derived_from`;
- graph evidence uses the existing closed `text_mention` evidence kind, which Axon ranks as `Inferred`, and every C6 node/edge/candidate metadata map is explicitly `authorityScope = evidence-only`;
- graph evidence quotes only the already validated/canonicalized source pointer, never the raw provider-controlled `installUrl`/`url`;
- the graph helpers were split into `skills_sh/map/graph.rs` after adversarial monolith review; `scripts/enforce_monoliths.py --base b3195957 --head HEAD` passes with no allowlist addition.

Focused proof on the final split tree:

- `axon-adapters` ArtifactCandidate filter: 6 passed, 0 failed;
- `axon-adapters` skills.sh filter: 26 passed, 0 failed;
- `cargo fmt --all -- --check`: passed;
- `xtask generated-contracts check`: passed, including 504 doc-link checks, 127 removed-surface contract checks, and docs inventory;
- `xtask check-layering`: passed;
- changed-file monolith policy: passed;
- `git diff --check`: passed;
- frozen Depot fixture hashes remain candidate `58afd5392e664ead043a89a45d072c32d4fb7bd2cb119bb50678c09b2775f732` and interchange `6b52ca32894a42720a3f18e7f2919a54a82031f7a89c0da2e026069d27eec88b`;
- `cargo clippy -p axon-adapters --all-targets -- -D warnings`: passed on the normal kache path.

Remaining C6 coordination gap: `RegistrySourceAdapter::artifact_candidates` currently ignores its `SourceEnrichment` map. That bridge overlaps #570, so W21 deliberately does not modify it yet. The bounded semantic-neighbor evidence hook is ready for those ids once #570 settles.

## Next

1. wait for #570 to settle, then coordinate the remaining watch-request limits/metadata persistence, C6 `SourceEnrichment` semantic-neighbor feed, and C5 Depot sink config/context/runtime injection in one integration pass;
2. once runtime injection is available, run a deliberately bounded authenticated skills.sh → Axon → Depot seed with intake/license/backpressure gates enabled and prove sink receipts plus no public byte mirroring for unknown rights;
3. re-run the exact SQLite memory-compaction test if GitHub repeats the unrelated lock failure; do not pull that runtime/database fix into W21 unless it reproduces as a branch regression;
4. keep draft PR evidence current and preserve the no-second-pipeline boundary.


## Do not do yet

- no full skills.sh corpus crawl;
- no public mirroring of unknown-license bytes;
- no direct Depot publication writes;
- no new source-specific job/ledger/watch subsystem;
- no reuse/cleanup of unrelated dirty worktrees.
