import type { Dispatch, KeyboardEvent, SetStateAction } from "react";

import type { PaletteAction } from "@/lib/actions";
import { argumentFor, parseCommand, type ParsedCommand } from "@/lib/paletteView";
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
      // Keyboard events can arrive before React commits the latest onChange.
      // Parse the input element's current value so the first Enter submits what
      // the user can actually see instead of a one-render-old query snapshot.
      const liveQuery = event.currentTarget.value;
      const liveParsed = parseCommand(liveQuery);
      // Terminal is a privileged local launcher. Merely filtering to it (for
      // example by typing "t") must not launch a shell on Enter; Tab is the
      // explicit selection gesture and owns execution for this no-arg action.
      if (
        input.active.subcommand === "terminal" &&
        !input.modeAction &&
        liveParsed.invoked?.subcommand !== "terminal"
      )
        return;
      const liveArgument = argumentFor(input.active, input.modeAction, liveParsed, liveQuery);
      input.requestSubmit(input.active, liveArgument);
    } else if (event.key === "Tab") {
      event.preventDefault();
      if (!input.active) return;
      if (input.active.argMode === "none") input.requestSubmit(input.active);
      else input.enterActionMode(input.active);
    }
  };
}
