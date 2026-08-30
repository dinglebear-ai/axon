# E2E flake and quarantine governance

Required E2E tests begin with zero quarantine entries. A failing scenario is a
failure until an owner accepts a genuine flake by adding the minimal record in
`quarantine.json`: scenario, owner, substantive rationale, tracked HTTPS issue,
tier/environment, creation and expiration dates, and measurable restoration
criteria. The scenario continues to execute. Its results remain in the
canonical report, its first failure remains visible, and it contributes nothing
to healthy coverage.

Quarantine is forbidden for security, trust-boundary, authentication,
secret-redaction, and teardown/cleanup scenarios. It also cannot turn product,
authentication/network, or cleanup failures into passing results. Expired,
unowned, unknown, duplicated, or incompletely described entries fail closed.

To restore hard-gate status, fix the tracked defect, demonstrate the restoration
criteria using the segmented rolling reliability artifact, remove the entry,
and let the scenario pass without a diagnostic retry. Renewal is a reviewed
change to the existing tracked issue with new evidence, rationale, criteria,
and a new bounded expiration; silently extending a date is not acceptable.

Hermetic required scenarios never retry. A live diagnostic retry is limited to
one declared retry-safe attempt, serialized after a randomized bounded backoff
in a fresh namespace. It requires verified teardown of ambiguous mutation and
consumes the suite-wide budget. Upload, watch, migration, and destructive work
uses saved diagnostics unless that safety proof exists. Queue expiry and open
circuit-breaker outcomes are explicit failures.

Run locally:

```bash
just e2e-flake-governance target/e2e/report.json local target/e2e/reliability.json
```

The reliability artifact segments rolling pass rate and runtime by scenario,
tier, environment, and exact provider-version set. Three failures in the latest
five observations emit a tracked-defect escalation signal. Workflow artifacts
provide the notification surface without write permissions or new services.
