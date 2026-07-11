# axon-authz Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-authz` owns caller identity, scope checks, execution-affinity policy, and
security decisions that must be shared across CLI, REST, MCP, jobs, and apps.

## Owns

- `CallerContext`, `AuthScope`, `ExecutionAffinity`, and visibility rules
- local path, network, tool execution, artifact, and secret-access decisions
- policy evaluation results with deny/degrade/audit reasons
- propagation of caller context into source jobs and watch jobs
- fake policies for tests

## Must Not Own

- OAuth or bearer-token HTTP middleware
- MCP transport auth handshake
- source acquisition, SSRF HTTP implementation, or redaction detectors
- persistence of user accounts or secrets

**Decision (F5-14/C1-15, 2026-07-09 audit):** the crate currently violates
this rule — `crates/axon-authz/src/http.rs` owns OAuth/bearer HTTP middleware
and depends on `axum` + `lab-auth`, contradicting both this section and
"Dependencies Forbidden" below. Resolution: this contract's separation
stands as written — `http.rs` must move to `axon-web` (the transport crate
that already owns REST routing), not be grandfathered in here. See
`code_followups` for the code-side move; do not implement this in the
docs-only workstream.

## Public Modules

```text
lib.rs
caller.rs
scope.rs
policy.rs
decision.rs
visibility.rs
affinity.rs
testing.rs
```

## Public API

- `CallerContext`
- `AuthScope`
- `ExecutionAffinity`
- `SecurityPolicy`
- `SecurityDecision`
- `VisibilityPolicy`
- `PolicyEvaluator`
- `FakePolicyEvaluator`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core` value helpers
- serde/schema crates for policy DTOs

## Dependencies Forbidden

- transport frameworks, provider clients, stores, source adapters
- `axon-services`, `axon-jobs`, `axon-cli`, `axon-mcp`, `axon-web`

## Generated Artifacts

- auth and visibility components in API/OpenAPI schemas
- policy fixture examples for docs and tests

## Fixtures And Fakes

- allow-all local developer policy
- deny network policy
- deny tool execution policy
- redacted visibility policy
- admin/read-only caller contexts

## Tests

- scope matching is deterministic and closed by default
- denied decisions include stable machine-readable reasons
- job propagation preserves caller context without secrets
- fake policies can force allow, deny, and degrade paths

## Acceptance Criteria

- all security-sensitive operations can ask `axon-authz` before execution
- transports only authenticate callers; they do not duplicate policy logic
- source jobs can reconstruct enough caller context to enforce policy later

See [../README.md](../README.md) and
[../../runtime/auth-contract.md](../../runtime/auth-contract.md).
