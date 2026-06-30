# Parsing Contract
Last Modified: 2026-06-30

## Contract

This is the target parsing boundary. The dedicated `axon-parse` crate,
`ParserRegistry`, `SourceParseFacts`, and generalized `GraphCandidate` pipeline
do not exist as standalone implementation boundaries today.

`axon-parse` owns deterministic extraction of structured facts from source
documents and source manifests. Parsing is a first-class pipeline boundary, not
an incidental helper inside chunking, adapters, or graph storage.

Parsing turns source content into:

- `SourceParseFacts`
- `GraphCandidate`
- parser warnings/errors
- parser metadata used by `DocumentPreparer` and `SourceGraph`

Parsing does not persist graph rows, write vector points, mutate ledger
generations, or call LLM providers unless routed through `SourceEnrichment`.

## Current Implementation Snapshot

Implemented today:

- Parsing/chunking behavior lives primarily inside `axon-vector`
  `SourceDocument` preparation and source-specific ingest helpers.
- Current code preparation emits `PreparedDoc` and per-chunk metadata, including
  chunk locators, source ranges, language, and code symbol metadata where the
  current chunker can extract it.
- Tree-sitter-backed code chunking exists, but the output is structural chunk
  metadata rather than the target graph-ready parser facts boundary.
- General `SourceParseFacts` and `GraphCandidate` persistence are target
  architecture.

## Ownership

| Component | Owns |
|---|---|
| `ParserRegistry` | parser discovery, priority, capability matching |
| `Parser` | one parser implementation boundary |
| `ManifestParser` | dependency/schema/config/session manifest extraction |
| `CodeParser` | AST/symbol/dependency extraction for source code |
| `StructuredParser` | JSON/YAML/TOML/XML/OpenAPI/GraphQL/etc. facts |
| `SessionParser` | Claude/Codex/Gemini JSONL sessions, turns, tools, skills, agents |
| `ToolSchemaParser` | CLI/MCP tool schemas, arguments, outputs |
| `ParseResult` | serializable parser output |

## Public Types

```rust
#[async_trait]
pub trait Parser: Send + Sync {
    fn parser_id(&self) -> &'static str;
    fn parser_version(&self) -> &'static str;
    fn supports(&self, input: &ParseInput) -> bool;
    async fn parse(&self, input: ParseInput) -> Result<ParseResult>;
    async fn capabilities(&self) -> Result<ParserCapability>;
}

pub struct ParseInput {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub document_id: DocumentId,
    pub canonical_uri: String,
    pub content_kind: ContentKind,
    pub language: Option<String>,
    pub path: Option<String>,
    pub content: ContentRef,
    pub structured_payload: Option<serde_json::Value>,
    pub hints: Vec<ParserHint>,
    pub metadata: MetadataMap,
}

pub struct ParseResult {
    pub header: StageResultHeader,
    pub parser_id: String,
    pub parser_version: String,
    pub document_id: DocumentId,
    pub facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
    pub warnings: Vec<SourceWarning>,
    pub errors: Vec<SourceError>,
}

pub struct ParserCapability {
    pub parser_id: String,
    pub parser_version: String,
    pub content_kinds: Vec<ContentKind>,
    pub languages: Vec<String>,
    pub file_patterns: Vec<String>,
    pub emits_fact_kinds: Vec<String>,
    pub emits_graph_node_kinds: Vec<String>,
    pub emits_graph_edge_kinds: Vec<String>,
    pub deterministic: bool,
}
```

## Required Parser Families

| Family | Inputs | Required Facts |
|---|---|---|
| Code | source files | language, symbol declarations, imports, package references, test markers |
| Rust | `Cargo.toml`, `Cargo.lock`, `*.rs` | crates, features, workspace members, modules, functions, structs, traits |
| JavaScript/TypeScript | `package.json`, lockfiles, `*.js`, `*.ts`, `*.tsx` | packages, scripts, exports, imports, components |
| Python | `pyproject.toml`, `requirements.txt`, `setup.py`, `*.py` | packages, extras, modules, functions, classes |
| Docker | `Dockerfile`, `docker-compose*.yml` | images, services, ports, volumes, env refs, networks, dependencies |
| Env Example | `.env.example`, `.env.sample` | variable names, defaults, required/optional markers, secret-looking fields |
| API Schema | OpenAPI, GraphQL, protobuf | endpoints, methods, schemas, operations, auth requirements |
| Sessions | Claude/Codex/Gemini JSONL | sessions, turns, tool calls, skills, agents, files touched, decisions |
| CLI/MCP Tool | command help, MCP schema/output | tools, arguments, return shapes, side-effect class |

## Parser Selection

Parser selection order:

1. explicit `ParserHint`
2. adapter-declared parser support
3. content kind
4. MIME type
5. path/extension
6. lightweight content sniffing

Multiple parsers may run when they emit different fact families. Example:
`docker-compose.yaml` can use both YAML structure parsing and Docker-specific
semantic parsing.

## AST and Structural Parsing

Code parsers should use AST/tree-sitter or language-native parsers where
available. Regex-only parsing is allowed only as a fallback and must set:

- `parser_method = "regex_fallback"`
- `confidence < 0.75`
- a warning explaining the fallback

Symbol facts must include:

- symbol name
- symbol kind
- language
- source range
- parent symbol when known
- export/public visibility when known
- parser method
- confidence

## Graph Candidate Rules

Parsers may emit candidates for:

- repository/package dependency edges
- source file/module/symbol containment
- API endpoint/schema relationships
- Docker service dependency links
- session/tool/skill/agent relationships
- project-to-package/repo/docs links

Parsers must not decide final graph merges. `axon-graph` owns merge, evidence,
dedupe, and conflict policy.

## Error and Degradation

- Parser failure for one item does not fail the full source job unless the
  parser is marked required for that source/scope.
- Unsupported parser input returns `Skipped`, not `Failed`.
- Malformed structured files produce item-level parse errors and still allow
  raw text chunking when safe.
- Parser panics are fatal implementation bugs and must be caught by tests.

## Testing Requirements

- every parser has golden input/output fixtures
- every parser has malformed-input tests
- every parser has path/content-kind routing tests
- AST parsers have symbol range tests
- graph candidates have stable key tests
- parser outputs are serde round-tripped through `axon-api`
