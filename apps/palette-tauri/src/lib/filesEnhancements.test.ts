import { describe, expect, it } from "vitest";

import {
  countTextMatches,
  extractFileSymbols,
  languageForFile,
  selectionPrompt,
} from "@/lib/filesEnhancements";

describe("filesEnhancements", () => {
  it("extracts useful symbols from code, markup, styles, and markdown", () => {
    const symbols = extractFileSymbols(
      [
        "# Overview",
        "export function runAxon() {}",
        "class Palette {}",
        '<section id="hero">',
        ".command-bar {",
      ].join("\n"),
    );

    expect(symbols.map(({ label, line }) => [label, line])).toEqual([
      ["Overview", 1],
      ["runAxon", 2],
      ["Palette", 3],
      ["hero", 4],
      [".command-bar", 5],
    ]);
  });

  it("counts non-overlapping case-insensitive file matches", () => {
    expect(countTextMatches("Axon axon AXON", "axon")).toBe(3);
    expect(countTextMatches("anything", "")).toBe(0);
  });

  it("detects extension aliases and special filenames", () => {
    expect(languageForFile("Dockerfile")).toBe("dockerfile");
    expect(languageForFile("view.mjs")).toBe("javascript");
    expect(languageForFile("icon.svg")).toBe("xml");
  });

  it("builds explicit selection prompts", () => {
    expect(selectionPrompt("explain", "const answer = 42")).toContain("Explain this selection");
  });
});
