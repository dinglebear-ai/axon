import type { Dispatch, SetStateAction } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { buildHelpRun, helpAction } from "@/lib/actionHelp";
import type { PaletteAction } from "@/lib/actions";
import type { ViewIntent } from "@/lib/paletteViewState";
import type { RunState } from "@/lib/runState";
import { capHistory } from "@/lib/useActionRunner";

interface PaletteHelpInput {
  dispatchView: Dispatch<ViewIntent>;
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>;
  setQuery: Dispatch<SetStateAction<string>>;
  setRun: Dispatch<SetStateAction<RunState>>;
}

export function usePaletteHelp(input: PaletteHelpInput) {
  return (action?: PaletteAction, unknownTarget?: string) => {
    const cleanUnknownTarget = !action && unknownTarget?.trim() ? unknownTarget.trim() : undefined;
    const helpRun = buildHelpRun(action, cleanUnknownTarget);
    const localHelpAction = helpAction();
    const historyItem: HistoryItem = {
      action: localHelpAction,
      target: action?.subcommand ?? cleanUnknownTarget ?? "catalog",
      status: helpRun.result.status,
      title: helpRun.title,
      subtitle: helpRun.subtitle,
      text: helpRun.text,
      outputKind: "markdown",
      result: helpRun.result,
      when: "just now",
    };
    input.dispatchView({ type: "showHelp", action: localHelpAction });
    input.setQuery(action?.subcommand ?? cleanUnknownTarget ?? "");
    input.setRun(helpRun);
    input.setHistory((items) => capHistory([historyItem, ...items]));
  };
}
