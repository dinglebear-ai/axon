# Graph Schema Contract
Last Modified: 2026-06-30

## Contract

Graph schemas define SourceGraph node kinds, edge kinds, evidence records,
stable keys, merge rules, and parser-emitted graph candidates.

## Generated Artifacts

```text
docs/reference/sources/graph.schema.json
docs/reference/sources/graph.md
```

Generator:

```bash
cargo xtask schemas graph
cargo xtask schemas graph --check
```

## Source Inputs

The graph schema generator reads:

```text
crates/axon-graph/src/**
crates/axon-parse/src/graph*.rs
crates/axon-api/src/graph.rs
docs/pipeline-unification/sources/source-graph.md
docs/pipeline-unification/sources/parsing-contract.md
```

The generated artifact records these paths in `x-axon.source_inputs`.

## Required Schemas

- `GraphNode`
- `GraphEdge`
- `GraphEvidence`
- `GraphCandidate`
- `GraphKindRegistry`
- `GraphMergeRule`
- `GraphNodeKind`
- `GraphEdgeKind`
- `GraphResolveRequest`
- `GraphQueryRequest`
- `GraphQueryResult`

## Graph Node Schema

```json
{
  "type": "object",
  "required": ["node_id", "kind", "stable_key", "label", "properties", "created_at"],
  "properties": {
    "node_id": { "type": "string", "pattern": "^node_" },
    "kind": { "$ref": "#/$defs/GraphNodeKind" },
    "stable_key": { "type": "string" },
    "label": { "type": "string" },
    "properties": { "type": "object", "additionalProperties": true },
    "authority": { "$ref": "#/$defs/AuthorityLevel" },
    "created_at": { "type": "string", "format": "date-time" },
    "updated_at": { "type": "string", "format": "date-time" }
  },
  "additionalProperties": false
}
```

## Graph Edge Schema

```json
{
  "type": "object",
  "required": ["edge_id", "kind", "from_node_id", "to_node_id", "evidence"],
  "properties": {
    "edge_id": { "type": "string", "pattern": "^edge_" },
    "kind": { "$ref": "#/$defs/GraphEdgeKind" },
    "from_node_id": { "type": "string" },
    "to_node_id": { "type": "string" },
    "properties": { "type": "object", "additionalProperties": true },
    "evidence": {
      "type": "array",
      "items": { "$ref": "#/$defs/GraphEvidence" }
    },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}
```

## Kind Registry Shape

Every node/edge kind includes:

- kind name
- description
- required properties
- optional properties
- allowed evidence kinds
- parser families that may emit it
- merge key algorithm
- examples

Generated registry shape:

```json
{
  "kind": "repository",
  "kind_type": "node",
  "description": "Source repository.",
  "stable_key_template": "repo:{provider}:{owner}/{repo}",
  "required_properties": ["provider", "owner", "repo"],
  "optional_properties": ["default_branch", "homepage", "description"],
  "allowed_evidence_kinds": ["git_remote", "github_api", "package_metadata"],
  "parser_families": ["git", "package_manifest", "session"],
  "merge": {
    "strategy": "stable_key",
    "conflict_policy": "keep_highest_authority_with_evidence",
    "confidence_floor": 0.5
  },
  "fixtures": [
    "crates/axon-graph/tests/fixtures/schema/repository.valid.json"
  ]
}
```

Kind rules:

- every kind has a stable key template
- every required property has a JSON type
- every edge kind declares allowed from/to node kind sets
- graph store rejects unknown kinds before write
- parser-emitted candidates validate before merge

## Required Kind Families

- source/repository/package/docs/site
- file/module/symbol/function/class/trait
- dependency/package/version
- API endpoint/schema/operation
- Docker service/image/network/volume
- session/turn/tool_call/skill/agent/decision
- issue/pull_request/artifact

## Required Node Kinds

| Kind | Stable Key |
|---|---|
| `source` | `source:{source_id}` |
| `site` | `site:{normalized_host}` |
| `docs_site` | `docs:{canonical_uri}` |
| `repository` | `repo:{provider}:{owner}/{repo}` |
| `local_checkout` | `local:{source_id}` |
| `package` | `pkg:{ecosystem}:{name}` |
| `package_version` | `pkg:{ecosystem}:{name}@{version}` |
| `file` | `file:{source_id}:{source_item_key}` |
| `module` | `module:{language}:{path_or_name}` |
| `symbol` | `symbol:{source_id}:{path}:{symbol_kind}:{name}` |
| `api_endpoint` | `api:{method}:{canonical_uri}` |
| `api_schema` | `schema:{name}:{hash}` |
| `docker_service` | `compose:{source_id}:{service}` |
| `session` | `session:{session_id}` |
| `session_turn` | `turn:{session_id}:{turn_index}` |
| `tool_call` | `tool:{session_id}:{call_id}` |
| `skill` | `skill:{name}:{version_or_path}` |
| `agent` | `agent:{name_or_id}` |
| `decision` | `decision:{session_id}:{hash}` |
| `issue` | `issue:{provider}:{owner}/{repo}#{number}` |
| `pull_request` | `pr:{provider}:{owner}/{repo}#{number}` |
| `artifact` | `artifact:{artifact_id}` |

## Required Edge Kinds

