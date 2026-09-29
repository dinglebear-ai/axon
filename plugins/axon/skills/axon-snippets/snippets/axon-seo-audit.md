---
name: axon-seo-audit
description: Map and scrape a site for an SEO review.
tags: [axon, research]
inputs:
  url:
    type: string
    required: true
    description: Site root URL
tools:
  - Axon::axon
---

# axon-seo-audit

Use for crawlable URL inventory; inspect metadata and page quality separately. This snippet runs map before scrape because both touch the same source and concurrent refreshes can conflict. It reports each result or failure independently. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  const requests = [
    { action: "map", url: input.url, response_mode: "path" },
    { action: "scrape", inputs: [{ input: input.url }] }
  ];
  const results = [];
  const failures = [];
  for (const request of requests) {
    try {
      const response = await callTool("Axon::axon", request);
      results.push({
        action: request.action,
        shape: response.data?.shape ?? response.shape ?? null,
        artifact: response.data?.artifact_handle ?? response.data?.artifact ?? response.path ?? null,
        preview: JSON.stringify(response.data ?? response).slice(0, 1200)
      });
    } catch (error) {
      failures.push({ action: request.action, error: JSON.stringify(error).slice(0, 1000) });
    }
  }
  return {
    ok: failures.length === 0,
    snippet: "axon-seo-audit",
    input: { url: input.url },
    results,
    failures
  };
}
```
