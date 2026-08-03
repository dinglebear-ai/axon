# axon-parse Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-parse` owns source parsing and fact extraction. It turns `SourceDocument`
content into `SourceParseFacts` and `GraphCandidate` values that downstream
graph and chunking stages can consume.

## Owns

- parser registry and parser capability declarations
- code AST facts, dependency manifests, schemas, REST/OpenAPI specs, sessions,
  tool calls, skills, agents, env examples, Docker Compose, and config facts
- language/filetype parser selection
- graph candidates with evidence references
- parse fixtures and parser fakes

## Must Not Own

- acquisition, ledger persistence, graph persistence, chunking output, vector
  writes, or LLM extraction commands
- source routing or canonical source identity
- IDE/LSP live query behavior

## Public Modules

```text
lib.rs
parser.rs
registry.rs
facts.rs
graph_candidate.rs
code.rs
manifest.rs
schema.rs
session.rs
tool.rs
env.rs
docker.rs
config.rs
testing.rs
```

## Public API

- `SourceParser`
- `ParserRegistry`
- `ParserCapability`
- `SourceParseFacts`
- `GraphCandidate`
- `ParseEvidence`
- `CodeSymbolFact`
- `DependencyFact`
- `ApiSchemaFact`
- `SessionFact`
- `FakeParser`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`
- tree-sitter and format-specific parser crates behind parser modules

## Dependencies Forbidden

- Qdrant/TEI/LLM clients
- ledger or graph store implementations
- transport crates

## Generated Artifacts

- parser capability registry for docs
- graph schema examples in [../../schemas/graph-schema.md](../../schemas/graph-schema.md)
- source-specific metadata examples in
  [../../sources/metadata-payload.md](../../sources/metadata-payload.md)

## Fixtures And Fakes

- Cargo, npm, Python, Docker Compose, env example, OpenAPI, JSONL session, and
  code symbol fixtures
- parser degradation fixture for unsupported content
- fake parser emitting known facts and graph candidates

## Tests

- every parser reports capability and version
- parse facts include evidence spans where possible
- dependency and schema parsers emit graph candidates
- unsupported files degrade cleanly without blocking ingestion

## Acceptance Criteria

- source-specific intelligence lives here before graph/chunking
- code parsing is AST-backed where a parser exists
- every graph candidate has enough evidence to audit why the edge exists

See [../README.md](../README.md) and
[../../sources/parsing-contract.md](../../sources/parsing-contract.md).
