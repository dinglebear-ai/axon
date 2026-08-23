# Restored Source Surfaces Design

Date: 2026-08-23

Bead: `axon_rust-twfx7`

## Goal

Restore ergonomic `scrape`, `crawl`, `embed`, `ingest`, and `code-search`
operations across Axon's CLI, MCP tool, and REST API without restoring the
deleted command-specific execution pipelines. The unified source, job, ledger,
authorization, redaction, embedding, vector publication, and query services
remain authoritative.

The universal surfaces remain supported:

- CLI: `axon <source>` and `axon source <source>`
- MCP: `action=source` and `action=query`
- REST: `POST /v1/sources` and the existing query endpoint

The restored names are first-class, documented projections, not deprecated
aliases and not alternate service implementations.

## Public Semantics

| Operation | Canonical service projection | Fixed behavior | Focused caller controls |
|---|---|---|---|
| `scrape` | `SourceRequest` | `scope=page`, `intent=acquire`, at most one page/item, foreground by default | source, collection, refresh, output/inline, render options, priority |
| `crawl` | `SourceRequest` | `scope=site`, `intent=acquire`, embedding enabled by default | source, collection, refresh, wait/detach, crawl limits, adapter options, priority |
| `embed` | `SourceRequest` | embedding and vector publication forced on | source, collection, scope, refresh, wait/detach, limits, priority |
| `ingest` | `SourceRequest` | automatic adapter selection; embedding enabled by default | source, collection, scope, refresh, wait/detach, limits, `no_embed`, priority |
| `code-search` / `code_search` | canonical query request/service | `content_kind=code` | query, collection, source/path/language filters, limit, hybrid controls |

`embed` means “prepare and publish this target through the unified source
pipeline.” It does not restore a standalone embedding queue or direct TEI to
Qdrant path. `ingest` is the general named acquisition operation and may skip
embedding when explicitly requested. `code-search` is read-only retrieval; it
does not restore the historical indexing or watch engine.

Invalid fixed-option overrides fail at parsing/deserialization instead of being
silently ignored. For example, `crawl --scope page` is not accepted because
`crawl` does not expose a scope flag.

## Shared Projection Contracts

Add transport-neutral projection functions alongside the source DTOs in
`axon-api`, in a focused source projection module. They accept narrow request
DTOs and return canonical requests:

```text
ScrapeRequest     -> SourceRequest(scope=page, max_pages=1, max_items=1)
CrawlRequest      -> SourceRequest(scope=site)
EmbedRequest      -> SourceRequest(embed=true)
IngestRequest     -> SourceRequest(embed=!no_embed)
CodeSearchRequest -> QueryRequest(content_kind=code)
```

The DTOs own defaults and validation shared by all three transports. Each
focused request carries a non-empty typed input list rather than having separate
singular and batch shapes. Mutating source projections use
`Vec<SourceProjectionInput>`, whose items contain the input string and an
optional caller-supplied idempotency key. Read-only `code-search` uses
`Vec<QueryProjectionInput>` and intentionally has no idempotency field. CLI
accepts one or more inputs and maps them into the same typed list used by MCP and
REST. Duplicate source strings remain distinct ordered items unless their
idempotency keys cause the canonical job layer to reuse work. This prevents the
transports from independently encoding cardinality, correlation, scope,
embedding, or filter semantics.

Projection returns a canonical batch envelope containing one ordered result per
input, even when the request contains one input. Per-input success or failure is
preserved so one failed target does not discard successful siblings. The shared
batch executor owns ordering, concurrency bounds, detached job creation, and
aggregate status; transports only render or serialize the returned envelope.

The reusable API contracts are generic:

```text
BatchRequest<I, P> {
  inputs: Vec<I>,
  options: P
}

BatchResult<T> {
  status: BatchStatus,
  items: Vec<BatchItem<T>>,
  summary: BatchSummary
}

BatchItem<T> {
  index: usize,
  input: Option<String>,
  status: LifecycleStatus,
  result: Option<T>,
  error: Option<ApiError>
}

SourceProjectionInput {
  input: String,
  idempotency_key: Option<String>
}

QueryProjectionInput {
  input: String
}
```

`BatchResult<SourceResult>` and `BatchResult<QueryResult>` are the only batch
envelopes; the transports must not introduce parallel batch response types.
`BatchItem.input` is present for synchronous initiating-caller results and
omitted from detached job-descriptor responses under the disclosure rule below.

