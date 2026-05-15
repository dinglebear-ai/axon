# Ask Synthesis Backend

`axon ask` uses the Gemini CLI headless path. The path is intended only for synthesis: no tools, no permissions, no warm session, and no workspace mutation.

Gemini is selected by `AXON_HEADLESS_GEMINI_CMD` (default: `gemini`). Set `AXON_HEADLESS_GEMINI_MODEL` to override the Gemini model. Set `AXON_HEADLESS_GEMINI_HOME` to copy auth from a prepared Gemini home instead of the process HOME.

Safety rules:

- Gemini yolo and auto-approval modes are forbidden.
- Any observed tool event in headless output is a hard error.

Benchmarking:

- Latency harness: `scripts/bench-ask.sh`
- Perf notes: `docs/perf/README.md`
- Quality set: `docs/eval/README.md`
- Parity report: `docs/perf/quality-parity-2026-05-07.md`

Explain mode:

- `axon ask --explain --json "<question>"` runs retrieval, reranking, and context assembly, then skips Gemini synthesis.
- The response includes `explain.retrieval`, per-candidate `score_components`, `filter_decisions`, `selection_decisions`, and `explain.context.final_source_order`.
- Use `--diagnostics` for aggregate counters; use `--explain` when debugging why a specific source ranked, survived filtering, or entered/left the prompt context.
- Score scale is mode-specific: cosine/dense scores can be compared to `ask.min-relevance-score`; RRF scores are rank-fusion values and mark additive rerank boosts as skipped.
