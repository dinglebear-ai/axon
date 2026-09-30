# axon completions
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | <code>axon completions</code> |
| REST | Not exposed in this registry |
| MCP atomic tools | Not exposed in this registry |
| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |

MCP names describe the atomic projection. The legacy `axon` tool uses the corresponding action/subaction selectors; `both` exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.

Family-level navigation does not imply identical suboperations or request shapes across transports.
<!-- END GENERATED ACTION SURFACES -->


Generate shell completion scripts for `axon`.

## Synopsis

```bash
axon completions <bash|zsh|fish>
```

## Examples

```bash
# Bash
axon completions bash > ~/.local/share/bash-completion/completions/axon

# Zsh
mkdir -p ~/.zfunc
axon completions zsh > ~/.zfunc/_axon

# Fish
mkdir -p ~/.config/fish/completions
axon completions fish > ~/.config/fish/completions/axon.fish
```

## Installation

- Bash: write the script to `~/.local/share/bash-completion/completions/axon`, then restart the shell or `source` the file.
- Zsh: write the script to a directory in `fpath` such as `~/.zfunc/_axon`, add `fpath=(~/.zfunc $fpath)` to `.zshrc` if needed, then run `autoload -Uz compinit && compinit`.
- Fish: write the script to `~/.config/fish/completions/axon.fish`; fish loads it automatically in new shells.

## Notes

- `axon completion` (singular) is an accepted alias for `axon completions`.
- Completion generation is local-only and does not require Axon service env vars.
- Scripts are generated from the canonical clap CLI definition, so command trees, flags, and value-enum options stay in sync with runtime parsing.
