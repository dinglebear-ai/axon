# axon ask
Last Modified: 2026-08-02

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon ask ...` |
| REST | See docs/reference/rest/routes.md |
| MCP | `{ "action": "ask" }` |
| Service | `Shared domain/service implementation` |
<!-- END GENERATED ACTION SURFACES -->

`ask` answers a question from content already indexed in Axon. It embeds the
question, retrieves ranked chunks through the shared retrieval engine, renders
top-chunk context, calls the configured synthesis backend, and validates the
answer's citations.

`ask` does not search the live web. Use `research` for web discovery plus
synthesis, or `source` to add material to the local collection first.

## Related Commands

| Command | Purpose |
|---|---|
| `source` | Acquire and index a source through the unified pipeline. |
| `query` | Return ranked indexed chunks without LLM synthesis. |
| `retrieve` | Reconstruct stored content for a known canonical URL or source. |
| `ask` | Synthesize a citation-grounded answer from ranked indexed chunks. |
| `evaluate` | Compare the RAG answer with a baseline or dense-only RAG lane. |

## Synopsis

```bash
axon ask <question> [FLAGS]
axon ask --query "<question>" [FLAGS]
```

The positional question and global `--query` flag are alternative input forms.
`ask` runs synchronously; it does not enqueue a durable source job and does not
use `--wait`.

## Important Flags

| Flag | Behavior |
|---|---|
| `--collection <name>` | Search this Qdrant collection. The configured/default value is `axon`. |
| `--since <bound>` | Require `embedded_at` on or after `Nd`, `Nw`, `YYYY-MM-DD`, or RFC3339. |
| `--before <bound>` | Require `embedded_at` on or before the same supported formats. |
| `--no-hybrid-search` | Use named-dense search instead of dense+BM42 RRF. |
| `--diagnostics` | Include retrieval/context health counters. |
| `--explain` | Return a per-hit retrieval/selection trace and skip LLM synthesis. Use with `--json` for the complete payload. |
| `--no-stream` | Buffer the answer instead of streaming interactive tokens. |
| `--follow-up`, `--continue`, `-c` | Include bounded turns from the selected local ask session. |
| `--session <name>` | Select a named local ask session. |
| `--resume <name>` | Shorthand for `--follow-up --session <name>`. |
| `--reset-session` | Delete the selected session before asking. |
| `--new-session` | Start fresh in an explicit or auto-generated session. |
| `--list-sessions` | List local ask sessions and exit without running a question. |
| `--json` | Return the typed result as JSON. |

Interactive CLI synthesis streams by default. Explain mode never calls the LLM,
and JSON output is buffered.

The global `--limit` flag does not control ask retrieval. The active retrieval
depth controls are documented under [Active Tuning](#active-tuning).

## Examples

```bash
# Ask the configured collection
axon ask "How does the unified source pipeline publish a generation?"

# Use a different collection and a time window
axon ask "What changed in retrieval?" \
  --collection engineering \
  --since 30d

# Compare the current hybrid path with an explicit dense-only query
axon ask "How is cleanup debt drained?" --no-hybrid-search

# Inspect retrieval and context selection without synthesis
axon ask "How are canonical citations built?" --explain --json

# Buffer the final normalized answer
axon ask "What is the reset safety contract?" --no-stream

# Continue a local ask session
axon ask --session pipeline "Summarize the source lifecycle"
axon ask --resume pipeline "What happens after publish?"

# Manage local ask sessions
axon ask --list-sessions
axon ask --new-session --session experiments "Explain the retrieval boundary"
```

## Live Retrieval and Context Path

The current `ask` path is implemented by
`crates/axon-services/src/query/ask_retrieval.rs` and
`crates/axon-retrieval/src/`:

1. Resolve the active vector store and embedding provider.
2. Embed the question once. There is no keyword rewrite or second query
   embedding.
3. When hybrid is enabled, compute a BM42 sparse query and ask Qdrant to fuse
   the dense and sparse prefetches with RRF. When hybrid is disabled, issue one
   named-dense query.
4. Apply the current retrieval filters: allowed visibility, clean redaction,
   published document status, and optional `embedded_at` bounds. Memory records
   are excluded from ordinary query/ask retrieval.
5. Request
   `max(ask_hybrid_candidates, ask_chunk_limit, 1)` ranked hits.
6. Walk those hits in order and admit a strict prefix bounded by
   `ask_chunk_limit` and `ask_max_context_chars`.
7. Render each selected hit as `## Top Chunk [S#]` inside an
   `<retrieved_content trust="evidence_only">` boundary.
8. Send `Question: ...\n\nContext:\nSources: ...` to the configured LLM.
9. Normalize citations, require unique citations for non-trivial answers, and
   retry synthesis once when citation validation fails.

There is no separate lexical/domain reranker after Qdrant, so the retrieval and
rerank scores exposed for compatibility are equal. There is also no
full-document fetch or supplemental backfill on this path. Those diagnostic
counts are zero and their explain-plan lists are empty.

For the full prompt and trust-boundary details, see
[Context Injection Pipeline](../../guides/context-injection.md).

## Context Shape

