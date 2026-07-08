# Adding a Parser

`axon-parse` owns source parsing and fact extraction — it turns
`SourceDocument` content into `SourceParseFacts` and `GraphCandidate` values
that downstream chunking and graph stages consume. This guide describes the
real, live `SourceParser`/`ParserRegistry` pattern in
`crates/axon-parse/src/`.

See also: crate guide `crates/axon-parse/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/sources/parsing-contract.md`.

Parsers run **after** acquisition and **before** graph persistence or
chunking — `axon-document`'s `parse.rs` bridge runs
`builtins::production_registry()` over each `SourceDocument` on the
acquisition path, so parse facts flow into `PreparedDocument` and drive
parser-aware chunk routing.

## Step 1: Implement `SourceParser`

`crates/axon-parse/src/parser.rs` defines the trait:

```rust
pub trait SourceParser: Send + Sync {
    fn capability(&self) -> &ParserCapability;
    fn parse(&self, input: &ParseInput) -> ParseResult;
}
```

`ParserCapability` declares how the registry matches documents to your
parser:

```rust
pub struct ParserCapability {
    pub parser_id: String,
    pub parser_version: String,
    pub content_kinds: Vec<ContentKind>,
    pub mime_types: Vec<String>,
    pub file_extensions: Vec<String>,
    pub path_suffixes: Vec<String>,
    pub sniff_prefixes: Vec<String>,
    pub priority: u32,
}
```

Every parser **must** report a `parser_id` and `parser_version` in its
capability — this is what shows up in generated capability docs and lets
callers request a specific parser explicitly (`ParseInput.requested_parser`
or a document's first `parser_hints` entry).

Follow the existing pattern from `crates/axon-parse/src/builtins.rs`: a
zero-sized marker struct per parser, with `capability()` returning a
`static OnceLock<ParserCapability>`-cached value so the capability struct is
built once. Real facts extraction usually lives in a dedicated module (e.g.
`code::symbol_facts_with_graph`, `manifest::dependency_facts`,
`markdown::heading_facts`) that the parser's `parse()` method calls into and
wraps into a `ParseResult` via the shared `completed_result`/`stage_header`
helpers.

## Step 2: Register it in the production registry

`crates/axon-parse/src/builtins.rs::production_registry()` is the composed
`ParserRegistry` actually used on the acquisition path:

```rust
pub fn production_registry() -> ParserRegistry {
    ParserRegistry::new()
        .with_parser(SchemaParser)
        .with_parser(DockerManifestParser)
        .with_parser(EnvExampleParser)
        .with_parser(CodeSymbolsParser)
        .with_parser(ManifestParser)
        .with_parser(MarkdownParser)
        .with_parser(ToolParser)
        .with_parser(SessionParser)
}
```

Add your new parser via `.with_parser(YourParser)`. `ParserRegistry` keeps
parsers sorted by `priority` (lower priority wins ties) — see selection logic
below.

## How parser selection works (`registry.rs`)

`ParserRegistry::parse(input)`:

1. If the input requests an explicit parser (`ParseInput.requested_parser`,
   falling back to the document's first `parser_hints` entry), look it up by
   `parser_id`. If found, use it. If requested but not found, return a
   `parse.requested_parser_unavailable` degraded result — never silently fall
   through to auto-selection.
2. Otherwise, run `select_best_match`: score every registered parser against
   the input using `match_score`, which checks (in priority order, taking
   the **maximum** matching score):

   | Match | Score |
   |---|---|
   | Path extension/suffix (`matches_path`) | 50 |
   | MIME type (`matches_mime_type`) | 40 |
   | Content sniffing — text prefix match (`matches_sniffing`) | 30 |
   | Content kind (`matches_content_kind`) | 10 |

   Ties on score are broken by lower `priority` value winning. A parser with
   no match on any axis is excluded entirely (`match_score` returns `None`).
3. If nothing matches, return an `unsupported_result` — a
   `CompletedDegraded` result with a `parse.unsupported` warning and empty
   facts/candidates, not an error. **Unsupported content must degrade
   cleanly and never block ingestion.**

Design your `ParserCapability` to be specific: prefer `file_extensions`/
`path_suffixes` over a bare `content_kinds` match when you can, since path
matches score highest and are the least ambiguous signal.

## Step 3: Emit evidence-backed facts and graph candidates

- Parse facts (`SourceParseFacts`) should include evidence spans wherever the
  parser can produce them — this is what lets downstream consumers (and
  humans) audit *why* a fact was extracted.
- Dependency and schema parsers should emit `GraphCandidate` values.
  **Every graph candidate must carry enough evidence to audit why the edge
  exists** — do not emit a bare edge with no supporting span/reason.
- Code parsing is AST-backed wherever a parser exists (tree-sitter). Line
  heuristics are acceptable only as an explicitly documented interim state
  (see the crate's own CLAUDE.md status note on code symbol extraction).

## Step 4: Add tests

Follow the sidecar `_tests.rs` convention: `your_parser.rs` +
`your_parser_tests.rs` declared via `#[path]`. Look at
`crates/axon-parse/src/markdown_tests.rs` or
`crates/axon-parse/src/schema_tests.rs` for the pattern. `crates/axon-parse/
src/testing.rs` exposes `FakeParser` plus fixtures (Cargo/npm/Python/
compose/env/OpenAPI/JSONL/code) reusable by downstream crates.

```bash
cargo test -p axon-parse
```

## Boundary reminders

- Acquisition, ledger persistence, **graph persistence**, chunking output,
  vector writes, and LLM extraction commands do not belong in `axon-parse`.
- No source routing or canonical source identity here.
- No IDE/LSP live query behavior.
- Allowed dependencies: `axon-api`, `axon-error`, `axon-core`,
  `axon-observe`; tree-sitter and format-specific parser crates behind
  parser modules; `axon-document` types only if needed. Forbidden:
  Qdrant/TEI/LLM clients, ledger or graph store implementations, transport
  crates — enforced by `cargo xtask check-layering`.
