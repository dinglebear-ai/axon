# Axon Ask Model Comparison

Date: 2026-06-07
Binary: `axon 5.1.2`

## Files

- `01-gpt-5.5-current.md` - `gpt-5.5` via `https://cli-api.tootie.tv/v1`
- `02-gemini-3.5-flash-low.md` - `gemini-3.5-flash-low` via `https://cli-api.tootie.tv/v1`
- `03-gemma-4-e4b-q4-llamacpp.md` - `ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M` via local llama.cpp at `http://127.0.0.1:8080/v1`

## Run Health

All 30 `axon ask --no-stream` calls exited with status `0`.

Elapsed time summary:

| Model | Total | Avg | Min | Max |
|---|---:|---:|---:|---:|
| gpt-5.5 | 173s | 17.3s | 12s | 22s |
| gemini-3.5-flash-low | 101s | 10.1s | 8s | 13s |
| Gemma 4 E4B Q4 | 92s | 9.2s | 6s | 12s |

No stderr logs contained context-limit errors, provider failures, panics, or non-zero command statuses.

## Gemma Context Limits

Before running the Gemma suite, a smoke `ask --explain --json` run verified the rebuilt binary resolves Gemma to the local tier:

```json
{
  "ask_max_context_chars": 300000,
  "ask_chunk_limit": 20,
  "ask_candidate_limit": 120,
  "ask_hybrid_candidates": 100,
  "context_chars": 47983,
  "full_docs_selected": 1,
  "chunks_selected": 20
}
```

This is appropriate for the currently loaded llama.cpp server, which reports a 131072-token context window for the Gemma 4 E4B Q4 model.

## Quality Findings

Gemma 4 E4B Q4 did a decent job overall. Across the ten Axon-domain questions, it generally covered the same major facts as `gpt-5.5` and `gemini-3.5-flash-low`: architecture boundaries, services-first routing, SQLite jobs, watch scheduler status, SearXNG/Tavily behavior, LLM backend configuration, Spider feature-flag gotchas, Qdrant hybrid search, and ask context safeguards.

Strengths:

- Gemma answers were concise, well structured, and citation-bearing.
- It matched the other models on the most operationally important facts, especially the Spider feature flags, watch scheduler implemented/parsed subcommands, and high-level subsystem map.
- It was the fastest of the three in this run.
- It completed all ten questions without context errors against the local llama.cpp server.

Weaknesses:

- Gemma was somewhat more generic in a few answers and sometimes omitted nuance that `gpt-5.5` included, such as older-doc conflict notes or deeper caveats.
- Like the other models, it can only answer from indexed context; for Q10 it described the indexed model-tier docs and did not know about the just-applied Gemma-specific tier unless that code/doc is indexed.
- It occasionally leaned on broader statements where `gpt-5.5` gave more precise implementation detail.

Bottom line: Gemma 4 E4B Q4 is good enough for local Axon `ask` use on this corpus when the context budget is capped correctly. I would not treat it as equal to the larger remote models for subtle code-review-grade distinctions, but for grounded operational Q&A it produced similar and usable results.
