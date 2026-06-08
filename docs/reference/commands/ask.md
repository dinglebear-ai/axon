# axon ask
Last Modified: 2026-06-01

RAG-powered Q&A. Retrieves relevant chunks from the local Qdrant knowledge base, reranks them by relevance, builds a context window, and calls the configured LLM to generate a grounded answer.

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
| `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for answer generation when using Gemini headless. |
| `AXON_OPENAI_BASE_URL` | OpenAI-compatible API root when `AXON_LLM_BACKEND=openai-compat`. |
| `AXON_OPENAI_MODEL` | Model name for the OpenAI-compatible backend. |
| `AXON_OPENAI_API_KEY` | Optional bearer token for the OpenAI-compatible backend. |
| `AXON_SERVER_URL` | Optional generic server endpoint. If set, buffered `ask` requests use `axon serve`; streaming requests stay in-process. |

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
| `--diagnostics` | `false` | Print retrieval diagnostics (candidate pool, reranked pool, chunks selected, full docs, supplemental, context chars, authority ratio, dropped by allowlist, top domains). |
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
axon ask "what is the max crawl depth?" --json

# Ask through a running server with buffered output
AXON_SERVER_URL=http://127.0.0.1:8001 axon ask --no-stream "what changed in server mode?"
```

## RAG Pipeline

1. Embed the question via TEI
2. Query Qdrant for top `ask.candidate-limit` (default: 250) candidate chunks
3. Apply the score threshold only on cosine/dense paths. `ask.min-relevance-score` (default: 0.45) is used for legacy unnamed-vector collections, named dense searches, named-vector collections with hybrid disabled, and named-vector searches whose sparse query is empty.
4. Skip that threshold in hybrid/RRF named-vector mode. RRF scores are rank-fusion outputs rather than cosine scores, so ask keeps the loose topical-overlap gate and uses Qdrant's fused ordering.
5. Rerank by the mode-appropriate score/order; take top `ask.chunk-limit` (default: 20)
6. For top `AXON_ASK_FULL_DOCS` (default: 6) documents, backfill additional chunks from the same document
7. Assemble context up to `AXON_ASK_MAX_CONTEXT_CHARS` (model-tiered fallback: 1,000,000 large, 400,000 GPT/Codex, 128,000 local Gemma, 40,000 unknown)
8. Call the configured LLM backend with context + question
9. Apply response-quality gates (citations + policy checks)
10. Print the normalized answer

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

Raw rendered retrieval context is omitted from default explain JSON so CLI, MCP, REST, and runner artifacts do not leak the full prompt fragment by accident. Use `.explain.context.final_source_order` for source ordering metadata, `.explain.context.context_bytes_used` / `.context_bytes_budget` for the concrete budget invariant, `.explain.context.context_chars_used` for Unicode character count, and `.explain.candidates[]` for candidate scores, filter decisions, selected context ranks, insertion modes, and snippets.

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
context selection + bounded full-doc fetch
  |
  v
ask --explain retrieval harness
  |
  +--> ranking bug? tune scoring/filtering
  +--> selection bug? tune context selection
  +--> corpus gap? crawl/index better docs
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
      "score_kind": "cosine",
      "vector_mode": "unnamed",
      "hybrid_search_enabled": true
    },
    "candidates": [
      {
        "id": "candidate-1",
        "url": "https://code.claude.com/docs/en/plugins",
        "retrieval_score": 0.17,
        "rerank_score": 0.62,
        "score_components": [
          { "name": "retrieval_score", "value": 0.17, "status": "applied" },
          { "name": "authority_boost", "value": 0.35, "status": "applied" }
        ],
        "filter_decisions": [{ "kind": "kept" }],
        "selection_decisions": [{ "kind": "selected_top_chunk" }]
      }
    ],
    "context": {
      "planned_full_doc_urls": [],
      "full_doc_fetch_skipped": true,
      "full_doc_fetch_mode": "cosine",
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

`retrieval_score` scale depends on retrieval mode. Cosine/dense paths use cosine-like scores and may apply `ask.min-relevance-score`; RRF paths use rank-fusion scores, mark additive rerank components as `skipped`, and do not apply the cosine threshold.

## RAG Tuning

The core retrieval-selection knobs live in `~/.axon/config.toml` under `[ask]`.
Env vars with the same names are compatibility overrides, not the normal place
to store these values.

| TOML key | Env override | Default | Effect |
|----------|--------------|---------|--------|
| `ask.min-relevance-score` | `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Raise to tighten relevance on cosine/dense paths (0.6-0.7 for high-precision); lower if you get "no candidates". Skipped for hybrid/RRF named-vector mode because RRF scores are not cosine scores. |
| `ask.candidate-limit` | `AXON_ASK_CANDIDATE_LIMIT` | `250` | More candidates = better recall, slower reranking |
| `ask.chunk-limit` | `AXON_ASK_CHUNK_LIMIT` | `20` | Chunks in final LLM context |

Remaining runtime ask controls are still env-only until typed TOML fields exist:

| Variable | Default | Effect |
|----------|---------|--------|
| `AXON_ASK_MAX_CONTEXT_CHARS` | Model-tiered | Total context characters; defaults by model family unless explicitly overridden |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | `` | Optional comma-separated domains to boost in reranking |
| `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | Score boost for authoritative-domain matches |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | Minimum unique citations for non-trivial answers |

## Notes

- LLM answer generation goes through the configured backend. By default this is Gemini headless; `AXON_HEADLESS_GEMINI_MODEL` is used as the Gemini model override on that backend.
- If you get "No candidates met relevance threshold", lower `ask.min-relevance-score` in `~/.axon/config.toml` or run `axon crawl`/`axon embed` to add more content to the collection. This message comes from cosine/dense retrieval paths; hybrid/RRF named-vector mode skips the cosine threshold.
- `ask` queries the local knowledge base only. To search the live web, use `axon research`.
- For benchmarking RAG quality vs a baseline, use `axon evaluate`.
- `ask` enforces citation-quality gates:
  - Answers must include inline `[S#]` citations from retrieved context.
  - Non-trivial responses must satisfy `AXON_ASK_MIN_CITATIONS_NONTRIVIAL`.
  - Failed gates return structured insufficient-evidence output with next-index suggestions.
