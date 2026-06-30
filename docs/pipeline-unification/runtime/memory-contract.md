# Memory Contract
Last Modified: 2026-06-30

## Contract

Memory is a first-class durable knowledge subsystem. It is not a source adapter
and not a generic note table.

`axon-memory` owns memory lifecycle, scoring, decay, review, compaction,
reinforcement, supersession, contradiction handling, and durable memory
metadata. It coordinates with `VectorStore` for semantic recall and
`SourceGraph` for relationships.

Memory uses shared pipeline components where appropriate:

```text
MemoryRequest
  -> MemoryService
  -> MemoryStore
  -> SourceDocument(memory://...)
  -> DocumentPreparer
  -> EmbeddingProvider
  -> VectorStore
  -> SourceGraph
  -> MemoryResult
```

Memory must not masquerade as a source adapter. Source ingestion indexes
external material; memory stores durable assertions, preferences, decisions,
tasks, incidents, procedures, and working context.

## Ownership Boundary

| Area | `axon-memory` Owns | Other Boundary Owns |
|---|---|---|
| memory records | type, status, body, scope, score, decay, history | SQLite physical storage through `MemoryStore` |
| semantic recall | memory query policy, ranking blend, reinforcement signals | embeddings and vector search through providers |
| graph links | memory-to-source/entity/decision relationships | graph persistence through `GraphStore` |
| context assembly | token budget, exclusions, ordering, redaction | LLM prompt use by `AskService`/other callers |
| review lifecycle | review queues, contradictions, decay prompts | UI rendering by CLI/REST/MCP/app surfaces |
| compaction | distillation rules and source-memory status changes | LLM provider when synthesis is needed |

Memory must not own:

- source acquisition
- adapter routing
- source generations
- Qdrant collection schema outside memory payload fields
- graph persistence internals
- app-specific UI state

## Required Crate Shape

```text
crates/axon-memory/src/
  lib.rs
  memory.rs
  request.rs
  result.rs
  store.rs
  score.rs
  decay.rs
  review.rs
  reinforce.rs
  supersede.rs
  contradict.rs
  compact.rs
  context.rs
  graph.rs
  vector.rs
  redaction.rs
  testing.rs
```

Required public service:

```rust
#[async_trait]
pub trait MemoryService: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult>;
    async fn get(&self, request: MemoryGetRequest) -> Result<MemoryDetail>;
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult>;
    async fn reinforce(&self, request: MemoryReinforceRequest) -> Result<MemoryResult>;
    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult>;
    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult>;
    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult>;
    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult>;
    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult>;
    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult>;
    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult>;
}
```

## Memory Types

Memory types are closed enum values.

| Type | Meaning | Default Decay |
|---|---|---|
| `decision` | Durable design/implementation decision. | slow |
| `fact` | Observed factual project/system state. | normal |
| `preference` | User preference or standing instruction. | very slow |
| `task` | Work item or pending follow-up. | normal |
| `bug` | Known defect or failure pattern. | normal |
| `procedure` | Repeatable operational procedure. | slow |
| `incident` | Specific outage/failure/investigation. | normal |
| `entity` | Stable entity profile such as repo/service/person/package. | slow |
| `episode` | Session or event summary. | fast |
| `working` | Short-lived working context. | very fast |

Type rules:

- `preference` and `decision` require higher confidence before automatic use
- `working` memories are excluded from long-term exports by default
- `task` memories can transition to archived/completed states
- `incident` memories should link to affected sources, jobs, logs, or issues
- `entity` memories should mirror into SourceGraph when possible

## Memory Status

| Status | Meaning |
|---|---|
| `active` | Eligible for recall. |
| `review` | Needs user/agent confirmation. |
| `superseded` | Replaced by another memory. |
| `contradicted` | Conflicts with another memory and needs resolution. |
| `archived` | Hidden from normal recall but retained. |
| `forgotten` | Removed from recall and redacted/deleted according to policy. |
| `working` | Temporary memory with short TTL. |

Status rules:

- `forgotten` memories are excluded from vector and context results
- `archived` memories are excluded unless `include_archived=true`
- `superseded` memories point to the replacement memory
- `contradicted` memories appear in review queues
- status changes append memory history events

## Memory DTOs

Required DTO fields:

