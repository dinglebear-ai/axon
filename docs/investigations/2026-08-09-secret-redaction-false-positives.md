---
title: "Secret-redaction false-positive investigation"
created: 2026-08-09
updated: 2026-08-11
---

# Secret-redaction false-positive investigation

Date: 2026-08-09
Branch: `codex/secret-redaction-investigation`
Bead: `axon_rust-lfcab`

## Executive finding

Axon is treating secret-related *vocabulary and syntax* as proof that a value
is a secret. This is a detector-precision bug, not evidence that the GoFastMCP
crawl contained live credentials.

Four partially overlapping implementations disagreed at the investigation
baseline:

1. `axon_core::redact::redact_secrets` replaces matching spans before parsing
   and chunking.
2. `DefaultRedactor` replaces an entire free-text value when any shared value
   detector matches.
3. The vector payload validator leaves `chunk_text` untouched, then drops the
   entire chunk when the broad shared value detector matches it.
4. CLI-tool and MCP-tool adapters maintain additional ad hoc substring lists.

The written contract requires contextual detection: a bearer *header/value*,
dotenv assignments with *secret-key classification*, and entropy only as a
secondary signal with key/path context. The implementation instead treats the
bare phrase `bearer `, every nonempty uppercase `KEY=value` line, and some
high-entropy runs without key/path context as secrets.

## Production evidence

The affected source job was:

- job: `f11bed5b-a524-4a51-b30a-e4048d5d9bc8`
- source: `src_38bb96f3814aa5ba`
- generation: `gen_6`
- canonical URI: `https://gofastmcp.com/`
- final status: `completed_degraded`

Ledger and Qdrant evidence:

- 539 documents prepared
- 7,092 chunks prepared and embedded
- 6,909 vector points written
- 183 chunks omitted, exactly matching the 183 warning log entries
- omissions were reported over 16 vector batches
- 206 of the 539 source items also emitted
  `document.content.pre_chunk_redacted`
- six authorization-related source items have no points in generation 6:
  `integrations/eunomia-authorization`,
  `python-sdk/fastmcp-utilities-authorization`,
  `servers/authorization`, and the corresponding available v2/v3 paths
- `jobs.warnings_json` is still `[]`; the skip counts exist only in job events
  and logs

This investigation queried Qdrant with payload fields limited to source,
document, and chunk identity. It did not print chunk bodies or credential
values.

## Reproduction

An isolated page scrape used a fresh `/tmp/axon-redaction-probe.*` data root,
`--no-embed`, and no production collection writes:

```text
axon scrape https://gofastmcp.com/servers/authorization \
  --no-embed --render-mode http --wait true --json
```

The page completed degraded with both:

- `parse.unsupported` for `Html`
- `document.content.pre_chunk_redacted`

Replaying the prepared content through the real `DocumentPreparer` and a
sanitized detector probe produced two final-validator rejections. Both were
caused solely by the fragment `bearer ` in public authorization documentation.
No credential value was printed or required to reproduce the rejection.

The same page became 537 chunks because the HTTP result retained a very large
HTML/application payload on a parser-unsupported path. That is a separate web
normalization/chunk amplification problem. It increases the number of detector
opportunities, but it does not cause the detector false positive.

## Root causes

### 1. Forbidden value fragments lack value context

`FORBIDDEN_VALUE_FRAGMENTS` includes `bearer `, `authorization:`, `cookie:`,
and assignment markers as plain case-insensitive substrings. The vector body
validator applies them to all of `chunk_text`.

Consequences include rejecting narrative text such as “Bearer authentication”
and documentation examples even when no credential follows the phrase.

This conflicts with the contract's required bearer detector:
case-insensitive `authorization: bearer <token>` header/value detection.

### 2. Dotenv detection classifies shape, not secret keys

`raw_dotenv_assignment` returns true for every nonempty uppercase assignment.
It does not call `secret_like_field_name` or otherwise classify the key.

Therefore ordinary configuration examples such as `PORT=3000`, `DEBUG=true`,
or `HOST=127.0.0.1` are treated exactly like a credential assignment.

This conflicts with the contract's requirement for dotenv parsing *with
secret-key classification*.

### 3. Pre-chunk and final policies are inconsistent

The pre-chunk pass uses `redact_secrets`, while the final vector pass uses
`value_contains_secret` plus vector-specific checks. A value can survive the
first pass and then trigger a hard drop in the second. That is exactly what the
GoFastMCP `bearer ` reproduction demonstrates.

The pre-chunk pass also applies a high-entropy fallback without key/path
context, while the contract says entropy is secondary and must have key/path
context. This can silently replace hashes, identifiers, or documentation
fixtures before parsing and embedding.

### 4. Whole-value tombstoning causes collateral data loss

`DefaultRedactor::redact_text` returns one `[REDACTED]` placeholder for the
entire input if any detector matches. That behavior reaches durable memory
bodies/titles, job-event messages, transport responses, CLI JSON, graph
evidence, artifact metadata, upload metadata, and vector metadata.

A long memory or event message that merely discusses bearer authentication or
contains a benign uppercase assignment can therefore be replaced in full.

### 5. Tool adapters bypass the shared policy

The CLI-tool adapter replaces a whole line for substrings including
`authorization`, `Bearer `, `secret`, `password`, and `token=`. The MCP-tool
adapter has a similar but non-identical list and can replace entire JSON string
values. These lists are broader than credential detection and drift from the
shared contract.

