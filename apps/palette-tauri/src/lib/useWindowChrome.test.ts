// @vitest-environment node
// @ts-expect-error Vitest runs this file in Node; the app tsconfig intentionally omits Node globals.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { resolvePaletteWindowSize } from "./useWindowChrome";

describe("resolvePaletteWindowSize", () => {
  it("uses a compact result window for expanded crawl jobs", () => {
    expect(
      resolvePaletteWindowSize(
        {
          actionSwitcherOpen: false,
          jobExpanded: true,
          jobMinimized: false,
          settingsOpen: false,
          historyOpen: false,
          showResultsLayout: true,
          showContent: true,
          filteredLength: 0,
        },
        { width: 2560, height: 1440 },
        () => 468,
      ),
    ).toEqual({ width: 1280, height: 470 });
  });

  it("grows the compact launcher while the action switcher is open", () => {
    expect(
      resolvePaletteWindowSize(
        {
          actionSwitcherOpen: true,
          jobExpanded: false,
          jobMinimized: false,
          settingsOpen: false,
          historyOpen: false,
          showResultsLayout: false,
          showContent: false,
          filteredLength: 0,
        },
        { width: 2560, height: 1440 },
        () => 468,
      ),
    ).toEqual({ width: 720, height: 480 });
  });

});

describe("crawl job layout CSS contract", () => {
  const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

  function rule(selector: string): string {
    const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`${escaped}\\s*\\{(?<body>[^}]*)\\}`).exec(css)?.groups?.body ?? "";
  }

  it("keeps the expanded crawl job shell capped to the compact window height", () => {
    const body = rule(".palette-shell-job");
    expect(body).toContain("height: min(62vh, 470px)");
    expect(body).toContain("max-height: 470px");
  });

  it("keeps crawl stat chips vertically centered with stable compact sizing", () => {
    const body = rule(".running-stat");
    expect(body).toContain("align-items: center");
    expect(body).toContain("min-height: 25px");
    expect(body).toContain("padding: 0 8px");
    expect(body).toContain("line-height: 1.1");
    expect(rule(".running-stat strong")).toContain("line-height: 1");
  });

  it("keeps the minimized running-operation tray aligned to the compact shell", () => {
    const body = rule(".idle-tray");
    expect(body).toContain("display: grid");
    expect(body).toContain("grid-template-columns: auto auto minmax(0, max-content) minmax(160px, 1fr) auto auto");
    expect(body).toContain("font-family: var(--aurora-font-body)");
    expect(rule(".idle-tray > span:nth-of-type(2)")).toContain("text-overflow: ellipsis");
    expect(rule(".idle-tray-bar")).toContain("width: 100%");
    expect(rule(".idle-tray-bar")).toContain("height: 3px");
  });

  it("keeps the command palette focus and dropdown geometry visually stable", () => {
    expect(css).toContain(".command-bar {\n  position: relative;");
    expect(rule(".palette-shell-compact:has(.command-action-menu) .command-bar")).toContain("border-radius: 14px 14px 0 0");
    expect(rule(".command-input-wrap")).toContain("position: static");
    expect(rule(".command-input-wrap:has(.command-input:focus-visible)")).toContain("inset 0 0 0 1px");
    expect(rule(".command-action-switcher")).toContain("position: static");
    expect(rule(".command-action-menu")).toContain("left: -1px");
    expect(rule(".command-action-menu")).toContain("width: calc(100% + 2px)");
    expect(rule(".command-action-menu")).toContain("top: calc(100% - 1px)");
    expect(rule(".command-action-menu")).toContain("max-height: min(392px, calc(100vh - 76px))");
    expect(rule(".command-action-menu")).toContain("overflow: hidden");
    expect(rule(".command-action-menu")).toContain("border-radius: 0 0 10px 10px");
    expect(rule(".command-action-options")).toContain("overflow-y: auto");
    expect(rule(".command-action-footer")).toContain("border-top: 1px solid");
    expect(rule(".command-action-option kbd")).toContain("font-family: var(--aurora-font-body)");
    expect(rule(".command-action-footer kbd")).toContain("font-family: var(--aurora-font-body)");
  });
});
