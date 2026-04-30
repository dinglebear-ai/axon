# Review Scope

## Target

All RAG-related code in `axon_rust`: embedding, vector retrieval, context assembly, ask/evaluate/query/suggest/research service paths, CLI/MCP entry points, and directly relevant docs.

## Files

- `crates/vector/ops/commands/ask.rs`
- `crates/vector/ops/commands/ask/**`
- `crates/vector/ops/commands/evaluate.rs`
- `crates/vector/ops/commands/evaluate/**`
- `crates/vector/ops/commands/query.rs`
- `crates/vector/ops/commands/suggest.rs`
- `crates/vector/ops/commands/streaming.rs`
- `crates/vector/ops/qdrant.rs`
- `crates/vector/ops/qdrant/**`
- `crates/vector/ops/tei.rs`
- `crates/vector/ops/tei/**`
- `crates/vector/ops/ranking.rs`
- `crates/vector/ops/ranking/**`
- `crates/vector/ops/sparse.rs`
- `crates/vector/ops/input.rs`
- `crates/vector/ops/input/**`
- `crates/services/query.rs`
- `crates/services/search.rs`
- `crates/services/embed.rs`
- `crates/services/ingest.rs`
- `crates/services/migrate.rs`
- `crates/services/acp_llm.rs`
- `crates/services/acp_llm/**`
- `crates/cli/commands/{ask,query,retrieve,evaluate,suggest,search,research,embed}.rs`
- `crates/mcp/server/handlers_query.rs`
- `crates/mcp/server/handlers_embed_ingest.rs`
- `docs/CONTEXT-INJECTION.md`
- `docs/commands/{ask,query,retrieve,evaluate,suggest,search,research,embed}.md`

## Review Flags

- Security focus: yes
- Performance critical: yes
- Strict mode: yes
- Framework: Rust async services with Qdrant, TEI, ACP LLM adapters, CLI, MCP

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