### 6. Observability suppresses the decisive information

`VectorPayloadValidationError::ForbiddenValue` carries the rejected field,
but `VectorPointBatchBuilder` destructures it with `{ .. }` and logs only the
chunk ID. The detector name is not carried by that error at all.

As a result, operators cannot distinguish body text, metadata, a bearer rule,
a dotenv rule, or a bare-token rule without recreating the document. The job's
aggregate `warnings_json` also remains empty despite `completed_degraded`.

## Blast radius

The broad shared detectors affect:

- document preparation and parse/graph inputs
- vector inclusion and retrieval completeness
- graph evidence labels/properties
- memory remember, lifecycle, and compaction writes
- job-event messages and details
- observability event persistence
- CLI JSON and MCP/REST response redaction
- artifact, upload, extract, reset, and prune metadata
- CLI-tool and MCP-tool source ingestion
- provider/error logging paths

The result is not limited to missing vectors. Depending on the surface, Axon
can drop a chunk, replace a full string, replace a span, drop a metadata field,
or degrade a job for the same input.

## Test gaps

Current tests prove positive detection and fail-closed behavior but omit the
negative fixtures that would have caught this incident:

- narrative “Bearer authentication” with no credential
- `Authorization` as a topic/title rather than a header field
- benign dotenv assignments (`PORT`, `DEBUG`, `HOST`, feature flags)
- redacted/example placeholders such as `${TOKEN}` or `<token>`
- public documentation code blocks
- high-entropy non-secret hashes and identifiers
- mixed long text where one suspicious span must not erase the whole value
- parity across document, vector, memory, event, CLI-tool, and MCP-tool surfaces

The targeted existing suites pass (`axon-core` redaction: 73 tests;
`axon-vectors` payload: 45 tests), confirming that the current aggressive
behavior is encoded by tests rather than being an uncovered runtime branch.

## Recommended correction sequence

1. Add cross-surface negative fixtures first, including the exact GoFastMCP
   narrative and benign dotenv shapes.
2. Replace substring lists with typed detector results carrying a safe rule ID,
   confidence, and location; never carry the matched value into logs.
3. Require header structure and a credential-shaped value for bearer/cookie
   detection. A scheme word or topic name alone must not match.
4. Parse dotenv lines and require a secret-like key. Treat documented
   placeholders separately from concrete credential-shaped values.
5. Remove context-free entropy redaction. Keep entropy only behind a
   secret-like key/path context.
6. Make pre-chunk and final vector validation consume the same detector model.
   The final fail-closed guard remains, but it should catch real unredacted
   secret material, not vocabulary.
7. Replace the CLI/MCP adapter lists with the shared structured detector.
8. Preserve the rejected field and safe detector ID in warnings, attach the
   source item key, and propagate aggregate warnings to `jobs.warnings_json`.
9. Separately correct web normalization so parser-unsupported HTML/application
   payloads are not exploded into hundreds of redundant chunks, and avoid
   indexing both canonical and `.md` documentation representations when they
   are equivalent.
10. Reingest the affected GoFastMCP source after deployment and verify prepared
    chunks equal published vector points unless a deliberately real-secret
    fixture is present.

## Safety invariant to retain

Fail closed when a detector has high-confidence evidence of real credential
material or when redaction itself fails. The correction should narrow what
constitutes evidence; it should not permit known token formats, private keys,
credential-bearing URLs, or actual auth header values into Qdrant or durable
public surfaces.

## Implemented correction

The investigation branch now applies one contextual detector policy across
document preparation, structured public writes, vector validation, and the
CLI/MCP source adapters:

- auth and cookie rules require a header-shaped field with a concrete value;
  narrative authentication prose and documented placeholders are preserved
- dotenv syntax is secret-bearing only when its key is secret-like
- credential-bearing URL detection requires valid URL-userinfo characters and
  preserves documented placeholders and serialized-code lookalikes
- URI schemes such as `secret://` and symbolic values such as
  `token=jwt_access_token` are not treated as concrete assignments
- context-free entropy redaction is removed; entropy remains a secondary
  signal under a secret-like structured key/path
- span redaction preserves surrounding text instead of tombstoning the entire
  value
- vector rejection warnings carry only safe detector IDs, rejected field
  names, source item keys, and counts; persisted terminal job warnings are
  derived from already-redacted job events
- HTML article chunking excludes script, style, template, noscript, SVG, and
  canvas payloads before windowing
- `Docs` discovery prefers an explicit `.md` route when the same discovered
  route also exists without the extension

The final vector guard remains fail-closed for known token formats, private
keys, concrete authorization/cookie values, credential-bearing URLs, and
secret-like assignments.

## Corrected production-page probe

The same captured GoFastMCP authorization HTML was replayed through the final
`DocumentPreparer` and contextual detector without embedding or Qdrant writes:

```text
probe summary: uri=https://gofastmcp.com/servers/authorization chunks=12 content_bytes=23526 rejected=0
```

The corrected HTML lane continues to use the HTML article projection for
large documents, removes executable/style/template payloads, coalesces DOM
text nodes into bounded windows, and anchors lossy projected chunks to the
full raw source range. The original unsupported/fallback lane produced 537
chunks for this page; the corrected lane produces 12 and none trigger secret
validation.
