---
name: install-axon
description: Install or repair Axon. Use when bootstrapping Axon, choosing Incus or systemd, configuring bearer/OAuth auth, exposing MCP, selecting Codex app-server synthesis, connecting an agent, or verifying the RAG stack.
---

# Install Axon

Use Axon's installer/setup/deploy surfaces.

## Rules

- Inspect Axon/data, host, ports, Incus/systemd/GPU, providers, clients, and public URL.
- Until the signing trust anchor is published, use reviewed source with `AXON_INSTALL_METHOD=build ./install.sh`. Never pipe mutable main.
- Ask local/Incus/systemd mode and bearer/Google OAuth/dual auth. Never run host and guest servers on shared SQLite. Non-loopback requires auth; do not claim unverified Authelia support.
- Existing proxy/Tailscale/Incus/systemd config edits require current docs, verified backup/checksum, exact changes, and explicit approval.

## Flow

1. Follow [setup](references/setup.md): install, run `axon setup init`, preserve Axon config/secrets.
2. Configure auth. OAuth needs public URL, Google credentials, admin email, callback `<public>/auth/google/callback`; a static `AXON_HTTP_TOKEN` remains valid in OAuth dual mode.
3. Deploy. Incus is preferred, but enable its Axon server only with guest-exclusive SQLite. Bare metal uses the committed systemd unit. Verify reboot persistence.
4. Configure providers and optional `AXON_LLM_BACKEND=codex-app-server`; keep isolated Codex config unless explicitly opted out.
5. For remote HTTPS, preserve Axon's own auth behind the proxy. Apply the config approval gate and require domain verification.
6. Run `axon preflight`/`doctor` and optional `smoke`. Configure the agent plugin/MCP, restart it, and prove a fresh read-only doctor/status/help call.
