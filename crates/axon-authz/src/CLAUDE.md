# axon-authz — Agent Guide

`axon-authz` owns **caller identity, scope checks, execution-affinity policy, and
security decisions** shared across CLI, REST, MCP, jobs, and apps. Transports
*authenticate* callers; this crate *authorizes* them — they must not duplicate
policy logic. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-authz/README.md](../../../docs/pipeline-unification/crates/axon-authz/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/auth-contract.md](../../../docs/pipeline-unification/runtime/auth-contract.md).

## Status — PR0 skeleton, partly live
The **OAuth scope constants are already real, not markers**: `AXON_READ_SCOPE`
(`axon:read`), `AXON_WRITE_SCOPE` (`axon:write`), `AXON_FULL_ACCESS_SCOPE`, and
scope-satisfaction logic (`http.rs`) are live and load-bearing — these strings
are embedded in issued OAuth tokens, so changing the literals invalidates every
existing token (a hard security invariant). The broader policy surface
(`CallerContext`, `SecurityPolicy`, `ExecutionAffinity`, `SecurityDecision`,
`VisibilityPolicy`) folds in as the auth/scope-policy boundary is generalized. Do
not add OAuth/bearer HTTP middleware, source fetching, or redaction detectors.

## Module map
| File | Owns |
|---|---|
| `lib.rs` | **live** — OAuth scope constants + scope-satisfaction logic; the crate's public surface |
| `http.rs` | **live** — scope checks against required-scope for Axon read/write routes |
| `lib_tests.rs` | scope-matching + satisfaction tests |
| _(planned per contract)_ | `caller.rs`/`scope.rs`/`policy.rs`/`decision.rs`/`visibility.rs`/`affinity.rs`/`testing.rs` — `CallerContext`, `AuthScope`, `ExecutionAffinity`, `SecurityPolicy`, `SecurityDecision`, `VisibilityPolicy`, `PolicyEvaluator`, `FakePolicyEvaluator` |

## Boundary — keep OUT of this crate
- OAuth / bearer-token HTTP middleware, MCP transport auth handshake.
- source acquisition, SSRF HTTP client implementation, redaction detectors.
- persistence of user accounts or secrets.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core` value helpers; serde/schema crates for policy DTOs.
- **Forbidden:** transport frameworks, provider clients, stores, source adapters; `axon-services`, `axon-jobs`, `axon-cli`, `axon-mcp`, `axon-web`. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- **Do not alter the literal scope strings** — they are baked into issued tokens.
- Scope matching is **deterministic and closed by default**; ambiguous security decisions **fail closed**.
- Denied decisions carry **stable machine-readable reasons**.
- Job propagation preserves **enough caller context to re-check policy later, without secrets**.
- Fake policies can force allow / deny / degrade paths.

## DTO ownership
Serializable auth/visibility/policy DTOs and their OpenAPI components are defined
in **`axon-api`**; this crate evaluates policy and returns those shapes — it does
not redefine transport-facing schemas.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/auth-contract.md` ·
`runtime/security-contract.md` · the auth/visibility DTO components in `axon-api`.
