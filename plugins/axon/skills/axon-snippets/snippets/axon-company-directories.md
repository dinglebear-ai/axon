---
name: axon-company-directories
description: Extract company records from an authorized directory.
tags: [axon, research]
inputs:
  url:
    type: string
    required: true
    description: Directory page URL
  prompt:
    type: string
    required: true
    description: Fields to extract, such as name, website, and location
tools:
  - Axon::axon
---

# axon-company-directories

Use for a known public or authorized directory page; verify extracted fields against the source. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  if (typeof input.url !== "string" || !input.url.trim()) throw new Error("url is required");
  if (typeof input.prompt !== "string" || !input.prompt.trim()) throw new Error("prompt is required");
  const request = {
    action: "extract",
    urls: [input.url],
    prompt: input.prompt,
    response_mode: "path"
  };
  const result = await callTool("Axon::axon", request);
  return {
    ok: result.ok === true,
    snippet: "axon-company-directories",
    action: request.action,
    input: {url: input.url, prompt: input.prompt},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