Every accepted request receives one opaque `batch_id`. The batch envelope,
per-item job metadata, structured events, traces, and safe logs carry that ID so
operators can correlate work without parsing inputs. This is correlation
metadata on canonical jobs and events, not a new job kind or independent batch
queue. Detached responses include the `batch_id` plus every ordered per-item job
ID. Initial cancellation remains per item through the canonical jobs surface;
the correlation contract leaves room for future grouped cancellation without
fabricating it in this change.

Correlation survives restart. The owning `axon-jobs` migration adds nullable
`batch_id` columns to canonical `jobs` and `job_events`, plus an index supporting
batch-filtered lifecycle lookup. Event constructors copy the job's batch ID;
observe projections carry the same field through their existing metadata
contract. The generated database schema and job/event DTO references own and
verify these additions. No batch table, batch state machine, or batch job kind
is introduced.

Idempotency applies only to mutating `scrape`, `crawl`, `embed`, and `ingest`
items. Keys are scoped by `(operation, authenticated caller identity,
idempotency_key)`. Repeating an identical normalized request within the
canonical idempotency-retention window reuses the existing item result or job.
Reusing a key with a different normalized payload returns `409 Conflict` during
preflight and executes nothing. Loopback callers use a stable loopback-system
identity. The job layer, not individual transports, owns key lookup and request
fingerprinting. `code-search` is read-only and neither accepts nor stores an
idempotency key.

Existing `SourceRequest`, `SourceResult`, `QueryRequest`, and query-result DTOs
remain the service boundary and response shape. No new domain service trait,
job kind, SQLite table, adapter, ledger path, or vector write path is added.

## Batch Policy and Preflight

Batch execution is transport-neutral and resource bounded. Add typed tuning
under the existing `[server]` configuration section, with generated TOML and
environment documentation:

- `projection-batch-max-inputs`
- `projection-batch-max-concurrency`
- `projection-batch-max-expanded-items`

Defaults must be conservative and identical for CLI, MCP, and REST. A caller
cannot raise server-configured ceilings through request fields. Input count is
checked before route resolution. The expanded-item budget is shared across the
batch and consumed by discovered crawl pages, prepared documents, or query
results as appropriate; reaching it stops scheduling further expansion and
produces structured per-item limit outcomes rather than unbounded work.

The aggregate expanded-item ceiling is converted during preflight into
deterministic per-item limits that are persisted in each canonical request/job.
Each input receives `floor(max_expanded_items / input_count)` units, and the
first `max_expanded_items % input_count` ordered inputs receive one additional
unit. A ceiling smaller than the input count is rejected before execution.
Unused units are not dynamically redistributed. Crawl projections apply the
allocation to page/item discovery limits; embed/ingest apply it to prepared
document/item limits; code-search applies it to result limits. This makes the
aggregate ceiling enforceable after restart without an in-memory batch
coordinator.

Batch admission does not create a private concurrency lane. Every item still
passes through existing scheduler claims and provider reservations, including
provider cooldown and global worker limits. Batch concurrency is an additional
upper bound, never a replacement for scheduler admission, so a large request
cannot monopolize TEI, Chrome, Qdrant, or source workers.

Validation, route resolution, and authorization are a whole-batch preflight.
Every item is projected, validated, resolved, and authorization-checked before
any acquisition, embedding, vector publication, query, or durable job enqueue
begins. Any request-shape, fixed-option, route, or authorization failure rejects
the entire batch with no partial writes. Provider and runtime failures after a
successful preflight remain per-item outcomes so successful siblings are not
discarded.

Preflight is side-effect free. It may parse, normalize, classify, resolve local
configuration, inspect caller scopes, and consult idempotency metadata. It must
not fetch remote content, invoke Chrome, clone repositories, execute tools,
create artifacts, reserve providers, enqueue jobs, or write ledger/vector state.

Aggregate transport status is deterministic:

- malformed, over-limit, unrouteable, or unauthorized preflight: request-level
  `4xx`, with no `BatchResult` execution payload;
- idempotency-key collision with a different normalized payload: request-level
  `409`, with no execution;
- completed inline batch, including mixed runtime outcomes: `200` with ordered
  `BatchResult` items;
- successfully accepted detached batch: `202` with an ordered job descriptor
  for every item;
