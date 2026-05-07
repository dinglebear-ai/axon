---
name: dr
description: Use when the user wants to check if axon services are healthy, diagnose connectivity problems, verify Qdrant/TEI/Chrome are reachable, troubleshoot why axon isn't working, or run a health check. Triggers on "axon doctor", "check axon health", "is axon working", "troubleshoot axon", "why is axon failing", "check services", "health check", "can axon connect to". Always run this first when something seems broken.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-dr

Pings all axon dependencies and reports their health. Run this first when anything isn't working.

## MCP (preferred)

```json
{ "action": "doctor" }
```

## CLI fallback

```bash
axon doctor
axon debug   # doctor + LLM-assisted troubleshooting
```

## What it checks

| Service | Required for |
|---------|-------------|
| Qdrant | All search/embed operations |
| TEI | scrape, crawl, embed, query, ask |
| Chrome | Chrome render mode |
| Tavily | search, research |
| LLM/Gemini | ask, extract, research |

## Common fixes

**Qdrant unreachable** → check `QDRANT_URL` and that the container is running:
```bash
docker compose -f config/docker-compose.services.yaml ps
```

**TEI unreachable** → scrape/crawl will work but embed will fail. Check `TEI_URL`.

**Tavily failed** → `TAVILY_API_KEY` missing or invalid.

**Chrome unreachable** → only affects `render_mode: "chrome"`. HTTP scraping still works.

## Full action map

```json
{ "action": "help" }
```
