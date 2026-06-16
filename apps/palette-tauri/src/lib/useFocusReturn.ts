import { useEffect, useRef, type RefObject } from "react";

import type { PaletteAction } from "@/lib/actions";
import { focusInput } from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";
import { invoke } from "@/lib/invoke";

// R-M1/H3/P-H2 — global palette keydown handler bound ONCE. Volatile state is
// read through `stateRef` (updated each render by the caller) so the listener
// never re-binds on keystrokes or streamed tokens. Actions are stable React
// setters / helpers, captured once.
export interface PaletteHotkeyState {
  settingsOpen: boolean;
  historyOpen: boolean;
  browseOpen: boolean;
  query: string;
  modeAction: PaletteAction | null;
  run: RunState;
}

export interface PaletteHotkeyActions {
  closeSettings: () => void;
  toBrowseFromHistory: () => void;
  closeBrowse: () => void;
  clearMode: () => void;
  clearQuery: () => void;
  copyOutput: (text: string) => void;
}

export function usePaletteHotkeys(
  stateRef: RefObject<PaletteHotkeyState>,
  actions: PaletteHotkeyActions,
) {
  const actionsRef = useRef(actions);
  actionsRef.current = actions;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      const state = stateRef.current;
      const act = actionsRef.current;
      if (event.key === "Escape") {
        event.preventDefault();
        if (state.settingsOpen) act.closeSettings();
        else if (state.historyOpen) act.toBrowseFromHistory();
        else if (state.browseOpen && !state.query && !state.modeAction && state.run.kind === "idle") act.closeBrowse();
        else if (state.modeAction && !state.query) act.clearMode();
        else if (state.query) act.clearQuery();
        else void invoke("hide_palette");
      } else if (modifier && event.key.toLowerCase() === "l") {
        event.preventDefault();
        focusInput(true);
      } else if (modifier && event.key.toLowerCase() === "k") {
        event.preventDefault();
        void invoke("show_palette").then(() => focusInput(true));
      } else if (modifier && event.key.toLowerCase() === "c" && "text" in state.run) {
        const target = event.target as HTMLElement | null;
        if (target?.tagName !== "INPUT" && target?.tagName !== "TEXTAREA") {
          event.preventDefault();
          act.copyOutput(state.run.text);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
    // Bind once: `stateRef`/`actionsRef` are stable ref containers and the
    // volatile values are read through `.current` inside the handler. Depending
    // on `stateRef.current` (a fresh object each render) would re-bind the
    // listener on every keystroke/stream tick — the exact churn P-H2 removed.
  }, [stateRef, actionsRef]);
}

// A11Y-H2 — focus management for transient overlays (Settings / History / result
// panels). When `open` flips true we move focus into the overlay container; when
// it flips back to false we restore focus to whatever was focused before the
// overlay opened (typically the command bar input or the control that opened it).
//
// Returns a ref to attach to the overlay's focusable container. The container
// should be focusable (e.g. `tabIndex={-1}` on a wrapper) so `ref.focus()` lands
// somewhere meaningful; if it already contains a natural focus target the caller
// can instead focus that and pass its ref here for the restore-on-close behaviour.
export function useFocusReturn<T extends HTMLElement = HTMLElement>(open: boolean) {
  const containerRef = useRef<T | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (open) {
      // Capture the element that had focus before the overlay mounted.
      previousFocusRef.current = document.activeElement as HTMLElement | null;
      // Move focus into the overlay on the next frame so the node is mounted.
      const container = containerRef.current;
      if (container) {
        const target =
          container.matches("[tabindex], a[href], button, input, select, textarea")
            ? container
            : container.querySelector<HTMLElement>(
                "[tabindex]:not([tabindex='-1']), a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled])",
              ) ?? container;
        target.focus();
      }
      return;
    }
    // Restore focus to the previously-focused element on close.
    const previous = previousFocusRef.current;
    previousFocusRef.current = null;
    if (previous && typeof previous.focus === "function" && previous.isConnected) {
      previous.focus();
    }
  }, [open]);

  return containerRef;
}
