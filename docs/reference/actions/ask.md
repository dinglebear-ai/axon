# axon ask
Last Modified: 2026-06-13

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon ask ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


RAG-powered Q&A. Retrieves relevant chunks from the local Qdrant knowledge base, reranks them by relevance, builds a context window, and calls the configured LLM to generate a grounded answer.

## Related Retrieval Commands

| Command | Meaning |
|---|---|
| `search` | External web discovery; current runtime also auto-queues bounded Source jobs for results. |
| `query` | Ranked semantic search over content already indexed in Qdrant. |
| `retrieve` | Stored content lookup/reconstruction by known URL or source identity. |
| `ask` | RAG synthesis over indexed context with an LLM answer. |

## Synopsis

```bash
axon ask <question> [FLAGS]
axon ask --query "<question>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<question>` | Question to answer (positional, or via `--query`) |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `TEI_URL` | TEI embeddings base URL. Used to embed the question before Qdrant search. |
| `QDRANT_URL` | Qdrant base URL. Searched for relevant chunks. |
| `AXON_LLM_BACKEND` | Answer-generation backend. Defaults to `gemini-headless`; set `openai-compat` for OpenAI-compatible chat completion endpoints. |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command used by the default Gemini headless backend. Defaults to `gemini`. |
| `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for answer generation when using Gemini headless. |
| `AXON_HEADLESS_GEMINI_MODEL` | Legacy alias for `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL`. |
| `AXON_OPENAI_BASE_URL` | OpenAI-compatible API root when `AXON_LLM_BACKEND=openai-compat`. |
| `AXON_SYNTHESIS_OPENAI_MODEL` | Model name for the OpenAI-compatible backend. |
| `AXON_OPENAI_MODEL` | Legacy alias for `AXON_SYNTHESIS_OPENAI_MODEL`. |
| `AXON_OPENAI_API_KEY` | Optional bearer token for the OpenAI-compatible backend. |
| `AXON_SYNTHESIS_HIGH_CONTEXT` | Explicitly force high (`true`) or small (`false`) synthesis context tier; unset uses model/backend detection. |

`ask` uses Qdrant + TEI retrieval and the configured LLM backend for synthesis. The default backend is Gemini headless.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Question text (alternative to positional argument). |
| `--collection <name>` | `axon` | Qdrant collection to search. Also settable via `AXON_COLLECTION`. |
| `--no-hybrid-search` | `false` | Disable hybrid (dense + BM42 sparse + RRF) retrieval; force dense-only. Overrides `AXON_HYBRID_SEARCH=true`. |
| `--since <date>` | — | Filter retrieved context to content indexed on or after this date. Accepts `7d`, `30d`, `1w`, `YYYY-MM-DD`, or RFC3339. |
| `--before <date>` | — | Filter retrieved context to content indexed on or before this date. Same formats as `--since`. |
| `--diagnostics` | `false` | Print retrieval diagnostics (candidate/reranked pool, selected chunks, configured and effective context ceilings, complexity, authority ratios, corpus health, and top domains). Legacy full-doc/supplemental counters remain compatibility fields and are zero on the unified path. |
| `--explain` | `false` | Emit a per-candidate ranking/context trace. Implies diagnostics and skips LLM synthesis; use with `--json` for the full payload. |
| `--stream` | `true` | Stream answer tokens as they arrive for interactive use. Uses the in-process ask path; JSON and explain output remain buffered. |
| `--no-stream` | `false` | Disable answer streaming and render only the final response. |
| `--follow-up` / `--continue` / `-c` | `false` | Include recent turns from the selected local ask session as conversation context. `--continue` and `-c` are aliases. |
| `--session <name>` | latest | Local ask session name used for saved turns and follow-up context. If omitted, Axon uses the most recently successful ask session, falling back to `default`. |
| `--reset-session` | `false` | Clear the selected ask session before running this question. Mutually exclusive with `--new-session`. |
| `--new-session` | `false` | Force a fresh ask session, deleting prior turns for the selected (or auto-generated) session name and running without follow-up context. Mutually exclusive with `--follow-up` and `--reset-session`. |
| `--resume <name>` | — | Resume a named ask session. Shorthand for `--follow-up --session <name>`. Mutually exclusive with `--session` and `--new-session`. |
| `--list-sessions` | `false` | Print all local ask sessions (name, turn count, last used, latest marker) and exit. Cannot be combined with a query argument; pair with `--json` for machine-readable output. |
| `--json` | `false` | Machine-readable JSON output. |

Note: `ask` runs synchronously and does not support `--wait`.

`--limit` is a global flag but is not used by `ask` retrieval. Ask retrieval depth is controlled by `AXON_ASK_*` tuning env vars.

## Examples

```bash
# Basic ask
axon ask "how does spider.rs handle JavaScript-heavy sites?"

