import type { HistoryItem } from "@/components/palette/HistoryPanel";
import type { RemotePaletteAction } from "@/lib/actions";
import { answerParts, appendAskPendingTurn, completeLastAssistantTurn } from "@/lib/askTranscript";
import { executeAction, type Client, type PaletteConfig, type PaletteResult } from "@/lib/axonClient";
import { formatPayload, outputKindFor, type OutputKind } from "@/lib/format";
import type { AskTurn, RunState } from "@/lib/runState";
import { makeErrorRun } from "@/lib/useActionRunner/runFactories";

export interface OneShotInput {
  action: RemotePaletteAction;
  argument: string;
  config: PaletteConfig;
  client: Client;
  transcript?: AskTurn[];
}

export type PushHistory = (
  action: RemotePaletteAction,
  target: string,
  entry: {
    status: number;
    title: string;
    subtitle: string;
    text?: string;
    outputKind?: OutputKind;
    result?: PaletteResult;
    prompt?: string;
    transcript?: HistoryItem["transcript"];
  },
) => void;

export async function runOneShotAction({
  input,
  setRunning,
  setTerminal,
  pushHistory,
}: {
  input: OneShotInput;
  setRunning: (run: RunState) => void;
  setTerminal: (run: RunState) => void;
  pushHistory: PushHistory;
}): Promise<RunState> {
  const { action, argument, config, client, transcript } = input;
  const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
  const isConversation = action.subcommand === "ask" || action.subcommand === "chat";
  const pendingTranscript = isConversation
    ? appendAskPendingTurn(transcript, argument, `oneshot:${Date.now()}`)
    : undefined;
  setRunning({
    kind: "running",
    title: isConversation ? action.label : `Running ${action.label}`,
    subtitle: isConversation ? `POST ${action.subcommand === "ask" ? "/v1/ask" : "/v1/chat"}` : commandLine,
    prompt: isConversation ? argument : undefined,
    transcript: pendingTranscript,
  });
  try {
    const result = await executeAction(client, action, argument, config);
    const rawText = formatPayload(action.subcommand, result.payload);
    const parts = isConversation ? answerParts(rawText, result.payload) : { answer: rawText, sources: [] };
    const text = parts.answer;
    const title = isConversation ? action.label : `${action.label} ${result.ok ? "completed" : "failed"}`;
    const subtitle =
      action.subcommand === "ask"
        ? `RAG over ${config.collection || "axon"} | ${result.path}`
        : action.subcommand === "chat"
          ? result.path
          : `${result.method} ${result.path} | HTTP ${result.status}`;
    const completedTranscript = isConversation
      ? completeLastAssistantTurn(pendingTranscript, text, parts.sources)
      : undefined;
    pushHistory(action, argument || action.subcommand, {
      status: result.status,
      title,
      subtitle,
      text,
      outputKind: outputKindFor(action.subcommand),
      result,
      prompt: isConversation ? argument : undefined,
      transcript: completedTranscript,
    });
    const next: RunState = {
      kind: result.ok ? "success" : "error",
      title,
      subtitle,
      text,
      outputKind: outputKindFor(action.subcommand),
      prompt: isConversation ? argument : undefined,
      transcript: completedTranscript,
      result,
    };
    setTerminal(next);
    return next;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const title = isConversation ? action.label : `${action.label} failed`;
    const subtitle =
      action.subcommand === "ask"
        ? `RAG over ${config.collection || "axon"} | /v1/ask`
        : action.subcommand === "chat"
          ? "/v1/chat"
          : commandLine;
    const completedTranscript = isConversation
      ? completeLastAssistantTurn(pendingTranscript, message)
      : undefined;
    const next = makeErrorRun({
      title,
      subtitle,
      message,
      path: "",
      outputKind: outputKindFor(action.subcommand),
      prompt: isConversation ? argument : undefined,
      transcript: completedTranscript,
    });
    pushHistory(action, argument || action.subcommand, {
      status: 0,
      title,
      subtitle,
      text: message,
      outputKind: outputKindFor(action.subcommand),
      result: next.result,
      prompt: isConversation ? argument : undefined,
      transcript: completedTranscript,
    });
    setTerminal(next);
    return next;
  }
}
