---
name: axon-knowledge-ingest
description: Ingest an authorized knowledge portal.
tags: [axon, indexing]
inputs:
  url:
    type: string
    required: true
    description: Portal root URL
tools:
  - Axon::axon
---

# axon-knowledge-ingest

Use for an accessible portal section; browser login and URL discovery happen outside this snippet. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  const request = {
    action: "source",
    source: input.url,
    scope: "site",
    detached: true,
    response_mode: "path"
  };
  const result = await callTool("Axon::axon", request);
  return {
    ok: result.ok === true,
    snippet: "axon-knowledge-ingest",
    action: request.action,
    input: {url: input.url},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