| DTO | Required Fields | Optional Fields |
|---|---|---|
| `MemoryRequest` | `type`, `body`, `confidence`, `salience`, `scope` | `title`, `tags`, `links`, `decay`, `embed`, `visibility` |
| `MemoryResult` | `memory_id`, `type`, `status`, `memory_score`, `confidence`, `salience`, `created_at`, `updated_at` | `graph_node_id`, `document_id`, `vector_point_ids`, `warnings` |
| `MemoryRecord` | `memory_id`, `type`, `status`, `body`, `confidence`, `salience`, `scope`, `history` | `title`, `links`, `decay`, `embedding_refs` |
| `MemorySearchRequest` | `query`, `limit` | `filters`, `include_graph`, `include_archived`, `reinforce` |
| `MemorySearchResult` | `results`, `query_embedding_model` | `graph`, `warnings` |
| `MemoryContextRequest` | `token_budget` | `query`, `source_id`, `graph_node_id`, `filters`, `depth`, `include_working` |
| `MemoryContextResult` | `context`, `memories`, `exclusions`, `token_estimate` | `warnings` |
| `MemoryReviewRequest` | none | `reason`, `type`, `scope`, `limit`, `cursor` |
| `MemoryCompactRequest` | `memory_ids`, `strategy`, `result_type` | `archive_sources`, `instructions` |

All DTOs live in `axon-api`; `axon-memory` implements behavior.

## Scope Model

Memory scope controls recall and visibility.

| Scope Kind | Scope Value |
|---|---|
| `global` | empty or `global` |
| `project` | project key/name |
| `repo` | canonical repo URI or graph node id |
| `file` | source id plus file/item key |
| `source_id` | source id |
| `graph_node_id` | graph node id |
| `agent` | agent id/name |
| `user` | user id |
| `environment` | host/service/environment id |

Scope rules:

- narrower scopes rank higher when the query context matches them
- global memories need higher confidence/salience to rank above scoped memories
- source/repo/file scopes should create graph links when possible
- scope changes update graph mirror links

## Scoring and Recall

`memory_score` is computed, not caller supplied.

Required score inputs:

| Input | Meaning |
|---|---|
| semantic score | vector similarity between query and memory body/context |
| confidence | belief that the memory is true/useful |
| salience | importance assigned at creation or review |
| recency | time since creation/update/reinforcement |
| scope match | relevance of memory scope to caller context |
| reinforcement | prior successful use signals |
| decay | type/scope-specific decay curve |
| contradiction penalty | unresolved conflicts lower rank |
| archive/status penalty | archived/superseded/forgotten exclusion rules |

Recall rules:

- forgotten memories never return
- superseded memories return only when explicitly requested
- contradicted memories return only with warning unless resolved
- review memories may return with lower confidence and warning
- pinned memories have a minimum score floor but still respect auth/redaction
- memory search records access signals only when the caller requests
  reinforcement or the service policy explicitly enables it

## Decay Contract

Decay is explicit and inspectable.

Required fields:

| Field | Meaning |
|---|---|
| `decay_profile` | `very_fast`, `fast`, `normal`, `slow`, `very_slow`, `none`. |
| `half_life_days` | Effective half-life. |
| `last_reinforced_at` | Last positive use signal. |
| `reinforcement_count` | Count of positive signals. |
| `review_after` | Timestamp when memory should be reviewed. |
| `expires_at` | Optional expiration. |

Decay rules:

- user preferences default to very slow decay
- working memories default to very fast decay or explicit TTL
- pinned memories can disable decay while pinned
- review can reset decay, change type, archive, supersede, or forget
- decay never silently deletes memory; deletion uses forget/prune policy

## Graph Integration

Every memory may mirror into SourceGraph.

Required node/edge behavior:

| Memory Operation | Graph Behavior |
|---|---|
| remember | create/update `memory` node |
| link | create evidence-backed edge to source/repo/file/issue/pr/entity |
| supersede | create `supersedes` edge |
| contradict | create `contradicts` edge |
| compact | create new memory and `derived_from` edges to source memories |
| forget | remove recall edges or mark graph node redacted according to policy |

Graph evidence includes memory id, job/request id, caller, timestamp, and
reason. Memory graph links must not claim source authority unless evidence
supports it.

## Vector Integration

Memory body/content is embedded through the same `EmbeddingProvider` and
`VectorStore` boundaries.

Rules:

- memory vector payloads use `memory_*` metadata fields
- memory vectors include `memory_id`, type, status, scope, confidence, salience,
  and redaction status
- status changes that affect recall update or delete vector payloads
- body changes create new content hash and vector points
- memory vectors may use a dedicated memory collection or shared collection
  namespace, but the choice is configured through `VectorStore`

## Context Assembly

`MemoryContextRequest` builds bounded context for ask/research/session flows.

Rules:

- context respects token budget
- memories are sorted by score, scope relevance, and diversity
- context excludes forgotten, superseded, archived, and unauthorized memories by
  default
