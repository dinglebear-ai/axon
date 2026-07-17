import {
  type Dispatch,
  type SetStateAction,
  startTransition,
  useActionState,
  useEffect,
  useRef,
} from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { buildHelpRun, findHelpTarget, helpAction } from "@/lib/actionHelp";
import type { PaletteAction, RemotePaletteAction } from "@/lib/actions";
import { newRequestId, normalizeSubmitArgument } from "@/lib/appHelpers";
import { appendAskPendingTurn } from "@/lib/askTranscript";
import {
  buildActionRequest,
  type Client,
  executeAction,
  type PaletteConfig,
  type PaletteResult,
} from "@/lib/axonClient";
import { formatPayload, type OutputKind, outputKindFor } from "@/lib/format";
import { appWindow, invoke, isTauriRuntime } from "@/lib/invoke";
import {
  type AsyncJobFamily,
  asyncJobFamilyForSubcommand,
  pendingJobSnapshot,
  summarizeJob,
} from "@/lib/jobProgress";
import { argumentFor, type ParsedCommand, validationMessage } from "@/lib/paletteView";
import type { AskTurn, RunState } from "@/lib/runState";
import {
  jobLabel,
  makeErrorRun,
  makeStreamErrorRun,
  type PaletteStreamEvent,
  reduceStreamEvent,
  statusFallbackAction,
} from "@/lib/useActionRunner/runFactories";
import { type OneShotInput, runOneShotAction } from "@/lib/useActionRunner/runOneShotAction";

// Re-exported for import compatibility: other modules (and tests) import
// `reduceStreamEvent` from "@/lib/useActionRunner".
export { reduceStreamEvent };

export const HISTORY_LIMIT = 18;

export function capHistory(items: HistoryItem[]): HistoryItem[] {
  return items.slice(0, HISTORY_LIMIT);
}

interface UseActionRunnerArgs {
  client: Client | null;
  config: PaletteConfig | null;
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>;
  // A-M2 — intent callbacks replace the raw setModeAction/setQuery/setBrowseOpen
  // setter drilling. Each one carries the view transition AND the query reset it
  // implies; the view-transition rules (which overlays/browse/mode it clears)
  // live in App's reducer, not here.
  // `enterModeForRun(action, argument)`: lock in the running action's mode and
  // seed the command bar with its normalized argument (clears browse).
  enterModeForRun: (action: PaletteAction, argument: string) => void;
  // `showHelpRun(action, target)`: the local-help action becomes the active mode
  // and the command bar shows the help target (clears browse).
  showHelpRun: (action: PaletteAction, target: string) => void;
  modeAction: PaletteAction | null;
  parsed: ParsedCommand;
  query: string;
}