# Using --query flag
axon ask --query "what is the default chunk size for TEI batch requests?"

# Specific collection
axon ask "list all indexed rust crates" --collection rust-libs

# Debug: show retrieved chunks and scores
axon ask "qdrant HNSW parameters" --diagnostics

# Disable streaming when you need buffered output
axon ask "how does the ask pipeline choose sources?" --no-stream

# Continue from recent ask turns in the default local session
axon ask --follow-up "can you show a concrete example?"
axon ask --continue "can you show a concrete example?"   # alias
axon ask -c "can you show a concrete example?"           # short form

# Resume a specific named session (alias for --follow-up --session NAME)
axon ask --resume rust-tests "how would that look in this repo?"

# Use a named local session and clear it when changing topics
axon ask --session rust-tests --reset-session "what is the test sidecar pattern?"
axon ask --session rust-tests --follow-up "how would that look in this repo?"

# Start a fresh session (auto-named) for a brand-new line of questioning
axon ask --new-session "what is the architecture of axon's ask pipeline?"

# Overwrite a named session with a fresh thread
axon ask --new-session --session experiments "let's start over on this topic"

# List local ask sessions (human-readable)
axon ask --list-sessions

# List sessions as JSON for scripts
axon ask --list-sessions --json

# Explain ranking/context decisions without calling the LLM
axon ask "claude marketplace plugins" --explain --json

# JSON output
axon ask "what are the source acquisition limits?" --json

# Ask the local knowledge base with buffered output
axon ask --no-stream "what changed in server mode?"
```

## RAG Pipeline

1. Embed the question via TEI
2. Query Qdrant for top `ask.candidate-limit` candidate chunks. The fallback is model-tiered unless explicitly set.
3. Apply `ask.min-relevance-score` (default: 0.45) only when hybrid search is disabled and retrieval returns dense/cosine scores.
4. When hybrid search is enabled, the retrieval engine issues dense + BM42 prefetch arms and Qdrant returns RRF fusion scores. RRF values are not cosine similarities, so the cosine threshold is skipped.
5. Apply ask-specific post-retrieval policy: drop low-signal session/log/cache sources unless the question explicitly asks for them, enforce the dense relevance floor only on cosine scores, and use a narrow named-product identity guard instead of a general exact-keyword gate. Dense/cosine results may receive URL/text lexical, documentation-path, phrase, configured-authority, and verified product-authority boosts. RRF skips lexical additions; configured and product trust may raise a candidate, but their combined delta is capped at the candidate's original fused score.
6. Classify the question as simple, complex, or exhaustive and derive an effective per-query context budget. `AXON_ASK_CHUNK_LIMIT` and `AXON_ASK_MAX_CONTEXT_CHARS` remain hard configured ceilings; they are not targets that Axon tries to fill.
7. Assemble a document-diverse context from the reranked candidates. Each chunk has its own character cap, the selected-chunk count is bounded by the effective chunk limit, and the final context is bounded by the effective character budget. The unified retrieval path does not expand the prompt by fetching full documents.
8. Report both configured ceilings and effective runtime limits in diagnostics, including `effective_chunk_limit`, `effective_max_context_chars`, `max_chunk_chars`, `detected_complexity`, authority ratios, and selected-source ordering.
9. Call the configured LLM backend with context + question
10. Apply response-quality gates (citations + policy checks, including structured `citation_validation` when the model returns it)
11. Print the normalized answer

## Session Lifecycle

The seven session-related flags interact as follows:

| Flag | Selects which session? | Loads prior turns? | Wipes existing turns? | Exits without running query? |
|------|------------------------|--------------------|-----------------------|------------------------------|
| (none) | `latest` pointer, falling back to `default` | No | No | No |
| `--session <NAME>` | `<NAME>` | No | No | No |
| `--follow-up` (alias `--continue`, short `-c`) | from `--session` or `latest` | Yes | No | No |
| `--resume <NAME>` | `<NAME>` | Yes | No | No |
| `--reset-session` | from `--session` or `latest` | No (after wipe) | Yes (selected session) | No |
| `--new-session` | from `--session` or auto-`auto-YYYY-MM-DD-HHMMSS` | No | Yes (selected/new) | No |
| `--list-sessions` | — | — | — | Yes |

Mutually exclusive combinations (clap enforces at parse time):

- `--new-session` ⨯ `--follow-up` / `--continue` / `-c`
- `--new-session` ⨯ `--reset-session`
- `--new-session` ⨯ `--resume`
- `--resume` ⨯ `--session` (redundant)
- `--list-sessions` + any positional query argument is rejected at runtime.

Typical workflows:

```bash
# Pick up where I left off
axon ask --continue "what was the second option you mentioned?"

