# Session Log — MCP Hard Cutover
Date: 2026-02-26
Repo: `/home/jmagar/workspace/axon_rust`

## 1. Session overview
- Completed hard cutover of MCP action routing to top-level action names only.
- Removed grouped action handling (`rag`, `discover`, `ops`) from MCP request schema and server dispatch.
- Removed remaining parser shims/aliasing (fallback keys, token normalization, alias remapping).
- Updated MCP docs to match strict parser + top-level action contract.
- Rebuilt binaries and ran full test suite successfully.

## 2. Timeline of major activities
- Replaced grouped request variants in schema with top-level variants and strict parser entrypoint.
- Rewired MCP dispatch and handlers to top-level action handlers.
- Removed grouped action documentation and shim language.
- Removed parser convenience behavior (`command/op/operation`, aliasing, normalization).
- Verified with `cargo check`, `cargo test`, and binary builds.

## 3. Key findings (with references)
- MCP request schema now declares top-level request variants: `crates/mcp/schema.rs:7`.
- Top-level action request structs exist for query/retrieve/search/map/doctor/domains/sources/stats: `crates/mcp/schema.rs:206`, `:215`, `:223`, `:233`, `:242`, `:246`, `:254`, `:262`.
- Parser entry is strict deserialize path: `crates/mcp/schema.rs:318`.
- MCP server action list and dispatch now use top-level actions only: `crates/mcp/server.rs:342`, `:356-363`.
- Top-level handlers are implemented directly: `crates/mcp/server.rs:817`, `:842`, `:864`, `:900`, `:1211`, `:1237`, `:1252`, `:1267`.

## 4. Technical decisions and rationale
- Decision: remove grouped action shims completely.
- Rationale: avoid dual maintenance paths and enforce CLI-identical top-level actions.
- Decision: make parser strict (no fallback/alias/normalization).
- Rationale: deterministic contract and no implicit behavior.
- Decision: keep lifecycle subactions only for lifecycle families (`crawl|extract|embed|ingest|artifacts`).

## 5. Files modified/created and purpose
- `crates/mcp/schema.rs`: removed grouped variants + parser shims; strict parse path.
- `crates/mcp/server.rs`: top-level action dispatch/handlers; removed grouped handlers.
- `docs/MCP.md`: updated runtime contract and parser rules.
- `docs/MCP-TOOL-SCHEMA.md`: updated wire-contract parser rules.
- `crates/mcp/README.md`: updated crate contract text and smoke examples.
- `crates/mcp/CLAUDE.md`: updated internal MCP guidance/smoke examples.
- Existing in-progress prior-session changes remained present (from `git status --short`), including CLI/vector files and scripts.

## 6. Critical commands executed and outcomes
- `rg -n ... crates/mcp ...`: identified remaining grouped/shim references in code/docs.
- `cargo check -q`: passed.
- `cargo test -q`: passed with `384 passed; 0 failed`.
- `cargo build --bin axon -q`: passed.
- `cargo build --bin axon-mcp -q`: passed.
- `git status --short`: confirmed modified files list for this working tree.

## 7. Behavior changes (before/after)
- Before: grouped MCP domains existed (`rag|discover|ops`) with internal top-level normalization shims.
- After: grouped MCP domains removed from schema + dispatch; top-level actions are first-class.
- Before: parser accepted `command|op|operation`, normalized tokens, and remapped aliases.
- After: parser is strict serde decode with no fallback keys, no token normalization, and no alias remapping.
- Before: docs described legacy shims.
- After: docs describe strict parser contract and no top-level aliases.

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo check -q | compiles | success (no errors) | PASS`
- `cargo test -q | tests pass | 384 passed, 0 failed | PASS`
- `cargo build --bin axon -q | binary build succeeds | success (no errors) | PASS`
- `cargo build --bin axon-mcp -q | binary build succeeds | success (no errors) | PASS`
- `./scripts/axon embed ".../2026-02-26-mcp-hard-cutover-session.md" --json | embed returns JSON payload | {"job_id":"2f9996a7-ce29-49e1-8e3e-67d3136a2954","source":"rust","status":"pending"} | PASS (async accepted)`
- `./scripts/axon embed status 2f9996a7-ce29-49e1-8e3e-67d3136a2954 --json | completed embed shows collection/outcome | status=completed, result_json.collection=cortex, docs_embedded=1, chunks_embedded=1 | PASS`
- `./scripts/axon embed ".../2026-02-26-mcp-hard-cutover-session.md" --wait true --json | sync embed returns final summary | {"chunks_embedded":4,"collection":"cortex"} | PASS`
- `./scripts/axon retrieve "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-mcp-hard-cutover-session.md" --collection cortex --json | retrieve indexed content | chunks=4, url matches session path | PASS`

## 9. Source IDs + collections touched
- Axon embed target: `/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-mcp-hard-cutover-session.md`.
- Embed source ID (`data.url`): not present in observed embed outputs (`--json` async and `--wait true --json`).
- Embed collection (`data.collection`): not present as `data.collection`; observed collection fields were `result_json.collection=cortex` (embed status) and `collection=cortex` (`--wait true`).
- Retrieve verification attempt: `./scripts/axon retrieve "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-mcp-hard-cutover-session.md" --collection cortex --json` succeeded (`chunks=4`, `url` matched file path).
- Source-like values observed from embed outputs: `source="rust"` (async enqueue output), `result_json.source="rust"` (embed status output).

## 10. Risks and rollback
- Risk: strict parser may break clients relying on alias/fallback behavior.
- Risk: requests using grouped action names will now fail validation.
- Rollback option: restore parser normalization functions and grouped action variants in `schema.rs` + `server.rs`.
- Rollback option: reintroduce compatibility handling in docs and smoke examples.

## 11. Decisions not taken
- Did not keep compatibility shims for grouped actions.
- Did not keep fallback `command|op|operation` fields.
- Did not keep token normalization behavior.
- Did not add new compatibility wrappers for deprecated action forms.

## 12. Open questions
- Are any external MCP clients in active use that still send grouped or alias action forms?
- Should strict parser failures include migration hints in error text?
- Should compatibility be reintroduced behind a feature flag if downstream breakage is found?

## 13. Next steps
- Run Axon embed + retrieve verification for this session doc (mandatory workflow step).
- Persist session entities/relations/observations to Neo4j memory.
- If external clients break, decide on hard failure vs temporary migration helper.
