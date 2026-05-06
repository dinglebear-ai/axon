---
name: extract
description: Use when the user wants to extract structured data from a web page using an LLM, parse specific fields from a URL (prices, names, dates, specs), get structured JSON from a page, or do LLM-powered data extraction. Triggers on "extract data from", "pull structured data from", "get the pricing table from", "parse the fields from", "extract JSON from this URL", "structured extraction". Different from scrape (raw markdown) — extract uses an LLM to interpret and structure the content.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-extract

LLM-powered structured data extraction from URLs. Describe the schema you want in plain language.

## MCP (preferred)

```json
{
  "action": "extract",
  "urls": ["https://example.com/pricing"],
  "prompt": "Extract plan name, monthly price, and feature list as JSON"
}
```

Multiple URLs:
```json
{
  "action": "extract",
  "urls": ["https://example.com/pricing", "https://competitor.com/pricing"],
  "prompt": "Extract: plan_name, price_usd_monthly, max_users, storage_gb. Use null for missing fields."
}
```

Check status:
```json
{ "action": "extract", "subaction": "status", "job_id": "<uuid>" }
```

## CLI fallback

```bash
axon extract https://example.com/pricing --query "Extract plan name, price, and features as JSON"
```

## Reading results

```json
{ "action": "artifacts", "subaction": "head", "path": ".cache/axon-mcp/<uuid>.json", "limit": 30 }
```

Subactions: `status` | `cancel` | `list` | `cleanup` | `clear`
