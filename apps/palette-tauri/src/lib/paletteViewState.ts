// A-M1/A-M2 — single-source-of-truth view model for the palette's top-level view.
//
// Before this module, App.tsx encoded the view as ~4 interdependent useState
// flags (`settingsOpen`, `browseOpen`, `historyOpen`, `modeAction`) plus ~9
// derived booleans, and the two stateful hooks (`useActionRunner`,
// `useCrawlJob`) reached back into App by drilling 5–6 raw setters each to
// enforce view transitions. That made illegal combinations representable (e.g.
// settings + history both open) and scattered the transition rules.
//
// The fix models the top-level view as a discriminated `View` union driven by a
// `useReducer`. `RunState` stays ORTHOGONAL (owned separately in App) — a result
// or live job renders under any launcher view. The reducer owns ALL view
// transition rules (e.g. "minimizing a job returns to the launcher and clears
// settings/history/browse/mode") in ONE place, and the two hooks receive a small
// set of intent callbacks instead of raw setters.

import type { PaletteAction } from "@/lib/actions";

// ── The view model ───────────────────────────────────────────────────────────
// `launcher` is the base view: the command bar plus, optionally, the browse
// action-list (`browse`) and/or an argument-entry mode (`mode`). `settings` and
// `history` are full-screen overlays — mutually exclusive with each other and
// with the launcher, which is exactly what makes "settings + history" or
// "browse while in settings" unrepresentable.
// `browser` is a third full-screen overlay, alongside `settings`/`history`:
// mutually exclusive with them and with the launcher. It carries the initial
// URL/query argument the user typed before invoking the `browser` action (or
// `null` for a bare invocation, which opens to the browser's home page).
export type View =
  | { kind: "launcher"; mode: PaletteAction | null; browse: boolean }
  | { kind: "settings" }
  | { kind: "history" }
  | { kind: "browser"; initialTarget: string | null };

export const INITIAL_VIEW: View = { kind: "launcher", mode: null, browse: false };

// ── Intents ──────────────────────────────────────────────────────────────────
// Every legal view transition is one of these. App, useActionRunner, and
// useCrawlJob dispatch intents; only the reducer knows how each intent rewrites
// the view. Orthogonal `query`/`run` resets stay in App's intent-callback
// wrappers beside the dispatch (they are not view state) — but the view rules
// themselves (which overlays/modes a transition clears) live only here.
export type ViewIntent =
  // Launcher transitions
  | { type: "openBrowse" } //                       arrow-down / settings-saved → list all actions
  | { type: "closeBrowse" } //                      Escape out of an empty browse list
  | { type: "enterMode"; action: PaletteAction } // pick an action → collect its argument
  | { type: "switchMode"; action: PaletteAction } // swap the active mode in place
  | { type: "clearMode" } //                        Escape out of argument mode (mode pill cleared)
  | { type: "goToBrowse" } //                       global "back to the action list" (the back button)
  | { type: "reset" } //                            clear mode + collapse to bare launcher (browse-side reset)
  | { type: "collapse" } //                         collapse to bare launcher
  // Overlays
  | { type: "openSettings" }
  | { type: "closeSettings" } //                    settings → browse list
  | { type: "toggleSettings" }
  | { type: "openHistory" }
  | { type: "closeHistoryToBrowse" } //             Escape out of history → browse list
  | { type: "toggleHistory" }
  | { type: "openHistoryItem"; action: PaletteAction } // run a stored item → launcher in that mode
  | { type: "openBrowser"; initialTarget: string | null } // invoke the Browser action
  | { type: "closeBrowser" } //                     close the browser window/overlay → browse list
  // Action-runner driven
  | { type: "enterModeForRun"; action: PaletteAction } // submit() locks in the running action's mode
  | { type: "showHelp"; action: PaletteAction } //       local-help run shows under the launcher
  // Crawl-job driven
  | { type: "minimizeJob" } //                      job tray: bare launcher, mode cleared
  | { type: "expandJob" } //                        expand the tray back into the job card
  | { type: "closeJob" }; //                        dismiss the job → bare launcher

// ── Reducer ──────────────────────────────────────────────────────────────────
// Pure transition function. `RunState` is intentionally NOT an input — the view
// is orthogonal to run state. The reducer never produces an illegal combination
// because each branch returns a fully-formed `View`.
export function viewReducer(view: View, intent: ViewIntent): View {
  switch (intent.type) {
    case "openBrowse":
    case "goToBrowse":
    case "closeSettings":
    case "closeHistoryToBrowse":
      return { kind: "launcher", mode: null, browse: true };
    case "closeBrowse":
      return { kind: "launcher", mode: view.kind === "launcher" ? view.mode : null, browse: false };
    case "enterMode":
    case "switchMode":
    case "openHistoryItem":
    case "enterModeForRun":
    case "showHelp":
      return { kind: "launcher", mode: intent.action, browse: false };
    case "clearMode":
      return {
        kind: "launcher",
        mode: null,
        browse: view.kind === "launcher" ? view.browse : false,
      };
    case "reset":
    case "collapse":
    case "closeJob":
    case "minimizeJob":
      return { kind: "launcher", mode: null, browse: false };
    case "openSettings":
      return { kind: "settings" };
    case "toggleSettings":
      return view.kind === "settings"
        ? { kind: "launcher", mode: null, browse: true }
        : { kind: "settings" };
    case "openHistory":
      return { kind: "history" };
    case "toggleHistory":
      return view.kind === "history"
        ? { kind: "launcher", mode: null, browse: false }
        : { kind: "history" };
    case "openBrowser":
      return { kind: "browser", initialTarget: intent.initialTarget };
    case "closeBrowser":
      return { kind: "launcher", mode: null, browse: true };
    case "expandJob":
      return { kind: "launcher", mode: view.kind === "launcher" ? view.mode : null, browse: false };
  }
}

// ── View-derived accessors ───────────────────────────────────────────────────
// The single place that maps the `View` union back to the legacy
// `settingsOpen`/`browseOpen`/`historyOpen`/`modeAction` shape consumers still
// want, so no boolean is recomputed inline.
export function modeOf(view: View): PaletteAction | null {
  return view.kind === "launcher" ? view.mode : null;
}

export function isSettingsOpen(view: View): boolean {
  return view.kind === "settings";
}

export function isHistoryOpen(view: View): boolean {
  return view.kind === "history";
}

export function isBrowseOpen(view: View): boolean {
  return view.kind === "launcher" && view.browse;
}

/** True when the Browser overlay (a real in-app web browser window) is open. */
export function isBrowserOpen(view: View): boolean {
  return view.kind === "browser";
}

/** Initial URL/query argument for the Browser overlay, or `null` when absent. */
export function browserInitialTarget(view: View): string | null {
  return view.kind === "browser" ? view.initialTarget : null;
}
