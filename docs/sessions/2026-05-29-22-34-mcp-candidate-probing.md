# Session: Auto-synthesized MCP endpoint probing (`--probe-rpc-subdomains`)

**Date:** 2026-05-29
**Branch:** `feat/mcp-candidate-probing`
**Worktree:** `/home/jmagar/workspace/axon_rust/.worktrees/mcp-candidate-probing`
**Base:** `main` (v4.14.0, `9104fd71`)
**HEAD at write:** `8c8a0649`
**PR:** https://github.com/jmagar/axon/pull/148
**Target version:** v4.15.0 (feat → minor)

## What shipped

Adds well-known **MCP endpoint candidate probing** to the `endpoints` command. When
`--probe-rpc` is set, axon synthesizes candidate URLs from the target and probes
them with the existing strict JSON-RPC/MCP handshake — catching MCP servers a
site's frontend never references (the `deepwiki.com` → `mcp.deepwiki.com` gap) and
enabling direct probing of bare MCP endpoints.

- **Same-host candidates** (`/mcp`, `/api/mcp`) probed whenever `--probe-rpc` is set.
- **`mcp.<registrable-apex>` candidates** gated behind new **`--probe-rpc-subdomains`** flag (apex via the `psl` crate).
- **Strict probe**: positive-signal POST probes only (`initialize`/`rpc.discover`/`system.listMethods`/`-32601`); the weak SSE content-type fallback is skipped. Streamable-HTTP MCP (SSE response to `initialize`) still detected.
- **Non-fatal initial fetch under `--probe-rpc`**: 401/non-HTML targets no longer abort discovery.
- **Output**: confirmed candidates → `endpoints` as `synthesized_mcp`; every attempt recorded in new `mcp_candidates` field (`confirmed`/`unconfirmed`/`blocked`).
- **Parity across CLI, MCP (`endpoints` action), web `/v1/endpoints`, and the `/v1/actions` dispatcher.** `probe_rpc` itself is now settable over MCP/HTTP/action-api (was CLI-only).
- SSRF: every synthesized candidate passes `validate_url_with_dns_timeout()` before any request; private/loopback → `blocked`, never fetched.

## Design / Plan / Beads
- Spec: `docs/superpowers/specs/2026-05-29-mcp-candidate-probing-design.md`
- Plan: `docs/superpowers/plans/2026-05-29-mcp-candidate-probing.md`
- Beads epic: `axon_rust-ez1k` (children `tz85` `0l4a` `y3jp` `au4b` `rtg8` `ppia` `5n3r` `51yr`)

## Key files
- New: `src/services/endpoints/candidates.rs` (apex derivation + candidate synthesis + probe driver), `src/services/endpoints/fetch.rs` (extracted fetch helpers to keep `endpoints.rs` under the 500-line cap).
- Changed: `src/services/endpoints/probe.rs` (`probe_candidate` strict entry, signature change), `src/services/endpoints.rs` (non-fatal fetch + wiring), `src/services/types/endpoints.rs` (new types/variant/field), config plumbing, `src/cli/commands/endpoints.rs`, `src/mcp/{schema/requests.rs,server/handlers_query.rs}`, `src/web/server/handlers/exploration.rs`, `src/services/action_api/commands/dispatchers.rs`.

## Verification
- `cargo fmt --check` clean · `cargo clippy --all-targets` clean
- New tests pass: candidates 11, probe::tests 17, types::endpoints 3, endpoints::tests 9
- Parity: `cargo test --test http_api_parity_inventory` → 5 passed
- Full lib suite (`just test`, nextest): 2624 passed, 0 failed
- All changed `.rs` files ≤ 500 lines (monolith policy)

## Review waves run (work-it)
1. Parallel review wave: code-reviewer, silent-failure-hunter, type-design-analyzer, pr-test-analyzer, security-sentinel, code-simplifier, comment-analyzer.
2. Applied 10 consolidated fixes in `084b46d6`:
   - Split `endpoints.rs` (515 → 447) into `fetch.rs` (monolith blocker).
   - `first_party: false` for `ApexSubdomain` confirms + second `--first-party-only` filter pass after synthesis (was bypassed).
   - Surface `blocked`/`unconfirmed` candidates in CLI human output.
   - Corrected misleading "seed host only" warning/comment + the false "fetched target page" progress log on fetch failure.
   - Defense-in-depth SSRF guard inside `probe_candidate`.
   - Dedup candidate-construction loop + correct `mcp.<apex>` skip depth; `as_ref()` vs eager clone; doc the `Confirmed⟺Some` correlation + fix contradictory test; added dedup-filter + subdomain-dispatch tests.

## PR comments resolved
- **Codex bot** (`src/mcp/schema/requests.rs:295`): the `/v1/actions` dispatcher (`dispatch_endpoints`) didn't wire the new flags → fixed in `8c8a0649` (also resolved the `rest-api-parity` CI failure, same root cause). Thread replied + resolved. 0 unresolved threads.

## Remaining risks / open questions
- Test `synthesized_subdomain_attempt_is_recorded` makes real DNS/HTTP to `example.com`/`mcp.example.com`. Assertion is outcome-independent (only checks the `ApexSubdomain` attempt is *recorded*), so it passes offline via the `blocked` path, but it adds a DNS-timeout latency tail on network-isolated runners. Candidate for a future hermetic rewrite.
- `mcp-session-id` replay hardening (security Finding 2, LOW, pre-existing/non-exploitable — reqwest rejects invalid header values) left as-is to avoid touching pre-existing probe code.
- Same-host candidates inherit the target's scheme (incl. `http://`); subdomain candidates force `https`. Intentional.

## Handoff
PR #148 open with external reviewers (CodeRabbit/cubic/Copilot/Claude). CI green except where noted; `rest-api-parity` fixed. Once CI fully green and reviewers settle, ready to merge to `main` as v4.15.0.
