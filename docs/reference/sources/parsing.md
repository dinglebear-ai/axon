# Parsing

Last Modified: 2026-07-19

Parsing extracts structured facts (`SourceParseFacts`) and graph candidates
(`GraphCandidate`) from source documents, feeding both the chunk router and the
source graph. Parsers enrich records — they do not acquire content, write
vectors, persist graph rows, or call LLMs unless routed through
`SourceEnrichment`.

> Contract source:
> [`docs/pipeline-unification/sources/parsing-contract.md`](../../pipeline-unification/sources/parsing-contract.md).
> Implementation: [`crates/axon-parse/src/`](../../../crates/axon-parse/src/).
> Phase 7 wired — `axon-document`'s `parse.rs` bridge runs the production
> parser registry over each `SourceDocument`.

## What a parser emits

`ParseResult` carries a `StageResultHeader`, `parser_id`, `parser_version`,
`document_id`, `facts: Vec<SourceParseFacts>`, `graph_candidates: Vec<GraphCandidate>`,
`warnings`, `errors`. The fact DTOs (`SourceParseFacts`, `GraphCandidate`,
`ParseEvidence`, `CodeSymbolFact`, `DependencyFact`, `ApiSchemaFact`,
`SessionFact`) are defined in `axon-api`, not `axon-parse`.

## Parser families

| Family | Parser id | What it extracts |
|---|---|---|
| Code (tree-sitter) | `code_symbols` | AST symbols: Rust, Python, JS, TS, TSX (regex fallback for others) |
| Manifest | `manifest` | dependency facts: Cargo, npm, Python, etc. |
| Markdown | `markdown_headings` | headings, structure |
| API schema | `api_schema` | OpenAPI/GraphQL/protobuf operations + schemas |
| Docker | `docker_manifest` | Compose services, images, deps |
| Env example | `env_example` | variable names (redacted defaults), required/optional |
| Session | `session_jsonl` | Claude/Codex/Gemini transcript facts |
| Tool output / schema | `tool_output_jsonl`, `tool_schema` | tool calls, MCP tool schemas, skills, agents |

Each parser reports `parser_id` and `parser_version` in capability and results;
`graph_candidate.rs` stamps `parser_id` into evidence metadata.

## Module map (`crates/axon-parse/src/`)

`parser.rs` (trait + `ParserCapability`), `registry.rs` (language/filetype
selection), `facts.rs` (`SourceParseFacts`, `ParseEvidence` with spans),
`graph_candidate.rs`, `code.rs` (tree-sitter), `manifest.rs`, `schema.rs`,
`session.rs`, `tool.rs`/`tool_schema.rs`, `env.rs`/`docker.rs`/`config.rs`,
`markdown.rs`/`vertical.rs`, `testing.rs` (`FakeParser` + fixtures),
`builtins.rs` (`production_registry()`).

## Parser selection order

1. **Explicit `requested_parser`** — unregistered id degrades, never falls through.
2. **`ParserHint`** naming a registered parser — runs alone (exclusive, first
   hint only); unregistered hint falls through with an informational
   `parse.parser_hint_unregistered` warning. Routes must not fabricate hints.
3. **Specific identification** — MIME, path/extension, lightweight content sniff
   (ranked in that order). Every matching parser runs; outputs merge.
4. **Content kind** — last resort; single highest-priority content-kind match.

Multiple parsers may run when they emit different fact families (e.g.
`docker-compose.yaml` → YAML + Docker-semantic).

## Fallback-to-prose for unsupported code

Regex-only parsing is allowed **only** as fallback: it must set
`parser_method = "regex_fallback"`, `confidence < 0.75`, and emit a warning.
Symbol facts always include name, kind, language, source range, parent symbol,
visibility, parser method, confidence. Parser failure for one item does **not**
fail the source job unless the parser is marked required for that source/scope.
Unsupported input returns `Skipped`, not `Failed`. Malformed structured files
produce item-level parse errors and still allow raw-text chunking. Parser
panics are fatal bugs.

## Graph candidates

Parsers emit candidates (dependency edges, containment, API/schema relations,
Docker service deps, session/tool/skill/agent relations) but **must not**
decide final graph merges — `axon-graph` owns merge/evidence/dedupe/conflict
policy.

If a parser is added or its fact shape changes, update this file,
`crates/axon-parse/src/builtins.rs`, and the relevant fixtures in the same PR.