// Action-execution engine for the palette: turns a selected action + argument
// into a backend call, routing source jobs, streamed answers, and
// everything else → one-shot request, while recording each run into history.
// `run`/`history` state stays owned by App; this hook holds the logic only.
export function useActionRunner({
  client,
  config,
  run,
  setRun,
  setHistory,
  enterModeForRun,
  showHelpRun,
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
      prompt?: string;
      transcript?: AskTurn[];
    },
  ) {
    setHistory((items) =>
      capHistory([
        {
          action,
          target,
          ...entry,
          when: "just now",
          duration: entry.status === 0 ? "fail" : undefined,
        },
        ...items,
      ]),
    );
  }

  const [, dispatchOneShot, oneShotPending] = useActionState<RunState, OneShotInput>(
    async (_prev, input) =>
      runOneShotAction({
        input,
        setRunning: setRun,
        setTerminal: setRun,
        pushHistory,
      }),
    { kind: "idle" },
  );

  // Submit source-backed/extract jobs, then hand the live poll to useJobPoll.
  async function submitAsyncJob(
    action: RemotePaletteAction,
    argument: string,
    family: AsyncJobFamily,
    cli: Client,
    cfg: PaletteConfig,
  ) {
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    const label = jobLabel(argument);
    const startedAtMs = Date.now();
    setRun({
      kind: "asyncJob",
      family,
      title: `${action.label}`,
      subtitle: "submitting…",
      jobId: "",
      statusUrl: "",
      target: argument,
      startedAtMs,
      snapshot: pendingJobSnapshot(family, label),
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
        pushHistory(action, argument || action.subcommand, {
          status: result.status,
          title: `${action.label} failed`,
          subtitle,
          text,
          outputKind: "code",
          result,
        });
        setRun({
          kind: "error",
          title: `${action.label} failed`,
          subtitle,
          text,
          outputKind: "code",
          result,
        });
        return;
      }
      pushHistory(action, argument || action.subcommand, {
        status: result.status,
        title: `${action.label}`,
        subtitle: `job ${jobId}`,
        outputKind: "code",
        result,
      });
      setRun((current) =>
        current.kind === "asyncJob" && current.target === argument
          ? {
              ...current,
              jobId,
              statusUrl: `/v1/jobs/${jobId}`,
              subtitle: `job ${jobId}`,
              snapshot: summarizeJob(family, result.payload, { jobId, label }),
            }
          : current,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setRun(
        makeErrorRun({
          title: `${action.label} failed`,
          subtitle: commandLine,
          message,
          path: `/v1/${family}`,
        }),
      );
    }
  }

  // ── Stream branch: start the streamed request; terminal states arrive via the
  // `palette://stream` listener and reduceStreamEvent. Returns false when the
  // current (non-Tauri) runtime can't stream so the caller falls back to one-shot.
  async function submitStream(
    action: RemotePaletteAction,
    argument: string,
    cli: Client,
    cfg: PaletteConfig,
    transcript?: AskTurn[],
  ): Promise<boolean> {
    if (!isTauriRuntime) return false;
    const requestId = newRequestId();
    const request = buildActionRequest(cli, action, argument, cfg);
    const streamPath = action.subcommand === "chat" ? "/v1/chat/stream" : "/v1/ask/stream";
    const outputKind = outputKindFor(action.subcommand);
    const pendingTranscript = appendAskPendingTurn(transcript, argument, requestId);
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
      transcript: pendingTranscript,
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
              transcript: current.transcript,
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
    showHelpRun(localHelpAction, rawArgument.trim());
    pushHistory(localHelpAction, target?.subcommand ?? unknownTarget ?? "catalog", {
      status: helpRun.result.status,
      title: helpRun.title,
      subtitle: helpRun.subtitle,
      text: helpRun.text,
      outputKind: "markdown",
      result: helpRun.result,
    });
  }

  // ── Local files branch ───────────────────────────────────────────────────
  // "Files" has no request/response shape — it's a stateful local browser
  // rendered entirely by FilesView (own useState for cwd/selection/edits,
  // calling `invoke()` directly for fs ops). This just parks a `success`
  // RunState so OutputPanel renders the structured view immediately, the same
  // way `submitHelp` does for the other local action.
  function submitFiles(action: PaletteAction) {
    setRun({
      kind: "success",
      title: "Files",
      subtitle: "local filesystem",
      text: "",
      outputKind: "code",
      result: { ok: true, status: 200, path: "palette://files", method: "GET", payload: {} },
    });
    enterModeForRun(action, "");
  }

  // ── Local terminal branch ────────────────────────────────────────────────
  // `terminal` is a local, non-HTTP action (see `src-tauri/src/terminal.rs`):
  // it never calls `axonClient`. This just needs `run` to leave `idle` so
  // `App`'s `showOutput` flips on; `OutputPanel` special-cases
  // `active?.subcommand === "terminal"` and renders `TerminalView` directly
  // instead of reading `run.result`/`run.text`. No history entry is pushed —
  // an interactive shell session isn't a single request/response result like
  // the other actions.
  function submitTerminal(terminalAction: PaletteAction) {
    enterModeForRun(terminalAction, "");
    setRun({
      kind: "success",
      title: "Terminal",
      subtitle: "local shell session",
      text: "",
      outputKind: "code",
      result: { ok: true, status: 200, path: "palette://terminal", method: "GET", payload: null },
    });
  }

  // Latest in-flight guard for imperative streaming/job paths.
  const runRef = useRef(run);
  runRef.current = run;

  // Public entry — signature preserved for App.tsx: submit(action, override?).
  async function submit(action: PaletteAction, argumentOverride?: string) {
    if (!action) return;
    const current = runRef.current;
    // In-flight guard: one-shot serialization is the runtime's job (oneShotPending),
    // but a live source job or active stream must still block re-entry.
    if (
      oneShotPending ||
      current.kind === "streaming" ||
      (current.kind === "asyncJob" && !current.jobId)
    )
      return;
    const rawArgument = argumentOverride ?? argumentFor(action, modeAction, parsed, query);

    if (action.subcommand === "help") {
      submitHelp(rawArgument);
      return;
    }
    if (action.subcommand === "files") {
      submitFiles(action);
      return;
    }
    if (action.subcommand === "terminal") {
      submitTerminal(action);
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

    const normalizedArgument = normalizeSubmitArgument(action, rawArgument);
    const executableAction = statusFallbackAction(action, normalizedArgument);
    const argument = executableAction === action ? normalizedArgument : "";
    const previousTranscript =
      (executableAction.subcommand === "ask" || executableAction.subcommand === "chat") &&
      "transcript" in current
        ? current.transcript
        : undefined;
    // A-M5: failed validation surfaces inline instead of returning silently.
    const validation = validationMessage(executableAction, argument);
    if (validation) {
      setRun(
        makeErrorRun({
          title: `${executableAction.label} needs input`,
          subtitle: executableAction.subcommand,
          message: validation,
          path: "",
        }),
      );
      return;
    }
    if (executableAction.kind === "local") return;

    enterModeForRun(executableAction, argument);

    if (
      executableAction.subcommand === "source-site" ||
      executableAction.subcommand === "source" ||
      executableAction.subcommand === "extract"
    ) {
      await submitAsyncJob(
        executableAction,
        argument,
        asyncJobFamilyForSubcommand(executableAction.subcommand),
        client,
        config,
      );
      return;
    }
    if (executableAction.subcommand === "ask" || executableAction.subcommand === "chat") {
      const streamed = await submitStream(
        executableAction,
        argument,
        client,
        config,
        previousTranscript,
      );
      if (streamed) return;
      // Non-Tauri runtime: fall through to the one-shot dispatcher below.
    }
    startTransition(() => {
      dispatchOneShot({
        action: executableAction,
        argument,
        config,
        client,
        transcript: previousTranscript,
      });
    });
  }

  return { submit };
}
