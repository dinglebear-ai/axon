import { describe, expect, it } from "vitest";

import { ACTIONS } from "./actions";
import {
  actionHint,
  argumentFor,
  firstUrl,
  parseCommand,
  validationMessage,
} from "./paletteView";

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
    expect(actionHint(scrape, parsed.search)).toBe("Run URL");
  });

  it("validates required arguments and extracts pasted URLs", () => {
    expect(validationMessage(action("ask"), "")).toBe("Argument required");
    expect(firstUrl('read this: "https://example.com/docs".')).toBe("https://example.com/docs");
  });
});
