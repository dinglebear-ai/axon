# W21 Artifact Crawl/Index/Enrichment Implementation Plan

Status: active
Date: 2026-08-19
Worktree: /home/jmagar/workspace/axon-w21-artifact-engine
Branch: codex/w21-artifact-engine-20260819
Draft PR: #569

## Goal

Add ArtifactCandidate discovery/enrichment delivery to Axon's existing SourceRequest pipeline so Axon can seed and incrementally refresh Depot/Bazaar without becoming an artifact publication authority or regressing RAG behavior.

## Non-negotiable constraints

- no second crawler/job/ledger pipeline;
- Depot remains publication authority;
- candidate contract remains byte-free;
- unknown/restricted license state never enables public byte mirroring;
- structured/API discovery preferred over HTML scraping;
- bounded pagination, rate limits, concurrency, candidate batches, retries, and buffers;
- existing SourceDocument/RAG behavior preserved and differentially tested;
- no massive seed until intake/license/backpressure gates are proven.

## Checkpoints

### C0: repository isolation and governing contract

- [x] inspect all Axon worktrees/branches and leave dirty trees untouched;
- [x] fetch current origin/main and create fresh isolated W21 worktree;
- [x] confirm no pre-existing open PR owns this branch/lane;
- [x] read source-pipeline/ledger/adapter/service contracts;
- [x] read Phoenix ADR-0023 + Depot/Axon/schemas/licensing/meta-plan;
- [x] verify official skills.sh API/Terms and current ARD/APM docs.

### C1: candidate contract and provider boundary

- [x] introduce the initial typed evidence/sink seam and open draft PR #569;
- [x] project the Depot W20-owned shared `dinglebear.artifact-candidate/v1` payload in axon-api rather than defining a competing Axon candidate schema;
- [x] match the neutral camelCase field set and W20 G0 bounds/secret-safety rules;
- [x] add a copied neutral JSON fixture and exact serialize/deserialize parity tests;
- [x] keep source/job/generation/item correlation in shared crawl fields, bounded evidence metadata, and the separate Axon batch wrapper;
- [x] keep exact identity + content-aware SHA-256 dedupe helpers outside the shared top-level payload;
- [x] preserve a fail-closed public-byte guard over neutral license evidence;
- [x] define the separate versioned Axon ArtifactCandidateBatch/sink capability/result transport contract;
- [x] register ArtifactCandidate as a generated API schema root and refresh generated contract snapshots;
- [x] identify and fix PR #569 rust-contracts root cause; ci-gate was only the downstream gate failure.

Depot W20 G0 is frozen at `25de725`. Axon pins byte-identical copies of its canonical ArtifactCandidate and ArtifactInterchange v1 fixtures and treats the shared v1 field set as frozen. Evidence fields follow the G0 optional/default semantics while canonical serialization emits the full 18-field candidate shape.

### C2: shared pipeline wiring

- [x] inject ArtifactCandidateSink into ServiceContext with no-op default;
- [x] add a default no-candidate adapter/provider hook so existing adapters compile unchanged;
- [x] produce candidates beside normalized changed SourceDocuments from the same generation;
- [x] redact the complete candidate at Axon's public-write boundary before validation/delivery;
- [x] buffer candidate output until source generation commit, preventing failed-generation ghost intake;
- [x] submit only bounded candidate batches (hard ceiling 64, respecting smaller sink limits);
- [x] require sink wrapper-version support and idempotency capability; validate receipt accounting;
- [x] preserve candidate receipt/warnings in source execution evidence without altering committed vector/RAG semantics;
- [x] ensure sink failure policy is explicit and optional/degraded by default;
- [x] prove unchanged refresh does not replay candidate delivery;
- [x] prove existing SourceRequest pipeline characterization remains unchanged with default/no-op candidate behavior.

### C3: skills.sh structured discovery

