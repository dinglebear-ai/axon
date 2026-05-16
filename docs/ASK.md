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

### Retrieval Quality Gates

Fast gates:
- token policy unit tests
- explain/schema unit tests
- selection unit tests

Medium gates:
- `scripts/evaluate-retrieval.sh`
- `ALLOW_MISS=1 scripts/evaluate-retrieval.sh` for exploratory sweeps with known misses

Slow gates:
- `axon evaluate`
- `scripts/evaluate-ask-golden.sh`
- `scripts/bench-ask.sh`

Use slow gates for release signoff, not for every small retrieval tuning loop.

Explain mode:

- `axon ask --explain --json "<question>"` runs retrieval, reranking, and context assembly, then skips Gemini synthesis.
- The response includes `explain.retrieval`, per-candidate `score_components`, `filter_decisions`, `selection_decisions`, and `explain.context.final_source_order`.
- Use `--diagnostics` for aggregate counters; use `--explain` when debugging why a specific source ranked, survived filtering, or entered/left the prompt context.
- Score scale is mode-specific: cosine/dense scores can be compared to `ask.min-relevance-score`; RRF scores are rank-fusion values and mark additive rerank boosts as skipped.

Streaming mode:

- `axon ask "<question>"` prints Gemini token deltas as they arrive for interactive use by default.
- Use `--no-stream` to disable answer streaming and render only the final response.
- Streaming uses the in-process ask path. If `AXON_SERVER_URL` / `--server-url` is set, the CLI silently ignores it for that request because `/v1/ask` is a buffered JSON endpoint.
- `--json` and `--explain` remain buffered.

Follow-up mode:

- Successful non-explain `ask` turns are saved locally under `$AXON_DATA_DIR/ask-sessions/` (default: `~/.axon/ask-sessions/`).
- The human CLI output prints the active `Session:` after timing. JSON output includes `"session": "<name>"`.
- When `--session` is omitted, Axon uses the most recently successful ask session from `$AXON_DATA_DIR/ask-sessions/latest`, falling back to `default`.
- Use `axon ask --follow-up "<question>"` to include recent turns from the active local session.
- Use `--session <name>` to keep separate local threads and `--reset-session` to clear one before changing topics.
- Follow-up context is prepended by Axon before retrieval/synthesis; Gemini headless still runs as a stateless one-shot process, and answers must still be grounded in retrieved context with `[S#]` citations.
