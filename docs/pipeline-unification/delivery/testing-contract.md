# Testing Contract
Last Modified: 2026-06-30

## Contract

The source-pipeline refactor is complete only when its contracts are executable
through tests. Tests must prove that CLI, MCP, REST, jobs, providers, source
adapters, ledger, graph, memory, artifacts, and vector storage all project the
same semantics.

Testing is not only unit coverage. It is contract coverage.

## Design Rules

- Every boundary has a fake or in-memory implementation where realistic.
- Every source adapter has fixtures and golden outputs.
- Every transport has parity tests against shared `axon-api` DTOs.
- Every async path has lifecycle, progress, retry, recovery, and cancellation
  tests.
- Every provider failure mode has graceful degradation or hard-fail tests.
- Every removed command/action/route is absent from final help/schema/OpenAPI
  and cannot dispatch.
- Every metadata field promoted to the contract has payload/ledger/status tests.
- Tests must be deterministic by default and opt into live providers explicitly.

## Test Tiers

| Tier | Name | Uses Live Services | Purpose |
|---|---|---:|---|
| 0 | static | no | schema, docs, config, route inventory, clap/MCP/OpenAPI shape |
| 1 | unit | no | pure functions, parsers, routers, metadata builders |
| 2 | boundary fake | no | provider traits with in-memory/fake implementations |
| 3 | integration local | optional local services | SQLite, filesystem artifacts, fake providers, local Qdrant optional |
| 4 | live smoke | yes | TEI/Qdrant/Chrome/LLM happy path and degradation checks |
| 5 | cutover | no/optional | fresh schema, reset behavior, canonical reindex path |

Default CI runs tiers 0-3. Live smoke runs behind explicit environment gates.

## Tier 5 Cutover Tests

Tier 5 is required before declaring the clean break complete.

Required cases:

- incompatible current SQLite schema blocks unified worker startup before side
  effects
- `axon reset --dry-run` reports exact SQLite tables, Qdrant collections,
  artifact roots, config files, and row/file counts
- `axon reset --yes` deletes selected stores, recreates fresh schema, and writes
  a reset receipt artifact
- OAuth/static-auth token cache is invalidated or re-auth guidance is surfaced
- removed config keys fail validation with the known replacement registry
- removed CLI commands are absent from help and parser
- removed MCP actions are absent from schema and cannot dispatch
- removed REST routes are absent from router/OpenAPI/generated clients
- old job-family tables are absent after reset
- old code-index generations are absent after reset
- old Qdrant payload shape is absent after reset/reindex
- canonical local repo source job indexes from an empty store
- canonical web/docs source job indexes from an empty store
- canonical ask/query retrieves from the new payload shape
- provider backpressure prevents bulk reindex from starving interactive query
- partial generation interrupted before publish is not searchable after restart

## Boundary Fake Requirements

| Boundary | Fake Required | Must Simulate |
|---|---:|---|
| `SourceResolver` | yes | canonicalization, ambiguous source, authority map miss |
| `SourceRouter` | yes | adapter selection, unsupported scope |
| `SourceAdapter` | yes | discovery, fetch success, partial failure, not modified |
| `LedgerStore` | yes | manifests, generations, leases, cleanup debt, stale lease |
| `GraphStore` | yes | node/edge upsert, evidence merge, conflict |
| `MemoryStore` | yes | remember/search/decay/reinforce/forget |
| `DocumentPreparer` | yes | chunk routing, fallback, parse facts |
| `EmbeddingProvider` | yes | batch success, rate limit, timeout, dimension mismatch |
| `VectorStore` | yes | upsert, delete, filter, hybrid query, partial failure |
| `RetrievalEngine` | yes | ranking, citation assembly, empty result |
| `LlmProvider` | yes | streaming, refusal/error, timeout, JSON schema invalid |
| `ArtifactStore` | yes | write/read, traversal rejection, retention |
| `JobStore` | yes | lifecycle, events, heartbeat, retry, recovery |
| `WatchStore` | yes | schedule, due run, coalescing, pause/resume |

Fakes must be strict. They should reject unsupported calls instead of silently
pretending to be production providers.

## Contract Test Matrix

Every canonical operation has parity tests across transports.

| Operation | CLI | MCP | REST | Shared Assertion |
|---|---:|---:|---:|---|
| source create | yes | yes | yes | same `SourceRequest`, same job/result |
| source refresh | yes | yes | yes | same source id and generation behavior |
| map | yes | yes | yes | same item candidates and no embedding |
| watch create | yes | yes | yes | same watch descriptor |
| watch exec | yes | yes | yes | same child job behavior |
| extract | yes | yes | yes | same schema validation and result |
| query | yes | yes | yes | same retrieval request/result |
| retrieve | yes | yes | yes | same source/document lookup |
| ask | yes | yes | yes | same retrieval trace and citations |
| memory remember | yes | yes | yes | same memory id and graph links |
| prune plan | yes | yes | yes | same selectors and dry-run result |
| jobs get/events/cancel/retry | yes | yes | yes | same job state transitions |

