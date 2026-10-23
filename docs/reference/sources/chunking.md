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

Empty ancestor headings attach to their first content-bearing descendant,
retaining the literal source span and full heading breadcrumb. Empty sibling
or trailing sections do not produce standalone points. This is a structural
rule, independent of heading length; the empty-result fallback also applies it
so headings are not reintroduced as plain-text content.
If a fitting list or table forces its headings into a separate bounded
window, that window is omitted and the body retains the full breadcrumb.
Preparation lends bounded literal context to short structural bodies before
omitting heading-only windows. Exact deduplication follows omission so a
rejected heading cannot own an identical fenced comment's canonical span.

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

Recognized extensionless scripts use a bounded interpreter-header probe; the
shared parser resolves Python, shell and other recognized shebang languages
without executing the file. Explicit language hints and recognized extensions
remain authoritative. Unknown interpreters retain bounded fallback chunking.

Code-symbol embeddings may include a bounded prefix containing a safe relative
file path, language and symbol. The prefix uses only spare capacity within the
existing 4,096-byte and 2,000-character shared chunk ceiling (at most 768 bytes);
if capacity is unavailable, the optional prefix is omitted. Source text is never
truncated to make room. The embedding provider admits the complete input against
its configured limits without requesting truncation. The literal chunk content,
hash, source ranges and citations remain unchanged; contextual text is separate
from the returned source body. Preparation schema `axon-document/schema-18`
invalidates processing reuse for these changes.

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

## Semantic quality and duplicate locations

Executable bodies retain their nested locals and closures. Container scaffold,
imports, and declaration annotations attach to nearby declarations rather than
becoming standalone embeddings. Parser facts still retain the nested symbols
for graph construction. Provider size limits can split an oversized body.

The shared quality gate counts trimmed characters against
`pipeline.chunking.minimum-chars` (default 200; environment override
`AXON_CHUNK_MIN_CHARS`). Short symbols and path references first borrow
compatible, bounded literal context. Chunks that still fall below the configured
minimum are omitted. Punctuation-only and structurally empty chunks are removed
regardless of the minimum. A nonempty input with no surviving final chunks is
reported as `no_searchable_chunks`, with preparation-stage `chunk_quality_filter`
diagnostics identifying the source item, input size and configured minimum.
`empty_content` remains reserved for empty or whitespace-only acquired input.
This quality floor is separate from Markdown's
`markdown-min-chars` packing target. The effective minimum is clamped to 1 through
the smaller of 2,000 and the resolved Markdown maximum, and is included in processing identity, so changing
it re-prepares otherwise unchanged source documents.

Within a document, identical text with compatible context shares one embedding.
`additional_source_ranges` preserves the other literal locations and
`code_symbol_aliases` preserves their symbols. Structured records, session turns,
and temporal records keep their distinct identities. Deduplication never
normalizes code whitespace or literals.

Git inventory excludes historical plan/session documentation directories by
default. Set the source option `include_historical_docs: true` to include them;
`exclude_paths` still applies. Normal product documentation, tests, and generated
reference documentation remain eligible.

Partial source scopes (`diff`, `partial`, `fragment`, and `incremental`)
proportionally reduce concrete character ceilings using the routed token-budget
ratio. Markdown packing and overlap use the same scope adjustment. Token budgets
remain advisory; this adjustment does not claim exact tokenizer enforcement.
Meaningful session turns and structured records retain their identities even when
shorter than the general minimum chunk length.

Git inventory includes structured JSON, YAML, TOML and XML source/configuration,
conventional service/configuration suffixes (`.conf`, `.service`, `.socket`,
`.timer`, `.target`, `.mount`, `.automount`, `.preset`, `.te`, `.fc`) and named
build files including Containerfile. Extensionless files require a recognized
shebang plus a text-like bounded prefix; an executable permission bit alone does
not admit arbitrary files. Full, capped and retained-item discovery apply the
same content eligibility. Ignore files, explicit source exclusions, symlink
policy and acquisition size limits still apply.

Concise complete code documents below the configured prose floor are retained only
when a successful AST parse identifies executable/declaration structure. Comment,
import-only, unsupported and malformed tiny code keep the normal quality policy.
Tiny JSON, YAML and TOML mappings/sequences with successful serde validation
and concrete record anchors are also retained. These exceptions preserve literal bodies and parser anchors and do not
change per-chunk byte/character ceilings or the floor for ordinary prose/large files.

When a `CodeManifest`, `ApiSchema` or `StructuredRecords` projection loses every
chunk to quality filtering or emits a nonempty chunk without nonempty source byte
coordinates, a nonempty JSON/YAML mapping or sequence or TOML table is
validated again and represented by bounded literal source windows. This fallback
preserves field names, values and exact redacted source coordinates and emits
`chunk.structured_quality_fallback`; actual chunking is `plain_text_windows`.
The configured floor remains in force for normal-sized source documents; only
complete validated documents shorter than that floor use a one-character floor.
Malformed and scalar input does not qualify. Schema 18 requires reprocessing
schema-17 and older documents to acquire these previously missing configuration records.
