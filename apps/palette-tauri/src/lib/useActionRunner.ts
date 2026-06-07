import { useEffect, type Dispatch, type SetStateAction } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import type { PaletteAction } from "@/lib/actions";
import { crawlSeedUrl, newRequestId, normalizeSubmitArgument } from "@/lib/appHelpers";
import {
  buildActionRequest,
  executeAction,
  type Client,
  type PaletteConfig,
} from "@/lib/axonClient";
import { hostFromUrl, summarizeCrawl } from "@/lib/crawlJob";
import { formatPayload, outputKindFor } from "@/lib/format";
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
      const payload = event.payload;
      if (payload.type === "delta") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? { ...current, text: current.text + payload.text }
            : current,
        );
      } else if (payload.type === "done") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? {
                kind: "success",
                title: `${current.actionLabel} completed`,
                subtitle: current.subtitle,
                text: payload.answer ?? current.text,
                outputKind: current.outputKind,
                prompt: current.prompt,
                result: {
                  ok: true,
                  status: 200,
                  path: current.path,
                  method: "POST",
                  payload: { answer: payload.answer ?? current.text },
                },
              }
            : current,
        );
      } else if (payload.type === "error") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? {
                kind: "error",
                title: `${current.actionLabel} failed`,
                subtitle: current.path,
                text: payload.message,
                outputKind: current.outputKind,
                prompt: current.prompt,
                result: {
                  ok: false,
                  status: 0,
                  path: current.path,
                  method: "POST",
                  payload: { error: payload.message },
                },
              }
            : current,
        );
      }
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, []);

  async function submit(action: PaletteAction, argumentOverride?: string) {
    if (!client || !config || !action || run.kind === "running" || run.kind === "streaming") return;
    const argument = normalizeSubmitArgument(
      action,
      argumentOverride ?? argumentFor(action, modeAction, parsed, query),
    );
    const validation = validationMessage(action, argument);
    if (validation) return;
    setModeAction(action);
    setQuery(argument);
    setBrowseOpen(false);
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    if (action.subcommand === "crawl") {
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
        const result = await executeAction(client, action, argument, config);
        const payload = (result.payload ?? {}) as Record<string, unknown>;
        const jobId =
          typeof payload.job_id === "string"
            ? payload.job_id
            : typeof payload.id === "string"
              ? payload.id
              : null;
        if (!result.ok || !jobId) {
          const text = formatPayload(action.subcommand, result.payload);
          pushHistory(action, seedUrl, result.status, text, "code");
          setRun({
            kind: "error",
            title: "Crawl failed",
            subtitle: `${result.method} ${result.path} | HTTP ${result.status}`,
            text,
            outputKind: "code",
            result,
          });
          return;
        }
        pushHistory(action, seedUrl, result.status, undefined, "code");
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
        const text = err instanceof Error ? err.message : String(err);
        setRun({
          kind: "error",
          title: "Crawl failed",
          subtitle: commandLine,
          text,
          outputKind: "code",
          result: { ok: false, status: 0, path: "/v1/crawl", method: "POST", payload: null },
        });
      }
      return;
    }
    if (action.subcommand === "ask" || action.subcommand === "chat") {
      const requestId = newRequestId();
      const request = buildActionRequest(client, action, argument, config);
      const streamPath = action.subcommand === "chat" ? "/v1/chat/stream" : "/v1/ask/stream";
      if (isTauriRuntime) {
        setRun({
          kind: "streaming",
          title: `Streaming ${action.label}`,
          subtitle: `${request.method} ${streamPath}`,
          text: "",
          outputKind: outputKindFor(action.subcommand),
          requestId,
          path: streamPath,
          actionLabel: action.label,
          prompt: argument,
        });
        try {
          await invoke("axon_http_stream_request", {
            request: {
              ...request,
              requestId,
              path: streamPath,
              body: request.body,
            },
          });
          return;
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          setRun((current) =>
            current.kind === "streaming" && current.requestId === requestId
              ? {
                  kind: "error",
                  title: `${action.label} stream failed`,
                  subtitle: `${request.method} ${streamPath}`,
                  text: message,
                  outputKind: outputKindFor(action.subcommand),
                  prompt: current.prompt,
                  result: {
                    ok: false,
                    status: 0,
                    path: streamPath,
                    method: "POST",
                    payload: { error: message },
                  },
                }
              : current,
          );
          return;
        }
      } else {
        setRun({
          kind: "running",
          title: action.label,
          subtitle: `${request.method} ${request.path}`,
          prompt: argument,
        });
      }
    } else {
      setRun({
        kind: "running",
        title: `Running ${action.label}`,
        subtitle: commandLine,
      });
    }
    try {
      const result = await executeAction(client, action, argument, config);
      const text = formatPayload(action.subcommand, result.payload);
      pushHistory(action, argument || action.subcommand, result.status, text, outputKindFor(action.subcommand));
      const isConversation = action.subcommand === "ask" || action.subcommand === "chat";
      setRun({
        kind: result.ok ? "success" : "error",
        title: isConversation ? action.label : `${action.label} ${result.ok ? "completed" : "failed"}`,
        subtitle: action.subcommand === "ask"
          ? `RAG over ${config.collection || "axon"} | ${result.path}`
          : action.subcommand === "chat"
          ? result.path
          : `${result.method} ${result.path} | HTTP ${result.status}`,
        text,
        outputKind: outputKindFor(action.subcommand),
        prompt: isConversation ? argument : undefined,
        result,
      });
    } catch (err) {
      const text = err instanceof Error ? err.message : String(err);
      pushHistory(action, argument || action.subcommand, 0, text, outputKindFor(action.subcommand));
      const isConversation = action.subcommand === "ask" || action.subcommand === "chat";
      setRun({
        kind: "error",
        title: isConversation ? action.label : `${action.label} failed`,
        subtitle: action.subcommand === "ask"
          ? `RAG over ${config.collection || "axon"} | /v1/ask`
          : action.subcommand === "chat"
          ? "/v1/chat"
          : commandLine,
        text,
        outputKind: outputKindFor(action.subcommand),
        prompt: isConversation ? argument : undefined,
        result: { ok: false, status: 0, path: "", method: "POST", payload: null },
      });
    }
  }

  function pushHistory(action: PaletteAction, target: string, status: number, text?: string, outputKind?: "markdown" | "code") {
    setHistory((items) => [
      { action, target, status, text, outputKind, when: "just now", duration: status === 0 ? "fail" : undefined },
      ...items,
    ].slice(0, 18));
  }

  return { submit };
}
