import { describe, expect, it } from "vitest";
import type { LabbyToolDescriptor } from "../clients/labbyClient";
import { hasUnsavedSnippetChanges, insertToolCall, parseSnippetParams } from "./snippetDraft";

const tool: LabbyToolDescriptor = {
  kind: "mcpTool",
  id: "mcp:github::search-repos",
  label: "search-repos",
  description: "",
  source: "github",
  upstream: "github",
  tool: "search-repos",
  destructive: false,
  contractHash: "h",
  contractVersion: 1,
  catalogRevision: "catalog-7",
  inputSchema: { type: "object" },
  outputSchema: null,
  annotations: {},
};

describe("snippet drafts", () => {
  it("inserts an exact live tool reference without executing it", () => {
    const body = insertToolCall("async (input) => ({ ok: true })", tool, '{"query":"rust"}');
    expect(body).toContain("codemode.github.search_repos");
    expect(body).toContain('"query": "rust"');
    expect(body).not.toContain("fetch(");
  });
  it("requires object test params and tracks unsaved edits", () => {
    expect(parseSnippetParams('{"limit":3}')).toEqual({ limit: 3 });
    expect(() => parseSnippetParams("[]")).toThrow("JSON object");
    expect(
      hasUnsavedSnippetChanges({
        name: "a",
        description: "",
        body: "b",
        baseBody: "a",
        paramsText: "{}",
      }),
    ).toBe(true);
  });
});