- batch-wide internal failure before item execution: mapped canonical `5xx`;
- operational failures after execution begins: `200` with failed per-item
  statuses and structured errors.

Detached admission is atomic. After preflight succeeds, one `axon-jobs`
transaction inserts every item job, its persisted projection limits,
idempotency metadata, and shared `batch_id`; any insert or constraint failure
rolls back the entire transaction. A `202` response is emitted only after that
transaction commits. The transport must never loop over single-job enqueue and
return a partially admitted batch.

## CLI Design

The current `scrape` projection is retained and migrated to the shared
projection helper. Add real clap variants and `CommandKind` values for `crawl`,
`embed`, `ingest`, and `code-search` in:

- `crates/axon-core/src/config/cli.rs`
- `crates/axon-core/src/config/parse/build_config/command_dispatch.rs`
- `crates/axon-core/src/config/types/enums.rs`
- the adjacent sidecar tests

Source-shaped commands dispatch through the existing CLI source executor and
renderer in `crates/axon-cli/src/commands/source.rs`. Small command modules may
own request construction, but must call the shared projection functions and
`run_source_request`; they must not acquire, parse, embed, publish, or manage
jobs themselves. Each command accepts one or more positional inputs, using the
same shared batch request and result contracts as MCP and REST. `code-search`
builds the shared code-search projection and uses the existing query
command/service renderer for every ordered query result.

CLI artifact output is overwrite-safe. A one-input request may use
`--output FILE`. A request containing multiple inputs rejects `--output` and
requires either `--output-dir DIR` or an explicit filename template containing
an item discriminator such as `{index}` or `{input_hash}`. Paths are normalized
and collision-checked for the whole batch before execution. JSON written to
stdout remains one batch envelope; progress and diagnostics remain on stderr.

Per-item idempotency uses repeatable paired CLI groups:

```text
axon crawl --input URL_A --idempotency-key KEY_A \
           --input URL_B --idempotency-key KEY_B
```

Each `--idempotency-key` binds to the immediately preceding `--input`. Bare
positional inputs remain supported when keys are unnecessary, but keyed and
positional forms cannot be mixed in one invocation. A leading key, two keys for
one input, an unpaired input in keyed mode, or any idempotency option on
`code-search` is a parse error before configuration or service initialization.

Help and completions describe each operation in task language. The removed
command registries and negative parser tests are updated so these five names
are no longer rejected, while genuinely removed surfaces remain rejected.

## MCP Design

Extend `axon_api::mcp_schema::AxonRequest` with first-class variants:

- `Scrape(ScrapeRequest)`
- `Crawl(CrawlRequest)`
- `Embed(EmbedRequest)`
- `Ingest(IngestRequest)`
- `CodeSearch(CodeSearchRequest)` serialized as `action=code_search`

Remove these names from `removed_action_guidance`. Register them in
`MCP_ACTION_SPECS`, schema/help metadata, task metadata, and dispatch.

The four source actions share one MCP execution helper extracted from the
current source handler. That helper receives the shared non-empty input list,
projects each item to a canonical `SourceRequest`, and delegates to the common
batch executor while preserving:

- collection validation;
- detached versus inline execution;
- caller `AuthSnapshot` propagation;
- per-target `SafetyClass` resolution;
- `axon:local` and `axon:execute` fine-grained scope enforcement;
- response-mode and artifact behavior.

`code_search` delegates to the existing query handler/service after applying
the fixed code-content filter. It is registered as `axon:read`; the four
source projections are `axon:write` and retain the additional per-target
authorization checks.

MCP request handling completes whole-batch scope preflight before it creates a
service context with mutating capability or enqueues any work. This prevents an
unauthorized later item from leaving earlier items partially indexed.

## REST Design

Add focused endpoints alongside `POST /v1/sources`:

```text
POST /v1/scrape
POST /v1/crawl
POST /v1/embed
POST /v1/ingest
POST /v1/code-search
```

Each endpoint accepts the same narrow one-or-many DTO used by CLI and MCP and
returns the shared ordered batch envelope. Source endpoints project their body
and call a shared internal source-handler function extracted from
`crates/axon-web/src/server/handlers/sources.rs`; they do not issue an internal
HTTP request or duplicate runtime orchestration. `code-search` similarly calls
the query handler/service seam for each projected request through the shared
batch executor.

