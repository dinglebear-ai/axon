# Session: SearXNG Research Script Optimization

**Date:** 2026-03-03
**Branch:** `feat/sidebar`

## Session Overview

Continued development and optimization of `scripts/searxng-research` — a multi-round web research script combining SearXNG search, local LLM query generation (qwen3.5:4b via Ollama), remote LLM gap analysis (Gemini), and streaming synthesis. This session focused on benchmarking local models, tuning the research flow, and fixing SearXNG reliability issues.

## Timeline

1. **Local model benchmarking for query gen** — Tested multiple models for fast search query generation:
   - `qwen3:0.6b`: 998ms, junk output (slugified identifiers) — rejected
   - `qwen3:8b`: 6844ms, good quality — too slow
   - `qwen3.5:4b` with thinking ON: 28-42s — way too slow
   - `qwen3.5:4b` with thinking OFF (Ollama native `/api/chat` + `think: false`): ~900ms, reliable JSON — **winner**
   - `qwen3.5:2b`: ~1s, 2/3 valid format — unreliable

2. **Ollama infrastructure update** — Updated Ollama from 0.12.6 to 0.17.5 to support qwen3.5 architecture (Gated Delta Networks + sparse MoE). Required `docker compose pull && docker compose up -d --force-recreate`.

3. **Script architecture split** — Query gen uses local Ollama (qwen3.5:4b, ~900ms), gap gen stays on remote Gemini (needs large context for full scrapes ~175k tokens).

4. **Multiple flow redesigns**:
   - v1: 6 queries, top 3 deduped each, no gap, straight synthesis
   - v2: User query + 2 complementary → scrape 9 → gap via Gemini → 2 gap queries → scrape 6 gap + backfill 9
   - v3 (final): Same as v2 but batch-a reduced to 6 URLs (2 per query) for testing

5. **SearXNG outage** — All engines (brave, duckduckgo, startpage, google) hit CAPTCHA/rate limits simultaneously. Resolved on its own (transient). User considered swapping to Tavily but SearXNG recovered.

6. **Final successful run** — 33s end-to-end, 9 pages, 364k chars into synthesis, properly cited output with structured comparison table.

## Key Findings

- **Ollama native API vs OpenAI-compatible**: `/api/chat` with `"think": false` and `"stream": false` disables chain-of-thought reasoning, achieving 30x speedup over OpenAI-compatible endpoint with thinking enabled. The OpenAI `/v1/chat/completions` endpoint does NOT support `think: false`.
- **qwen3.5:4b on RTX 4070 12GB**: Safe max context ~128K tokens. 256K risks OOM under load.
- **Token-to-character ratio**: ~4 chars per token (700k chars ≈ 175k tokens).
- **`docker compose up -d` does NOT recreate after `docker pull`**: Need `--force-recreate` flag. Compose only recreates on service definition changes, not image changes alone.
- **SearXNG engine CAPTCHA/rate-limiting is transient**: All engines can go down simultaneously but recover within minutes. `unresponsive_engines` field in API response reveals which backends are affected.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| qwen3.5:4b over 2b for query gen | 4b reliably produces valid JSON arrays; 2b fails ~33% of the time |
| Ollama native API over OpenAI-compatible | Only way to disable thinking mode; 30x faster (900ms vs 28s) |
| Hardcoded model/URL defaults in script | User preference — no env var friction for common case |
| 2 URLs per query (6 batch-a) over 3 (9 batch-a) | Final tuning — faster initial scrape, more goes to backfill pool |
| Keep SearXNG over Tavily swap | SearXNG recovered; no cost per query vs Tavily's API pricing |
| Remote Gemini for gap analysis | Gap gen needs full untruncated scrapes (~175k tokens), exceeds local model context |

## Files Modified

| File | Change |
|------|--------|
| `scripts/searxng-research` | Multiple rewrites: local Ollama query gen, concurrent phase architecture, batch-a/backfill flow, dedup logic, batch-a count tuned from 3→2 per query |
| `scripts/time-query-gen` | Benchmark script for comparing model query gen performance (created in prior linked session) |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Query generation | Remote LLM (slow, ~2-5s) | Local qwen3.5:4b via Ollama (~900ms) |
| Batch-a size | 9 URLs (3 per query) | 6 URLs (2 per query) |
| Research flow | Single-pass search+scrape+synthesize | 3-phase: initial scrape → gap analysis → gap scrape + backfill (concurrent) |
| Total runtime | Variable | ~33s end-to-end |
| Page truncation in gap gen | Truncated to 3000 chars | Full content (untruncated) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `./scripts/searxng-research "how do you create a claude code plugin..."` | Complete research output | 33s, 9 pages, 364k chars, cited synthesis | PASS |
| Query gen timing | <2s | 1.2s | PASS |
| SearXNG concurrent search | Results from 3 queries | 3 URLs batch-a + 7 backfill | PASS |
| Gap analysis | 2 follow-up queries | 2 gap queries generated | PASS |
| Concurrent gap+backfill | Overlapping execution | Backfill 5s, gap 7.7s (overlap) | PASS |

## Risks and Rollback

- **SearXNG reliability**: All engines can CAPTCHA simultaneously. Mitigation: Tavily is ready to swap in (`TAVILY_API_KEY` in `.env`). Rollback: revert search function to Tavily API calls.
- **Ollama dependency**: Script assumes `steamy-wsl:11434` is reachable. Fallback: OpenAI-compatible endpoint for query gen (slower but works).
- **No persistent changes to Rust codebase**: All changes are in bash scripts — zero risk to core axon binary.

## Decisions Not Taken

- **Tavily swap**: Considered when SearXNG went down, but SearXNG recovered. Tavily has per-query costs; SearXNG is free.
- **qwen3.5:2b**: Faster but unreliable JSON output (~33% failure rate). Not worth the retry complexity.
- **Page truncation in gap gen**: User explicitly rejected truncation — full scrape content goes to Gemini.

## Open Questions

- How stable is SearXNG long-term with all engines CAPTCHA-prone? May need Tavily as automatic fallback.
- Could gap analysis use local model if context window grows (qwen3.5:4b at 128K vs current ~175K need)?
- Optimal batch-a size: 6 (2/query) vs 9 (3/query) — needs more A/B testing across different query types.

## Next Steps

- A/B test 6 vs 9 batch-a URLs across different query types to find optimal balance
- Consider automatic SearXNG→Tavily fallback when `unresponsive_engines` exceeds threshold
- Index the script's output into Axon for knowledge base growth
