import { describe, expect, it } from "vitest";

import { ACTIONS, actionMatches } from "./actions";
import {
  actionDisplayMeta,
  sortActionsByRelevance,
  sortActionsForDisplay,
  actionHint,
  argumentFor,
  firstUrl,
  parseCommand,
  validationMessage,
} from "./paletteView";

function rankedSubcommands(query: string): string[] {
  return sortActionsByRelevance(
    ACTIONS.filter((candidate) => actionMatches(candidate, query)),
    query,
  ).map((item) => item.subcommand);
}

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

describe("palette view parsing helpers", () => {
  it("parses action aliases and keeps the remaining argument", () => {
    expect(parseCommand("deepsearch qdrant hybrid tuning")).toMatchObject({
      invoked: action("research"),
      search: "deepsearch",
      arg: "qdrant hybrid tuning",
    });
  });

  it("uses direct URLs as action arguments for URL-aware actions", () => {
    const scrape = action("scrape");
    const parsed = parseCommand("https://example.com/docs");

    expect(argumentFor(scrape, null, parsed, parsed.search)).toBe("https://example.com/docs");
    expect(actionHint(scrape, parsed.search)).not.toBe("Select");
  });

  it("treats bare domains as URL-like input for URL-aware actions", () => {
    const scrape = action("scrape");
    const parsed = parseCommand("docs.rs/serde");

    expect(argumentFor(scrape, null, parsed, parsed.search)).toBe("docs.rs/serde");
    expect(actionHint(scrape, parsed.search)).toBe("Run URL");
  });

  it("validates required arguments and extracts pasted URLs", () => {
    expect(validationMessage(action("ask"), "")).not.toBe("");
    expect(firstUrl('read this: "https://example.com/docs".')).toBe("https://example.com/docs");
  });

  it("exposes mock-aligned route metadata for browse rows", () => {
    expect(actionDisplayMeta(action("scrape"))).toEqual({
      category: "Fetch & read",
      endpoint: "/v1/scrape",
      input: "one URL",
      output: "content",
      label: "Scrape",
      method: "POST",
    });
  });

  it("sorts browse actions by mock category order", () => {
    const sorted = sortActionsForDisplay([
      action("scrape"),
      action("crawl"),
      action("map"),
      action("summarize"),
      action("retrieve"),
      action("diff"),
      action("screenshot"),
    ]).map((item) => item.subcommand);

    expect(sorted.slice(0, 5)).toEqual(["scrape", "map", "retrieve", "screenshot", "diff"]);
  });

  it("ranks subcommand prefix matches above substring matches when filtering", () => {
    const ranked = rankedSubcommands("cr");
    // "crawl" starts with the query; "scrape" only contains it — crawl must win.
    expect(ranked[0]).toBe("crawl");
    expect(ranked.indexOf("crawl")).toBeLessThan(ranked.indexOf("scrape"));
  });

  it("surfaces the prefix-matching subcommand first ('doc' -> doctor)", () => {
    expect(rankedSubcommands("doc")[0]).toBe("doctor");
  });

  it("falls back to the browse order for an empty query", () => {
    expect(rankedSubcommands("").slice(0, 3)).toEqual(
      sortActionsForDisplay(ACTIONS).map((item) => item.subcommand).slice(0, 3),
    );
  });

  it("keeps first browse-row descriptions aligned with the handoff mock", () => {
    expect(action("scrape").description).toBe(
      "Fetch a single page, convert it to clean markdown, and optionally embed it into the collection.",
    );
    expect(action("map").description).toBe(
      "Walk a domain and return the URL graph without fetching page bodies. Fast reconnaissance.",
    );
  });
});
