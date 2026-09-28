# axon-llm

Own synthesis/completion backends, streaming, capability handling, and provider concurrency.

## Read before changing

[adding provider](../../../docs/development/adding-provider.md) · [providers](../../../docs/reference/runtime/providers.md) · [ask rag](../../../docs/guides/ask-rag.md) · [codex control](../../../docs/guides/codex-control.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[provider.rs](provider.rs) · [runtime.rs](runtime.rs) · [runtime/](runtime/) · [completion.rs](completion.rs) · [stream.rs](stream.rs) · [reservation.rs](reservation.rs) · [prompt.rs](prompt.rs)

## Change requirements

- Inspect runtime/ and BackendTextCompleter as well as LlmProvider. Do not assume every production call flows exclusively through the trait just because a design contract describes that target.

- Keep Gemini, OpenAI-compatible, and Codex completion selection inside this boundary. Source/transport handlers must not launch their own synthesis subprocesses.

- The isolated Codex synthesis pool is not the trusted-control runtime in axon-codex. Do not share homes, queues, approvals, or lifecycles by accident.

- Bound output, timeouts, concurrency, and retries; validate structured results and identify actual fallback/degradation. Preserve cancellation and final-event/error behavior while streaming.

## Verification for code changes

Provider/runtime backend sidecars plus synthesis/streaming tests in axon-services; include malformed output, partial streams, authentication, timeout, and redaction.

Use focused `cargo test -p axon-llm` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
