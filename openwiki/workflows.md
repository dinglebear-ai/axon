---
type: "Reference"
title: "Workflows"
description: "Updated CI and OpenWiki workflow behavior for `e7d34a6b`, including restored gates, new docs checks, and automation payload changes."
---

# Workflows

This page tracks repository workflow behavior relevant to this update.

## CI (`.github/workflows/ci.yml`)

Core CI behavior changed in three ways:

- Temporary `false &&` disablements were removed on many jobs, so path-gated execution is active again.
- `jdx/mise-action` is used for install-shim consistency in selected jobs (`taplo-fmt`, `mcp-smoke`, security checks, etc.).
- Some previously deferred checks are now active by default when their path predicates match (`test`, `clippy`, `release-smoke`, security checks, live RAG on PRs, etc.).

Notable job-level changes:

- Added `chrome-extension` test job behind `needs.changes.outputs.chrome == 'true'`.
- `mcp-schema-doc-sync` now runs both schema sync and documentation generation + docs contract checks.
- `verify required jobs` is stricter: required jobs must succeed; conditionally-skipped jobs must only be skipped when their predicates are false.

Example flow in the updated `verify required jobs` phase:

```text
- require_success_or_intentional_skip(job, result, should_run)
- if should_run && result == skipped => hard failure
- if result in {failure, cancelled, timed_out} => hard failure
```

## OpenWiki update workflow (`.github/workflows/openwiki-update.yml`)

The update workflow now performs a guarded run flow:

1. connect Tailscale,
2. install `openwiki`,
3. preflight the OpenAI-compatible API endpoint,
4. run `openwiki --update --print`,
5. open PR with expanded payload.

The payload now includes:

- `openwiki/`
- `AGENTS.md`
- `CLAUDE.md`
- `.github/workflows/openwiki-update.yml`

This ensures control docs and workflow definitions stay synchronized with generation.

## Environment/permissions note

The workflow requires a compatible API token (`OPENAI_COMPATIBLE_API_KEY`) and host endpoint availability; preflight fails fast with a clear error if the endpoint does not return HTTP 200.

## Why this changed

- CI and doc automation are now both stricter and more closely synchronized with real source conditions.
- The update path now has stronger guardrails for tool availability and generated file scope.
