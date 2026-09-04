import { describe, expect, it, vi } from "vitest";
import {
  boundedText,
  emptyBuffer,
  idempotencyKey,
  MAX_RENDERED_TEXT,
  validateBuffer,
} from "./model";

describe("artifact authoring model", () => {
  it.each([
    ["skill", "SKILL.md"],
    ["prompt", "PROMPT.md"],
    ["agent", "AGENT.md"],
    ["hook", "HOOK.json"],
  ] as const)("creates a safe %s form", (family, path) => {
    expect(emptyBuffer("p1", family).files[0].path).toBe(path);
  });
  it("rejects traversal and bounds inert rendering", () => {
    const draft = emptyBuffer("p1", "skill");
    draft.files.push({ path: "../escape", content: "x" });
    expect(validateBuffer(draft)).toContain("traversal-free");
    expect(boundedText("x".repeat(MAX_RENDERED_TEXT + 1))).toHaveLength(MAX_RENDERED_TEXT);
  });
  it("uses a fresh bounded idempotency key", () => {
    vi.stubGlobal("crypto", { randomUUID: () => "uuid" });
    expect(idempotencyKey("save", "artifact")).toBe("palette-save-artifact-uuid");
    vi.unstubAllGlobals();
  });
});
