# axon-codex maintenance contract

`axon-codex` owns the typed Codex app-server protocol and the dedicated trusted-control runtime. It does not own synthesis; `axon-llm` keeps the isolated completion pool.

Public modules cover typed actions, capability drift, control-home validation, bounded/redacted events, durable approval and operation state, JSON-RPC framing, and supervised subprocess transport. Transport crates consume this domain through `axon-services::codex_control`; they must not depend on `axon-codex` directly.

Safety invariants:

- Never expose a generic JSON-RPC or shell passthrough.
- Mutations require policy authorization, semantic secret validation, revision binding, an expiring single-use approval capability, audit persistence, and action-specific reconciliation.
- Recovery is exact-ID addressed and must revalidate home identity, runtime boot, and policy version.
- Event payloads, pending requests, frames, concurrency, and timeouts remain bounded.
- The control home/process is separate from the synthesis home/process by default.

Run `cargo test -p axon-codex` and `cargo test -p axon-services codex_control` after changes. Protocol inventory changes require `cargo xtask generated-contracts refresh` followed by `cargo xtask generated-contracts check`.
