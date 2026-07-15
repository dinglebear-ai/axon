# Troubleshooting
Last Modified: 2026-07-15

Troubleshooting starts with evidence from the real checkout and runtime.

## Order

1. Run doctor.
2. Check config resolution.
3. Check service connectivity.
4. Inspect durable job and source status.
5. Inspect recent events and artifacts.
6. Reproduce with the smallest command.

## Common Areas

- Qdrant or TEI unavailable
- Chrome/CDP render failures
- auth or local-source policy denial
- schema drift after generated artifacts change
- stale source generations or cleanup debt
