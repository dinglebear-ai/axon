---
name: axon-crawl
description: Index a bounded documentation site.
tags: [axon, indexing]
inputs:
  url:
    type: string
    required: true
    description: Documentation root URL
tools:
  - Axon::axon
---

# axon-crawl

Use when an entire site or docs section should enter the index. This changes the Axon corpus. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

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
    snippet: "axon-crawl",
    action: request.action,
    input: {url: input.url},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