Routes are mounted and documented through the existing router, schema registry,
and OpenAPI generator. `/v1/scrape`, `/v1/crawl`, `/v1/embed`, and `/v1/ingest`
use the same broad write route policy as `/v1/sources`, followed by the same
per-target authorization boundary. `/v1/code-search` uses the query/read policy.
Loopback behavior remains identical to the canonical endpoints. The loopback
mutation guard must classify the four source endpoints exactly as it classifies
`/v1/sources`.

Every focused endpoint declares an explicit stable OpenAPI operation ID:
`scrapeSources`, `crawlSources`, `embedSources`, `ingestSources`, and
`codeSearch`. Generated clients and drift tests treat these identifiers as
owned public contract inputs rather than generator-derived names.

REST handlers complete whole-batch validation and per-target authorization
before calling the executor. They must not stream execution while preflight is
still evaluating later inputs.

## Compatibility and Errors

This is an additive contract change. Universal source/query callers continue to
work unchanged. Restored calls return the same ordered batch contract through
CLI, MCP, and REST, containing canonical per-input result DTOs, job descriptors,
warnings, and structured errors. MCP and REST never reject batching merely
because of their transport.

The shared REST/MCP contract version advances to `2026-08-23`. Version constants,
capability documents, schema resources, OpenAPI, and contract tests change in
the same commit as the new public surfaces. This version change describes an
additive capability; it does not remove the universal source/query contracts.

## Observability

The shared executor emits structured `accepted`, `started`, and `completed`
batch events carrying `batch_id`, operation, input count, scheduled count,
success/failure/canceled counts, duration, and limit exhaustion. Per-item events
carry `batch_id` and stable input index alongside existing job/source IDs.

Metrics and ordinary logs never include raw input strings, query text,
credentials, headers, local paths, or caller idempotency keys. Diagnostics use
the batch ID, item index, existing opaque source/job IDs, or a boundary-approved
redacted hash. Existing shared redaction and artifact boundaries remain
mandatory.

The initiating authenticated caller receives its original raw input string in
the synchronous `BatchItem.input` response so ordered results remain usable.
That value is not copied into shared events, metrics, ordinary logs, artifact
names, traces, or capability documents. Detached responses use input index and
opaque job/source IDs instead of echoing raw inputs; callers already possess the
submitted request and correlate it through stable ordering and `batch_id`.

Projection validation errors identify the focused operation and field. Provider,
authorization, routing, redaction, job, and publication failures pass through
the canonical service error mapping without translation into legacy error
types. No removed database or configuration keys are revived.

Current docs that state these operations are removed must be changed to describe
them as supported projections. Historical delivery documents remain historical;
living CLAUDE.md files, generated command/schema references, MCP help, REST
OpenAPI, and client types must reflect the restored surface.

## Executable Contract First

Implementation begins with an executable contract slice before any CLI, MCP, or
REST handler is wired. The contract has one handwritten source of truth in
`axon-api` and generated projections everywhere else; no transport owns a
parallel operation definition.

Add focused, modern-layout modules under `crates/axon-api/src/` for:

- the typed projection input/options/result DTOs;
- pure projection and validation rules;
- a canonical operation registry describing operation name, transport spelling,
  fixed fields, accepted caller fields, scope, mutation class, batch support,
  idempotency support, result type, and contract version.

The registry generates these owned artifacts:

- `docs/reference/sources/projections.json` — machine-readable operation
  contract;
- `docs/reference/sources/projections.md` — rendered human reference;
- CLI command/help registry entries;
- MCP action/schema/help metadata;
- REST schema-registry/OpenAPI operation metadata;
- generated client operation names and request/result type references.

Add canonical fixtures under `tests/fixtures/source-projections/` covering all
five operations:

- minimal and fully populated valid requests;
- fixed defaults and forbidden override failures;
- single and batch projection outputs;
- source versus query idempotency behavior;
- batch limits, deterministic budget allocation, and stable ordering;
- request-level validation/auth/idempotency errors;
- inline, detached, mixed-runtime, and disclosure-aware result envelopes.

Cross-surface tests feed equivalent CLI arguments, MCP JSON, and REST JSON into
their parsers and compare the resulting canonical requests byte-for-byte after
normalization. Golden result tests compare the same typed `BatchResult<T>`
serialization for MCP and REST and the equivalent CLI JSON output. A transport
cannot add a field, default, alias, status rule, or action spelling that is not
declared by the registry.

