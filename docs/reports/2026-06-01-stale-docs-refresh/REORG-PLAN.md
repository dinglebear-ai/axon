# Phase 2 — proposed aggressive docs/ restructure

Designed from the 8 agent reports' reorg observations. All moves via `git mv` (history preserved);
every internal link + code/CI/script reference updated; final link-check verifies no dangling paths.

## Target tree

```
docs/
  README.md                         # rewritten navigation hub / index

  guides/                           # task-oriented + conceptual
    getting-started.md              # <- SETUP.md (+ fold stack/PRE-REQS prereq table)
    configuration.md                # <- CONFIG.md
    ask-rag.md                      # <- ASK.md
    reindexing.md                   # <- REINDEX-GUIDE.md
    context-injection.md            # <- CONTEXT-INJECTION.md
    ingest/                         # <- ingest/*  (github, gitlab, reddit, youtube, sessions, ingest)

  reference/
    commands/                       # <- commands/*  (all CLI pages incl. 6 new ones)
    mcp/                            # <- MCP.md, MCP-TOOL-SCHEMA.md, mcp/*, commands/mcp.md
    http-api.md                     # <- API.md
    api-parity.md                   # <- API-PARITY.md
    endpoints.md                    # <- ENDPOINTS.md
    shell-completions.md            # <- SHELL-COMPLETIONS.md
    cargo-features.md               # <- FEATURES.md  (clearer name; it's the Cargo feature matrix)
    spider-feature-flags.md         # <- SPIDER-FEATURE-FLAGS.md
    job-lifecycle.md                # <- JOB-LIFECYCLE.md
    inventory.md                    # <- INVENTORY.md
    qdrant-payload-schema.md        # <- contracts/qdrant-payload-schema.md
    env-matrix.md / env-matrix.toml # <- env-migration-matrix.md + config/env-migration-matrix.toml

  architecture/
    overview.md                     # <- ARCHITECTURE.md
    stack/                          # <- stack/*  (ARCH, TECH, PRE-REQS, CLAUDE)
    specs/                          # <- specs/server-mode-*, specs/vertical-extractor-metadata

  operations/
    deployment.md                   # <- DEPLOYMENT.md
    operations.md                   # <- OPERATIONS.md
    performance.md                  # <- PERFORMANCE.md
    security.md                     # <- SECURITY.md
    auth/                           # <- auth/*  (MCP-AUTH, API-TOKEN)

  contributing/
    rust.md                         # <- RUST.md
    testing.md                      # <- TESTING.md
    monolith-policy.md              # <- LIVE-TEST-SCRIPTS.md (misnamed; it IS the monolith policy)
    guardrails.md                   # <- GUARDRAILS.md
    checklist.md                    # <- CHECKLIST.md
    feature-delivery-framework.md   # <- FEATURE-DELIVERY-FRAMEWORK.md
    repo/                           # <- repo/*  (RECIPES, SCRIPTS, REPO, MEMORY, RULES, CLAUDE)

  history/                          # all dated / point-in-time records (no accuracy upkeep)
    sessions/                       # <- sessions/        (617)
    reports/                        # <- reports/         (161, incl. this refresh's audit)
    plans/                          # <- plans/           (80)
    archive/                        # <- archive/ + production-readiness-sprint-report + specs/android-redesign
    superpowers/                    # <- superpowers/     (plans/specs)
    perf-snapshots/                 # <- perf/*.md + perf/*.json (dated bead artifacts)

  assets/
    palette-demo/                   # <- palette-demo/ (screenshots)
```

## Risk handling
- CI/script refs to move: `scripts/check_mcp_schema_doc.sh`, `scripts/check_legacy_runtime_terms.sh`,
  `scripts/test-mcp-tools-mcporter.sh`, `tests/http_api_parity_inventory.rs`, plus the MCP-TOOL-SCHEMA
  generator path in `src/mcp/*`. Each updated in lockstep with the move.
- Code refs (src/**/*.rs, src/**/README.md, Cargo.toml, .cargo/config.toml, CHANGELOG.md, root
  CLAUDE.md, README.md, apps/desktop/README.md) updated for every moved living doc.
- Historical docs (sessions/plans) that mention old doc paths are NOT rewritten — they're point-in-time.
- Final: a link-checker pass over all living docs + a grep sweep for stale `docs/<OLD>` paths in code.
