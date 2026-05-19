# Fastlearn Design
Last Updated: 22:00:00 | 02/26/2026 EST

## Goal
Add a new `fastlearn` command/action that leaves existing `research` behavior fully intact while providing:
- bounded concurrent web extraction (default 8 workers),
- best-effort auto-embedding for every successful extraction,
- live streamed runtime activity and synthesis output,
- explicit stage timings and structured failure reporting.

## Non-Goals
- No behavior change to existing `research` command/action.
- No Qdrant-first retrieval path (that remains `ask`).
- No required hard-fail on partial embed errors.

## Product Decisions
- New command name: `fastlearn`.
- Default extraction concurrency: `8`.
- Data source: web-first (`Tavily -> URL fetch -> extract`).
- Embedding policy: auto-embed touched pages, best-effort, graceful degradation.
- Output UX: verbose by default (stage + per-page live events).

## High-Level Architecture
`fastlearn` is a separate execution path:
1. Search: Tavily returns ranked URLs.
2. Extract: bounded-concurrency workers fetch + extract per URL.
3. Embed: each successful extraction is embedded immediately (best-effort).
4. Synthesize: LLM synthesizes from successful extractions only (stream tokens where possible).
5. Report: counters, failures, and stage timings.

### Separation Guarantee
- `research` keeps current implementation and output behavior.
- `fastlearn` is implemented in new modules/routes with no shared behavior toggles that can alter `research`.

## Interfaces

### CLI
- New command: `axon fastlearn <query>` (with existing global flags where applicable).
- New handler: `run_fastlearn(cfg: &Config) -> Result<(), Box<dyn Error>>`.
- JSON mode returns structured payload matching `research` shape plus new diagnostics/timings.

### MCP
- New `action: "fastlearn"` in unified `axon` tool routing.
- Response mirrors CLI JSON payload fields for parity.

## Data Contract
`FastlearnResult` payload:
- `query`
- `search_results[]`
- `extractions[]`
- `summary`
- `usage { prompt_tokens, completion_tokens, total_tokens }`
- `embed_stats { attempted, succeeded, failed }`
- `timing_ms { search, extraction, embed, synthesis, total }`
- `counters { searched, fetched_ok, extracted_ok, embedded_ok, failed }`
- `failures[]` entries with `{ url, stage, error }`

## Execution Model

### Concurrency
- Use bounded stream processing (`buffer_unordered(8)`) for fetch/extract units.
- Each URL is isolated; errors are captured per item and do not abort the full run.

### Progress Streaming (Verbose Default)
- Stage markers:
  - search started/completed (+timing, URL count)
  - extraction started/completed
  - synthesis started/completed
- Per-page events as workers finish:
  - fetch/extract success/failure
  - embed success/failure
- Rolling counters printed continuously.

### Synthesis Streaming
- Attempt SSE token streaming for synthesis output.
- If stream unsupported/fails, log one fallback warning and continue non-streaming.

## Error Handling & Exit Semantics
- Best-effort policy:
  - individual fetch/extract/embed failures are non-fatal.
  - synthesis uses only successful extractions.
- Exit non-zero only when no meaningful output can be produced (for example, zero successful extractions and no summary).
- Partial success exits zero with warnings and full failure diagnostics.

## Testing Strategy
1. Unit tests for aggregation/counters under mixed success/failure.
2. Unit tests for best-effort embedding behavior (embed failures recorded, pipeline continues).
3. Unit tests for timing fields and deterministic payload shape.
4. Contract tests for CLI JSON and MCP parity.
5. Integration smoke test for live progress + streaming fallback.

## Migration Notes
- `research` remains untouched and available.
- `fastlearn` provides the new parallel/streamed/auto-embed workflow without command collision.

## Risks and Mitigations
- LLM/host saturation at concurrency 8:
  - Mitigation: bounded concurrency and clear failure capture.
- Provider-dependent SSE differences:
  - Mitigation: robust fallback to non-streaming synthesis.
- Partial embedding consistency:
  - Mitigation: explicit per-page embed accounting and final diagnostics.
