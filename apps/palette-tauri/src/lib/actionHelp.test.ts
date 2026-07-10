import { describe, expect, it } from "vitest";

import { ACTIONS } from "./actions";
import { buildActionHelp, buildHelpRun, findHelpTarget, isHelpRequest } from "./actionHelp";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

describe("action help", () => {
  it("recognizes only exact help tokens", () => {
    expect(isHelpRequest("help")).toBe(true);
    expect(isHelpRequest("--help")).toBe(true);
    expect(isHelpRequest("-h")).toBe(true);
    expect(isHelpRequest("?")).toBe(true);
    expect(isHelpRequest("help me scrape")).toBe(false);
  });

  it("finds targets by subcommand and alias", () => {
    expect(findHelpTarget("scrape")?.subcommand).toBe("scrape");
    expect(findHelpTarget("fetch")?.subcommand).toBe("scrape");
    expect(findHelpTarget("crawl")?.subcommand).toBe("crawl");
    expect(findHelpTarget("missing")).toBeUndefined();
  });

  it("builds target help from neutral action metadata", () => {
    const help = buildActionHelp(action("scrape"));
    expect(help.title).toBe("Scrape URL");
    expect(help.route).toEqual({ method: "POST", path: "/v1/sources" });
    expect(help.usage).toBe("scrape https://docs.rs/serde");
    expect(help.parameters).toEqual(expect.arrayContaining(["url from input", "collection from palette settings when configured"]));
  });

  it("builds catalog run state with structured local payload", () => {
    const run = buildHelpRun();
    expect(run.kind).toBe("success");
    expect(run.result.path).toBe("palette://help");
    expect(run.outputKind).toBe("markdown");
    expect(run.text).toContain("# Axon Palette Help");
    expect(run.text).toContain("`scrape`");
  });

  it("builds unknown-target help with a visible note", () => {
    const run = buildHelpRun(undefined, "nope");
    expect(run.text).toContain("No matching action: `nope`");
  });
});
