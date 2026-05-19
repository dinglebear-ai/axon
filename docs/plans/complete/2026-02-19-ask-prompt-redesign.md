# Design: `ask` Prompt Redesign

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Status:** Approved

---

## Problem

The current `ask` system prompt is 10 words: `"Answer only using provided context. Cite sources like [S1]."` It:

- Duplicates grounding rules already in the context preamble (both say "answer only from sources")
- Gives no citation placement guidance (the model cites sporadically or not at all)
- Gives no synthesis guidance (model quotes single sources instead of integrating)
- Gives no footer sources guidance (no URL list for traceability)
- Gives no gap-handling guidance (model fills missing information from general knowledge)

Primary observed failure mode: **vague or missing citations** on technical implementation questions.

---

## Design

### Approach

**Option B — system prompt owns all behavioral rules; context preamble becomes pure data.**

Rationale: the system prompt is where the model is most attentive to behavioral instructions. Having rules split between the system turn and the user turn creates redundancy and reduces reliability. Consolidating into the system turn and reducing the preamble to a clean "Sources:" header is architecturally cleaner and produces more consistent behavior.

---

### 1. New System Prompt (`ask_llm_streaming` / `ask_llm_non_streaming`)

```
You are a precise technical research assistant. You answer questions exclusively from the retrieved source documents provided in the context. Your rules:

1. CITATIONS — Cite inline immediately after each claim using [S#] labels. When multiple sources support the same point, cite all of them: [S1][S3]. Never make a claim without a citation.
2. FOOTER — After your answer, add a "## Sources" section listing each cited source number and its URL, e.g. "[S1] https://..."
3. SYNTHESIS — Integrate information from multiple sources into a unified answer. Do not quote or summarize sources one by one.
4. GAPS — If the sources do not fully answer the question, explicitly state what is covered and what is not. Do not fill gaps from general knowledge.
5. PRECISION — For technical questions, be specific: include exact values, function names, file paths, and configuration keys when the sources provide them.
```

### 2. New Context Preamble (`build_ask_context` in `ask.rs`)

Remove the three instruction lines from the preamble. Replace with:

```
Sources:
{joined source entries}
```

The preamble currently opens with:
```
Answer only from the provided sources.
Cite supporting sources inline using [S#] labels.
If the sources are incomplete, say so explicitly.
```
These lines are removed — the system prompt now owns all rules.

### 3. New Baseline Prompt (`baseline_llm_streaming` / `baseline_llm_non_streaming`)

```
You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them.
```

---

## Token Impact

| Prompt | Before | After | Delta |
|--------|--------|-------|-------|
| System prompt (ask) | ~15 tokens | ~110 tokens | +95 |
| Context preamble | ~30 tokens | ~5 tokens | -25 |
| **Net per ask call** | | | **+70 tokens** |

Acceptable overhead for the quality gain on technical RAG queries.

---

## Files to Change

| File | Change |
|------|--------|
| `crates/vector/ops/commands/streaming.rs` | Replace system prompt in `ask_llm_streaming` + `ask_llm_non_streaming`; replace baseline system prompt in `baseline_llm_*` |
| `crates/vector/ops/commands/ask.rs` | Remove 3-line instruction block from context preamble in `build_ask_context`; keep `Sources:` header |

---

## Success Criteria

- Every factual claim in `axon ask` output has an `[S#]` citation
- A `## Sources` footer lists the cited URLs
- When sources only partially answer a question, the response explicitly says what is and is not covered
- No regressions in `cargo test` / `cargo clippy`
