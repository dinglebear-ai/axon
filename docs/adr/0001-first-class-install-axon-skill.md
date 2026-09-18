# ADR 0001: Make install-axon the first-class guided installer

- Status: Accepted
- Date: 2026-09-18

## Context

Axon already owns executable installation and deployment seams: a fail-closed release installer with source-build fallback, `axon setup`, a preferred Incus deployment model, supported bare-metal systemd, bearer/OAuth HTTP auth, provider preflight/doctor/smoke, and an existing Axon client plugin. The missing piece is one guided workflow that composes those surfaces safely for a new user.

The current release trust anchor is intentionally incomplete, so public release installation cannot yet be the default. Incus also uses a split model where guest `axon-native.service` is disabled unless the guest owns its SQLite database.

## Decision

Add `install-axon` to the existing `plugins/axon` package. Until the signing trust anchor is published, install from reviewed source with `AXON_INSTALL_METHOD=build ./install.sh`. Use `axon setup init` as the config owner. Offer local, Incus, and bare-metal modes while preventing shared SQLite from being opened by host and guest servers simultaneously.

Offer bearer, Google OAuth, or OAuth plus static bearer. OAuth requires public URL/client credentials/admin email. Custom OIDC endpoints remain advanced and unbranded until compatibility is proved. Non-loopback remains fail-closed without auth.

Codex app-server is a supported synthesis backend and stays isolated from user Codex configuration by default. Remote exposure and existing service/proxy changes use current official docs plus backup/diff/approval gates.

## Consequences

- One skill can guide install through live MCP verification without duplicating Axon runtime logic.
- Signed release installation can replace the source-build first step later without changing setup/deployment semantics.
- Incus onboarding reflects the current split-database safety model rather than older all-in-guest assumptions.
