---
name: axon-extract
description: Extract requested structured fields from one page.
tags: [axon, research]
inputs:
  url:
    type: string
    required: true
    description: Source page URL
  prompt:
    type: string
    required: true
    description: Fields and output shape to extract
tools:
  - Axon::axon
---

# axon-extract

Use when a page is known and field-level data is needed; verify values before export. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  if (typeof input.prompt !== "string" || !input.prompt.trim()) throw new Error("prompt is required");
  const request = {
    action: "extract",
    subaction: "start",
    urls: [input.url],
    prompt: input.prompt
  };
  const result = await callTool("Axon::axon", request);
  return {
    ok: result.ok === true,
    snippet: "axon-extract",
    action: request.action,
    input: {url: input.url, prompt: input.prompt},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
