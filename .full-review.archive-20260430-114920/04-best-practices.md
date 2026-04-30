# Phase 4: Best Practices and Standards

## Findings

- High — `crates/vector/ops/commands/evaluate.rs:8`
  The vector command layer imports a job backend function directly instead of going through typed services.
  Impact: this violates the repo's services-first contract and makes MCP/CLI/web behavior diverge.
  Fix: keep `crates/vector` pure over retrieval/scoring/LLM payload generation. Put job mutations behind `crates/services`.

- Medium — `crates/vector/ops/commands/evaluate/streaming.rs:1`
  The file is explicitly marked as scaffolding with module-wide `dead_code` allowance, yet docs describe parts of this behavior as active.
  Impact: stale scaffolding increases review burden and creates false confidence about streaming/concurrent evaluate behavior.
  Fix: either wire it into the command path with tests or remove/defer it and update docs.

- Medium — `crates/vector/ops/tei/qdrant_store.rs:26`
  The process-wide collection mode cache documents that process restart is needed after migration, but the key does not include endpoint identity and no operational guard warns when the same process switches Qdrant URLs.
  Impact: local tests and long-lived MCP servers can produce hard-to-debug cross-environment behavior.
  Fix: key cache entries by Qdrant base URL and collection, and add tests for same collection name on different Qdrant URLs.

- Medium — `docs/CONTEXT-INJECTION.md:217`
  Config defaults are duplicated manually in docs instead of generated from or checked against `build_config.rs`.
  Impact: docs drift repeatedly as RAG tuning changes.
  Fix: add a documentation owner note or test/check script for RAG config defaults, or centralize the defaults in generated command docs.

## Critical Issues for Phase 5 Context

- Fix order should start with side-effect removal and prompt/source isolation, then retrieval parity, then performance/docs cleanup.

