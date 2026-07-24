# Dependency Layering

Last Modified: 2026-07-19

The Cargo workspace enforces a strict dependency direction: lower crates must
not depend on higher ones, and transports must not reach into domain-crate
internals. This keeps the source pipeline one path and keeps transports as
thin projections over `axon-services`.

> Implementation: [`xtask/src/checks/layering.rs`](../../xtask/src/checks/layering.rs)
> (+ `layering_tests.rs`). Run via `cargo xtask check-layering` or the
> aggregate `cargo xtask check`.
> Ownership rule: [crate-ownership.md](crate-ownership.md).

## Direction

```text
axon-error            (leaf)
   ↓
axon-api              (transport-neutral DTOs; no domain deps)
   ↓
axon-core, axon-authz, axon-observe
   ↓
axon-route, axon-parse, axon-adapters (→ axon-extract),
axon-ledger, axon-graph, axon-memory, axon-document,
axon-embedding, axon-vectors, axon-retrieval, axon-llm, axon-prune
   ↓
axon-jobs             (job runtime; depends on provider + domain crates)
   ↓
axon-services         (composition facade; depends on all lower crates)
   ↓
axon-cli, axon-mcp, axon-web   (transports; depend on axon-services)
   ↓
axon                  (root binary bootstrap)
```

## What `check-layering` enforces

The check reads all non-test `.rs` files in `crates/axon-{cli,web,mcp}/src`
(test files — `*/tests/*`, `*_tests.rs`, `*_test.rs` — are skipped) and fails
on any `use`/reference to these forbidden domain-internal prefixes:

- `axon_crawl::engine::`
- `axon_extract::registry::`
- `axon_extract::verticals::`
- `axon_ingest::github::` / `axon_ingest::rss::` / `axon_ingest::youtube::`
- `axon_vector::ops::`

These prefixes reference the legacy single-purpose crates that were folded
into the unified pipeline. The forbidden list is retained as a guardrail so
they cannot silently reappear.

### Grandfathered allowlist

A small, fixed set of pre-existing reaches are allowed (documented as
transitional debt — **do not extend**):

| File | Allowed prefix |
|---|---|
| `crates/axon-cli/src/commands/crawl/audit/sitemap.rs` | `axon_crawl::engine::` |
| `crates/axon-cli/src/commands/scrape.rs` | `axon_crawl::engine::`, `axon_vector::ops::` |
| `crates/axon-cli/src/commands/sources.rs`, `stats.rs` | `axon_vector::ops::` |
| `crates/axon-mcp/src/server/artifacts/respond.rs` | `axon_crawl::engine::`, `axon_vector::ops::` |
| `crates/axon-web/src/server/handlers/rest/sync_post.rs` | `axon_crawl::engine::`, `axon_vector::ops::` |

### PR9 provider-crate surface ban

The three "PR9 provider crates" — `axon-embedding`, `axon-vectors`,
`axon-retrieval` — must NOT appear as a dependency (`[dependencies]`,
`[dev-dependencies]`, or `[build-dependencies]`) of any transport manifest
(`crates/axon-{cli,web,mcp}/Cargo.toml`). Transports reach vector/embedding/
retrieval behavior through `axon-services`, never directly.

## Invariants

- **No transport reaches into a domain-crate internal module.** Transports call
  `axon-services` (or a domain crate's public `pub fn`), never `::ops::*`.
- **Shared DTOs live in `axon-api`**, not in transports or services.
- **`axon-api`/`axon-error` have no axon-domain deps.** They are the foundation.
- **Source execution crosses crate boundaries through service traits or public
  domain APIs**, not through internal modules.
- **The root binary remains a small bootstrapper** (`src/main.rs` +
  `src/lib.rs` re-exporting `axon_cli::run`).

## Forbidden dependency examples

These edges would violate the contract's dependency matrix (some are enforced
by `check-layering`, others by review):

- `axon-api` → `axon-services`
- `axon-error` → `axon-api`
- `axon-ledger` → `axon-vectors` (and the reverse)
- `axon-document` → `axon-embedding`
- `axon-adapters` → `axon-vectors`
- `axon-mcp` → `axon-adapters`
- `axon-web` → `axon-vectors`
- `axon-cli` → `axon-ledger`

## Verification

```bash
cargo xtask check-layering     # the layering check alone
cargo xtask check              # aggregate (layering + crate-contracts + others)
```

The check currently passes clean. The grandfathered allowlist is the only
remaining debt; each item is a candidate for a follow-up that routes the call
through `axon-services` instead.

If the layering rules change, update this file and
[`xtask/src/checks/layering.rs`](../../xtask/src/checks/layering.rs) in the
same PR.
