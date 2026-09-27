---
name: axon-competitive-intel
description: Search and research competitor changes with source receipts.
tags: [axon, research]
inputs:
  query:
    type: string
    required: true
    description: Competitors and change question
tools:
  - Axon::axon
---

# axon-competitive-intel

Use for pricing, feature, or launch comparisons; check source dates before reporting changes. This snippet runs search, research as a bounded batch and reports each result or failure independently. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.query !== "string" || !input.query.trim()) throw new Error("query is required");
  const requests = [
    { action: "search", query: input.query, response_mode: "path" },
    { action: "research", query: input.query, response_mode: "path" }
  ];
  const batch = await codemode.batch(requests.map(request => () => callTool("Axon::axon", request)));
  return {
    ok: batch.all_ok,
    snippet: "axon-competitive-intel",
    input: { query: input.query },
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
