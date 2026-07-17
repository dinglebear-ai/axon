import { useCallback } from "react";

import { ACTIONS, type RemotePaletteAction } from "@/lib/actions";
import { type Client, executeAction, type PaletteConfig } from "@/lib/axonClient";
import { normalizeChatSuggestions } from "@/lib/paletteHistoryStorage";
import { strField, unwrapPayload } from "@/lib/payload";
import type { ChatSuggestion } from "@/lib/runState";

export function useSuggestMessage(client: Client | null, config: PaletteConfig | null) {
  return useCallback(
    async (message: string): Promise<ChatSuggestion[]> => {
      if (!client || !config) throw new Error("Axon is not connected.");
      const queryAction = ACTIONS.find(
        (action): action is RemotePaletteAction =>
          action.subcommand === "query" && action.kind !== "local",
      );
      if (!queryAction) throw new Error("Query action is unavailable.");
      const result = await executeAction(client, queryAction, message, config);
      if (!result.ok) {
        const payload = unwrapPayload(result.payload);
        throw new Error(
          strField(payload, "message") ??
            strField(payload, "error") ??
            strField(payload, "detail") ??
            `Query failed with HTTP ${result.status}`,
        );
      }
      return normalizeChatSuggestions(result.payload);
    },
    [client, config],
  );
}
