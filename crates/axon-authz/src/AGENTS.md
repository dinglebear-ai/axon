# axon-authz

Evaluate caller scopes, execution affinity, visibility, and security decisions independently of transport authentication.

## Read before changing

[auth](../../../docs/reference/runtime/auth.md) · [security](../../../docs/reference/runtime/security.md) · [mcp auth](../../../docs/operations/auth/mcp-auth.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[lib.rs](lib.rs) · [caller.rs](caller.rs) · [decision.rs](decision.rs) · [http.rs](http.rs) · [policy.rs](policy.rs) · [visibility.rs](visibility.rs) · [affinity.rs](affinity.rs)

## Change requirements

- Keep shipped scope literals and the actual compatibility behavior in lib.rs/http.rs aligned with issued tokens. Do not equate a string rename with a harmless refactor or assume every operation uses identical read/write/admin semantics.

- Transports authenticate; this crate evaluates policy. Keep OAuth middleware, token parsing, credential storage, SSRF clients, and source acquisition out of policy evaluation.

- Fail closed on ambiguous caller/affinity decisions. Propagate sufficient non-secret caller context to jobs for later checks; do not silently share a different user’s authority.

- Denied decisions need stable reasons, required scope/target, and a safe corrective action. Test allowed, denied, anonymous, trusted-local, and compatibility-token paths.

## Verification for code changes

lib_tests, caller_tests, decision_tests, policy_tests, affinity_tests, visibility_tests, and transport authorization regressions.

Use focused `cargo test -p axon-authz` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
