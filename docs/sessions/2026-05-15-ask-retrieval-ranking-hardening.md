# Ask Retrieval Ranking Hardening

Date: 2026-05-15
Repo: `/home/jmagar/workspace/axon_rust`
Branch: `main`
Final HEAD: `da9b9078 test: cover ask context source ordering`

## Context

This session followed a retrieval-quality investigation where `axon ask` answered a Claude Code agents question with sources such as `debug-your-config` and `desktop` ranked ahead of the actual agent documentation. The goal was to address the underlying retrieval, full-document selection, diagnostics, and citation-failure behavior without hardcoding ranking for one specific prompt or one specific documentation site.

The relevant live question was:

```text
how do i setup agents to not inherit all of my mcp servers / skills in claude code
```

A related Qdrant retrieval prompt was also used to check whether official/domain-dominant Qdrant sources stay ahead of external summaries:

```text
how does qdrant hybrid query retrieval work with dense and sparse vectors
```

## Changes Completed

### Generic Full-Doc Selection

Full-document candidate selection was hardened around general signals:

- Prefer candidates from dominant retrieval hosts when a host strongly dominates the retrieved pool.
- Use URL/path entity-token matches as a full-document promotion signal.
- Keep top chunk selection separate from full-document selection so full-doc expansion does not disappear when the same URLs already occupy top chunk slots.

This is intentionally not a prompt-specific boost. The new regression fixture uses Qdrant-like candidates only as an example of the generic rule: scarce full-doc slots should not be consumed by an external summary when the retrieved pool is overwhelmingly from the official host.

Touched area:

- `src/vector/ops/commands/ask/context/build/selection.rs`
- `src/vector/ops/commands/ask/context/tests.rs`

### Explainability

`ask --explain` now exposes final context ordering metadata:

- `sort_rank`
- `sort_score`

This makes it visible why a final context source landed above or below another source after bucket flattening and renumbering.

Touched area:

- `src/services/types/service.rs`
- `src/vector/ops/commands/ask/context/build.rs`
- `src/vector/ops/commands/ask/context/build/trace.rs`
- `src/vector/ops/commands/ask/context/build/trace/tests.rs`

### Diagnostics

`ask --diagnostics` now includes effective retrieval/context tuning knobs so live output explains what settings were actually used:

- `ask_candidate_limit`
- `ask_chunk_limit`
- `ask_backfill_chunks`
- `ask_doc_chunk_limit`
- `ask_hybrid_candidates`
- `ask_full_docs_configured`
- `ask_full_docs_explicit`
- `ask_fulldoc_skip_enabled`
- `ask_max_context_chars`

Touched area:

- `src/vector/ops/commands/ask.rs`
- `src/services/types/service.rs`

### Citation Failure Handling

Citation validation no longer collapses a generated answer into generic “insufficient evidence” when context exists but citation formatting is bad or incomplete. Instead, Axon preserves the generated answer and appends:

```text
## Citation Validation Failed
...
## Retrieved Sources
...
```

When no retrieved context exists, the old insufficient-evidence fallback remains in place.

Touched area:

- `src/vector/ops/commands/ask/normalize.rs`
- `src/vector/ops/commands/ask/tests.rs`

## Verification

Local checks:

```bash
cargo fmt --check
cargo test vector::ops::commands::ask --lib
cargo check --bin axon
```

Result:

- `cargo test vector::ops::commands::ask --lib`: 109 passed.
- `cargo check --bin axon`: passed.

## Live Deployment

The dev Docker image was rebuilt and the `axon` service was recreated using the dev compose stack:

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.yaml -f docker-compose.dev.yaml build axon
docker compose --env-file ~/.axon/.env -f docker-compose.yaml -f docker-compose.dev.yaml rm -sf axon
docker compose --env-file ~/.axon/.env -f docker-compose.yaml -f docker-compose.dev.yaml up -d --no-deps --no-build axon
```

Final live container verification:

```text
healthy axon:local
```

An earlier external recreate briefly put the container back on `ghcr.io/jmagar/axon:latest`; it was forced back to the dev compose image and rechecked.

## Live Ask Evidence

Claude agents prompt, `--explain` source ordering:

- `S1` `https://code.claude.com/docs/en/agent-sdk/mcp`
- `S2` `https://code.claude.com/docs/en/agent-sdk/skills`
- `S3` `https://code.claude.com/docs/en/mcp`
- `S4` `https://code.claude.com/docs/en/agent-sdk/claude-code-features`
- `S5` `https://code.claude.com/docs/en/agent-sdk/python`
- `S6` `https://code.claude.com/docs/en/sub-agents`

The normal answer cited the relevant agent/MCP/skills docs and specifically described `mcpServers`, `allowedTools`, `strict_mcp_config`, `skills: []`, and explicit skills lists.

Qdrant hybrid prompt, `--explain` source ordering:

- `S1` `https://qdrant.tech/articles/sparse-vectors`
- `S2` `https://qdrant.tech/course/essentials/day-3/sparse-vectors`
- `S3` `https://qdrant.tech/course/essentials/day-3/sparse-retrieval-demo`
- `S4` `https://qdrant.tech/blog/comparing-qdrant-vs-pinecone-vector-databases`
- `S5` `https://qdrant.tech/articles/modern-sparse-neural-retrieval`
- `S6` `https://qdrant.tech/articles/rapid-rag-optimization-with-qdrant-and-quotient`

The normal answer cited Qdrant sources and explained dense/sparse storage, sparse inverted indexing, dot-product scoring, Query API prefetch/search-batch style retrieval, RRF, and two-stage reranking.

## Current Repo State

After the retrieval hardening work, the repository was clean and `main` matched `origin/main` at:

```text
da9b9078 test: cover ask context source ordering
```

This session note is a new uncommitted artifact under `docs/sessions/`.

## Open Questions

- The live answer quality is now materially better for the tested prompts, but broader retrieval evaluation should continue through the existing retrieval fixtures and any future prompt corpus.
- The Qdrant crawl gap should still be watched separately: ranking now prefers dominant official Qdrant sources, but this does not by itself guarantee every desired Qdrant documentation URL has been crawled.
- `vibin:save-to-md` was requested, but no callable `save-to-md` skill was available in this Codex session, so this file was created manually as the equivalent saved session artifact.
