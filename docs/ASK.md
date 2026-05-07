# Ask Backend Selection

`axon ask` uses ACP by default. The headless path is opt-in and intended only
for synthesis: no tools, no permissions, no warm session, and no workspace
mutation.

| Backend | Selection | Behavior |
|---------|-----------|----------|
| ACP | `AXON_ASK_BACKEND=acp` | Default. Uses the configured ACP adapter, warm sessions, remote WS routing, and existing adapter isolation. |
| Headless | `AXON_ASK_BACKEND=headless` | Starts a short-lived agent CLI subprocess for answer synthesis. No warm session. Unsafe or unavailable agent cells fail closed. |
| Auto | `AXON_ASK_BACKEND=auto` | Currently ACP-equivalent. No heuristic selection is enabled. |

Agent selection stays on `AXON_ASK_AGENT=claude|codex|gemini`.

`[ask] backend = "acp"` is accepted in `~/.axon/config.toml`, but backend
selection is security-sensitive. Prefer `AXON_ASK_BACKEND` for local
experiments. Priority is environment over TOML over default.

Safety rules:

- Claude headless launches with stream JSON, session persistence disabled, an
  explicit empty tool list, strict empty MCP config, and plan permission mode.
- Codex headless is unavailable until a no-tool synthesis posture is proven.
  Full-auto, bypass, and danger-full-access modes are forbidden.
- Gemini headless is unavailable until a no-tool synthesis posture is proven.
  Yolo and auto-approval modes are forbidden.
- Any observed tool event in headless output is a hard error.

Benchmarking:

- Latency harness: `scripts/bench-ask.sh --backend all --agent all`
- Perf notes: `docs/perf/README.md`
- Quality set: `docs/eval/README.md`
- Parity report: `docs/perf/quality-parity-2026-05-07.md`
