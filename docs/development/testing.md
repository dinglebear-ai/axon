---
title: "Testing Guide"
created: 2026-02-26
updated: 2026-09-29
---

# Testing Guide

Last reviewed: 2026-09-29

Test the owning crate and the actual runtime surface changed. CLI, MCP, and
HTTP are projections over shared services; passing a parser or compiling the
root binary does not prove job execution, authorization, or publication.
The [Justfile](../../Justfile), [workflow definitions](../../.github/workflows/),
and suite source define the commands and prerequisites.

For catalog-driven end-to-end execution, isolation, evidence, teardown, and
release qualification, use [E2E operations](../guides/e2e-operations.md).

## Test lanes

### Focused inner loop

```bash
cargo test -p <affected-crate> <test-filter> --locked
cargo fmt --all -- --check
cargo xtask check-layering
```

Replace the crate and filter with real selectors. Confirm the output includes
the intended tests; a zero-test success is not coverage. Preserve sidecar
filenames, `#[path]` declarations, and feature/cfg gates. `cargo check` does
not compile every test module or execute assertions. Use `cargo test -p
<affected-crate> --no-run --locked` to compile test targets when needed, then
run the selected tests.

### Local recipe and explicit workspace coverage

```bash
just test
just test-fast
```

`just test` uses `cargo nextest run --locked --workspace` when nextest is
available. Its `cargo test` fallback is lockfile-checked but does **not** add
`--workspace`; do not describe that fallback as identical workspace coverage.
`just test-fast` is library-focused and also does not request the entire
workspace. For explicit full-workspace coverage, run:

```bash
cargo nextest run --workspace --locked --no-tests=fail
# Alternative without nextest:
cargo test --workspace --locked
```

Feature-gated and ignored/live tests still need their documented flags and
providers. A complete local command is not automatically a complete live test.

### Watch lifecycle lane

```bash
just test-watch
```

This nextest recipe selects watch tests in `axon-jobs`, `axon-services`,
`axon-cli`, and `axon-web`, and fails if the filter selects no tests. Changes
to orchestration also need cancellation, heartbeats, stale recovery, retries,
and existing-state upgrade coverage.

### Generated contracts and prose

```bash
cargo xtask generated-contracts refresh
cargo xtask generated-contracts check
python3 scripts/test_operational_docs.py
```

Refresh only after intentional source/input changes, inspect the diff, and
check again without writes. A passing generator means its outputs match its
inputs, not that every handwritten explanation is correct. Follow
[documentation maintenance](documentation.md) for generator ownership and
local links. Do not hand-patch generated regions.

### Broad integration gate

```bash
just verify
```

The recipe composes runtime/ownership checks, plugin and operational contracts,
web checks, formatting, clippy, dependency hygiene, compilation, and tests.
Read the current Justfile instead of maintaining a second stale recipe list
here. Prose-only changes use the documentation lane; source changes require
the relevant code gates as well.

## Coverage areas and owners

| Boundary | Where to find its tests | Required behavior to verify |
|---|---|---|
| Source adapters | [axon-adapters](../../crates/axon-adapters/src/) | Added/modified/removed/unchanged items, incomplete inventories, normalization, and access boundaries |
| Preparation and embedding | [axon-document](../../crates/axon-document/src/), [axon-embedding](../../crates/axon-embedding/src/) | Chunk identity, parser behavior, provider retry/cooling, and bounded batches |
| Publication and retrieval | [axon-vectors](../../crates/axon-vectors/src/), [axon-retrieval](../../crates/axon-retrieval/src/) | Committed generation visibility, filters, refresh/removal, and no stale retrieval |
| Durable state | [axon-jobs](../../crates/axon-jobs/src/), [axon-ledger](../../crates/axon-ledger/src/) | Attempts, stages, cancellation, recovery, cleanup debt, and migrations from existing state |
| Shared orchestration | [axon-services](../../crates/axon-services/src/) | One source job across stages, policy binding, failure diagnostics, and safe retry boundaries |
| CLI | [axon-cli](../../crates/axon-cli/src/), [root integration tests](../../tests/) | Actual parser/dispatch, JSON/stdout separation, progress, and terminal exit status |
| HTTP/MCP | [axon-web](../../crates/axon-web/src/), [axon-mcp](../../crates/axon-mcp/src/) | Discovery, schemas, dispatch, authorization, tasks, resources, and invalid inputs |
| Property tests | [HTTP proptests](../../crates/axon-core/src/http/proptest_tests.rs), [URL proptests](../../crates/axon-adapters/src/web_engine/engine/url_utils_proptest_tests.rs) | Arbitrary inputs at safety and discovery boundaries |

Do not rely on historical test counts or the removed root `src/services` /
`src/web` layout to select coverage. Compile the affected crate's sidecars
and inspect the actual test list.

