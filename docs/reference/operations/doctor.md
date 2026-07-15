# Doctor
Last Modified: 2026-07-15

Doctor checks local configuration and service readiness.

## Checks

Doctor should verify required paths, Qdrant, TEI, Chrome/CDP, LLM backend
configuration, auth settings, schema versions, and common misconfiguration
patterns.

## Output

Human output should be actionable and concise. JSON output should use structured
diagnostic records with severity, component, status, and remediation text.

## Rule

Doctor reports should not mutate runtime state unless an explicit repair mode
is requested.
