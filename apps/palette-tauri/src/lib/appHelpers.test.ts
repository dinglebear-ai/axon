import { describe, expect, it } from "vitest";

import { ACTIONS, type PaletteAction } from "./actions";
import { normalizeSubmitArgument } from "./appHelpers";

function action(subcommand: string): PaletteAction {
  const found = ACTIONS.find((a) => a.subcommand === subcommand);
  if (!found) throw new Error(`no action for ${subcommand}`);
  return found;
}

describe("normalizeSubmitArgument", () => {
  it("coerces a scheme-less argument to https for URL-only actions", () => {
    expect(normalizeSubmitArgument(action("scrape"), "docs.rs/serde")).toBe("https://docs.rs/serde");
  });

  it("leaves an explicit http(s) argument untouched", () => {
    expect(normalizeSubmitArgument(action("crawl"), "http://example.com")).toBe("http://example.com");
  });

  it("does NOT coerce ingest shorthand into a URL (owner/repo stays verbatim)", () => {
    expect(normalizeSubmitArgument(action("ingest"), "unraid/api")).toBe("unraid/api");
  });

  it("leaves an ingest GitHub URL untouched", () => {
    expect(normalizeSubmitArgument(action("ingest"), "https://github.com/unraid/api")).toBe(
      "https://github.com/unraid/api",
    );
  });

  it("does NOT coerce non-URL ingest targets (subreddit shorthand)", () => {
    expect(normalizeSubmitArgument(action("ingest"), "r/rust")).toBe("r/rust");
  });

  it("does NOT coerce embed free-text or file/dir targets", () => {
    expect(normalizeSubmitArgument(action("embed"), "some notes to embed")).toBe("some notes to embed");
    expect(normalizeSubmitArgument(action("embed"), "./docs")).toBe("./docs");
  });

  it("passes a scheme-less embed argument through verbatim (bare target, not a guessed URL)", () => {
    // embed accepts file/dir/text/URL; a bare host is ambiguous, so it is passed
    // through rather than coerced into https://docs.rs/serde.
    expect(normalizeSubmitArgument(action("embed"), "docs.rs/serde")).toBe("docs.rs/serde");
  });

  it("trims surrounding whitespace", () => {
    expect(normalizeSubmitArgument(action("ingest"), "  owner/repo  ")).toBe("owner/repo");
  });
});
