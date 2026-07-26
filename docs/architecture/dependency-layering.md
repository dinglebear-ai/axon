# Dependency Layering

Last Modified: 2026-07-25

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

The check parses production `.rs` items under the three transport crates and
`axon-services` with `syn`. Grouped, multiline, renamed, chained,
block-scoped, and `extern crate` aliases are resolved before traversal with
lexical shadowing and cycle protection. Provider-bearing macro tokens are
inspected structurally while comments, string literals, and bare macro metadata
keys remain data. Provider-typed bindings, local provider type aliases, and
bindings initialized from known or provider-module-owned concrete
implementations are tracked through standard wrapper references/clones,
positional tuple patterns, syntax-visible assignments, and lexical pattern
scopes (`if let`, `while let`, `for`, match arms, and closures). Collision-prone
calls are therefore rejected only on proven provider receivers. Assignment
tracking uses strong replacement in straight-line code, so a later proven
non-provider assignment clears the binding's provider state. Branch and match
exits merge their possible states, including guard-false effects that can reach
later match arms. Loop bodies use conservative monotonic assignment and a
stabilized loop-head state so loop-carried values, abrupt exits, and bare
`loop` expressions cannot clear possible provider state; `while` conditions
are included in stabilization and `while`/`for` retain the entry state because
their bodies may not run. Rust 2024 `if`/`while` let-chain bindings remain in
scope through later `&&` operands and their successful body. Closure body
effects, async-block body effects, and short-circuit Boolean right-hand sides
merge as optional while calls inside those bodies are still scanned. Newly
shadowed bindings remain isolated. Syntax-visible block tails, `if` branch
results, and match-arm results—including branch-local tail bindings—propagate
provider shape into local bindings and assignment targets. Direct match-arm
tails retain arm-pattern binding shape before the pattern scope closes.
Whitelisted wrapper call/method chains, references/dereferences, and indexed
provider-bearing values retain provider shape when used directly as method
receivers. Custom
interprocedural helper-return inference is intentionally outside this lexical
gate's scope. `#[cfg(test)]` items and external modules reachable only through
test declarations are excluded by transitive traversal from each crate root,
carrying test-only ancestry through both external and inline module trees; a
production route to the same normalized module path always keeps it in scope.
Unreachable source files are not part of the compiled crate surface. An
unreadable source tree/file, malformed Rust file, unreadable manifest, or
malformed inherited workspace dependency fails closed.

It rejects:

- transport access to `axon-adapters::web_engine`, `axon-llm`, private
  `axon-services::source` execution modules, and selected domain internals;
- transport manifest dependencies on `axon-adapters`, `axon-embedding`,
  `axon-llm`, `axon-retrieval`, or `axon-vectors`, in normal, dev, or build
  dependency tables, including their target-specific
  `[target.'cfg(...)'.*dependencies]` forms, renamed Cargo dependencies whose
  `package` field names a forbidden crate, and `workspace = true` aliases
  resolved through the root `[workspace.dependencies]` table;
- raw `EmbeddingProvider`, `VectorStore`, `SearchProvider`, `FetchProvider`,
  `RenderProvider`, `NetworkCaptureProvider`, `GraphStore`, `ArtifactStore`,
  and `LlmProvider` type/import/UFCS access, known concrete provider
  construction, provider-bearing domain-crate globs, named handle
  destructuring, low-collision provider methods, and every method invoked on a
  proven provider binding. The sanctioned `axon_services::*` facade glob is
  not provider-bearing and remains allowed;
- raw provider-handle member access outside the fixed
  `crates/axon-services/src/reserved_call.rs` scheduler facade.

### Exact temporary exceptions

Existing cutover debt is not a broad allowlist. Every exception records the
exact path, structural rule, owning bead, and expected occurrence count.
Manifest exceptions additionally record the exact dependency table. New or
excess occurrences, removed/stale entries, duplicate exception rows, invalid
owners, and table drift all fail the check. The table lives in
`xtask/src/checks/layering/exception_table.rs`; do not widen or recalculate an
entry merely to make a new reach pass.

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
cargo xtask check-layering         # the layering check alone
cargo xtask check-crate-contracts  # standalone contract/inventory audit
cargo xtask check                  # aggregate includes layering, not crate-contracts
```

The check currently passes with exact-count debt owned by the named pipeline
cutover beads. Removing a raw reach requires reducing or deleting its matching
exception in the same change.

If the layering rules change, update this file and
[`xtask/src/checks/layering.rs`](../../xtask/src/checks/layering.rs) in the
same PR.