- context includes citations/ids for every memory fragment
- sensitive memories are redacted or omitted according to caller auth
- exclusions list why memories were omitted: budget, auth, status, low score,
  contradiction, redaction

## Import and Export

Memory import/export is portable but policy controlled.

Rules:

- export writes an artifact or stream with redacted content according to caller
  scope
- import supports dry-run plans
- import deduplicates by content hash, scope, type, and source metadata
- replace-scope import requires admin/write scope
- imported memories are marked with provenance and may enter review state

## Observability

Memory operations emit standard progress/events when durable or async.

Required log/event fields:

- `memory_id`
- `memory_type`
- `memory_status`
- `memory_scope_kind`
- `job_id` or `request_id`
- `phase`
- `severity`
- `visibility`
- `score_before` and `score_after` for scoring changes
- `review_reason` when applicable

Memory-specific phases:

```text
remembering
embedding
linking
reviewing
reinforcing
compacting
forgetting
```

If these phases are exposed publicly, they must be added to the canonical
`PipelinePhase` enum or represented as operation detail under an existing phase.

## Security and Redaction

Memory must:

- redact secrets before embedding
- classify every memory by visibility
- avoid storing raw auth headers, tokens, cookies, private env values, or secret
  prompts
- support forget/archive/supersede without leaving recallable stale vectors
- respect caller scopes for search/context/export
- keep history/provenance internal unless caller can view it

Memory must not:

- bypass `RedactionProvider`
- expose hidden memories through graph edges
- use archived/forgotten memories in ask context by default
- treat agent-generated memory as user-approved preference without review

## REST, CLI, and MCP Surface

Memory surfaces are grouped but map to the same DTOs.

Required operations:

| Operation | CLI | MCP | REST |
|---|---|---|---|
| remember | `axon memory remember` | `memory/remember` | `POST /v1/memories` |
| search | `axon memory search` | `memory/search` | `POST /v1/memories/search` |
| context | `axon memory context` | `memory/context` | `POST /v1/memories/context` |
| show | `axon memory show` | `memory/show` | `GET /v1/memories/{memory_id}` |
| link | `axon memory link` | `memory/link` | `POST /v1/memories/{memory_id}/links` |
| supersede | `axon memory supersede` | `memory/supersede` | `POST /v1/memories/{memory_id}/supersede` |
| reinforce | `axon memory reinforce` | `memory/reinforce` | `POST /v1/memories/{memory_id}/reinforce` |
| contradict | `axon memory contradict` | `memory/contradict` | `POST /v1/memories/{memory_id}/contradict` |
| pin | `axon memory pin` | `memory/pin` | `POST /v1/memories/{memory_id}/pin` |
| archive | `axon memory archive` | `memory/archive` | `POST /v1/memories/{memory_id}/archive` |
| forget | `axon memory forget` | `memory/forget` | `DELETE /v1/memories/{memory_id}` |
| review | `axon memory review` | `memory/review` | `GET /v1/memories/review` |
| compact | `axon memory compact` | `memory/compact` | `POST /v1/memories/compact` |

## Testing Contract

Required fixtures:

```text
crates/axon-memory/fixtures/remember/decision.valid.json
crates/axon-memory/fixtures/remember/preference.valid.json
crates/axon-memory/fixtures/search/query.valid.json
crates/axon-memory/fixtures/context/budget.valid.json
crates/axon-memory/fixtures/review/contradiction.valid.json
crates/axon-memory/fixtures/compact/compact.valid.json
crates/axon-vectors/tests/fixtures/payload/memory.valid.json
crates/axon-graph/fixtures/memory-links.valid.json
```

Required tests:

- remember creates memory row, vector payload, and graph node
- search excludes forgotten/superseded/archived memories by default
- context respects token budget and reports exclusions
- reinforcement changes score and history
- decay changes score predictably over time
- supersede hides old memory and links replacement
- contradiction sends both memories to review
- compact creates new memory and archives source memories when requested
- forget removes recallable vectors and graph recall edges
- redaction blocks secret memory body from embedding
- REST/CLI/MCP operations map to the same DTOs

## Acceptance Criteria

- memory has a dedicated `axon-memory` crate boundary
- memory DTOs live in `axon-api`
- memory metadata validates against `metadata-payload.md`
- memory vector payloads validate against vector payload schema
- memory graph nodes/edges validate against source graph schema
- memory recall respects status, scope, decay, confidence, salience,
  reinforcement, auth, and redaction
- memory context is bounded, cited, and explainable
- memory import/export is artifact-backed and policy controlled
- all memory surfaces use the same service and DTOs
