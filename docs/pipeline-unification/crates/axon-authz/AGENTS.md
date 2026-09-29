# axon-authz design-contract maintenance

This directory documents the `axon-authz` boundary: Evaluate caller scopes, execution affinity, visibility, and security decisions independently of transport authentication.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-authz/src/AGENTS.md) · [Crate exports](../../../../crates/axon-authz/src/lib.rs)

[auth](../../../../docs/reference/runtime/auth.md) · [security](../../../../docs/reference/runtime/security.md) · [mcp auth](../../../../docs/operations/auth/mcp-auth.md)

## Review the actual boundary

- Keep shipped scope literals and the actual compatibility behavior in lib.rs/http.rs aligned with issued tokens. Do not equate a string rename with a harmless refactor or assume every operation uses identical read/write/admin semantics.

- Denied decisions need stable reasons, required scope/target, and a safe corrective action. Test allowed, denied, anonymous, trusted-local, and compatibility-token paths.

For implementation evidence, inspect [lib.rs](../../../../crates/axon-authz/src/lib.rs), [caller.rs](../../../../crates/axon-authz/src/caller.rs), [decision.rs](../../../../crates/axon-authz/src/decision.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

lib_tests, caller_tests, decision_tests, policy_tests, affinity_tests, visibility_tests, and transport authorization regressions.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