- [x] add a structured skills.sh adapter/path inside SourceRequest routing;
- [x] bounded leaderboard/search discovery with configurable page/result ceilings;
- [x] project rate-limit `Retry-After` as a capped provider-scope retry hint without adapter-local sleep/retry loops;
- [x] resolve `source`/`installUrl` to a canonical repo/base URL while leaving ref/path unset unless exact evidence exists;
- [x] stable manifest item keys + hashes for ledger diffing;
- [x] emit SourceDocument metadata and ArtifactCandidate evidence from the same item;
- [x] preserve installs/sourceType/duplicate signals as aggregator evidence;
- [x] add opt-in, hard-bounded audit-signal enrichment with zero default N+1 expansion and fail-soft fan-out stop;
- [x] do not call the detail/files endpoint or copy third-party file bytes into candidate delivery when license is unknown;
- [x] deterministic mock/server tests for pagination, search, 429/Retry-After, 503, and hard limits;
- [x] prove provider-future cancellation stops without hidden page fan-out.

### C4: incremental refresh/watch

- [x] use existing SourceManifest/SourceManifestDiff/SourceGeneration lifecycle;
- [x] unchanged entries avoid candidate resubmission;
- [x] added/modified entries generate candidates;
- [x] removed entries produce reconciliation evidence rather than authoritative deletion;
- [x] existing watch execution reconstructs the persisted source/options/scope/embed/collection selection and applies only execution-time refresh/wait/reason overrides;
- [ ] persist/replay the remaining full SourceRequest fields, notably limits/metadata, after #570 settles rather than duplicating or conflicting with its watch/source hardening;
- [x] differential refresh test proves changed-only work.

### C5: Depot sink

- [x] inspect current Depot intake/API operations before choosing endpoint shape;
- [ ] wait for Depot's bounded ArtifactCandidate intake ledger/operation to be implemented and frozen; do not substitute existing authoritative Skill/repository ingest APIs;
- [ ] implement versioned serialized HTTP sink using Axon's existing HTTP/provider infrastructure once that intake contract exists;
- [ ] bounded batch size + max in-flight delivery;
- [ ] idempotency/delivery keys;
- [ ] auth kept out of candidate envelope;
- [ ] classify 2xx/4xx/429/5xx with bounded retry/degradation semantics;
- [ ] cross-repo JSON fixture contract with Depot once intake endpoint is frozen.

### C6: semantic/graph enrichment hooks

- [ ] attach exact/near-duplicate evidence without redefining exact dedupe identity;
- [ ] add graph candidates for source/repo/artifact relationships;
- [ ] semantic and graph signals remain non-authoritative evidence;
- [ ] no graph/enrichment failure may grant publication or rights.

### C7: bounded seed and hardening

- [ ] run a deliberately small skills.sh seed with intake/license/backpressure gates enabled;
- [ ] prove candidate count, source/doc counts, sink receipts, and no public byte mirroring for unknown rights;
- [ ] run fmt + generated-contract check;
- [ ] clippy/warnings-as-errors on changed crates;
- [ ] focused unit/integration/differential/concurrency tests;
- [ ] layering check;
- [ ] adversarial review and resolve all findings;
- [ ] update docs/progress/PR evidence;
- [ ] leave PR draft only while unresolved gates remain.

## Test matrix

Minimum focused matrix before bounded seed:

| Area | Required proof |
|---|---|
| contract | exact neutral fixture round trip, `dinglebear.artifact-candidate/v1`, old Axon-field rejection, shared bounds, byte/authority-free shape |
| license | unknown/restricted/index-only fail closed; redistributable/forkable only permissive states |
| dedupe | deterministic, delimiter-safe, content changes preserve identity but change content key |
| sink | disabled/no-op, batch ceiling, idempotent retry, partial/rejected responses |
| skills.sh | page/total caps, 429/Retry-After, canonical pointer resolution, duplicate evidence, opt-in audit cap/404/fail-soft stop/trust-boundary validation |
| ledger | initial add, unchanged refresh, modified refresh, removal/reconciliation |
| RAG differential | same existing SourceDocument/chunk/vector results when candidate sink disabled |
| concurrency | bounded discovery and sink in-flight counts under delayed providers |
| security | redacted metadata, no token leakage, no raw byte field, SSRF policy inherited from source acquisition |

## Commit discipline

Each checkpoint is committed and pushed independently. The draft PR is updated after meaningful evidence changes. Long repository hooks that exceed LABBY Code Mode's wall-clock envelope are run explicitly as bounded validation commands; hooks may be skipped for the mechanical commit/push only after equivalent checks are recorded.
