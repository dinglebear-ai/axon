---
name: axon-monitor
description: List configured Axon source watches.
tags: [axon, research]
tools:
  - Axon::axon
---

# axon-monitor

Use to inspect existing watches. Creating, changing, or deleting a watch needs separate review. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  input = input ?? {};
  const request = {
    action: "watch",
    subaction: "list",
    response_mode: "path"
  };
  const result = await callTool("Axon::axon", request);
  return {
    ok: result.ok === true,
    snippet: "axon-monitor",
    action: request.action,
    input: {},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
