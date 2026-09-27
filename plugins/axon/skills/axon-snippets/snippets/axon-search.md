---
name: axon-search
description: Search current web sources.
tags: [axon, research]
inputs:
  query:
    type: string
    required: true
    description: Web search query
tools:
  - Axon::axon
---

# axon-search

Use for current source discovery; Axon may enqueue indexing as a side effect. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.query !== "string" || !input.query.trim()) throw new Error("query is required");
  const request = {
    action: "search",
    query: input.query,
    response_mode: "path"
  };
  const result = await callTool("Axon::axon", request);
  return {
    ok: result.ok === true,
    snippet: "axon-search",
    action: request.action,
    input: {query: input.query},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
