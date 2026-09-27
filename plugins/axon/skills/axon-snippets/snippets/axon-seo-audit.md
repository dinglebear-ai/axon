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

Use for crawlable URL inventory; inspect metadata and page quality separately. This snippet runs map, scrape as a bounded batch and reports each result or failure independently. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  const requests = [
    { action: "map", url: input.url, response_mode: "path" },
    { action: "scrape", inputs: [{ input: input.url }] }
  ];
  const batch = await codemode.batch(requests.map(request => () => callTool("Axon::axon", request)));
  return {
    ok: batch.all_ok,
    snippet: "axon-seo-audit",
    input: { url: input.url },
    results: batch.ok.map(entry => {
      const response = entry.value;
      return {
        action: requests[entry.i].action,
        shape: response.data?.shape ?? response.shape ?? null,
        artifact: response.data?.artifact_handle ?? response.data?.artifact ?? response.path ?? null,
        preview: JSON.stringify(response.data ?? response).slice(0, 1200)
      };
    }),
    failures: batch.failed.map(entry => ({ action: requests[entry.i].action, error: JSON.stringify(entry.error).slice(0, 1000) }))
  };
}
```
