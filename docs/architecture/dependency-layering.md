# Dependency Layering
Last Modified: 2026-07-15

This page summarizes the dependency direction enforced by `cargo xtask
check-layering`.

## Direction

Lower crates must not depend on higher crates:

```text
axon-api / axon-error / axon-authz
axon-core / axon-observe
domain crates
axon-jobs
axon-services
axon-cli / axon-mcp / axon-web
root binary
```

## Invariants

- No transport reaches into a domain crate internal implementation module.
- Shared DTOs live in `axon-api`, not in transports.
- Source execution crosses crate boundaries through service traits or public
  domain APIs.
- The root binary remains a small bootstrapper.

## Verification

Run `cargo xtask check-layering` or the aggregate `cargo xtask check`.
