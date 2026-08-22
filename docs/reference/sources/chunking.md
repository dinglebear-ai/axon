# Chunking

Last Modified: 2026-07-19

Chunking converts `SourceDocument` values into embedding-ready `PreparedDocument`
chunks. Only `axon-document` emits `PreparedDocument`; all adapters emit
`SourceDocument`. Each published chunk becomes exactly one vector point.

> Contract source:
> [`docs/pipeline-unification/sources/chunking-contract.md`](../../pipeline-unification/sources/chunking-contract.md).
> Implementation: [`crates/axon-document/src/`](../../../crates/axon-document/src/)
> (`preparer.rs`, `parse.rs` bridge, `chunk_router.rs`, `profile.rs`, `code.rs`).

## Pipeline

```text
SourceAdapter → SourceDocument → DocumentPreparer → ChunkRouter
  → Chunker / Parser → SourceParseFacts → GraphCandidate
  → PreparedDocument → EmbeddingBatch
```

The `parse.rs` bridge runs the `axon-parse` production registry over each
`SourceDocument`; when parse facts are present, they drive a parser-aware
profile override (code → `CodeSymbol`, manifest → `CodeManifest`, schema →
`ApiSchema`, …). Parsers may emit facts/candidates during preparation, but
chunkers do **not** persist graph facts — that is a separate step in `axon-graph`.

## Chunk profiles

| Profile | Parser family | Fallback chain | Max tokens | Overlap |
|---|---|---|---:|---:|
| `CodeSymbol` | tree_sitter | [tree_sitter, code_blocks, line_window] | 1400 | 90 |
| `CodeManifest` | structured | [structured_manifest, atomic_metadata] | 1200 | 0 |
| `MarkdownSections` | markdown | [heading_sections, plain_text_windows] | 1600 | 120 |
| `HtmlArticle` | html | [dom_to_markdown, plain_text_windows] | 1600 | 120 |
| `PlainTextWindows` | plain_text | [paragraph_windows, line_window] | 1500 | 140 |
| `TranscriptSegments` | transcript | [timestamp_turns, line_segments] | 1300 | 60 |
| `StructuredRecords` | structured | [structured_records, atomic_metadata] | 1200 | 40 |
| `ApiSchema` | structured | [schema_records, structured_records, atomic_metadata] | 1600 | 60 |
| `ToolOutput` | tool_output | [command_records, line_segments] | 1200 | 60 |
| `SessionTurns` | session | [turn_segments, line_segments] | 1300 | 0 |
| `AtomicMetadata` | atomic | [atomic_metadata] | 1600 | 0 |

The token limits above remain profile-level routing guidance. Markdown's
concrete character packing is operator-tunable under `[pipeline.chunking]`
with `markdown-max-chars`, `markdown-min-chars`, and `overlap-chars`; the
corresponding environment overrides are documented in the generated config
reference. `overlap-chars` applies only between consecutive windows cut from
the same prose span. It never crosses frontmatter, heading paths, fenced code,
or other incompatible Markdown block metadata. A fenced code block remains
intact even when it alone exceeds `markdown-max-chars`; surrounding prose is
still bounded by the configured maximum.

## ChunkRouter decision

Inputs: content kind, MIME, file path/extension, source kind & adapter, scope,
`chunk_hint`, `parser_hint`, structured payload type, document size, source
metadata. Routing order:

1. Explicit trusted `chunk_hint.profile`
2. Strong content kind: code, markdown, transcript, structured, html
3. File path/extension + MIME
4. Source adapter defaults
5. Size-based fallback
6. Plain text fallback

The adapter is checked first — a fragment-prone adapter forces the fallback
method regardless of size; scope narrows token/overlap limits for partial
captures. Document size alone does not force intact Markdown onto paragraph
windows: the fence-aware heading walker is linear and preserves the configured
minimum chunk packing on large reference pages. Large code documents may still
use their cheaper structural fallback.

## Tree-sitter-aware code chunks

Detect language from parser support/extension/shebang/adapter metadata; parse
with tree-sitter when supported; extract declarations and stable symbol names;
prefer chunks around named definitions (functions, methods, classes, structs,
enums, traits, modules, constants, tests); mark generated/vendor/minified files;
emit manifest/dependency facts for manifest files; fall back to line-window
chunks when parsing fails. A language is "supported" only when parser coverage,
symbol extraction rules, **and** fallback tests exist. `symbol_extraction_status=
unsupported` is valid and searchable. Initial targets: Rust, TS/JS, Python, Go,
Java/Kotlin, C/C++, C#, Ruby/PHP/Elixir/Dart, Shell/PowerShell.

## Chunk identity (1:1 with vector points)

Each chunk has deterministic `chunk_id`/`chunk_key`, `content_hash`,
`chunk_locator`, `source_range`, and the full required vector payload (see
[metadata-payload.md](metadata-payload.md)). Recommended `chunk_key`:

```text
hash(source_id, source_generation | immutable version, source_item_key,
     chunking_profile, chunk_locator, content_hash)
```

Rules: do **not** use array index alone, do **not** use Qdrant-assigned ids, do
**not** include wall-clock timestamps, always include generation so old/new
snapshots coexist. Point ids default to UUIDv5 over `url:idx`.

## Source ranges

- text/code: `line_start/line_end/byte_start/byte_end`
- HTML/web: adds `dom_selector`
- transcript: `time_start_ms/time_end_ms/turn_start/turn_end`
- structured record: `json_pointer` + byte range

If raw and normalized ranges differ, store normalized in chunk metadata, raw
in parse facts or an artifact.

If the chunk profiles or sizing change, update this file and
`crates/axon-document/src/chunk_router.rs::profile_defaults` in the same PR.
