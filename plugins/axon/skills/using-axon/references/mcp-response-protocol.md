# MCP response protocol

Axon returns a success envelope with `ok`, resolved `action`, optional `subaction`, and `data`. A failing MCP call can be a protocol error; inspect it instead of treating an absent `data` field as an empty result.

`response_mode` is `path`, `inline`, `both`, or `auto_inline`. Omission usually inlines small payloads and gives artifact metadata for larger ones. `retrieve` is inline-first for document reading. `ask`, `research`, and `summarize` keep key answer fields inline while saving full output to an artifact. Under `data`, inspect `shape`, `preview`, and the artifact receipt before requesting more bytes.

Path-mode receipts can refer to a server-side file under `AXON_MCP_ARTIFACT_DIR` or Axon's data directory. A remote client cannot assume that path exists locally. The live schema also exposes an `artifacts` MCP action for artifact-id access; inspect its subactions and authorization in the current server before using it. `retrieve` reads indexed chunks for a URL and does not accept an artifact path.

The generated source reference is `docs/reference/mcp/tool-schema.md`. The running server publishes `axon://schema/mcp-tool`; use it when source and server revisions differ.
