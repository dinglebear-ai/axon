---
title: "Maintaining Axon documentation"
created: 2026-09-27
updated: 2026-09-27
---

# Maintaining Axon documentation

Last reviewed: 2026-09-27

This guide explains where facts belong and how to change documentation without
creating a competing source of truth. Start with [AGENTS.md](../../AGENTS.md)
and the [documentation map](../README.md).

## Sources of truth

| Question | Owning source |
|---|---|
| How should an agent work in this repository? | Root AGENTS.md, extended by the nearest scoped AGENTS.md |
| What version, toolchain, or workspace members exist? | Cargo.toml, rust-toolchain.toml, and member manifests |
| What CLI commands and options exist? | reference/cli/commands.json and its generated Markdown projection |
| What MCP tools are advertised? | crates/axon-mcp/src/server.rs and a matching runtime tools/list; the action schema alone is not the whole catalog |
| What are the MCP request and response shapes? | reference/mcp/ and their schema generator inputs |
| What REST routes exist? | Web routers and apps/web/openapi/axon.json |
| What configuration keys and defaults exist? | Typed config parsing, config.example.toml, reference/config/ |
| What tables and migrations exist? | Owning crate migrations and reference/runtime/database-schema.json |
| Where does an operation belong? | architecture/crate-ownership.md and architecture/crate-structure.md |
| How does a component release? | release/components.toml, release workflows, and development/release-checklist.md |
| How does production run? | operations/deployment.md and deploy/incus/ or deploy/systemd/ |

Paths in this table without links are relative to the repository or docs tree
as named. Follow the linked documentation index for the complete navigation.
Do not copy version numbers or complete generated inventories into agent
instructions. An audit date records when prose was reviewed, not proof that a
future release still has the same shape.

## Canonical agent files

Every instruction directory has one regular AGENTS.md file and two direct
relative symlinks: CLAUDE.md -> AGENTS.md and GEMINI.md -> AGENTS.md. Scoped
guides retain their scope; do not point every alias at the root guide.

For a new scope, write AGENTS.md first, then create aliases from that directory:

~~~bash
ln -s AGENTS.md CLAUDE.md
ln -s AGENTS.md GEMINI.md
~~~

Never overwrite a divergent alias without preserving/reconciling its content.
Relative links must survive a clone or worktree move. A chain through another
alias, an absolute path, a copied file, or a symlinked AGENTS.md is invalid.
On platforms that materialize Git symlinks as text, enable proper symlink
support before editing; do not commit those placeholder files as replacements.

The compatibility-named cargo xtask check-claude-symlinks command enforces this
AGENTS-first contract. The crate-structure validator calls the same directory
validator. Other worktrees and immutable review snapshots are excluded.

The fleet workflow uses `scripts/check_repository_contract.py` around its
immutable upstream validator. That adapter replaces only the obsolete
CLAUDE-first symlink rule with an index-mode/blob check for canonical AGENTS.md.
It does not filter failures or disable any unrelated fleet checks. Its tests
cover malformed aliases, staged-versus-working-tree differences, and preservation
of unrelated failures and upstream exceptions.

Keep inherited guidance concise. Codex defaults to a 32 KiB project instruction
budget; a large root guide can prevent a nested guide from loading fully.
The operational documentation check caps each tracked root-to-scope chain at
30 KiB, leaving space for loader separators. Move detailed reference prose
into linked guides rather than requiring every user to raise a local limit.
The root guide is limited to 7,500 characters; every scoped guide must have at
least three working references. The operational documentation check enforces
these constraints across all tracked scopes, not only the root.

## Local, untracked instructions

Use `AGENTS.override.md` for Codex and `CLAUDE.local.md` for Claude Code,
not `CLAUDE.md.local`. Keep the override as a regular local file and make
`CLAUDE.local.md` a direct relative symlink to it. Both names are Git-ignored.
Shared AGENTS.md files must not contain private hostnames, endpoints,
personal checkout paths, hardware assignments, or personal workflows.

Codex selects at most one instruction file per directory and prefers the
override. Begin it with an explicit instruction to read the sibling AGENTS.md
if not already loaded; the two files are not automatically concatenated.
Claude Code loads CLAUDE.local.md alongside CLAUDE.md. Shared aliases still
point to AGENTS.md; only the local alias points to AGENTS.override.md.

After creating the override, run from that same directory:

~~~bash
ln -s AGENTS.override.md CLAUDE.local.md
git check-ignore -v AGENTS.override.md CLAUDE.local.md
~~~

Preserve existing local notes before replacing either path. Ignored files do
not automatically follow a clone or new worktree. Never commit the local alias
alone because its target would be absent in another checkout.

Sources: [Codex instruction discovery](https://developers.openai.com/codex/guides/agents-md/)
and [Claude Code local instructions](https://code.claude.com/docs/en/memory).

## Reviewing a documentation change

Read the implementation, current generated contract, and relevant scoped guide
before updating prose. Search the living docs for the old path, flag, or claim,
not just the page where the problem was first noticed. Preserve dated history
under sessions, reports, plans, investigations, superpowers, perf, and archive.
The pipeline-unification packet describes the implemented clean break; its
future-tense delivery notes are historical, not a live migration requirement.

Keep examples safe: use placeholder credentials, make destructive operations
explicit, and do not run deployment or data-reset commands to check prose.
Do not infer that a worktree feature is shipped on main. Source additions use
the existing SourceAdapter/pipeline abstractions, not new per-source queues.

## Verification by changed surface

For prose and aliases, start with:

~~~bash
git diff --check
python3 scripts/test_operational_docs.py
~~~

The operational documentation test checks canonical agent files in the tracked
checkout, along with deployment/auth/installer claims. An already-built xtask
can additionally run check-claude-symlinks, check-broken-symlinks,
check-doc-links, and check-doc-contracts. Do not incur a full product build
solely to revalidate prose. Check relative links in changed pages even outside
the generated-reference link check's scope.

When the Rust validators change, run their sidecar tests (the claude_symlinks
and repo_structure test filters in the xtask package), rustfmt, and the updated
checks against the real checkout. When CI path routing changes, run the tests/ci_changed_paths.rs
regression suite against the Python classifier. Use the repository's normal broader gates for runtime code.

When schema inputs change, run cargo xtask generated-contracts refresh followed
by cargo xtask generated-contracts check. Do not hand-patch generated Markdown
to hide schema drift. Documentation and development-tool-only changes do not
require a product version bump; release/components.toml owns shipping paths.

## Publishing without losing existing work

Record git status --short --branch and git worktree list before changing files.
Preserve unrelated worktrees, stashes, and ignored runtime data. When the user
explicitly requests including pre-existing changes, inspect those changes for
secrets and accidental build output, include the intended files, and report
what was included. Do not force-add ignored credentials or runtime state.

Before publishing, review the staged diff, run the applicable checks, fetch
origin, and reconcile a changed remote without discarding either side. Use a
normal non-force push to the authorized branch. Verify that local HEAD matches
the remote branch and that the checkout is clean; a successful local commit
alone is not publication.
