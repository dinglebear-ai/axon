# Hermetic E2E CI

The promoted pull-request check is named **`E2E Hermetic Required`** in the
workflow of the same name. This exact, case-sensitive job name is the repository-owned
branch-protection context. It builds Axon, denies non-loopback network
access, runs the composed double-backed scenarios, verifies teardown, and
retains only a sanitized JSON report. The report is uploaded even when a time
budget or report-validation gate fails so operators can diagnose the run.

Task `axon_rust-nnzde.24` promotes the repository contract; it does not mutate
GitHub branch protection. An administrator may separately add that context only
after the checked-in qualification preflight passes. The preflight consumes the
catalog denominator, critical lifecycles, fixed eight-mutant sensitivity set,
canonical parity/reporting, signed cleanup evidence, zero-quarantine policy,
trusted rolling reliability, wall/resource ceilings, and artifact size.
Missing, unknown, expired, redaction-unsafe, or cleanup-uncertain evidence fails.
The workflow p95 is computed from at least five digest-validated canonical runs
in the restored trusted `main` history. Every sample carries a distinct GitHub
workflow run/attempt identity and exact repository, workflow, and ref provenance;
the restore step cross-checks those claims and tested commit SHA against the
GitHub Actions API-derived run index before qualification;
the current pull-request run is never counted as trusted history. A claimed run
count, duplicate report, or a single current duration cannot satisfy promotion.

Bypass is limited to a repository administrator during a declared incident.
The merge record must link the incident, explain why the check could not run,
and receive a successful post-merge rerun. Flakes, slow runs, and convenience
are not bypass reasons. Required runs use `cancel-in-progress: false`, so a
superseding push cannot interrupt teardown or evidence retention. Concurrent
pull requests use distinct workflow/ref and E2E run namespaces.

Rollback removes only `E2E Hermetic Required` from the external required-status
contexts. Do not disable or delete `.github/workflows/e2e-hermetic.yml`: its
push, schedule, dispatch, evidence, and reliability-history measurement remain
active. Re-promotion requires the qualification preflight to pass again.

Local macOS reproduction uses `just e2e-hermetic`. The launcher discovers a
Homebrew OpenSSL 3 prefix when available and otherwise leaves TLS discovery to
Cargo/pkg-config; it never relies on a transient machine-specific `/tmp` path.
The authoritative budgets and decision record live in
`config/e2e/hermetic-required-policy.json`.