| Kind | From | To |
|---|---|---|
| `documents` | `source` | `docs_site`/`site` |
| `repository_for` | `repository` | `package`/`docs_site` |
| `depends_on` | `repository`/`package_version` | `package` |
| `contains` | `repository`/`file`/`module` | `file`/`module`/`symbol` |
| `defines` | `file` | `symbol` |
| `imports` | `file`/`module` | `package`/`module` |
| `exposes_endpoint` | `source`/`repository` | `api_endpoint` |
| `uses_schema` | `api_endpoint` | `api_schema` |
| `compose_service_uses_image` | `docker_service` | `package` |
| `session_contains_turn` | `session` | `session_turn` |
| `turn_invoked_tool` | `session_turn` | `tool_call` |
| `tool_invoked_skill` | `tool_call` | `skill` |
| `agent_participated` | `session` | `agent` |
| `decision_touched_file` | `decision` | `file` |
| `issue_references_pr` | `issue` | `pull_request` |
| `artifact_from_tool` | `tool_call` | `artifact` |

## Evidence Shape

```json
{
  "type": "object",
  "required": ["evidence_id", "source_id", "source_item_key", "kind", "confidence"],
  "properties": {
    "evidence_id": { "type": "string", "pattern": "^ev_" },
    "source_id": { "$ref": "#/$defs/SourceId" },
    "source_item_key": { "type": "string" },
    "document_id": { "$ref": "#/$defs/DocumentId" },
    "chunk_id": { "$ref": "#/$defs/ChunkId" },
    "kind": { "type": "string" },
    "range": { "$ref": "#/$defs/SourceRange" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "metadata": { "type": "object" }
  },
  "additionalProperties": false
}
```

## Graph Candidate Shape

Parsers and adapters emit candidates, not direct graph rows.

```json
{
  "type": "object",
  "required": [
    "candidate_id",
    "job_id",
    "source_id",
    "source_item_key",
    "producer",
    "nodes",
    "edges",
    "evidence"
  ],
  "properties": {
    "candidate_id": { "type": "string", "pattern": "^gc_" },
    "job_id": { "$ref": "#/$defs/JobId" },
    "source_id": { "$ref": "#/$defs/SourceId" },
    "source_item_key": { "type": "string" },
    "document_id": { "$ref": "#/$defs/DocumentId" },
    "producer": {
      "type": "object",
      "required": ["adapter", "parser", "version"],
      "properties": {
        "adapter": { "type": "string" },
        "parser": { "type": "string" },
        "version": { "type": "string" }
      },
      "additionalProperties": false
    },
    "nodes": {
      "type": "array",
      "items": { "$ref": "#/$defs/GraphNodeCandidate" }
    },
    "edges": {
      "type": "array",
      "items": { "$ref": "#/$defs/GraphEdgeCandidate" }
    },
    "evidence": {
      "type": "array",
      "items": { "$ref": "#/$defs/GraphEvidence" }
    }
  },
  "additionalProperties": false
}
```

Candidate rules:

- node candidates refer to stable keys, not generated `node_id`s
- edge candidates refer to node stable keys
- every edge candidate references at least one evidence id
- candidates carry producer adapter/parser/version
- graph write converts candidates into merged durable nodes/edges
- candidates that fail schema validation degrade or fail the source item
  according to scope policy

## Parser-to-Graph Matrix

| Parser Family | Node Kinds | Edge Kinds |
|---|---|---|
| dependency manifest | `package`, `package_version` | `depends_on` |
| code AST | `file`, `module`, `symbol` | `contains`, `defines`, `imports` |
| Docker compose | `docker_service`, `package` | `depends_on`, `compose_service_uses_image` |
| API schema | `api_endpoint`, `api_schema` | `exposes_endpoint`, `uses_schema` |
| session JSONL | `session`, `session_turn`, `tool_call`, `skill`, `agent`, `decision` | `session_contains_turn`, `turn_invoked_tool`, `tool_invoked_skill`, `agent_participated`, `decision_touched_file` |

## Merge Rules

Merge behavior is schema-defined.

| Strategy | Meaning |
|---|---|
| `stable_key` | Same kind and stable key merge into one node. |
| `edge_tuple` | Same kind/from/to merge evidence onto one edge. |
| `versioned` | Stable key plus version/ref creates distinct node. |
| `never_merge` | Always create separate node/edge. |

Conflict policy:

| Policy | Meaning |
|---|---|
| `keep_highest_authority_with_evidence` | Prefer official/user-pinned evidence. |
| `keep_all_as_conflict` | Preserve conflicting claims as graph conflicts. |
| `last_observed_wins` | Allowed only for mutable observed state. |
| `manual_review` | Write conflict and require review. |

Merge output must record:

- merged node/edge id
- input candidate ids
- evidence ids
- conflict ids when created
- confidence calculation
- authority calculation

## Drift Checks

Fail when:

- parser emits candidate kind absent from graph schema
- graph store accepts kind absent from schema
- documented node/edge kind has no test fixture
- graph API schema differs from `axon-api`
- edge kind references node kinds absent from registry
- stable key template references missing required properties
- merge policy lacks fixture coverage
- candidate schema differs from parser output fixtures

## Validation Fixtures

Required fixtures:

```text
crates/axon-graph/tests/fixtures/schema/repo_package.valid.json
crates/axon-graph/tests/fixtures/schema/code_symbol.valid.json
crates/axon-graph/tests/fixtures/schema/session_tool_skill.valid.json
crates/axon-graph/tests/fixtures/schema/docker_compose.valid.json
crates/axon-graph/tests/fixtures/schema/unknown_kind.invalid.json
crates/axon-graph/tests/fixtures/schema/missing_evidence.invalid.json
```

## Acceptance Criteria

- every parser-emitted graph candidate validates before store write
- every node/edge kind has stable key algorithm docs
- every node/edge kind has fixture coverage
- graph store rejects unknown kinds
- graph query API uses the same schema definitions
- graph evidence always links back to source/item/document/chunk when available
- graph merge output is deterministic for the same candidates
- conflicts are explicit graph records, not overwritten silently
