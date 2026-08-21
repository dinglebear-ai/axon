---
title: "Artifact Candidate Pipeline"
created: 2026-08-19
updated: 2026-08-19
---

# Artifact Candidate Pipeline

Status: active W21 implementation
Last reviewed: 2026-08-19

## Purpose

Axon is the scalable crawl/index/enrichment engine that discovers artifact-shaped resources for Depot/Bazaar. Axon emits typed evidence. It does not publish Artifact revisions, decide hosted visibility, or become the artifact authority.

This slice extends the existing unified SourceRequest pipeline. It does not introduce a second crawler or a parallel job/ledger lifecycle.

## Governing boundaries

The cross-repo authority model is defined by Phoenix ADR-0023 and the accompanying Depot/Axon/licensing/schema documents:

- Axon owns broad acquisition, refresh/watch, semantic indexing, graph enrichment, and duplicate signals.
- Depot/Bazaar owns hosted artifact intake, normalization, catalog/publication state, distribution, collaboration, and curation.
- Labby owns the open personal/local Artifact + MCP/runtime gateway.
- Discovery evidence never grants authorization or redistribution rights.

Depot W20 froze the shared neutral Artifact payload contracts at commit `25de725`. Axon must consume/emit those exact v1 contracts rather than defining a competing candidate domain schema; shared v1 fields are frozen.

## Pipeline placement

~~~text
SourceRequest
  -> resolve / route
  -> adapter materialize + discover
  -> ledger manifest diff + generation
  -> acquire changed items
  -> source enrichment
  -> SourceDocument[]
  |  -> shared ArtifactCandidate[]   (buffered sibling evidence output)
  -> parse / prepare / embed
  -> vector publish + commit source generation
  -> durable candidate outbox     (staged before commit, eligible after commit)
  -> Axon ArtifactCandidateBatch  (optional bounded transport wrapper)
  -> background ArtifactCandidateSink drain
  -> graph
  -> cleanup
  -> SourceResult
~~~

The candidate path is an output branch from the same source generation. It uses the same existing job/source/generation lifecycle and must not bypass discovery, diffing, leases, cancellation, or publication gates. Candidate sink failures are optional/degraded evidence failures and must not silently change existing RAG publication semantics.

Candidates are buffered during changed-item processing and durably staged before publication. They become eligible for the background Depot drain only after their Axon source generation commits; failed generations remove their staged intent. Accepted/disabled deliveries are retired idempotently, while failed deliveries remain available for a later drain. A failed generation therefore cannot leak ghost candidates into the hosted registry, and a process interruption after commit does not erase delivery intent.

## Shared neutral candidate payload

Each serialized candidate uses the shared schema identifier:

- schemaVersion: dinglebear.artifact-candidate/v1

The frozen open-contract copies live at:

- `crates/axon-api/tests/fixtures/artifact-registry/artifact-candidate-v1.json`;
- `crates/axon-api/tests/fixtures/artifact-registry/artifact-interchange-v1.json`.

The generated-schema validator also consumes an identical candidate copy at `crates/axon-api/tests/fixtures/schema/artifact_candidate.v1.neutral.json`.

The top-level field set is intentionally identical to the W20 G0 contract:

- schemaVersion
- id
- canonicalSourceUri
- sourceProvider
- observedAt
- repository
- ref
- sourcePath
- kindHints
- observedFiles
- manifestMetadata
- contentDigests
- discoveryEvidence
- popularitySignals
- licenseEvidence
- crawlGenerationId
- crawlJobId
- warnings

Axon-specific information that is not part of the neutral contract must live either in the Axon batch wrapper or inside an allowed bounded evidence map. W21 currently reserves manifestMetadata.axonSourceItemKey for correlation back to the normalized SourceDocument item. Exact dedupe keys may also be placed in bounded manifest metadata rather than becoming new top-level candidate fields.

The candidate payload is byte-free and contains no publication, owner, revision authority, authoritative license, or authorization field.

## Bounds and safety parity

Axon validates candidate output against the W20 G0 effective limits before delivery:

- whole candidate JSON: 262,144 bytes maximum;
- canonicalSourceUri: 4,096 bytes, credential-free, blocked schemes rejected;
- candidate id: 160 bytes and lowercase id syntax;
- sourceProvider/kindHints: lowercase kind syntax, 64 bytes each;
- kindHints: at most 32 entries;
- repository/ref: 512 bytes each;
- sourcePath: safe relative slash path, at most 4,096 bytes;
- observedFiles/manifestMetadata/discoveryEvidence/licenseEvidence: at most 65,536 JSON bytes each;
- popularitySignals: at most 32,768 JSON bytes;
- contentDigests: SHA-256 digest syntax and the shared list bound;
- crawlGenerationId/crawlJobId: 256 bytes each;
- warnings: at most 128 entries, 1,024 bytes each;
- nested JSON: depth/map/list/string bounds aligned with W20 validation;
- secret-like metadata keys are rejected before sink delivery.

Fixture round-trip tests reject the previous Axon-only candidate fields so accidental schema drift fails loudly.

