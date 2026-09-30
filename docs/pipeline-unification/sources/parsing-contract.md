# Parsing Contract
Last Modified: 2026-09-27

## Contract

`axon-document` admits and decodes acquired content, redacts text, then invokes
`axon-parse` and chunking. `axon-services` coordinates publication through the
ledger, vector, and graph boundaries. Adapters retain source inventory and
acquire content; shared preparation decides whether it is indexable.

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

## Content Admission and Preparation

`DocumentPreparer` returns either `Prepared` or `Skipped`. Skips retain the
source item identity and a fixed reason: `unsupported_binary`,
`unsupported_encoding`, `empty_content`, `unresolved_content_reference`, or
`size_limit_exceeded`. They produce no placeholder text, chunks, or embeddings.
Intentionally authored text metadata, including memory records, remains text.

Git and Local acquire raw bytes without deciding binary eligibility. Shared
preparation recognizes unsupported byte signatures, decodes strict UTF-8 and
BOM-marked UTF-16, and skips empty or whitespace-only text. File extensions and
MIME hints do not override the byte policy. Artifact and external references
that reach preparation unresolved are skipped; preparation does not fetch them.
Decoded text is redacted before parsing and chunking. Source ranges refer to
the resulting UTF-8 text, rather than UTF-16 transport offsets.

Direct preparation defaults to a finite 64 MiB content ceiling. Production further
bounds it by the preparation memory allowance and request per-item limit.
Admission checks encoded allocation size before base64 alphabet validation;
malformed admitted base64 is a contract error. Raw decoded bytes and expanded
UTF-8 text must each fit the ceiling. Git and Local also bound discovery reads,
individual acquisition reads and batches, and enforce requested cumulative
acquired-byte budgets. Exhausting a job byte budget fails the attempted
generation and preserves committed output;
a per-item oversized file remains in inventory with an explicit skip. Discovery
I/O is separate from the acquired-payload byte total. These Git/Local acquisition
limits do not establish HTTP provider network-byte limits.

`axon-services` persists skipped document statuses and exact `documents_skipped`
counts. Complete inventories can authorize deletion; partial or unknown
inventories retain unvisited prior items with validated committed provenance.
A text-to-skipped transition retires its prior vectors and graph evidence;
a skipped-to-text transition prepares it normally. All-skipped generations can
complete without calling the embedding provider.

Preparation schema and effective processing settings participate in reuse
identity. A changed identity reparses unchanged content, including cached HTTP
304 and identical-body 200 responses. A partial refresh cannot retain unvisited
items under an incompatible identity; a complete refresh can migrate them.

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

1. explicit `requested_parser` — a caller demand. An unregistered id degrades
   the parse; it never falls through to auto-selection.
2. a document `ParserHint` naming a registered parser — runs alone, exclusive
   like a request, suppressing the multi-parser fan-out below. Only the
   document's first hint is consulted; later entries are ignored. Hints are
   advisory metadata stamped by upstream stages (adapter capability
   declarations passed through the route, enrichment): a hint naming an
   unregistered parser falls through to the signals below, recording an
   informational `parse.parser_hint_unregistered` warning even when nothing
   below matches, instead of degrading the document. Routes must not
   fabricate hints — a hint is only valid when it names a registered parser
   id.
3. specific identification — MIME type, path/extension, and lightweight
   content sniffing (ranked in that order for the primary identity). Every
   parser that specifically identifies the document runs, and their outputs
   merge.
4. content kind — the last-resort fallback; the single highest-priority
   content-kind match runs alone.

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
