# Fixed sensitivity program

This directory declares repository-owned oracle self-tests for Axon's critical
E2E invariants. A mutant changes only an in-memory copy of an actual scenario fixture;
it never patches, configures, or adds a control surface to the production Axon
binary. Every worker also receives an independent temporary ownership root,
which is destroyed before its result is returned.

`just e2e-mutations` runs the bounded eight-mutant PR subset. It covers
lifecycle, publication, transport parity, grounding, redaction, teardown, and
provider failure propagation. `just e2e-mutations full` runs all eleven fixed
mutants and is suitable for scheduled or manual qualification. The hermetic
workflow measures the representative subset as its `mutation-sensitivity`
stage; broader rotating/generated mutation engines remain deferred.

Outcomes use the suite's sole canonical evidence/report schema and are retained
beside the hermetic report. They include only mutant IDs, repository codepaths,
detecting scenarios, invariants, outcomes, shards, and runtimes. A survivor or
harness failure makes the command fail. A temporary exception must name an
owner, tracked reason, and unexpired ISO date in `exceptions.json`; invalid or
expired entries fail closed. Timeouts and worker errors are harness failures
and never earn a killed-mutant result.
