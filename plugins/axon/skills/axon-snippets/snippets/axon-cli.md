---
name: axon-cli
description: Check Axon CLI-facing runtime readiness through MCP.
tags: [axon, research]
tools:
  - Axon::axon
---

# axon-cli

Use before CLI automation to inspect provider health; shell-specific commands remain in using-axon. The exact `Axon::axon` ID and action parameters were checked against the live Labby gateway catalog. The saved snippet narrows execution to that one upstream tool; the caller must already have authority to use it.

```js
async (input) => {
  input = input ?? {};
  const request = {
    action: "doctor",
    response_mode: "path"
  };
  const result = await callTool("Axon::axon", request);
  return {
    ok: result.ok === true,
    snippet: "axon-cli",
    action: request.action,
    input: {},
    shape: result.data?.shape ?? result.shape ?? null,
    artifact: result.data?.artifact_handle ?? result.data?.artifact ?? result.path ?? null,
    preview: JSON.stringify(result.data ?? result).slice(0, 3000)
  };
}
```
