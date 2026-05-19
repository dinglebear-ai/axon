# Session Log — MCP Artifact-First + Resource + Help

## 1. Session overview
- Implemented context-safe MCP defaults for Axon: artifact-first responses with optional inline modes.
- Added `response_mode` request field (`path|inline|both`, default `path`) and wired heavy handlers to artifact outputs.
- Added new MCP action family `artifacts` (`head|grep|wc|read`) and direct action `help`.
- Added MCP resource exposure for `axon://schema/mcp-tool` and enabled server resource capability.
- Updated MCP docs and crate docs to reflect contract and behavior.

## 2. Timeline of major activities
- Extended request schema: new `ResponseMode`, `HelpRequest`, `ArtifactsRequest`, parser-shim alias/default updates.
- Added server artifact policy utilities (`.cache/axon-mcp`, sha256, preview, truncation, path validation).
- Routed heavy responses through `respond_with_mode(...)` and added list pagination (`limit` + `offset`) where implemented.
- Implemented `handle_artifacts(...)` and `handle_help(...)`.
- Implemented `list_resources`/`read_resource` for `axon://schema/mcp-tool` and enabled resources capability.

## 3. Key findings with `path:line` references when relevant
- `AxonRequest` now includes `Help` and `Artifacts` variants: `crates/mcp/schema.rs:15`, `crates/mcp/schema.rs:16`.
- `ResponseMode` enum introduced and attached across MCP request structs: `crates/mcp/schema.rs:25`, `crates/mcp/schema.rs:39`.
- Parser shim normalizes and supports new alias mapping for artifact subactions: `crates/mcp/schema.rs:423`.
- Artifact-first server helpers and root path `.cache/axon-mcp` are implemented: `crates/mcp/server.rs:146`.
- Resource URI constant and resource handlers for schema export are implemented: `crates/mcp/server.rs:60`, `crates/mcp/server.rs:1575`, `crates/mcp/server.rs:1602`.

## 4. Technical decisions and rationale
- Chose artifact-first default to reduce LLM context pressure and token usage for large payloads.
- Kept inline mode opt-in (`inline|both`) to preserve debuggability and compatibility.
- Added artifact inspection actions in MCP instead of shell-only workflows for deterministic, tool-native file inspection.
- Added one durable resource (`axon://schema/mcp-tool`) first to expose source-of-truth contract without over-expanding resource surface.
- Preserved existing parser-shim aliases and extended them to `artifacts.*` for backward-compatible UX.

## 5. Files modified/created and purpose
- `crates/mcp/schema.rs`: Added `response_mode`, `help`, `artifacts`, pagination fields, parser alias/default updates.
- `crates/mcp/server.rs`: Added artifact policy utilities, response-mode routing, artifact actions, help action, resource handlers, capabilities update.
- `docs/MCP.md`: Updated runtime guide for artifact-first defaults, new actions, resources, pagination.
- `docs/MCP-TOOL-SCHEMA.md`: Updated schema contract, parser rules, response policy, resources.
- `README.md`, `crates/mcp/README.md`, `crates/mcp/CLAUDE.md`: Updated docs to reflect new MCP behavior and resource URI.

## 6. Critical commands executed and outcomes
- `cargo check --bin axon-mcp` → passed after fixing resource handler error types.
- `cargo check --bin axon` → passed after MCP changes.
- `rg -n ... crates/mcp/schema.rs` → verified schema additions/line refs.
- `rg -n ... crates/mcp/server.rs` → verified handlers, artifact helpers, resource methods, capabilities.
- `rg -n ... docs/* README.md` → verified doc coverage for new contract.

## 7. Behavior changes (before/after)
- Before: heavy MCP actions typically returned inline structured content.
- After: heavy MCP actions default to artifact metadata response (`response_mode=path`) and persist full payloads to `.cache/axon-mcp/`.
- Before: no MCP artifact-inspection action family.
- After: `artifacts.head|grep|wc|read` available.
- Before: no explicit help action for action/resource discovery.
- After: `action=help` returns action/subaction catalog + resource list.
- Before: no MCP resources exposed by server implementation.
- After: `axon://schema/mcp-tool` exposed via resources list/read.

## 8. Verification evidence (`command | expected | actual | status`)
| command | expected | actual | status |
|---|---|---|---|
| `cargo check --bin axon-mcp` | compile success | `Finished 'dev' profile ...` | PASS |
| `cargo check --bin axon` | compile success | `Finished 'dev' profile ...` | PASS |
| `rg -n "enum AxonRequest|ResponseMode|ArtifactsRequest|HelpRequest" crates/mcp/schema.rs` | new types present | matched lines including `Help`, `Artifacts`, `ResponseMode` | PASS |
| `rg -n "handle_help|handle_artifacts|list_resources|read_resource|enable_resources" crates/mcp/server.rs` | new handlers/capability present | matched all expected symbols | PASS |
| `rg -n "response_mode|axon://schema/mcp-tool" docs/MCP.md docs/MCP-TOOL-SCHEMA.md` | docs updated | matched in both docs | PASS |

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Pending Axon embed/retrieve execution in this workflow step.
- Saved session file for embedding target: `docs/sessions/2026-02-25-mcp-artifact-first-session.md`.
- Source ID: pending.
- Collection: pending.
- Outcome: pending.

## 10. Risks and rollback
- Risk: response-shape changes for some operations may impact clients expecting full inline payloads by default.
- Mitigation: `response_mode=inline|both` is available for explicit override.
- Risk: artifact path handling could be abused if path validation is weak.
- Mitigation: artifact operations enforce canonical path inside `.cache/axon-mcp/`.
- Rollback: revert `crates/mcp/schema.rs` + `crates/mcp/server.rs` and corresponding docs updates.

## 11. Decisions not taken
- Did not add multiple resources beyond `axon://schema/mcp-tool` in this pass.
- Did not add resource templates in this pass.
- Did not refactor all handlers to use artifact mode (focused on heavy/list/search paths requested).
- Did not alter external env namespace (kept stack env reuse policy).

## 12. Open questions
- Should `extract/embed/ingest` `response_mode` fields become fully active (currently parsed but not behavior-driving in all subactions)?
- Should `ops.stats` and additional status endpoints also route through artifact-first mode?
- Should artifact filenames include timestamp/job-id consistently for all actions?
- Should `help` include per-action JSON examples generated from schema at runtime?

## 13. Next steps
- Run mcporter smoke tests for: `help`, `artifacts.head`, `resources/list`, `resources/read`.
- Decide whether to enforce artifact mode on remaining handlers (`extract/embed/ingest` status/list paths).
- Add integration tests for parser shim aliases (`head|grep|wc|read`, `help`) and resource read path.
- Add docs snippet in quick-start for `response_mode` and artifact inspection workflow.
