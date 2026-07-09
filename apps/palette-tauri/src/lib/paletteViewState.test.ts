import { describe, expect, it } from "vitest";

import { ACTIONS, type PaletteAction } from "@/lib/actions";
import {
  browserInitialTarget,
  INITIAL_VIEW,
  isBrowseOpen,
  isBrowserOpen,
  isHistoryOpen,
  isSettingsOpen,
  modeOf,
  viewReducer,
  type View,
  type ViewIntent,
} from "@/lib/paletteViewState";

function action(subcommand: string): PaletteAction {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

const scrape = action("scrape");
const ask = action("ask");

const launcher = (mode: PaletteAction | null = null, browse = false): View => ({
  kind: "launcher",
  mode,
  browse,
});

describe("viewReducer — single transitions", () => {
  it("starts at the bare launcher", () => {
    expect(INITIAL_VIEW).toEqual({ kind: "launcher", mode: null, browse: false });
  });

  it.each<[string, View, ViewIntent, View]>([
    // Browse open/close
    ["openBrowse", launcher(), { type: "openBrowse" }, launcher(null, true)],
    [
      "closeBrowse keeps mode",
      launcher(scrape, true),
      { type: "closeBrowse" },
      launcher(scrape, false),
    ],
    [
      "goToBrowse clears mode",
      launcher(scrape, false),
      { type: "goToBrowse" },
      launcher(null, true),
    ],
    // Mode enter/switch/clear
    [
      "enterMode",
      launcher(null, true),
      { type: "enterMode", action: scrape },
      launcher(scrape, false),
    ],
    ["switchMode", launcher(scrape), { type: "switchMode", action: ask }, launcher(ask, false)],
    ["clearMode keeps browse", launcher(scrape, true), { type: "clearMode" }, launcher(null, true)],
    ["reset", launcher(scrape, true), { type: "reset" }, launcher()],
    ["collapse", launcher(scrape, true), { type: "collapse" }, launcher()],
    // Settings overlay
    ["openSettings", launcher(scrape, true), { type: "openSettings" }, { kind: "settings" }],
    [
      "closeSettings → browse",
      { kind: "settings" },
      { type: "closeSettings" },
      launcher(null, true),
    ],
    // Browser overlay
    [
      "openBrowser with a target",
      launcher(scrape, true),
      { type: "openBrowser", initialTarget: "docs.rs/serde" },
      { kind: "browser", initialTarget: "docs.rs/serde" },
    ],
    [
      "openBrowser with no target",
      launcher(null, true),
      { type: "openBrowser", initialTarget: null },
      { kind: "browser", initialTarget: null },
    ],
    [
      "closeBrowser → browse",
      { kind: "browser", initialTarget: "example.com" },
      { type: "closeBrowser" },
      launcher(null, true),
    ],
    // History overlay
    ["openHistory", launcher(scrape, true), { type: "openHistory" }, { kind: "history" }],
    [
      "closeHistoryToBrowse",
      { kind: "history" },
      { type: "closeHistoryToBrowse" },
      launcher(null, true),
    ],
    [
      "openHistoryItem",
      { kind: "history" },
      { type: "openHistoryItem", action: ask },
      launcher(ask, false),
    ],
    // Action-runner driven
    [
      "enterModeForRun",
      launcher(null, true),
      { type: "enterModeForRun", action: ask },
      launcher(ask, false),
    ],
    [
      "showHelp",
      launcher(null, true),
      { type: "showHelp", action: scrape },
      launcher(scrape, false),
    ],
    // Crawl-job driven
    ["minimizeJob clears mode", launcher(scrape, true), { type: "minimizeJob" }, launcher()],
    [
      "expandJob keeps mode",
      launcher(scrape, false),
      { type: "expandJob" },
      launcher(scrape, false),
    ],
    ["closeJob clears all", launcher(scrape, true), { type: "closeJob" }, launcher()],
  ])("%s", (_name, from, intent, expected) => {
    expect(viewReducer(from, intent)).toEqual(expected);
  });
});

describe("viewReducer — toggles", () => {
  it("toggleSettings opens from launcher and closes to browse", () => {
    const opened = viewReducer(launcher(), { type: "toggleSettings" });
    expect(opened).toEqual({ kind: "settings" });
    expect(viewReducer(opened, { type: "toggleSettings" })).toEqual(launcher(null, true));
  });

  it("toggleHistory opens from launcher and closes to a bare launcher", () => {
    const opened = viewReducer(launcher(), { type: "toggleHistory" });
    expect(opened).toEqual({ kind: "history" });
    expect(viewReducer(opened, { type: "toggleHistory" })).toEqual(launcher());
  });
});

describe("viewReducer — illegal combinations are unrepresentable", () => {
  // The whole point of the union: there is no value where two overlays, or an
  // overlay plus a launcher mode/browse, are simultaneously active.
  it("settings has no mode or browse fields to coexist with", () => {
    const settings = viewReducer(launcher(scrape, true), { type: "openSettings" });
    expect(settings.kind).toBe("settings");
    expect(isHistoryOpen(settings)).toBe(false);
    expect(isBrowseOpen(settings)).toBe(false);
    expect(modeOf(settings)).toBeNull();
  });

  it("opening history from settings replaces it (never both)", () => {
    const settings: View = { kind: "settings" };
    const history = viewReducer(settings, { type: "openHistory" });
    expect(history.kind).toBe("history");
    expect(isSettingsOpen(history)).toBe(false);
  });

  it("entering a mode never leaves an overlay open", () => {
    const history: View = { kind: "history" };
    const moded = viewReducer(history, { type: "enterMode", action: scrape });
    expect(isSettingsOpen(moded)).toBe(false);
    expect(isHistoryOpen(moded)).toBe(false);
    expect(modeOf(moded)).toBe(scrape);
  });
});

describe("viewReducer — Escape ladder mapping", () => {
  // The Escape back-stack in usePaletteHotkeys maps each rung to one intent.
  // These assertions pin the rung → intent → view transitions so the explicit
  // ladder stays behavior-equivalent to the old hand-coded 6-branch handler.
  it("settings → closeSettings → browse launcher", () => {
    expect(viewReducer({ kind: "settings" }, { type: "closeSettings" })).toEqual(
      launcher(null, true),
    );
  });

  it("history → closeHistoryToBrowse → browse launcher", () => {
    expect(viewReducer({ kind: "history" }, { type: "closeHistoryToBrowse" })).toEqual(
      launcher(null, true),
    );
  });

  it("empty browse → closeBrowse → bare launcher", () => {
    expect(viewReducer(launcher(null, true), { type: "closeBrowse" })).toEqual(
      launcher(null, false),
    );
  });

  it("mode with no query → clearMode → mode cleared", () => {
    expect(viewReducer(launcher(scrape, false), { type: "clearMode" })).toEqual(
      launcher(null, false),
    );
  });
});

describe("view accessors", () => {
  it("derive the legacy boolean/modeAction shape from the discriminant", () => {
    expect(modeOf(launcher(scrape))).toBe(scrape);
    expect(modeOf({ kind: "settings" })).toBeNull();
    expect(modeOf({ kind: "history" })).toBeNull();
    expect(isBrowseOpen(launcher(null, true))).toBe(true);
    expect(isBrowseOpen(launcher(null, false))).toBe(false);
    expect(isSettingsOpen({ kind: "settings" })).toBe(true);
    expect(isHistoryOpen({ kind: "history" })).toBe(true);
  });

  it("isBrowserOpen/browserInitialTarget reflect the browser overlay only", () => {
    const browser: View = { kind: "browser", initialTarget: "example.com" };
    expect(isBrowserOpen(browser)).toBe(true);
    expect(browserInitialTarget(browser)).toBe("example.com");
    expect(isBrowserOpen(launcher())).toBe(false);
    expect(browserInitialTarget(launcher())).toBeNull();
    expect(isBrowserOpen({ kind: "settings" })).toBe(false);
  });
});

describe("viewReducer — browser overlay illegal combinations", () => {
  it("opening the browser never leaves another overlay/mode open", () => {
    const history: View = { kind: "history" };
    const browser = viewReducer(history, { type: "openBrowser", initialTarget: null });
    expect(browser.kind).toBe("browser");
    expect(isHistoryOpen(browser)).toBe(false);
    expect(isSettingsOpen(browser)).toBe(false);
    expect(modeOf(browser)).toBeNull();
  });
});
