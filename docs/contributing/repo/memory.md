# Knowledge Persistence -- Axon

Axon uses `bd` (beads) for issue tracking and persistent knowledge, not markdown TODO lists or MEMORY.md files.

## Beads (bd)

All task tracking and knowledge persistence uses the `bd` CLI:

```bash
bd prime              # Full workflow context and commands
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
bd remember           # Save persistent knowledge
```

### Rules

- Use `bd` for ALL task tracking -- do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge -- do NOT create MEMORY.md files

## Session completion protocol

When ending a work session, complete ALL steps:

1. File issues for remaining work (`bd` create)
2. Run quality gates if code changed (`just verify`)
3. Update issue status (`bd close` / `bd update`)
4. Push to remote:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # Must show "up to date with origin"
   ```
5. Clean up stashes and remote branches
6. Provide context for next session

Work is NOT complete until `git push` succeeds.

## What NOT to persist

- Code patterns visible in the codebase (read the code)
- Git history facts (use `git log`)
- Debugging sessions (ephemeral)
- Temporary state
- Information already in `CLAUDE.md` or documentation files

## Related

- Root `CLAUDE.md` -- primary project instructions
- `docs/` -- structured documentation
- `bd prime` -- full beads workflow reference
