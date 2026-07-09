import { describe, expect, it } from "vitest";

import { parseGitHubTarget } from "./actionRequest";

describe("parseGitHubTarget", () => {
  it("parses a bare owner into a repos-listing request", () => {
    expect(parseGitHubTarget("jmagar")).toEqual({ kind: "repos", owner: "jmagar" });
  });

  it("parses owner/repo into a tree request", () => {
    expect(parseGitHubTarget("jmagar/axon")).toEqual({ kind: "tree", owner: "jmagar", repo: "axon" });
  });

  it("parses owner/repo/path into a file request", () => {
    expect(parseGitHubTarget("jmagar/axon/README.md")).toEqual({
      kind: "file",
      owner: "jmagar",
      repo: "axon",
      path: "README.md",
    });
  });

  it("parses a nested file path", () => {
    expect(parseGitHubTarget("jmagar/axon/src/lib/actionRequest.ts")).toEqual({
      kind: "file",
      owner: "jmagar",
      repo: "axon",
      path: "src/lib/actionRequest.ts",
    });
  });

  it("trims surrounding slashes", () => {
    expect(parseGitHubTarget("/jmagar/axon/")).toEqual({ kind: "tree", owner: "jmagar", repo: "axon" });
  });

  it("collapses duplicate slashes", () => {
    expect(parseGitHubTarget("jmagar//axon")).toEqual({ kind: "tree", owner: "jmagar", repo: "axon" });
  });

  it("throws on an empty target", () => {
    expect(() => parseGitHubTarget("")).toThrow("owner or owner/repo[/path] is required");
  });

  it("throws on a whitespace-only target", () => {
    expect(() => parseGitHubTarget("   ")).toThrow("owner or owner/repo[/path] is required");
  });
});
