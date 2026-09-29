# axon-error

Own the lowest shared typed error taxonomy and safe retry/cooling/degradation context.

## Read before changing

[error handling](../../../docs/pipeline-unification/runtime/error-handling.md) · [redaction](../../../docs/reference/runtime/redaction.md) · [observability](../../../docs/reference/runtime/observability.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[api_error.rs](api_error.rs) · [code.rs](code.rs) · [stage.rs](stage.rs) · [severity.rs](severity.rs) · [retry.rs](retry.rs) · [cooling.rs](cooling.rs) · [degradation.rs](degradation.rs) · [context.rs](context.rs) · [conversion.rs](conversion.rs)

## Change requirements

- Do not depend on higher Axon crates or add logging, response rendering, clients, stores, or scheduling here. axon-api owns shared transport projections.

- ALL failures and warnings must enable agent course-correction: stable code/stage/severity, affected IDs/provider, safe cause and expected-versus-observed context, retry policy, and concrete recovery guidance.

- Preserve known partial effects and mark unknown commit status explicitly. Do not turn provider timeouts into permission for blind mutation retries or flatten useful causes into an unclassified string.

- Keep JSON names stable and redaction safe in every projection. A context field or derived Debug implementation is not evidence that all supplied text is safe; test representative secret-bearing causes.

## Verification for code changes

Taxonomy, conversion, retry/degradation, schema, and redaction sidecars. Assert recovery data and visibility, not only is_err().

Use focused `cargo test -p axon-error` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
