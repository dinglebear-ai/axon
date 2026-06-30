import { useCallback } from "react";
import type { Dispatch, SetStateAction } from "react";

import { ACTIONS, type PaletteAction } from "@/lib/actions";
import { actionNeedsConfirmation } from "@/lib/actionGuard";
import { buildHelpRun } from "@/lib/actionHelp";
import { executeAction, type Client, type PaletteConfig } from "@/lib/axonClient";
import { appendAskPendingTurn, completeAssistantTurnById } from "@/lib/askTranscript";
import { chatToolMessage } from "@/lib/chatToolActions";
import type { RunState } from "@/lib/runState";

export function useChatToolRunner({
  active,
  client,
  config,
  run,
  setRun,
  onFallbackRunAction,
}: {
  active?: PaletteAction;
  client: Client | null;
  config: PaletteConfig | null;
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  onFallbackRunAction: (subcommand: string, argument: string) => void;
}) {
  return useCallback(
    (subcommand: string, argument: string) => {
      if (active?.subcommand !== "chat") {
        onFallbackRunAction(subcommand, argument);
        return;
      }
      const action = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
      if (!action) return;
      if (actionNeedsConfirmation(action)) {
        appendAssistantMessage(
          setRun,
          `${action.label} changes stored state. Run this action from the main command bar to review and confirm it first.`,
        );
        return;
      }
      if ("transcript" in run && run.transcript?.some((turn) => turn.pending)) return;
      const requestId = `chat-tool:${Date.now()}`;
      const assistantId = `${requestId}:assistant`;
      const commandLine = `/${action.subcommand}${argument ? ` ${argument}` : ""}`;
      const previousTranscript = "transcript" in run ? run.transcript : undefined;
      setRun((current) =>
        "transcript" in current
          ? { ...current, transcript: appendAskPendingTurn(previousTranscript, commandLine, requestId) }
          : current,
      );
      void runChatTool({ action, argument, assistantId, client, config, setRun });
    },
    [active, client, config, onFallbackRunAction, run, setRun],
  );
}

async function runChatTool({
  action,
  argument,
  assistantId,
  client,
  config,
  setRun,
}: {
  action: PaletteAction;
  argument: string;
  assistantId: string;
  client: Client | null;
  config: PaletteConfig | null;
  setRun: Dispatch<SetStateAction<RunState>>;
}) {
  try {
    if (action.subcommand === "help") {
      completePending(setRun, assistantId, buildHelpRun(undefined, argument.trim() || undefined).text);
      return;
    }
    if (action.kind === "local") {
      completePending(setRun, assistantId, "This local action is only available from the main command bar.");
      return;
    }
    if (!client || !config) {
      completePending(
        setRun,
        assistantId,
        "Axon is not connected. Configure a server URL and token in Settings, then try again.",
      );
      return;
    }
    const result = await executeAction(client, action, argument, config);
    completePending(setRun, assistantId, chatToolMessage(action, argument, result));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    completePending(setRun, assistantId, `Action failed: ${message}`);
  }
}

function appendAssistantMessage(setRun: Dispatch<SetStateAction<RunState>>, content: string) {
  setRun((current) =>
    "transcript" in current
      ? {
          ...current,
          transcript: [
            ...(current.transcript ?? []),
            { id: `chat-tool-guard:${Date.now()}`, role: "assistant", content },
          ],
        }
      : current,
  );
}

function completePending(
  setRun: Dispatch<SetStateAction<RunState>>,
  assistantId: string,
  content: string,
) {
  setRun((current) =>
    "transcript" in current
      ? { ...current, transcript: completeAssistantTurnById(current.transcript, assistantId, content) }
      : current,
  );
}
