---
status: ok
subtopic: depth-adaptive-synthesis
---

# Depth-Adaptive RAG Synthesis Prompts

## Core finding: classifier beats single adaptive instruction

The weight of 2024-2025 evidence favors a **pre-synthesis classifier (router) over a single adaptive instruction** embedded in the synthesis prompt. The key reasons:

1. **Routing is cheap**: Pattern matching against keywords ("what is", "list all", "tell me everything about", "how do I", "show me all") costs zero tokens and sub-millisecond latency. Even an LLM-based classifier at a tiny model (haiku-class) adds ~100ms, acceptable given the 26x–90x speed improvement reported for simple queries when routing avoids full RAG pipeline activation.

2. **Prompt template specialization outperforms conditional instructions**: The promptql.io production case reports that "the same LLM gave much better answers when the prompt matched the query intent" with specialized templates. A single prompt with an adaptive clause ("answer concisely for simple questions, exhaustively for comprehensive ones") leaves ambiguity resolution to the LLM at synthesis time — a more expensive and less reliable point of control.

3. **Industry pattern (2024)**: PromptQL's intent-driven architecture categorizes queries into four buckets — General Information, Guide Requests, Example Requests, Troubleshooting — with query-specific response protocols per bucket. TowardDataScience (2024) confirms "routing via different prompt templates depending on the user query."

## What "depth" means (two axes)

The advisor distinguishes these correctly; found evidence supports the same split:

- **Verbosity** (length/elaboration per claim): controlled by word limits ("no more than 200 words"), chain-of-thought scratchpad, or explicit length constraints in the template.
- **Breadth** (enumeration completeness): controlled by asking for "diverse and comprehensive coverage," multiple passes (first pass: reference chunks individually; second pass: integrate comprehensively), or by activating multi-step retrieval with iterative refinement.

These need separate handling. A concise-vs-comprehensive switch does not address exhaustive enumeration (e.g., "list all configuration options"). Exhaustive enumeration requires a distinct template that instructs the model to treat the retrieved set as a complete inventory and to emit all items, not just the most relevant ones.

## How production systems distinguish intent

The Query Optimization Lifecycle (QOL) Framework (arxiv 2412.17558) provides a five-phase pipeline: Intent Recognition → Query Transformation → Retrieval Execution → Evidence Integration → Response Synthesis. Intent classification lives in phase 1, before the synthesis prompt is constructed — this is the architecture that works.

Concrete signals used in classifiers:
- Prefix patterns: "what is" / "define" → concise definitional template
- "list all" / "enumerate" / "show me all" → exhaustive inventory template
- "tell me everything about" / "comprehensive guide" / "in detail" → broad synthesis template
- "how do I" / "step-by-step" → procedural template
- Default (no pattern matched) → current concise template

## Current axon prompt gap

`ASK_RAG_SYSTEM_PROMPT` at `src/vector/ops/commands/streaming.rs:24` hardcodes "Provide a concise answer." This is the correct default for unknown query intent. The gap is that queries explicitly requesting exhaustive coverage receive the same concise instruction — the model will stop after finding enough evidence for a brief answer, systematically under-delivering on breadth.

## Recommended approach

A lightweight keyword classifier on the query string before `ask_payload` dispatches would select among 2-3 prompt templates:
- `concise` (current): "provide a concise answer" — default
- `exhaustive`: "enumerate ALL instances of X from the sources; do not stop after finding one or two examples; treat the source set as a complete inventory and list every relevant item"
- `detailed`: "provide a thorough, well-organized answer; prioritize completeness over brevity"

This avoids modifying the synthesis prompt's security-critical language (injection defense section) while enabling depth adaptation.

## Sources
- [Beyond Basic RAG: Intent-Driven Architectures](https://promptql.io/blog/beyond-basic-rag-promptqls-intent-driven-solution-to-query-inefficiencies)
- [Routing in RAG Driven Applications (TowardDataScience)](https://towardsdatascience.com/routing-in-rag-driven-applications-a685460a7220/)
- [A Survey of Query Optimization in LLMs (arxiv)](https://arxiv.org/html/2412.17558)
- [Agentic RAG Series Part 3](https://sajalsharma.com/posts/comprehensive-agentic-rag/)
- [Top 5 LLM Prompts for RAG (Scout)](https://www.scoutos.com/blog/top-5-llm-prompts-for-retrieval-augmented-generation-rag)
