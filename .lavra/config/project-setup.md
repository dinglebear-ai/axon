---
stack: general
review_agents:
  - code-simplicity-reviewer
  - security-sentinel
  - performance-oracle
  - architecture-strategist
  - systems-programming:rust-pro
plan_review_agents:
  - code-simplicity-reviewer
  - architecture-strategist
disabled_agents: []
---

<reviewer_context_note>
Rust + Next.js RAG engine. Monolith policy: 500 lines/file, 120 lines/fn. Services-first contract: CLI calls crates/services::*, not raw internals. No mod.rs — Rust 2018 file-per-module layout. Box<dyn Error> at command boundaries only.
</reviewer_context_note>
