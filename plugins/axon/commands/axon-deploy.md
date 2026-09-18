---
description: Start or restart Axon's local/reference Compose stack. Production Axon deployment uses the documented Incus or systemd path.
argument-hint: [up|restart|rebuild]
---

# Deploy Axon

Bring up the local/reference Compose stack on demand. Use this for development or local dependency provisioning when the stack is not running, or after editing `~/.axon/.env` / `~/.axon/config.toml`. Do not present this as Axon's supported production deployment; use `install-axon` with the Incus or bare-metal systemd contract for that.

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
