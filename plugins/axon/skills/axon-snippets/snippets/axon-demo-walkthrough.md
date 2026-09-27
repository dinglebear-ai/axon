---
name: axon-demo-walkthrough
description: Capture and summarize a product demo page.
tags: [axon, research]
inputs:
  url:
    type: string
    required: true
    description: Demo page URL
tools:
  - Axon::axon
---

# axon-demo-walkthrough

Use for visual evidence from one page; use browser automation for navigation and interaction. This snippet runs screenshot, summarize as a bounded batch and reports each result or failure independently. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  const requests = [
    { action: "screenshot", url: input.url, response_mode: "path" },
    { action: "summarize", url: input.url, response_mode: "path" }
  ];
  const batch = await codemode.batch(requests.map(request => () => callTool("Axon::axon", request)));
  return {
    ok: batch.all_ok,
    snippet: "axon-demo-walkthrough",
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
