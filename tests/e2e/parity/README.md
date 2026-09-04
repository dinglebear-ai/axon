# Cross-surface E2E reconciliation

`scripts/e2e/reconcile-surfaces.py` joins saved CLI, MCP, raw task-wire, and
HTTP evidence after the transport adapters run. It is deliberately offline:
the bundle points to evidence files beside itself and the reconciler never
contacts Axon or a provider.

Each execution records a `parent_scenario_id`, a unique per-surface
`execution_id`, the raw evidence path and digest, a transport-specific
`envelope`, and a shared `semantics` projection. Independent parity executions
compare fixture identity, lineage, terminal semantics, citations, error codes,
semantic values, and effects while allowing their envelopes to differ.
Multi-observer executions additionally require the exact same
`observed_operation_id`.

Coverage entries are joined by indexed operation ID against the single catalog
schema. Only passing records whose `kind` is `behavioral` and whose evidence
file exists earn the numerator. Contract-only declarations cannot satisfy the
threshold or the happy and negative evidence required for every critical
lifecycle. The denominator is the full authoritative advertised inventory.

Run it with:

```sh
python3 scripts/e2e/reconcile-surfaces.py path/to/bundle.json \
  --output path/to/parity-report.json
```

The canonical JSON report is stable for identical inputs. Every failure names
the scenario, capability, surface, differing invariant, and evidence path so a
CI artifact points directly at the relevant transport record.
