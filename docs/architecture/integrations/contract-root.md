---
title: "Palette Integration Contract Root"
created: 2026-08-30
updated: 2026-08-30
---

# Palette integration contract root

Status: accepted for `axon_rust-1p6q8.1`. This document is deliberately a profile and compatibility root, not a universal DTO authority.

Axon owns `contracts/integration-profile.schema.json`; `scripts/check-integration-contracts.py` checks its generated snapshot and fail-closed fixtures. Labby and Cortex own equivalent, product-specific roots in their repositories. Immediate slices must cite their owning product's request, response, error, pagination, revision, and SSE schemas when those operations land.

Palette persists only profile ID, product kind, canonical origin, pinned `server_id`, accepted API major, health, capability names, and product-specific auth material. A changed `server_id`, issuer, audience, token-endpoint origin, or final discovery origin requires explicit re-trust before credentials are sent. API and SSE redirects are rejected. Discovery is credential-free; if a product later permits a discovery redirect, the final origin is pinned before auth begins.

Cache keys include profile ID, product, stable server ID, API major, issuer and principal scope, credential generation, capability/catalog generation, object revision, query digest, and cursor lineage as applicable. Every cache declares its owner, TTL, byte/item cap, stale policy, and synchronous invalidation events. Identity, credential generation, capability generation, revision, and cursor-lineage changes invalidate synchronously.

Opaque server-issued revisions and stream cursors are never parsed or synthesized by Palette. Stream reconnect is bounded, preserves the pinned identity/origin, and fails closed on expired or foreign cursors.

Latency contracts are measured at profile health, catalog page/detail, session page, stream connect/resume, loadout resolution, first synthesis event, approval, exact call, IPC, and render commit. Metrics use bounded operation labels only; principals, payloads, schemas, arguments, credentials, and artifact content are forbidden in labels and logs. Redirects are not included as a performance optimization because authenticated redirects are forbidden.

Generator and drift map:

| Product | Canonical source | Generated artifact | Drift command |
|---|---|---|---|
| Axon | `contracts/integration-profile.schema.json` | `docs/architecture/integrations/generated/integration-profile.schema.json` | `python3 scripts/check-integration-contracts.py` |
| Labby | `contracts/integration-profile.schema.json` | `docs/contracts/generated/integration-profile.schema.json` | `python3 scripts/check-integration-contracts.py` |
| Cortex | `contracts/integration-profile.schema.json` | `docs/contracts/generated/integration-profile.schema.json` | `python3 scripts/check-integration-contracts.py` |

The fixture rule is common in intent but locally enforced: reject the wrong product and unsupported API major before sending credentials. Schema drift must cover every request, response, error, page, revision, and SSE event added by downstream slices.
