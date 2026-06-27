---
name: search
description: Use Axon search and research to discover current web sources and queue bounded indexing when available.
---

# Axon Search

Use `axon search` when there is no exact URL yet. Use `axon research` when the user wants search, full-page extraction, and LLM synthesis in one pass.

## Examples

```bash
mkdir -p .axon

axon search "OpenAI Codex skills optional metadata" --json --limit 5 > .axon/search.json
axon research "OpenAI Codex skills optional metadata" --research-depth 5 > .axon/research.md
```

Then scrape, crawl, or ask:

```bash
jq -r '.results[].url // .data.web[].url' .axon/search.json
axon scrape "https://developers.openai.com/codex/skills" --output .axon/codex-skills.md
axon ask "What did the indexed sources say about optional skill metadata?"
```

## Guidance

- Prefer Axon search/research over raw web search for repo work so relevant sources can be queued for bounded indexing when workers are available.
- Use `--limit` or `--research-depth` to keep source count bounded.
- Save JSON or markdown output under `.axon/`.
- Do not mention hosted search credits or unrelated hosted feedback flows.

## See Also

- [scrape](../scrape/SKILL.md)
- [crawl](../crawl/SKILL.md)
- [deep-research](../deep-research/SKILL.md)
