# Axon documentation maintenance

Start with the [documentation index](README.md) and [maintenance workflow](development/documentation.md).

## Place material by purpose

[Guides](guides/) explain tasks and configuration; [reference](reference/) describes current public shapes; [architecture](architecture/) explains boundaries; [development](development/) covers contribution/testing; [operations](operations/) covers deployment and recovery. [Testing](testing/) holds qualification material and [ADRs](adr/) record decisions.

The [pipeline-unification packet](pipeline-unification/README.md) contains implemented design contracts and historical delivery notes. Preserve dated sessions, reports, investigations, plans, and archive records; do not present their future-tense implementation steps as current deployment instructions.

## Keep related documents connected

An operation page belongs in [action reference](reference/actions/). A source guide explains acquisition, prerequisites, identity, scope, limitations, and troubleshooting. Adding a provider does not automatically create another command: follow [source onboarding](development/adding-source.md) and [adapter guidance](development/adding-source-adapter.md).

Link current source guides, generated references, implementation modules, and applicable tests. A reference to a basename without a resolvable link is not sufficient navigation. Verify paths and headings in the branch being edited.

Use [database provenance](reference/runtime/database-schema.json), [CLI registry](reference/cli/commands.md), and [MCP reference](reference/mcp/overview.md) instead of reproducing changing inventories. Generated output must follow changes to its owning inputs, not manual prose patches.

## Check claims and examples

Separate supported deployment contracts from a particular installation. An example env file is not a running process configuration. A container healthcheck is not dependency readiness. Record private observations in local instructions, not shared documentation.

For agent-facing errors and warnings, explain affected operations, safe cause, partial effects, retryability, and corrective action. Keep command examples valid for the actual transport and avoid retired per-family worker or queue commands.

Run documentation/link/scope checks from the [maintenance guide](development/documentation.md). Code or schema changes need their corresponding tests; prose-only edits do not need provider startup.
