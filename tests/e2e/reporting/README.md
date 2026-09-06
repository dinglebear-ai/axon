# Canonical E2E reporting

`scripts/e2e/lib/reporting.py` owns the sole execution/evidence result schema.
Later observability and release-qualification work must add or project fields;
it must not create a competing result record. Scenario attempt history is
append-only, so a diagnostic retry cannot erase the first failure.
Cleanup is fail-closed: the canonical record has one exact sanitized shape, and
`success` is true exactly when its residual list is empty. Release qualification
revalidates this contract before accepting a report as evidence.

`scripts/e2e/lib/redaction.py` is both the pre-log and pre-package security
boundary. Evidence packaging is allowlist-only, rejects special files and link
tricks, enforces byte ceilings, and scans dynamic canaries in transformed forms.
Only the sanitized package and its checksummed manifest may be uploaded.
