# Ask Backend Selection

`axon ask` uses the headless Gemini CLI path by default. The headless path is
intended only for synthesis: no tools, no permissions, no warm session, and no
workspace mutation.

| Backend | Selection | Behavior |
|---------|-----------|----------|
| Headless | `AXON_ASK_BACKEND=headless` | Default. Starts a short-lived Gemini CLI subprocess for answer synthesis. No warm session. Unsafe or unavailable agent cells fail closed. |
| ACP | `AXON_ASK_BACKEND=acp` | Uses the configured ACP adapter, warm sessions, remote WS routing, and existing adapter isolation. |
| Auto | `AXON_ASK_BACKEND=auto` | Currently ACP-equivalent. No heuristic selection is enabled. |

Agent selection stays on `AXON_ASK_AGENT=claude|codex|gemini`; the default is
`gemini` for headless ask.

`[ask] backend = "headless"` is accepted in `~/.axon/config.toml`, but backend
selection is security-sensitive. Prefer `AXON_ASK_BACKEND` for local overrides.
Priority is environment over TOML over default.

Safety rules:

- Claude headless launches with stream JSON, session persistence disabled, an
  explicit empty tool list, strict empty MCP config, and plan permission mode.
- Gemini headless launches `gemini-3.1-flash-lite-preview` by default with
  stream JSON, extension/MCP/skill/hook settings disabled in an isolated
  temporary HOME, and plan approval mode. Set `OPENAI_MODEL` to override the
  Gemini model. Set `AXON_HEADLESS_GEMINI_HOME` to copy auth from a prepared
  Gemini home instead of the process HOME.
- Codex headless is unavailable until a no-tool synthesis posture is proven.
  Full-auto, bypass, and danger-full-access modes are forbidden.
- Gemini yolo and auto-approval modes are forbidden.
- Any observed tool event in headless output is a hard error.

Benchmarking:

- Latency harness: `scripts/bench-ask.sh --backend all --agent all`
- Perf notes: `docs/perf/README.md`
- Quality set: `docs/eval/README.md`
- Parity report: `docs/perf/quality-parity-2026-05-07.md`
