import type { Dispatch, KeyboardEvent, SetStateAction } from "react";

import type { PaletteAction } from "@/lib/actions";
import { looksLikeUrl, type ParsedCommand } from "@/lib/paletteView";
import type { ViewIntent } from "@/lib/paletteViewState";

interface PaletteInputKeyDownInput {
  active: PaletteAction | undefined | null;
  askFallback: boolean;
  askSessionsLength: number;
  dispatchView: Dispatch<ViewIntent>;
  enterActionMode: (action: PaletteAction) => void;
  filteredLength: number;
  modeAction: PaletteAction | null;
  parsed: ParsedCommand;
  requestSubmit: (action: PaletteAction, argumentOverride?: string) => void;
  setAskSessionsOpen: Dispatch<SetStateAction<boolean>>;
  setSelected: Dispatch<SetStateAction<number>>;
}

export function usePaletteInputKeyDown(input: PaletteInputKeyDownInput) {
  return (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (input.modeAction?.subcommand === "ask" && input.askSessionsLength > 0) {
        input.setAskSessionsOpen(true);
        return;
      }
      if (!input.modeAction) input.dispatchView({ type: "openBrowse" });
      input.setSelected((index) => Math.min(index + 1, Math.max(input.filteredLength - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      input.setSelected((index) => Math.max(index - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (!input.active) return;
      if (
        !input.modeAction &&
        !input.parsed.invoked &&
        input.active.argMode !== "none" &&
        !looksLikeUrl(input.parsed.search)
      ) {
        input.enterActionMode(input.active);
      } else {
        input.requestSubmit(input.active, input.askFallback ? input.parsed.search : undefined);
      }
    } else if (event.key === "Tab") {
      event.preventDefault();
      if (!input.active) return;
      if (input.active.argMode === "none") input.requestSubmit(input.active);
      else input.enterActionMode(input.active);
    }
  };
}
