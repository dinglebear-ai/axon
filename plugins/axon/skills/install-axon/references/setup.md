# Axon setup

## Install

Current release trust is fail-closed. From reviewed source:

```sh
AXON_INSTALL_METHOD=build ./install.sh
axon setup init
```

Use a release only after its pinned installer/archive checksum + minisign verify against Axon's published trust anchor.

## Auth

Bearer setup generates `AXON_HTTP_TOKEN`. OAuth public/non-secret setup:

```sh
axon setup init --auth-mode oauth --oauth-public-url https://axon.example.com --google-client-id "$GOOGLE_CLIENT_ID" --auth-admin-email you@example.com
```

Google redirect: `https://axon.example.com/auth/google/callback`. `axon setup init` creates `~/.axon/.env` with mode 0600; preserve it and add `AXON_GOOGLE_CLIENT_SECRET` there with a dedicated file editor or secure local entry, never as a CLI argument. Do not print the secret. Claude MCP callback is already allowlisted. OAuth and static bearer can coexist.

## Deploy

Incus: `deploy/incus/README.md` + `bootstrap.sh`. Set `AXON_INCUS_RUN_SERVER=true` only when intentionally enabling the guest Axon server with guest-exclusive SQLite; never point host and guest servers at the same jobs DB.

Bare metal: `deploy/systemd/README.md`; env `/etc/axon/axon.env`, data `/var/lib/axon`, default 127.0.0.1:8001. A Dockerized Axon process is not a supported production deployment.

## LLM, client, proof

Backends: `gemini-headless`, `openai-compat`, `codex-app-server`. Codex is isolated by default; `AXON_CODEX_LOAD_USER_CONFIG=true` is an explicit trust expansion.

Preserve client config and credentials. Then:

```sh
axon preflight
axon doctor
axon status
```

`axon smoke` also indexes example.com, so ask before using it. Debug provider/auth/server/proxy/client/tool layers independently.
