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
    expect(normalizeSubmitArgument(action("source-site"), "http://example.com")).toBe("http://example.com");
  });

  it("does not coerce repository shorthand into a URL", () => {
    expect(normalizeSubmitArgument(action("source"), "unraid/api")).toBe("unraid/api");
  });

  it("leaves an explicit source URL untouched", () => {
    expect(normalizeSubmitArgument(action("source"), "https://github.com/unraid/api")).toBe(
      "https://github.com/unraid/api",
    );
  });

  it("does not coerce non-URL source targets", () => {
    expect(normalizeSubmitArgument(action("source"), "r/rust")).toBe("r/rust");
  });

  it("does not coerce source text or file and directory targets", () => {
    expect(normalizeSubmitArgument(action("source"), "some notes to index")).toBe("some notes to index");
    expect(normalizeSubmitArgument(action("source"), "./docs")).toBe("./docs");
  });

  it("passes a scheme-less source argument through verbatim", () => {
    // source accepts file/dir/text/URL; a bare host is ambiguous, so it is passed
    // through rather than coerced into https://docs.rs/serde.
    expect(normalizeSubmitArgument(action("source"), "docs.rs/serde")).toBe("docs.rs/serde");
  });

  it("trims surrounding whitespace", () => {
    expect(normalizeSubmitArgument(action("source"), "  owner/repo  ")).toBe("owner/repo");
  });
});