## Live infrastructure and isolation

Some suites return early when provider variables are unset; others are
ignored or feature-gated until explicitly enabled. Report skipped/ignored
coverage separately from passing real I/O. Read the selected suite's resolver
and prerequisites, including `AXON_TEST_QDRANT_URL` where used.

Pass temporary overrides to an invocation or use an isolated test configuration.
Do not rewrite `~/.axon/.env` or an existing production configuration to run a
smoke test. A local development provider stack can be started with
`just services-up`, but that uses the configured Compose environment and is
not automatically a disposable per-test stack. Inspect its targets first.

Qdrant collections, SQLite paths, artifacts, and job namespaces must be
allocated for the test and cleaned up through the suite's supported teardown.
Do not point a destructive/reset test at an ordinary development or production
collection. The tracked Compose definitions, not an old table of image tags
or test ports, define the local provider wiring. Production remains native
Axon under systemd/Incus with external providers.

## REST and MCP smoke tests

Build the matching binary before exercising a transport:

```bash
cargo build --bin axon
```

Select the intended local/server runtime and verify its revision/configuration.
REST callers use the real `/v1` routes and required authorization; web-panel
unlock credentials are separate. Start with a read-only `/v1/status` request
and then the specific changed operation against isolated test inputs. A live
HTTP listener does not prove embedding, browser, or worker availability.

The repository MCP suite is:

```bash
just mcp-smoke
# Equivalent entry point:
bash scripts/test-mcp-tools-mcporter.sh
```

Inspect the script for current prerequisites, configuration overrides, output
locations, and provider requirements. The old `client-server-smoke` recipe
and `scripts/test-client-server-mode.sh` are not present. Generic
`AXON_SERVER_URL` CLI forwarding is not the transport under test.

Read-only discovery and calls using the repository mcporter configuration:

```bash
mcporter --config config/mcporter.json list axon --schema
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json
mcporter --config config/mcporter.json call axon.axon action:jobs subaction:list limit:5 --output json
```

The last two calls assume the legacy `axon` projection is enabled. In atomic
mode, use the exact names and argument schemas returned by discovery instead.
Test the actual `tools/list` catalog, not only the primary action enum: the
auxiliary dashboard tool, resources, system/watch request types, and protocol
tasks are also part of the server. See [MCP overview](../reference/mcp/overview.md).

Include invalid-input and authentication failures, allowed-origin checks,
ownership/visibility denials, and returned opaque artifact IDs. For a queued
operation, inspect the same job to a terminal result; a descriptor alone does
not establish completion or committed publication.

## Browser and native clients

Use the app's own manifest and tests. The web panel's build/check entry points
are `just web-build` and `just web-check`; generated OpenAPI drift is checked
by `cargo xtask check-openapi-drift`. Do not claim the frontend has no tests
without inspecting its current package and source.

[Desktop Palette testing](desktop-palette-testing.md) separates deterministic
fixtures, native Windows packaging, and live API/job evidence. The Android and
Chrome extension surfaces likewise use shared contracts, not private DTO
copies maintained independently of the generators.

## Test-only security exceptions

[HTTP client helpers](../../crates/axon-core/src/http/client.rs) and
[SSRF test controls](../../crates/axon-core/src/http/ssrf.rs) contain scoped
test accommodations. These are not deployment settings or instructions to
disable production checks. Keep new bypasses under test-only cfg/features,
restore temporary state, and test rejection on the production path.

## CI and release evidence

Use the [current workflows](../../.github/workflows/) and
[release qualification](../testing/release-qualification.md) to determine which
checks actually ran for a change. A skipped job, filtered test set, or
fixture-only run is not evidence of a live integration pass. Do not waive a
failed gate by renaming it or widening a checker exception.

The repository-owned hermetic E2E entry point is `just e2e-hermetic`; its
required-check and promotion policy are documented in
[E2E hermetic CI](../guides/e2e-hermetic-ci.md). Live homelab qualification is
separate and requires its own authorization, provider credentials, evidence,
and teardown verification.

## Common failure modes

**No tests or skipped providers:** Fix the package/filter/feature selection or
provision the named test provider. Re-run the intended lane and record the
number of tests actually executed.

**Lockfile mismatch:** Inspect the manifest/lockfile diff. Update the lockfile
only for an intentional dependency change, then re-run with `--locked`. Do
not silently fetch a new dependency graph to mask unrelated drift.

**SQLite or artifact failures:** Check the suite's allocated paths, permissions,
existing schema, and cleanup report. Do not delete shared state to make an
upgrade test pass.

**Queued operation stalls:** Inspect worker and provider diagnostics and the
job's completed stages. Cancellation, wait timeout, retry, and unknown commit
status are distinct conditions; use the reported recovery guidance.