## Axon transport batch wrapper

The candidate domain payload and the Axon transport wrapper are separate contracts.

ArtifactCandidateBatch is Axon-specific and currently uses:

- contract_version: 1
- media type: application/vnd.dinglebear.axon.artifact-candidates+json;version=1
- delivery_id
- idempotency_key
- job_id
- source_id
- generation
- produced_at
- candidates[]

The wrapper supplies Axon source-level delivery correlation and bounded batching. It does not change the schema of any candidate inside candidates[]. Sink capabilities negotiate the batch wrapper version, not ownership of the neutral candidate payload.

## Dedupe model

Axon maintains two SHA-256 helper keys while producing candidates:

1. identity_key hashes canonical source URI + ref + source path. It remains stable when content changes at the same canonical source identity.
2. content_key additionally hashes the observed content digest. It changes when bytes change.

Inputs are length-prefixed before hashing to prevent delimiter aliases. The neutral candidate id is a shared-compatible cand_<hex> value derived from the selected digest. The raw SHA-256 identity/content keys may be carried in bounded manifestMetadata for Depot dedupe evidence. Near-duplicate/semantic clustering is evidence and never replaces exact source/content identity.

## License and mirroring policy

licenseEvidence is shared neutral evidence, not an Axon publication state machine. Axon only applies a conservative local byte-mirroring guard:

- redistributable or forkable evidence may pass the Axon-side public-byte gate;
- unknown, restricted, metadata_only, cache_for_index, missing, or unrecognized evidence fails closed.

Depot may always apply stricter policy. Discovery and candidate acceptance never upgrade publication or redistribution rights.

The source repository/ref/path is the authority to inspect for LICENSE/NOTICE evidence. Aggregators such as skills.sh remain discovery evidence only.

## skills.sh structured discovery

Verified against the official skills.sh API and Terms on 2026-08-19.

Preferred input is the structured API, not HTML crawling:

- GET /api/v1/skills for bounded catalog pagination;
- GET /api/v1/skills/search for bounded targeted discovery;
- curated/detail/audit endpoints only when required by the configured seed mode;
- preserve source, sourceType, installUrl, installs, duplicate, and audit signals as third-party evidence;
- resolve installUrl/source into a canonical repository/base URL and source path before candidate construction;
- do not use detail-file contents as a redistribution shortcut when repository license state is unknown.

Operational rules:

- configurable page/result ceilings and hard global candidate limits;
- respect published rate-limit and Retry-After headers;
- cache/reuse structured discovery where appropriate;
- bounded concurrency with cancellation/backpressure;
- seed mode defaults small and must not silently expand to the full catalog.

Official references:

- https://skills.sh/docs/api
- https://skills.sh/terms

## Incremental refresh and watch

skills.sh/public-catalog discovery uses the normal SourceManifest + SourceManifestDiff + SourceGeneration lifecycle. Stable source item keys and manifest content hashes make unchanged entries reusable. Watches resubmit the same SourceRequest through the existing watch/job scheduler; they do not own a second refresh store.

Only added/modified candidates are submitted by default. Removed observations are reconciled as source-generation evidence for Depot intake policy. Axon does not directly delete authoritative Depot artifacts.

## Depot sink

Depot integration receives the Axon ArtifactCandidateBatch wrapper whose candidates each conform to dinglebear.artifact-candidate/v1.

The sink must:

- negotiate/advertise supported Axon batch versions;
- enforce a finite max batch size;
- transmit delivery/idempotency keys as `X-Axon-Delivery-Id` and
  `Idempotency-Key` headers;
- keep network retry/backoff bounded;
- surface partial/rejected receipts without manufacturing publication state;
- never send credentials inside candidate metadata/evidence.

Depot remains free to reject, merge, normalize, enrich, review, or publish a candidate independently.

## Semantic and graph enrichment

Axon may attach semantic similarity, exact/near-duplicate clusters, ecosystem relationships, dependency/source links, and graph evidence inside the shared bounded evidence maps and its own GraphCandidate path. These remain evidence. They do not mutate Depot publication authority and they do not grant permissions.

## Backpressure and seed gates

A full-corpus seed is forbidden until all of these are tested:

- shared payload/fixture parity;
- intake/sink batch limits;
- license/public-byte fail-closed behavior;
- source API pagination limits and Retry-After handling;
- bounded discovery/acquisition concurrency;
- ledger incremental-diff behavior;
- sink idempotency/partial-failure semantics;
- cancellation and job correlation;
- no-regression differential tests for existing SourceDocument/RAG output.

The first live proof is a deliberately bounded skills.sh seed. Larger 1k/5k/10k/25k/100k capacity gates are follow-up scale exercises, not this initial proof.

## Upstream ecosystem notes

ARD remains a discovery/publication adapter, not a runtime or authority. Its ai-catalog schema is time-sensitive and must be reverified before an adapter ships.

APM remains a compiler/resolver/package/distribution adapter. Its registry behavior is time-sensitive/partly experimental and must not become Axon's canonical artifact model.
