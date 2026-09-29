# axon-codex

Own typed Codex app-server protocol and trusted-control runtime, separate from LLM synthesis.

## Read before changing

[codex control](../../../docs/guides/codex-control.md) · [auth](../../../docs/reference/runtime/auth.md) · [observability](../../../docs/reference/runtime/observability.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[api.rs](api.rs) · [protocol.rs](protocol.rs) · [transport.rs](transport.rs) · [control.rs](control.rs) · [approval.rs](approval.rs) · [operations.rs](operations.rs) · [events.rs](events.rs) · [capabilities.rs](capabilities.rs)

## Change requirements

- Transports reach trusted control through axon-services::codex_control, not direct generic JSON-RPC or shell passthrough.

- Mutations require authorization, secret validation, revision binding, expiring single-use approval, durable audit state, and operation-specific reconciliation. A transport timeout does not prove nothing committed.

- Recovery addresses exact IDs and revalidates control-home identity, runtime boot, and policy version. Bound frames, events, pending requests, concurrency, and timeouts.

- Keep control processes, homes, queues, and lifecycle separate from the completion pool in axon-llm. Shared protocol primitives do not authorize shared runtime state.

## Verification for code changes

cargo test -p axon-codex and axon-services codex_control tests; include approval replay, interrupted mutations, stale runtime identity, and capability drift.

Use focused `cargo test -p axon-codex` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
