import { ACTIONS, type PaletteAction } from "@/lib/actions";
import { answerParts, appendAskActivity, completeLastAssistantTurn, updateLastAssistantTurn } from "@/lib/askTranscript";
import type { HttpMethod, PaletteResult } from "@/lib/axonClient";
import { hostFromUrl } from "@/lib/crawlJob";
import type { OutputKind } from "@/lib/format";
import type { AskTurn, RunState } from "@/lib/runState";

export type PaletteStreamEvent =
  | { type: "started"; requestId: string; path: string }
  | { type: "delta"; requestId: string; text: string }
  | { type: "activity"; requestId: string; label: string; detail?: string | null; kind?: "thinking" | "tool" | "done" | string | null }
  | { type: "done"; requestId: string; answer?: string | null }
  | { type: "error"; requestId: string; message: string };

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

export function errorResult(message: string, path: string, method: HttpMethod = "POST"): PaletteResult {
  return { ok: false, status: 0, path, method, payload: { error: message } };
}

// A one-shot/local error RunState (missing client, thrown request, etc.).
export function makeErrorRun(args: {
  title: string;
  subtitle: string;
  message: string;
  path: string;
  outputKind?: OutputKind;
  prompt?: string;
  transcript?: AskTurn[];
}): TerminalRun {
  return {
    kind: "error",
    title: args.title,
    subtitle: args.subtitle,
    text: args.message,
    outputKind: args.outputKind ?? "code",
    prompt: args.prompt,
    transcript: args.transcript,
    result: errorResult(args.message, args.path),
  };
}

// Concise display label for an async-job target: the first token, shortened to
// a host when it is a URL (`https://docs.rs/x` → `docs.rs`), else verbatim
// (`unraid/api`, `r/rust`). Used by the live job card's "Ingesting <label>".
export function jobLabel(argument: string): string {
  const first = argument.trim().split(/\s+/)[0] ?? "";
  if (!first) return "";
  return /^https?:\/\//i.test(first) ? hostFromUrl(first) : first;
}

// Terminal error of a streamed action (the `palette://stream` "error" event or a
// failed stream invoke). Honest result — no fabricated 200.
export function makeStreamErrorRun(args: {
  actionLabel: string;
  path: string;
  message: string;
  outputKind: OutputKind;
  prompt?: string;
  transcript?: AskTurn[];
}): TerminalRun {
  return {
    kind: "error",
    title: `${args.actionLabel} failed`,
    subtitle: args.path,
    text: args.message,
    outputKind: args.outputKind,
    prompt: args.prompt,
    transcript: args.transcript,
    result: errorResult(args.message, args.path),
  };
}

// Terminal success of a streamed action. The answer never travelled through the
// one-shot HTTP path, so `status: 0` is the truthful marker (A-M4); the payload
// carries the streamed answer for downstream views.
export function makeStreamSuccessRun(args: {
  actionLabel: string;
  subtitle: string;
  text: string;
  outputKind: OutputKind;
  path: string;
  prompt?: string;
  transcript?: AskTurn[];
  payload?: unknown;
}): TerminalRun {
  const parts = answerParts(args.text, args.payload);
  return {
    kind: "success",
    title: `${args.actionLabel} completed`,
    subtitle: args.subtitle,
    text: parts.answer,
    outputKind: args.outputKind,
    prompt: args.prompt,
    transcript: completeLastAssistantTurn(args.transcript, parts.answer, parts.sources),
    result: {
      ok: true,
      status: 0,
      path: args.path,
      method: "POST",
      payload: { answer: parts.answer, sources: parts.sources },
    },
  };
}

export function statusFallbackAction(action: PaletteAction, argument: string): PaletteAction {
  if (action.kind !== "job" || argument.trim()) return action;
  const match = /^(crawl|embed|extract|ingest)-status$/.exec(action.subcommand);
  if (!match) return action;
  return ACTIONS.find((candidate) => candidate.subcommand === `${match[1]}-list`) ?? action;
}

// ── Streaming event reducer ──────────────────────────────────────────────────
// Folds a `palette://stream` event into the current RunState. Pure so it can be
// unit-tested without the Tauri event bridge.
export function reduceStreamEvent(current: RunState, payload: PaletteStreamEvent): RunState {
  if (current.kind !== "streaming" || !("requestId" in payload) || current.requestId !== payload.requestId) {
    return current;
  }
  if (payload.type === "delta") {
    const text = current.text + payload.text;
    return { ...current, text, transcript: updateLastAssistantTurn(current.transcript, text) };
  }
  if (payload.type === "activity") {
    const kind = payload.kind === "tool" || payload.kind === "done" ? payload.kind : "thinking";
    return {
      ...current,
      transcript: appendAskActivity(current.transcript, {
        id: `${payload.requestId}:activity:${current.transcript?.at(-1)?.activities?.length ?? 0}`,
        label: payload.label,
        detail: payload.detail ?? undefined,
        kind,
      }),
    };
  }
  if (payload.type === "done") {
    return makeStreamSuccessRun({
      actionLabel: current.actionLabel,
      subtitle: current.subtitle,
      text: payload.answer ?? current.text,
      outputKind: current.outputKind,
      path: current.path,
      prompt: current.prompt,
      transcript: current.transcript,
    });
  }
  if (payload.type === "error") {
    return makeStreamErrorRun({
      actionLabel: current.actionLabel,
      path: current.path,
      message: payload.message,
      outputKind: current.outputKind,
      prompt: current.prompt,
      transcript: current.transcript,
    });
  }
  return current;
}
