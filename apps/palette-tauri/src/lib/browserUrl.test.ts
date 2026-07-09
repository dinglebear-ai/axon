import { describe, expect, it } from "vitest";

import {
  BROWSER_HOME_URL,
  hostLabel,
  isHomeUrl,
  isSearchUrl,
  normalizeBrowserUrl,
  searchQueryFrom,
} from "@/lib/browserUrl";

describe("normalizeBrowserUrl", () => {
  it("maps empty input to the home sentinel", () => {
    expect(normalizeBrowserUrl("")).toBe(BROWSER_HOME_URL);
    expect(normalizeBrowserUrl("   ")).toBe(BROWSER_HOME_URL);
  });

  it("maps the literal 'home' to the home sentinel", () => {
    expect(normalizeBrowserUrl("home")).toBe(BROWSER_HOME_URL);
  });

  it("passes through an already-schemed https URL unchanged", () => {
    expect(normalizeBrowserUrl("https://example.com")).toBe("https://example.com");
  });

  it("passes through an already-schemed http URL unchanged", () => {
    expect(normalizeBrowserUrl("http://example.com/path")).toBe("http://example.com/path");
  });

  it("coerces a bare hostname to https", () => {
    expect(normalizeBrowserUrl("example.com")).toBe("https://example.com");
  });

  it("coerces a hostname with a path to https", () => {
    expect(normalizeBrowserUrl("docs.rs/serde")).toBe("https://docs.rs/serde");
  });

  it("coerces localhost to http, not https", () => {
    expect(normalizeBrowserUrl("localhost:3000")).toBe("http://localhost:3000");
  });

  it("coerces a loopback IP to http", () => {
    expect(normalizeBrowserUrl("127.0.0.1:8001")).toBe("http://127.0.0.1:8001");
  });

  it("routes a bare word with no dot to the search engine", () => {
    const result = normalizeBrowserUrl("rust");
    expect(result).toMatch(/^https:\/\/duckduckgo\.com\/\?q=/);
    expect(result).toContain(encodeURIComponent("rust"));
  });

  it("routes a multi-word phrase to the search engine", () => {
    const result = normalizeBrowserUrl("tauri v2 webview api");
    expect(result).toMatch(/^https:\/\/duckduckgo\.com\/\?q=/);
    expect(searchQueryFrom(result)).toBe("tauri v2 webview api");
  });

  it("trims surrounding whitespace before normalizing", () => {
    expect(normalizeBrowserUrl("  example.com  ")).toBe("https://example.com");
  });

  it("does not treat a dotted phrase containing whitespace as navigable", () => {
    const result = normalizeBrowserUrl("check example.com please");
    expect(isSearchUrl(result)).toBe(true);
  });
});

describe("isHomeUrl", () => {
  it("recognizes the sentinel and its aliases", () => {
    expect(isHomeUrl(BROWSER_HOME_URL)).toBe(true);
    expect(isHomeUrl("")).toBe(true);
    expect(isHomeUrl("home")).toBe(true);
  });

  it("rejects a real URL", () => {
    expect(isHomeUrl("https://example.com")).toBe(false);
  });
});

describe("hostLabel", () => {
  it("returns 'New Tab' for the home sentinel", () => {
    expect(hostLabel(BROWSER_HOME_URL)).toBe("New Tab");
  });

  it("extracts the hostname from a full URL", () => {
    expect(hostLabel("https://docs.rs/serde/latest/serde")).toBe("docs.rs");
  });

  it("falls back to the raw string for an unparseable value", () => {
    expect(hostLabel("not a url")).toBe("not a url");
  });
});

describe("isSearchUrl / searchQueryFrom", () => {
  it("round-trips a search query", () => {
    const url = normalizeBrowserUrl("hello world");
    expect(isSearchUrl(url)).toBe(true);
    expect(searchQueryFrom(url)).toBe("hello world");
  });

  it("returns false/empty for a non-search URL", () => {
    expect(isSearchUrl("https://example.com")).toBe(false);
    expect(searchQueryFrom("https://example.com")).toBe("");
  });
});