CLI human output is not byte-for-byte identical to MCP/REST. CLI `--json` must
match shared DTOs.

## Source Adapter Fixture Tests

Each adapter must provide:

- input examples
- resolved source examples
- capability examples
- manifest examples
- fetched item examples
- expected `SourceDocument`
- expected metadata fields
- expected graph candidates when supported
- expected degraded modes
- expected watch/refresh behavior

Required adapter fixture families:

| Family | Fixtures |
|---|---|
| web | page, docs subtree, sitemap, robots denied, thin page, redirect |
| local | file, directory, ignored files, binary, symlink policy |
| git/github | repo, branch, commit, PR, issue, private auth missing |
| registries | crates, npm, PyPI, Docker image metadata |
| feeds | RSS, Atom, JSON feed, broken entry |
| social | Reddit subreddit/thread, deleted/removed item |
| video | YouTube video/playlist/channel transcript missing |
| sessions | Claude/Codex/Gemini jsonl, tool calls, skills, agents |
| CLI tools | command help output, failing command, timeout |
| MCP tools | tool schema, successful call, tool error, auth missing |

## Pipeline Tests

Pipeline tests run against fake providers and assert stage order:

```text
SourceRequest
  -> resolve
  -> route
  -> acquire
  -> ledger diff
  -> parse/graph candidates
  -> prepare
  -> embedding batch reservation
  -> embed
  -> vector point build
  -> vector upsert
  -> publish generation
  -> cleanup debt
```

Required cases:

- first index of mutable source
- refresh with no changes
- refresh with added/modified/removed items
- partial item failure with allowed degradation
- required item failure
- provider unavailable before publish
- provider failure after some vector writes
- cancellation before publish
- cancellation after publish with cleanup debt
- stale lease recovery
- cleanup debt retry

## Job Tests

Required job lifecycle tests:

- queued to running to completed
- queued to canceled
- running to canceling to canceled
- running to failed
- running to completed_degraded
- stale heartbeat recovery creates new attempt
- retry keeps `job_id` and increments attempt
- child job failure aggregation
- event sequence monotonicity
- progress stream resumes from event cursor
- retention cleanup preserves required status summary
- worker panic records a failed attempt and releases leases
- bounded worker channels apply backpressure instead of unbounded memory growth
- heartbeat watchdog reports stalled interactive lanes
- recovery refuses to duplicate an already committed generation

Throughput/backpressure tests:

- bulk source embedding cannot starve interactive query embedding
- provider cooldown blocks new reservations
- watch jobs coalesce duplicate source refreshes
- vector writes are bounded separately from embedding requests
- LLM concurrency is independent of embedding concurrency
- queued provider reservations expire or cancel cleanly when jobs are canceled
- provider cooldown cancels queued background work without canceling safe
  cleanup/finalization

## Metadata and Payload Tests

For every source family, assert:

- required shared metadata fields exist
- source-specific fields use approved prefixes
- sensitive fields are absent or redacted
- `job_id`, `source_id`, `document_id`, `chunk_id`, and generation fields are
  consistent across ledger/status/vector/artifacts
- chunk locators map back to source content
- removed items produce cleanup debt and vector deletes

## Config Tests

Required:

- minimal `.env` boots with defaults
- secrets and URLs are accepted only in `.env`/environment
- tuning keys are accepted in `config.toml`
- unknown config keys fail with a clear path
- env overrides TOML only for allowed override keys
- generated examples are valid and not kitchen-sink dumps
- redaction hides secrets in doctor/status/debug

## Live Smoke Tests

Live tests are opt-in and must be skippable without failure.

Required smoke targets:

- Qdrant ready/unready
- TEI ready/unready
- Chrome ready/unready
- selected LLM backend ready/unready
- web page source job
- local repo source job
- ask over indexed content
- provider cooling after forced failure

Live smoke output must include service URLs, model names, collection name,
job ids, and degradation status with secrets redacted.

## Golden Files

Golden files are appropriate for:

- CLI help output
- CLI `--json` envelopes
- MCP tool schema
- OpenAPI route schemas
- adapter capability documents
- source resolution examples
- metadata payload examples
- error envelopes

Golden tests must provide an intentional update command and require review for
contract changes.

## Test Data Policy

- Keep fixtures small and committed.
- Large artifacts go through `ArtifactStore` test fixtures or generated temp
  files.
- Do not require private tokens for normal tests.
- Do not store secrets in fixtures.
- Use deterministic UUID/time providers in tests.
- Use fake embedding vectors with stable dimensions.

## Completion Gate

The refactor is not complete until:

- all target contracts have tests or explicit tracked exceptions
- old command/action/route behavior is removed and cannot dispatch
- transport parity tests pass
- job lifecycle tests pass
- provider backpressure tests pass
- metadata payload tests pass
- minimal config boot test passes
- live smoke passes in the intended deployment environment
