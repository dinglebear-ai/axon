# Axon observability E2E contract

`scripts/e2e/lib/observability-assertions.py` is the shared oracle boundary used
inside capability scenarios. It does not define a runner, telemetry envelope, or
report schema. A source/retrieval/stateful scenario captures its CLI, MCP, and
HTTP observations, reads the owned job's `axon_observe_*` rows with
`load_runtime`, calls `evaluate`, and appends the returned outcomes to the
canonical `.11` `Scenario.invariants` list.

The stable oracle IDs cover correlation, causal event order/cardinality,
retries, provider health and failure taxonomy, terminal agreement, redaction,
and timing reconciliation. `multi_observer` means several surfaces observe one
execution and therefore must share a job ID. `parity` means independent
executions that must have distinct job IDs but share an explicit equivalence
group. Epoch timestamps establish order only; duration assertions use the
scenario's monotonic clock and a declared tolerance.

Inputs must already be bounded to the run's ownership manifest. The loader
opens SQLite read-only, queries exactly one job, and reads provider-health rows
only for the explicitly owned provider IDs. The canonical supervisor accepts
`observability_capture` and `observability_db` plan fields (or matching CLI
flags), evaluates the oracles after the scenario child exits and before
teardown, appends outcomes to `.11` invariants, then runs the authoritative
teardown. Missing or contradictory observability fails the scenario as a
product failure. The canonical report retains only sanitized oracle details;
raw logs, databases, credentials, private paths, and provider payloads remain
teardown-owned and are never report artifacts.

The measured hermetic composed path executes the source contract against the
real Axon binary and owned loopback providers. The trusted-live workflow calls
`tests/e2e/scenarios/observability/live_entry.py` with its allocation descriptor;
the same production-shape assertions and canonical report projection run there,
without weakening the live trust gate or taking ownership away from teardown.