# Switch threads
axon ask --resume rust-tests "back to the test sidecar conversation"

# Start completely fresh, keep an auto-named history
axon ask --new-session "let's look at a different topic"

# See all my threads
axon ask --list-sessions
```

## Follow-Up Sessions

`axon ask` records successful non-explain turns to local JSONL files under
`$AXON_DATA_DIR/ask-sessions/` (default: `~/.axon/ask-sessions/`). After each
successful saved turn, Axon updates `$AXON_DATA_DIR/ask-sessions/latest` with
the active session name.

If `--session <name>` is omitted, Axon uses the most recently successful ask
session from that `latest` pointer, falling back to `default` when no prior
session exists. The human CLI output prints the active `Session:` after timing,
and JSON output includes `"session": "<name>"`.

Pass `--session <name>` to keep separate threads or to switch explicitly.

`--follow-up` loads the recent turns for the selected session and folds them
into the retrieval/synthesis question so references like "that" or "the second
option" can resolve without depending on Gemini CLI's interactive session state.
Facts still need to come from retrieved Axon context and still need `[S#]`
citations. Use `--reset-session` to clear a local session before changing
topics.

## Explain Trace

Use `--diagnostics` for aggregate health counters. Use `--explain --json` when a ranking result looks wrong and you need the per-candidate math and context decisions. Explain mode returns the normal `AskResult` shape with `answer: ""`, `timing_ms.llm: 0`, `explain.llm_skipped: true`, and no Gemini call.

Raw rendered retrieval context is omitted from default explain JSON so CLI, MCP, REST, and runner artifacts do not leak the full prompt fragment by accident. Use `.explain.context.final_source_order` for source ordering metadata, `.explain.context.context_chars_used` / `.context_char_budget` for the enforced Unicode-character invariant, and `.explain.context.context_bytes_used` for actual UTF-8 size. `.context_bytes_budget` is `0` on the character-bounded unified ask path because no separate byte ceiling is enforced. Use `.explain.candidates[]` for candidate scores, filter decisions, selected context ranks, insertion modes, and snippets.

When an internal caller explicitly includes rendered context, it is shaped as `.explain.context.rendered_context = { "format": "axon_sources_v1", "content": "...", "bytes_used": N, "chars_used": N }`.

```text
query
  |
  v
TEI embedding + Qdrant dense/BM42/RRF retrieval
  |
  v
rerank/filter + token/authority policy
  |
  v
corpus-health classification
  |
  v
document-diverse bounded chunk-context selection
  |
  v
ask --explain retrieval harness
  |
  +--> ranking bug? tune scoring/filtering
  +--> selection bug? tune context selection
  +--> corpus gap? source/index better docs
  +--> fixture mismatch? update tracked fixture notes
```

Compact example:

```json
{
  "query": "claude marketplace plugins",
  "answer": "",
  "diagnostics": { "candidate_pool": 15, "reranked_pool": 12 },
  "explain": {
    "mode": "explain_only",
    "retrieval": {
      "score_kind": "rrf",
      "vector_mode": "named_hybrid_rrf",
      "hybrid_search_enabled": true
    },
    "candidates": [
      {
        "id": "candidate-1",
        "url": "https://code.claude.com/docs/en/plugins",
        "retrieval_score": 0.17,
        "rerank_score": 0.34,
        "score_components": [
          { "name": "retrieval_score", "value": 0.17, "status": "applied" },
          { "name": "product_authority_boost", "value": 0.17, "status": "applied" }
        ],
        "filter_decisions": [{ "kind": "kept" }],
        "selection_decisions": [{ "kind": "selected_top_chunk" }]
      }
    ],
    "context": {
      "planned_full_doc_urls": [],
      "full_doc_fetch_skipped": true,
      "full_doc_fetch_skip_reason": "not_supported_by_retrieval_engine",
      "full_doc_fetch_mode": "rrf",
      "final_source_order": [
        { "source_id": "S1", "url": "https://code.claude.com/docs/en/plugins", "tier": "top_chunk" }
      ],
      "truncated_by_budget": false
    },
    "llm_skipped": true
  },
  "timing_ms": { "retrieval": 21, "context_build": 3, "llm": 0, "total": 24 }
}
```

`retrieval_score` scale depends on retrieval mode. Cosine/dense paths use cosine-like scores, apply `ask.min-relevance-score`, and may receive lexical/documentation/phrase rerank boosts. RRF paths use rank-fusion scores, skip the cosine threshold, and preserve Qdrant's fused relevance scale by marking those additive lexical components as `skipped`. Both paths still apply low-signal and named-product identity filtering. Dense mode keeps additive trust boosts; RRF caps the combined configured/product trust delta at the original fused score.

## RAG Tuning

The core retrieval-selection knobs live in `~/.axon/config.toml` under `[ask]`.
Env vars with the same names are compatibility overrides, not the normal place
to store these values.

| TOML key | Env override | Default | Effect |
|----------|--------------|---------|--------|
| `ask.min-relevance-score` | `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Raise to tighten relevance on cosine/dense paths (0.6-0.7 for high-precision); lower if you get "no candidates". Skipped for hybrid/RRF named-vector mode because RRF scores are not cosine scores. |
| `ask.candidate-limit` | `AXON_ASK_CANDIDATE_LIMIT` | Model-tiered | Raw candidates admitted to ask-specific reranking; more improves recall but costs rerank work. |
| `ask.chunk-limit` | `AXON_ASK_CHUNK_LIMIT` | Model-tiered | Configured maximum chunks. Runtime complexity/model policy may use fewer. |
| `ask.full-docs` | `AXON_ASK_FULL_DOCS` | `6` | Compatibility-only legacy control. Unified retrieval uses bounded chunk context and does not execute full-document backfill. |

Additional ask controls:

| TOML key | Env override | Default | Effect |
|----------|--------------|---------|--------|
| `ask.max-context-chars` | `AXON_ASK_MAX_CONTEXT_CHARS` | Model-tiered | Configured maximum context characters. Runtime adaptive policy uses a smaller effective budget for simple/complex/exhaustive questions; diagnostics report both. |
| `retrieval.ask-hybrid-candidates` | `AXON_ASK_HYBRID_CANDIDATES` | Model-tiered | Dense and sparse prefetch candidates per arm before RRF fusion for `ask`. |
| `ask.authoritative-domains` | `AXON_ASK_AUTHORITATIVE_DOMAINS` | `` | Optional exact/suffix domains boosted after retrieval, including RRF mode. |
| `ask.authoritative-boost` | `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | Authority weight: additive on dense scores; on RRF it shares a combined trust delta capped at the original fused score |
| `ask.min-citations-nontrivial` | `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | Minimum unique citations for non-trivial answers |

## Notes

- LLM answer generation goes through the configured backend. By default this is Gemini headless; `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` is the preferred Gemini model override, with `AXON_HEADLESS_GEMINI_MODEL` kept as a legacy alias. `openai-compat` requires both a valid `AXON_OPENAI_BASE_URL` and synthesis model.
- The legacy full-document/backfill/cache controls remain parseable for configuration compatibility but are not executed by the unified retrieval-engine ask path. An explicit legacy override produces an ask warning instead of silently pretending it is active.
- Normal product/documentation questions suppress session/log/cache sources unless the query explicitly asks for session, transcript, log, or history content. Built-in product authority is fail-closed to Axon's small registered official-domain map; arbitrary docs-looking hosts cannot self-declare authority by placing a product name in their hostname/path. Use `ask.authoritative-domains` for operator-controlled trust extensions. Web citations retain page-level URLs so multiple pages on the same documentation host remain distinct evidence sources.
- The generic CLI forwarding mode was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon ask` through HTTP; use `axon serve` directly for external REST/MCP clients.
- If dense-only retrieval returns no candidates above the relevance threshold, lower `ask.min-relevance-score` or index more relevant content. Hybrid/RRF skips the cosine threshold and does not require general lexical overlap; candidates can still be rejected by low-signal or explicit named-product identity guards.
- `ask` queries the local knowledge base only. To search the live web, use `axon research`.
- For benchmarking RAG quality vs a baseline, use `axon evaluate`.
- `ask` enforces citation-quality gates:
  - Answers must include inline `[S#]` citations from retrieved context.
  - Non-trivial responses must satisfy `AXON_ASK_MIN_CITATIONS_NONTRIVIAL`.
  - Failed gates return structured insufficient-evidence output with next-index suggestions.
