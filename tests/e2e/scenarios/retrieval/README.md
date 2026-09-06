# Retrieval and synthesis scenario contract

`execute.py --axon-bin PATH --outdir PATH` is the required CI entry point. It
preflights the supplied executable and its configured providers, indexes the
canonical corpus into a run-owned collection, executes every declared operation
without a shell, derives the transport-neutral response contract from actual
Axon stdout, and invokes `oracles/grounding.py`. It cannot accept a normalized
response file and has no synthetic or provider-error fallback. The same data
fixtures and semantic oracle IDs are suitable for projection through the shared
CLI, MCP, and HTTP catalog adapters as those surface groups are enabled.

The local/live command is:

```text
python3 tests/e2e/scenarios/retrieval/execute.py \
  --axon-bin target/debug/axon --outdir target/e2e-retrieval \
  --repetitions 5 \
  --http-url http://127.0.0.1:3000 \
  --mcporter-bin /path/to/mcporter --mcp-selector axon.axon
```

HTTP and MCP flags are mandatory and enable their applicable structured
transport paths; CLI is always exercised. Every real run unconditionally
launches the hermetic provider doubles and proves the unavailable, timeout,
queue-full, malformed-response, dimension-mismatch, schema-mismatch, and
token-limit classifications. This command is intentionally documented here but
is not yet claimed as CI-wired; the workflow integration bead owns that gate.

Semantic evidence must be emitted by Axon itself. The runner reads canonical
`source_id`, `chunk_id`, and `canonical_uri` lineage and resolves supporting
text only from the immutable corpus file named by that URI; it never infers a
source or citation from arbitrary response prose. Harness wall time is recorded
separately and never substituted for a public timing field.

Evidence requirements follow repository DTOs rather than one invented common
shape:

- `query`, `code-search`, `ask`, and `evaluate` require canonical citation
  lineage and grounding. Ask requires `retrieval`, `context_build`, `llm`, and
  `total`; evaluate requires its seven public `EvaluateTiming` fields.
- `retrieve` requires `content` and `matched_url`; `chat`, `search`, `extract`,
  `train`, and `suggest` use their operation-specific public result fields.
- `summarize` requires `SummarizeTiming` (`scrape`, `llm`, `total`); research
  requires its public total timing.
- Provider counters, reservation IDs, session IDs, and artifact IDs are audited
  and registered only when explicitly present in Axon output or events. Every
  emitted counter must be a nonnegative integer. Missing fields are never
  replaced with harness estimates or pre-registered identities.

Fixture-provider verification launches a real loopback OpenAI/TEI double per
failure mode, rewires the Axon subprocess through `AXON_OPENAI_BASE_URL` or
`TEI_URL`, and asserts the error code returned by Axon. Merely setting a mode
environment variable on a fake executable is not an accepted execution path.

The prompt set is correctness coverage, not latency percentile evidence. Timing
is reported as separate embed, retrieval, context, and synthesis buckets. Every
collection, job, source, artifact, chat/session, provider reservation, and
evidence file created by a runner must be registered in the run's append-only
isolation manifest before the operation can be considered passing.

Chat follow-ups must retain context only when the normalized response `run_id`
matches the current manifest run. A different run must begin with no history.
Provider failures are classified failures or expected negative outcomes; they
can never be converted to a passing synthesis result.
