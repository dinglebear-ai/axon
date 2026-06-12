// @ts-expect-error Vitest runs this file in Node; the app tsconfig intentionally omits Node globals.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { resolvePaletteWindowSize } from "./useWindowChrome";

describe("resolvePaletteWindowSize", () => {
  it("uses a compact result window for expanded crawl jobs", () => {
    expect(
      resolvePaletteWindowSize(
        {
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
});
