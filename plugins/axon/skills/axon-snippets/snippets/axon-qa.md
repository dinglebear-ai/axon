---
name: axon-qa
description: Map and screenshot a live site for QA evidence.
tags: [axon, research]
inputs:
  url:
    type: string
    required: true
    description: Page URL
tools:
  - Axon::axon
---

# axon-qa

Use for one visual checkpoint; interactive QA requires a browser. This snippet runs map, screenshot as a bounded batch and reports each result or failure independently. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  const requests = [
    { action: "map", url: input.url, response_mode: "path" },
    { action: "screenshot", url: input.url, response_mode: "path" }
  ];
  const batch = await codemode.batch(requests.map(request => () => callTool("Axon::axon", request)));
  return {
    ok: batch.all_ok,
    snippet: "axon-qa",
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