The contract slice is complete only when its DTO/projection unit tests, registry
validation, fixtures, generator tests, and `cargo xtask generated-contracts
check` pass without any restored transport handler. Subsequent CLI, MCP, and
REST implementation phases consume that established contract.

## Generated Contracts and Documentation

Implementation updates all owning inputs and regenerates their projections:

- CLI command registry/help/completions;
- MCP action enum, enriched tool schema, golden schema, help, and capabilities;
- REST schema registry and OpenAPI;
- API DTO/schema references and generated web/Palette client types where owned;
- public API surface and current CLI/MCP/REST reference docs;
- negative removed-surface fixtures and tests.

Run `cargo xtask generated-contracts refresh` only after all schema inputs are
complete, then use `cargo xtask generated-contracts check` for drift proof.

## Testing and Verification

Development follows sidecar-test conventions and proves projection behavior
before transport wiring.

1. Complete the executable-contract slice and its registry, fixtures, generated
   JSON/Markdown, cross-surface normalization harness, and drift checks before
   adding restored transport dispatch.
2. Unit tests for every narrow DTO and projection, including fixed defaults,
   caller controls, non-empty one-or-many inputs, stable ordering, partial
   failure, duplicate inputs, source-only per-item idempotency keys, generic
   envelope serialization, raw-input disclosure boundaries, unknown-field
   rejection, and forbidden overrides.
3. CLI parser/dispatch tests and request-projection tests for all five names;
   help and generated registry assertions prove their visibility.
4. MCP parsing, schema, action registry, scope, task metadata, dispatch, and
   result-shape tests. Explicit tests prove local/tool source targets cannot
   bypass fine-grained scopes through any restored source action, and that one
   denied item prevents every sibling from executing.
5. REST router/OpenAPI/request tests for all five routes, including mounted-auth,
   loopback, validation, write/read policy, detached `202`, and canonical result
   shapes. Test request-level `4xx`, completed `200` with mixed runtime results,
   and accepted detached `202` behavior.
6. Cross-surface contract tests prove equivalent single and batch CLI/MCP/REST
   requests project to the same ordered canonical requests and batch envelope.
7. Batch-policy tests prove input ceilings, bounded concurrency, shared expanded
   work-budget partition/restart persistence, scheduler/provider-reservation
   fairness, stable input indexes, durable batch/job/event correlation,
   cancellation metadata, atomic all-or-nothing detached enqueue, source
   idempotent reuse, `409` collisions, code-search idempotency rejection, and
   deterministic aggregate status.
8. Layering tests prove restored CLI, MCP, and REST modules cannot directly
   import adapters, embedding, vectors, ledger internals, job-store
   implementations, or acquisition clients. Only API DTO/projection and service
   facade imports are permitted.
9. Preflight tests prove denied or invalid later items cause no earlier enqueue,
   acquisition, artifact, provider reservation, ledger write, or vector write.
10. CLI output tests cover one-input files, multi-input directory/templates,
   collision rejection, keyed-input pairing and malformed pairs, positional/keyed
   mixing rejection, stdout JSON, and stderr-only progress. OpenAPI drift and
   generated-client tests pin all five operation IDs.
11. Observability tests prove batch lifecycle/count fields are emitted and raw
    inputs, queries, paths, headers, and idempotency keys are absent from shared
    telemetry while synchronous initiating-caller results retain ordered inputs.
12. Targeted formatting, layering, generated-contract, CLI, MCP, web, and API
   tests, widening to the repository pre-PR gate because code, schemas, generated
   artifacts, auth routing, and public contracts all change.

Runtime smoke proof should invoke one safe page scrape, one bounded crawl, one
small local/text embed or ingest target, and one code search against a configured
test runtime. Hosted CI and deployed-runtime verification remain separate
closeout boundaries.

## Non-Goals

- Restoring deleted `axon-crawl`, `axon-extract`, or `axon-ingest` crates.
- Adding command-specific job kinds, tables, workers, retries, ledgers, or vector
  publication code.
- Restoring `code-search-watch` or the historical code-search indexing engine.
- Removing or weakening `axon <source>`, `source`, `query`, or `/v1/sources`.
- Preserving every historical flag or response format from before issue #298.
- Adding compatibility aliases whose accepted fields differ from the documented
  focused DTOs.
