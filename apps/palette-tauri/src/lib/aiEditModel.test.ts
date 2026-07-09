import { describe, expect, it } from "vitest";

import { computeLineDiff } from "./aiEditModel";

describe("computeLineDiff", () => {
  it("marks unchanged lines as same", () => {
    const result = computeLineDiff("a\nb\nc", "a\nb\nc");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "same", text: "c" },
    ]);
  });

  it("marks an appended line as added", () => {
    const result = computeLineDiff("a\nb", "a\nb\nc");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "added", text: "c" },
    ]);
  });

  it("marks a removed trailing line as removed", () => {
    const result = computeLineDiff("a\nb\nc", "a\nb");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "removed", text: "c" },
    ]);
  });

  it("marks a changed middle line as removed+added (no in-place replace)", () => {
    const result = computeLineDiff("a\nb\nc", "a\nX\nc");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "removed", text: "b" },
      { kind: "added", text: "X" },
      { kind: "same", text: "c" },
    ]);
  });

  it("returns an empty array for two empty strings", () => {
    expect(computeLineDiff("", "")).toEqual([]);
  });
});
