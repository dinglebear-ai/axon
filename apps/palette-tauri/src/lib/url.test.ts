import { describe, expect, it } from "vitest";

import { firstUrl, hostLabel } from "./url";

describe("hostLabel", () => {
  it("returns the hostname without the port", () => {
    expect(hostLabel("https://example.com:8080/docs")).toBe("example.com");
  });

  it("returns the hostname for a plain https URL", () => {
    expect(hostLabel("https://docs.rs/serde/latest")).toBe("docs.rs");
  });

  it("falls back to the leading path segment for unparseable input", () => {
    expect(hostLabel("example.com/foo")).toBe("example.com");
  });

  it("falls back to the raw input when there is no slash", () => {
    expect(hostLabel("not a url")).toBe("not a url");
  });
});

describe("firstUrl", () => {
  it("extracts the first http(s) URL from surrounding text", () => {
    expect(firstUrl('read this: "https://example.com/docs".')).toBe("https://example.com/docs");
  });

  it("returns the first URL when several are present", () => {
    expect(firstUrl("see http://a.test and https://b.test")).toBe("http://a.test");
  });

  it("returns null when no URL is present", () => {
    expect(firstUrl("no links here")).toBeNull();
  });
});
