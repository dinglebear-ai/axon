---
description: Deploy or restart the axon stack (qdrant, tei, chrome, axon) via docker compose.
argument-hint: [up|restart|rebuild]
---

# Deploy Axon

Explicitly bring up the axon stack. Use this when the SessionStart hook reports
the stack is down, or after editing `~/.axon/.env` / `~/.axon/config.toml`.

The session-start hook only auto-deploys when `/readyz` is unreachable; it stays
silent when the stack is already healthy. This command is the manual escape hatch
for an on-demand deploy, restart, or rebuild.

```bash
"${CLAUDE_PLUGIN_ROOT:-plugins/axon}/bin/axon" compose ${ARGUMENTS:-up}
```

After it returns, confirm health:

```bash
"${CLAUDE_PLUGIN_ROOT:-plugins/axon}/bin/axon" doctor
```

`compose up` starts containers and waits for readiness. Pass `restart` to bounce
running containers, or `rebuild` to rebuild images from the checkout and bring
them back up. Report the readiness/doctor result; if a service is still not
ready, surface the failing phase rather than retrying blindly.
