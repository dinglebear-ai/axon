import { useActionState, useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import type { PaletteAction, RemotePaletteAction } from "@/lib/actions";
import { buildHelpRun, findHelpTarget, helpAction } from "@/lib/actionHelp";
import { crawlSeedUrl, newRequestId, normalizeSubmitArgument } from "@/lib/appHelpers";
import {
  buildActionRequest,
  executeAction,
  type Client,
  type HttpMethod,
  type PaletteConfig,
  type PaletteResult,
} from "@/lib/axonClient";
import { hostFromUrl, summarizeCrawl } from "@/lib/crawlJob";
import { formatPayload, outputKindFor, type OutputKind } from "@/lib/format";
import { appWindow, invoke, isTauriRuntime } from "@/lib/invoke";
import { argumentFor, validationMessage, type ParsedCommand } from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";

type PaletteStreamEvent =
  | { type: "started"; requestId: string; path: string }
  | { type: "delta"; requestId: string; text: string }
  | { type: "done"; requestId: string; answer?: string | null }
  | { type: "error"; requestId: string; message: string };

interface UseActionRunnerArgs {
  client: Client | null;
  config: PaletteConfig | null;
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>;
  setModeAction: Dispatch<SetStateAction<PaletteAction | null>>;
  setQuery: Dispatch<SetStateAction<string>>;
  setBrowseOpen: Dispatch<SetStateAction<boolean>>;
  modeAction: PaletteAction | null;
  parsed: ParsedCommand;
  query: string;
}

// ── Run/result factories ───────────────────────────────────────────────────
// Centralize the `{ ok:false, status:0, ... }` PaletteResult that submit used to
// hand-roll 4-5× (finding L1), and give streaming-derived terminal states an
// honest shape instead of fabricating a fake HTTP `{ ok:true, status:200 }`
// (finding A-M4). `status: 0` here means "no HTTP round-trip produced this
// result" (client-side error or a terminal stream event), distinct from a real
// backend status code.

// The terminal (success|error) RunState member — the only one carrying `result`.
// Extracting by the discriminating `result` property is robust to runState.ts
// modelling success/error as one combined `{ kind: "success" | "error" }` member.
type TerminalRun = Extract<RunState, { result: PaletteResult }>;

function errorResult(message: string, path: string, method: HttpMethod = "POST"): PaletteResult {
  return { ok: false, status: 0, path, method, payload: { error: message } };
}

// A one-shot/local error RunState (missing client, thrown request, etc.).
function makeErrorRun(args: {
  title: string;
  subtitle: string;
  message: string;
  path: string;
  outputKind?: OutputKind;
  prompt?: string;
}): TerminalRun {
  return {
    kind: "error",
    title: args.title,
    subtitle: args.subtitle,
    text: args.message,
    outputKind: args.outputKind ?? "code",
    prompt: args.prompt,
    result: errorResult(args.message, args.path),
  };
}

// Terminal error of a streamed action (the `palette://stream` "error" event or a
// failed stream invoke). Honest result — no fabricated 200.
function makeStreamErrorRun(args: {
  actionLabel: string;
  path: string;
  message: string;
  outputKind: OutputKind;
  prompt?: string;
}): TerminalRun {
  return {
    kind: "error",
    title: `${args.actionLabel} failed`,
    subtitle: args.path,
    text: args.message,
    outputKind: args.outputKind,
    prompt: args.prompt,
    result: errorResult(args.message, args.path),
  };
}

// Terminal success of a streamed action. The answer never travelled through the
// one-shot HTTP path, so `status: 0` is the truthful marker (A-M4); the payload
// carries the streamed answer for downstream views.
function makeStreamSuccessRun(args: {
  actionLabel: string;
  subtitle: string;
  text: string;
  outputKind: OutputKind;
  path: string;
  prompt?: string;
}): TerminalRun {
  return {
    kind: "success",
    title: `${args.actionLabel} completed`,
    subtitle: args.subtitle,
    text: args.text,
    outputKind: args.outputKind,
    prompt: args.prompt,
    result: {
      ok: true,
      status: 0,
      path: args.path,
      method: "POST",
      payload: { answer: args.text },
    },
  };
}

// ── Streaming event reducer ──────────────────────────────────────────────────
// Folds a `palette://stream` event into the current RunState. Pure so it can be
// unit-tested without the Tauri event bridge.
export function reduceStreamEvent(current: RunState, payload: PaletteStreamEvent): RunState {
  if (current.kind !== "streaming" || !("requestId" in payload) || current.requestId !== payload.requestId) {
    return current;
  }
  if (payload.type === "delta") {
    return { ...current, text: current.text + payload.text };
  }
  if (payload.type === "done") {
    return makeStreamSuccessRun({
      actionLabel: current.actionLabel,
      subtitle: current.subtitle,
      text: payload.answer ?? current.text,
      outputKind: current.outputKind,
      path: current.path,
      prompt: current.prompt,
    });
  }
  if (payload.type === "error") {
    return makeStreamErrorRun({
      actionLabel: current.actionLabel,
      path: current.path,
      message: payload.message,
      outputKind: current.outputKind,
      prompt: current.prompt,
    });
  }
  return current;
}

// Action-execution engine for the palette: turns a selected action + argument
// into a backend call, routing crawl → live job, ask → streamed answer, and
// everything else → one-shot request, while recording each run into history.
// `run`/`history` state stays owned by App; this hook holds the logic only.
export function useActionRunner({
  client,
  config,
  run,
  setRun,
  setHistory,
  setModeAction,
  setQuery,
  setBrowseOpen,
  modeAction,
  parsed,
  query,
}: UseActionRunnerArgs) {
  useEffect(() => {
    let disposed = false;
    const unlisten = appWindow.listen<PaletteStreamEvent>("palette://stream", (event) => {
      if (disposed) return;
      setRun((current) => reduceStreamEvent(current, event.payload));
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [setRun]);

  function pushHistory(
    action: PaletteAction,
    target: string,
    entry: {
      status: number;
      title: string;
      subtitle: string;
      text?: string;
      outputKind?: OutputKind;
      result?: PaletteResult;
    },
  ) {
    setHistory((items) =>
      [
        { action, target, ...entry, when: "just now", duration: entry.status === 0 ? "fail" : undefined },
        ...items,
      ].slice(0, 18),
    );
  }

  // ── R-H1: one-shot request/response actions via useActionState ────────────
  // The non-streaming, non-job branch is a textbook fit for a React 19 Action:
  // pending state, error capture, and in-flight serialization are owned by the
  // runtime. `dispatchOneShot` is wrapped by the runtime (no manual running-flag
  // guard); the action writes its terminal RunState into App-owned `run` and
  // history. `oneShotPending` replaces `run.kind === "running"` for these.
  interface OneShotInput {
    action: RemotePaletteAction;
    argument: string;
    config: PaletteConfig;
    client: Client;
  }
  const [, dispatchOneShot, oneShotPending] = useActionState<RunState, OneShotInput>(
    async (_prev, input) => runOneShot(input),
    { kind: "idle" },
  );

  async function runOneShot({ action, argument, config: cfg, client: cli }: OneShotInput): Promise<RunState> {
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    const isConversation = action.subcommand === "ask" || action.subcommand === "chat";
    setRun({
      kind: "running",
      title: isConversation ? action.label : `Running ${action.label}`,
      subtitle: isConversation ? `POST ${action.subcommand === "ask" ? "/v1/ask" : "/v1/chat"}` : commandLine,
      prompt: isConversation ? argument : undefined,
    });
    try {
      const result = await executeAction(cli, action, argument, cfg);
      const text = formatPayload(action.subcommand, result.payload);
      const title = isConversation ? action.label : `${action.label} ${result.ok ? "completed" : "failed"}`;
      const subtitle =
        action.subcommand === "ask"
          ? `RAG over ${cfg.collection || "axon"} | ${result.path}`
          : action.subcommand === "chat"
            ? result.path
            : `${result.method} ${result.path} | HTTP ${result.status}`;
      pushHistory(action, argument || action.subcommand, {
        status: result.status,
        title,
        subtitle,
        text,
        outputKind: outputKindFor(action.subcommand),
        result,
      });
      const next: RunState = {
        kind: result.ok ? "success" : "error",
        title,
        subtitle,
        text,
        outputKind: outputKindFor(action.subcommand),
        prompt: isConversation ? argument : undefined,
        result,
      };
      setRun(next);
      return next;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      const title = isConversation ? action.label : `${action.label} failed`;
      const subtitle =
        action.subcommand === "ask"
          ? `RAG over ${cfg.collection || "axon"} | /v1/ask`
          : action.subcommand === "chat"
            ? "/v1/chat"
            : commandLine;
      const next = makeErrorRun({
        title,
        subtitle,
        message,
        path: "",
        outputKind: outputKindFor(action.subcommand),
        prompt: isConversation ? argument : undefined,
      });
      pushHistory(action, argument || action.subcommand, {
        status: 0,
        title,
        subtitle,
        text: message,
        outputKind: outputKindFor(action.subcommand),
        result: next.result,
      });
      setRun(next);
      return next;
    }
  }

  // ── Crawl branch: submit the job, then hand the live poll off to useCrawlJob ─
  async function submitCrawl(action: RemotePaletteAction, argument: string, cli: Client, cfg: PaletteConfig) {
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    const seedUrl = crawlSeedUrl(argument);
    const startedAtMs = Date.now();
    const pendingSnapshot = summarizeCrawl({ job: { status: "pending" } }, { jobId: "", url: seedUrl });
    setRun({
      kind: "job",
      family: "crawl",
      title: `Crawling ${hostFromUrl(seedUrl)}`,
      subtitle: "submitting…",
      jobId: "",
      statusUrl: "",
      url: seedUrl,
      startedAtMs,
      maxPages: 0,
      maxDepth: 0,
      snapshot: pendingSnapshot,
      minimized: false,
    });
    try {
      const result = await executeAction(cli, action, argument, cfg);
      const payload = (result.payload ?? {}) as Record<string, unknown>;
      const jobId =
        typeof payload.job_id === "string"
          ? payload.job_id
          : typeof payload.id === "string"
            ? payload.id
            : null;
      if (!result.ok || !jobId) {
        const text = formatPayload(action.subcommand, result.payload);
        const subtitle = `${result.method} ${result.path} | HTTP ${result.status}`;
        pushHistory(action, seedUrl, {
          status: result.status,
          title: "Crawl failed",
          subtitle,
          text,
          outputKind: "code",
          result,
        });
        setRun({ kind: "error", title: "Crawl failed", subtitle, text, outputKind: "code", result });
        return;
      }
      pushHistory(action, seedUrl, {
        status: result.status,
        title: `Crawling ${hostFromUrl(seedUrl)}`,
        subtitle: `job ${jobId}`,
        outputKind: "code",
        result,
      });
      setRun((current) =>
        current.kind === "job" && current.url === seedUrl
          ? {
              ...current,
              jobId,
              statusUrl: `/v1/crawl/${jobId}`,
              subtitle: `job ${jobId}`,
              snapshot: { ...current.snapshot, jobId },
            }
          : current,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setRun(makeErrorRun({ title: "Crawl failed", subtitle: commandLine, message, path: "/v1/crawl" }));
    }
  }

  // ── Stream branch: start the streamed request; terminal states arrive via the
  // `palette://stream` listener and reduceStreamEvent. Returns false when the
  // current (non-Tauri) runtime can't stream so the caller falls back to one-shot.
  async function submitStream(action: RemotePaletteAction, argument: string, cli: Client, cfg: PaletteConfig): Promise<boolean> {
    if (!isTauriRuntime) return false;
    const requestId = newRequestId();
    const request = buildActionRequest(cli, action, argument, cfg);
    const streamPath = action.subcommand === "chat" ? "/v1/chat/stream" : "/v1/ask/stream";
    const outputKind = outputKindFor(action.subcommand);
    setRun({
      kind: "streaming",
      title: `Streaming ${action.label}`,
      subtitle: `${request.method} ${streamPath}`,
      text: "",
      outputKind,
      requestId,
      path: streamPath,
      actionLabel: action.label,
      prompt: argument,
    });
    try {
      await invoke("axon_http_stream_request", {
        request: { ...request, requestId, path: streamPath, body: request.body },
      });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setRun((current) =>
        current.kind === "streaming" && current.requestId === requestId
          ? makeStreamErrorRun({
              actionLabel: action.label,
              path: `${request.method} ${streamPath}`,
              message,
              outputKind,
              prompt: current.prompt,
            })
          : current,
      );
    }
    return true;
  }

  // ── Local help branch ────────────────────────────────────────────────────
  function submitHelp(rawArgument: string) {
    const localHelpAction = helpAction();
    const targetToken = rawArgument;
    const target = findHelpTarget(targetToken);
    const unknownTarget = targetToken.trim() && !target ? targetToken.trim() : undefined;
    const helpRun = buildHelpRun(target, unknownTarget);
    setRun(helpRun);
    setModeAction(localHelpAction);
    setQuery(rawArgument.trim());
    setBrowseOpen(false);
    pushHistory(localHelpAction, target?.subcommand ?? unknownTarget ?? "catalog", {
      status: helpRun.result.status,
      title: helpRun.title,
      subtitle: helpRun.subtitle,
      text: helpRun.text,
      outputKind: "markdown",
      result: helpRun.result,
    });
  }

  // Latest in-flight guard for the imperative (crawl/stream) paths. One-shots are
  // serialized by useActionState's `oneShotPending`; crawl/stream guard on `run`.
  const runRef = useRef(run);
  runRef.current = run;

  // Public entry — signature preserved for App.tsx: submit(action, override?).
  async function submit(action: PaletteAction, argumentOverride?: string) {
    if (!action) return;
    const current = runRef.current;
    // In-flight guard: one-shot serialization is the runtime's job (oneShotPending),
    // but a live crawl-job or active stream must still block re-entry.
    if (oneShotPending || current.kind === "streaming" || (current.kind === "job" && !current.jobId)) return;
    const rawArgument = argumentOverride ?? argumentFor(action, modeAction, parsed, query);

    if (action.subcommand === "help") {
      submitHelp(rawArgument);
      return;
    }
    if (action.kind === "local") return;

    // A-M5: missing client/config is no longer a silent no-op. Pressing Enter with
    // no server URL / token now surfaces a transient error RunState.
    if (!client || !config) {
      setRun(
        makeErrorRun({
          title: `${action.label} unavailable`,
          subtitle: "No Axon server configured",
          message: "Configure a server URL (and token, if required) in Settings, then try again.",
          path: "",
        }),
      );
      return;
    }

    const argument = normalizeSubmitArgument(action, rawArgument);
    // A-M5: failed validation surfaces inline instead of returning silently.
    const validation = validationMessage(action, argument);
    if (validation) {
      setRun(
        makeErrorRun({
          title: `${action.label} needs input`,
          subtitle: action.subcommand,
          message: validation,
          path: "",
        }),
      );
      return;
    }

    setModeAction(action);
    setQuery(argument);
    setBrowseOpen(false);

    if (action.subcommand === "crawl") {
      await submitCrawl(action, argument, client, config);
      return;
    }
    if (action.subcommand === "ask" || action.subcommand === "chat") {
      const streamed = await submitStream(action, argument, client, config);
      if (streamed) return;
      // Non-Tauri runtime: fall through to the one-shot dispatcher below.
    }
    dispatchOneShot({ action, argument, config, client });
  }

  return { submit };
}
