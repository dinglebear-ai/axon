# ArtifactCandidate Contract Integration

Status: active W21 integration of Depot W20 frozen G0 contract (`25de725`)
Last reviewed: 2026-08-20

## Ownership

The individual ArtifactCandidate payload is a shared neutral cross-repo contract frozen by Depot W20 at commit `25de725`. Axon consumes that G0 contract and does not define a competing candidate domain schema. Any field-set or semantic change to shared v1 requires a new shared version.

Shared payload identifier:

- dinglebear.artifact-candidate/v1

Axon Rust projection:

- crates/axon-api/src/source/artifact_candidate.rs

Copied frozen G0 fixtures:

- `crates/axon-api/tests/fixtures/artifact-registry/artifact-candidate-v1.json` copied byte-for-byte from Depot `25de725`;
- `crates/axon-api/tests/fixtures/artifact-registry/artifact-interchange-v1.json` copied byte-for-byte from Depot `25de725`;
- `crates/axon-api/tests/fixtures/schema/artifact_candidate.v1.neutral.json` is an identical candidate copy used by Axon's generated-schema fixture validator.

Axon sink/batch boundary:

- crates/axon-adapters/src/artifact_candidates.rs

The copied fixture is contract data only. Axon does not depend on or copy Depot implementation code.

## Shared ArtifactCandidate payload

Every candidate carries schemaVersion=dinglebear.artifact-candidate/v1 and serializes with exactly these top-level JSON names:

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

The Rust DTO uses serde camelCase projection and denies unknown top-level fields. Parity tests load the frozen `25de725` candidate fixture, deserialize it, validate the shared bounds, serialize it again, and require both exact JSON-value equality and exact canonical JSON byte content.

The previous Axon-only top-level fields contractVersion, candidateId, jobId, sourceId, generation, sourceItemKey, dedupe, provenance, license, enrichmentEvidence, and observedMetadata are explicitly rejected by tests.

## Correlation

The shared candidate exposes crawlGenerationId and crawlJobId. The Axon batch wrapper additionally carries source_id/job_id/generation for delivery-level correlation.

Axon's source_item_key is not promoted into the neutral top-level schema. Artifact-aware Axon adapters place it in bounded manifestMetadata.axonSourceItemKey, which the shared executor validates against the normalized changed-document batch before accepting a candidate for later delivery.

## Evidence placement

Axon-specific evidence must fit the shared fields instead of growing the payload:

- canonical source pointer: canonicalSourceUri, repository, ref, sourcePath;
- exact content hashes: contentDigests;
- identity/content dedupe helper keys: bounded manifestMetadata;
- skills.sh discovery/audit/duplicate metadata: discoveryEvidence;
- install/popularity counters: popularitySignals;
- LICENSE/NOTICE/redistribution observations: licenseEvidence;
- observed file/path/digest records: observedFiles;
- semantic/graph/compatibility signals: bounded discovery/manifest evidence plus Axon's independent GraphCandidate path; semantic neighbor ids sourced from `SourceEnrichment` remain sorted, deduplicated, capped evidence.

None of these fields is authoritative publication state.

## Shared bounds

The Axon projection validates the effective W20 G0 bounds before delivery:

- total candidate JSON <= 262,144 bytes;
- id <= 160 bytes and lowercase id syntax;
- canonicalSourceUri <= 4,096 bytes, no embedded credentials, blocked URI schemes rejected;
- sourceProvider/kindHints <= 64 bytes each and lowercase kind syntax;
- kindHints <= 32 entries;
- repository/ref <= 512 bytes;
- sourcePath <= 4,096 bytes and path-safe;
- observedFiles/manifestMetadata/discoveryEvidence/licenseEvidence <= 65,536 JSON bytes each;
- popularitySignals <= 32,768 JSON bytes;
- contentDigests use sha256:<64 lowercase hex> syntax;
- crawlGenerationId/crawlJobId <= 256 bytes;
- warnings <= 128 entries and <= 1,024 bytes each;
- nested JSON depth <= 8, maps <= 128 entries, lists <= 256 entries, strings <= 16,384 bytes;
- secret-like metadata keys are rejected.

## Authority and byte safety

The candidate payload has no publication, owner, revision authority, authoritative license, raw byte, archive, or bundle field.

licenseEvidence remains evidence. Axon's public-byte guard permits only explicit redistributable/forkable evidence and fails closed for unknown, restricted, metadata_only, cache_for_index, missing, or unrecognized state. Depot may apply stricter policy after intake.

Candidate acceptance does not create a published Artifact revision and does not grant authorization.

## Exact dedupe helper

Axon computes length-prefixed SHA-256 identity/content helper keys:

- identity key: canonical source URI + ref + source path;
- content key: identity inputs + observed content digest.

These are Axon production helpers, not shared top-level candidate fields. The shared-compatible candidate id uses cand_<hex> derived from the selected digest, while the raw helper keys may be carried in manifestMetadata.

## Axon ArtifactCandidateBatch transport wrapper

The batch wrapper is a separate Axon transport contract, not the shared candidate domain schema.

Current wrapper identity:

- contract_version: 1
- media type: application/vnd.dinglebear.axon.artifact-candidates+json;version=1

Wrapper fields:

- contract_version
- delivery_id
- idempotency_key
- job_id
- source_id
- generation
- produced_at
- candidates[]

Every candidates[] entry independently uses schemaVersion=dinglebear.artifact-candidate/v1. Sink capabilities negotiate wrapper versions. The wrapper supplies bounded delivery/idempotency/correlation only and does not make Axon authoritative for candidate normalization or publication.

## Compatibility rules

- shared payload drift is tested with a copied neutral JSON fixture;
- Axon-generated API schema snapshots must be refreshed whenever the Rust projection changes;
- unsupported batch wrapper versions fail explicitly;
- unsupported candidate schemaVersion values are rejected before delivery;
- cross-repo fixture updates follow the G0 contract owner and are copied as open contract fixtures, never imported through Depot implementation dependencies.

## Security rules

- redaction/secret-key rejection happens before candidate delivery;
- credentials/tokens are never evidence values;
- candidates cannot grant invocation/deployment authorization;
- candidate acceptance is not publication;
- unknown/restricted licensing cannot trigger public byte mirroring;
- removed/changed observations are reconciled through source generation semantics rather than direct destructive control over Depot.
