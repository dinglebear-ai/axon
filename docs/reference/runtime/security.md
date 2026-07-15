# Runtime Security
Last Modified: 2026-07-15

Security covers auth, SSRF defense, local-source policy, redaction, and
destructive-operation safeguards.

## Requirements

- Network acquisition must use SSRF-safe clients.
- Local execution and local paths require policy checks.
- HTTP auth is required for non-loopback service exposure.
- Destructive operations require confirmation and policy.
- Secrets are redacted before logging or persistence.

## Review Focus

Every new adapter must define trust boundaries, credential handling, and
execution behavior before it is exposed through source requests.
