# Domain Indexed Sources Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a first-class way to check whether one domain is indexed and list indexed URLs for that exact domain with bounded defaults.

**Architecture:** Keep Qdrant access inside `src/vector/ops/qdrant`, expose typed service functions from `src/services/system`, then wire CLI, MCP, and REST read-only surfaces through those services. Domain matching is exact against the indexed `payload.domain` field; subdomain expansion is deliberately out of scope.

**Tech Stack:** Rust, clap, serde, Axum, Qdrant scroll filters, existing Axon services and MCP schema.

---

### Task 1: Service Support

**Files:**
- Modify: `src/vector/ops/qdrant/client/scroll.rs`
- Modify: `src/services/types/service.rs`
- Modify: `src/services/system/sources.rs`
- Modify: `src/services/system/domains.rs`
- Modify: `src/services/system.rs`
- Test: `src/services/system/sources_tests.rs`
- Test: `src/services/system/domains_tests.rs`

- [ ] **Step 1: Add service result types**

Add `DomainSourcesResult` with `domain`, `count`, `limit`, `offset`, and `urls: Vec<String>`. Add `DomainIndexedResult` with `domain`, `indexed`, and `url_count`.

- [ ] **Step 2: Add domain URL listing service**

Create `system::sources_for_domain(cfg, domain, pagination)`. It must trim and lowercase the domain, reject an empty domain, call a Qdrant helper that filters by exact `domain` plus `chunk_index == 0`, fetch only `url`, and stop after `offset + limit + 1` unique URLs so it can report truncation without scanning a large domain by default.

- [ ] **Step 3: Add exact domain status service**

Create `system::domain_indexed(cfg, domain)`. It must use a direct Qdrant scroll with `limit=1`, `with_payload=false`, and `with_vector=false`; it must not call the full listing helper.

- [ ] **Step 4: Unit test mapping and pagination**

Add tests for empty-domain rejection, stable sorting/pagination, and indexed true/false mapping without live Qdrant.

### Task 2: CLI Support

**Files:**
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/config_impls.rs`
- Modify: `src/cli/commands/sources.rs`
- Modify: `src/cli/commands/domains.rs`
- Test: `src/core/config/parse_tests.rs` or nearest config parse test module

- [ ] **Step 1: Replace unit command variants**

Change `Sources` and `Domains` from unit variants to argument structs with optional `--domain <domain>`. Add `--all` to `sources` only for explicit full-domain export.

- [ ] **Step 2: Carry parsed domains through Config**

Add `sources_domain: Option<String>` and `domains_domain: Option<String>` to `Config`, defaults, dispatch output, and config literal population.

- [ ] **Step 3: Render CLI outputs**

`axon sources --domain docs.example.com` prints a bounded page of matching URLs and emits JSON `{domain,count,limit,offset,next_offset,truncated,urls}`. `axon sources --domain docs.example.com --all` performs the explicit full export. `axon domains --domain docs.example.com` prints indexed/not indexed and emits JSON `{domain,indexed}`.

- [ ] **Step 4: Parse tests**

Add focused parse tests proving both domain flags land in config and do not affect the opposite command.

### Task 3: MCP, REST, Docs, Verification

**Files:**
- Modify: `src/mcp/schema/requests.rs`
- Modify: `src/mcp/server/handlers_system.rs`
- Modify: `src/web/server/handlers/discovery.rs`
- Modify: `src/web/server/handlers/rest/read_only.rs`
- Modify: `docs/commands/sources.md`
- Modify: `docs/commands/domains.md`
- Modify: `docs/MCP-TOOL-SCHEMA.md`
- Modify: `docs/API.md`
- Test: `tests/mcp_option_mappers.rs` or `tests/mcp_contract_parity.rs`
- Test: `src/web/server/handlers/rest_tests.rs`

- [ ] **Step 1: Add MCP request fields**

Add optional `domain` to `SourcesRequest` and `DomainsRequest`. Preserve `deny_unknown_fields` compatibility by only adding fields, not removing existing ones.

- [ ] **Step 2: Route MCP and REST by domain**

If `domain` is set on sources, return the domain URL listing. If set on domains, return exact indexed status. Add `domain` to REST query params for `/v1/sources` and `/v1/domains`.

- [ ] **Step 3: Document exact semantics**

Docs must state exact `payload.domain` matching, not parent-domain/subdomain matching. Include examples for CLI, MCP, and REST.

- [ ] **Step 4: Verify**

Run `cargo fmt --check`, focused tests for config/services/MCP/REST, then `cargo check`.
