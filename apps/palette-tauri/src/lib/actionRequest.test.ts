import { describe, expect, it } from "vitest";

import { askBody, chatBody, parseGitHubTarget } from "./actionRequest";

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

describe("Labby profile selection", () => {
  const profiles = ["stale", "selected"].map((id) => ({
    id,
    label: id,
    product: "labby" as const,
    origin: `https://${id}.example`,
    credentialHandle: id,
    pinnedServerId: id,
    acceptedApiMajor: 1 as const,
  }));
  const context = {
    words: ["hello"],
    limit: 10,
    collectionBody: {},
    config: { backendProfiles: profiles, activeBackendProfiles: { labby: "selected" } },
  } as Parameters<typeof askBody>[0];

  it.each([["ask", askBody], ["chat", chatBody]] as const)(
    "%s binds the explicitly active Labby profile",
    (_name, builder) => {
      localStorage.setItem("axon.palette.loadout.v1:selected", JSON.stringify({
        profileId: "selected", integrationId: "labby", loadoutId: "loadout", name: "Selected",
        expectedRevision: 9, mode: "context", conversationBinding: "binding",
      }));
      expect(builder(context)).toMatchObject({
        loadout: { integrationId: "labby", loadoutId: "loadout", expectedRevision: 9 },
      });
    },
  );
});