```text
Sources:
## Top Chunk [S1]: docs.example.com

<retrieved_content trust="evidence_only">
...
</retrieved_content>

---

## Top Chunk [S2]: github.com

<retrieved_content trust="evidence_only">
...
</retrieved_content>
```

Chunk bodies are defanged before insertion so indexed text cannot manufacture
`[S#]` keys or source headers. The final structured result carries canonical
citation lineage for every selected chunk even though the prompt header uses a
short display source.

## Citation Policy

The synthesis prompt requires factual statements to cite retrieved `[S#]`
sources. Axon then normalizes the model output:

- citations must map to source identifiers in the rendered context;
- duplicate URL variants collapse to one canonical source entry;
- non-trivial answers require `ask.min-citations-nontrivial` unique sources
  when that many relevant sources are available;
- a self-reported evidence gap becomes an explicit insufficient-evidence
  response with suggested indexing targets;
- other validation failures trigger one non-streaming citation-repair attempt;
- if repair still fails, structured validation-failure details remain in the
  normalized output.

Successful normalized answers end with one canonical `## Sources` section.

## Explain Mode

`--explain` runs the same retrieval and context-selection pass as a normal ask,
then skips synthesis. The trace is intentionally narrower than the removed
reranker trace:

- each candidate's `retrieval_score` and `rerank_score` are the same returned
  score;
- selected candidates have `selected_top_chunk`; later candidates have
  `not_selected`;
- `planned_full_doc_urls` and full-document errors are empty;
- final source order contains only `top_chunk` rows;
- `llm_skipped` is `true` and LLM timing is zero.

The default JSON does not include the raw rendered context. Inspect
`explain.candidates`, `explain.context.final_source_order`,
`context_bytes_used`, `context_bytes_budget`, and `context_chars_used` instead.

## Active Tuning

Normal persistent tuning belongs in `~/.axon/config.toml`; environment values
override TOML and request/CLI overrides win over both.

| TOML key | Env override | Current effect |
|---|---|---|
| `providers.vector.hybrid-enabled` | `AXON_HYBRID_SEARCH` | Enable dense+BM42 RRF. `--no-hybrid-search` overrides it for one CLI call. |
| `retrieval.ask-hybrid-candidates` | `AXON_ASK_HYBRID_CANDIDATES` | Number of ranked hits requested before context selection; default is model-tiered. |
| `ask.chunk-limit` | `AXON_ASK_CHUNK_LIMIT` | Maximum top chunks admitted to context; default is model-tiered. |
| `ask.max-context-chars` | `AXON_ASK_MAX_CONTEXT_CHARS` | Maximum rendered context size; default is model-tiered. The live builder enforces it using UTF-8 bytes. |
| `ask.min-citations-nontrivial` | `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | Required unique citations for non-trivial normalized answers; default `2`. |

The config and MCP request DTOs still accept compatibility fields from the
retired context builder: `candidate-limit`, `full-docs`, `backfill-chunks`,
document-fetch controls, `min-relevance-score`, and authority-domain controls.
The current retrieval-engine ask path does not read those values. They do not
change ranking, filtering, or context assembly.

See the generated [TOML registry](../config/config-toml.md) and
[environment registry](../config/env.md) for the complete configuration
surface.

## LLM Backends

`AXON_LLM_BACKEND` selects `gemini-headless`, `openai-compat`, or
`codex-app-server`. Configure the backend's synthesis model, endpoint, and
credentials with the corresponding variables in the generated environment
registry. Retrieval still requires Qdrant plus the configured embedding
provider. The synthesis model profile can change model-tiered retrieval-depth
defaults, but it does not replace the vector or embedding providers.

## Local Ask Sessions

Successful non-explain CLI turns are appended to JSONL files under
`$AXON_DATA_DIR/ask-sessions/` (normally `~/.axon/ask-sessions/`). Axon updates
the `latest` pointer after each saved turn.

When follow-up mode is active, the CLI uses bounded prior turns to form the
effective retrieval question and adds a delimited history source to synthesis
context. History is treated as untrusted context, and retrieved claims still
need source citations.

| Invocation | Session behavior |
|---|---|
| no session flag | Use `latest`, falling back to `default`; do not load history. |
| `--session NAME` | Select `NAME`; do not load history. |
| `--follow-up` / `--continue` / `-c` | Load bounded history from the selected/latest session. |
| `--resume NAME` | Select `NAME` and load its bounded history. |
| `--reset-session` | Clear the selected session before the question. |
| `--new-session` | Clear an explicit or auto-generated session and ask without history. |
| `--list-sessions` | List sessions and exit. |

## Failure Boundaries

- Missing `QDRANT_URL` or `TEI_URL` fails before retrieval.
- Hybrid search against a collection without the sparse namespace fails; it
  does not silently downgrade to dense-only.
- Missing required payload lineage or non-clean redaction status fails closed
  when hits are mapped.
- A context budget too small to admit any retrieved chunk leaves no grounded
  evidence for synthesis; increase `ask.max-context-chars` or reduce chunk
  size at indexing time.
- To add missing evidence, run `axon source <SOURCE> --wait true`; to search the
  live web first, run `axon research <QUERY>`.
