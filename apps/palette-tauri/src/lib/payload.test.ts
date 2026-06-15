import { describe, expect, it } from "vitest";

import { firstArray, isRecord, shortId, titleCase } from "./payload";

describe("isRecord", () => {
  it("accepts plain objects", () => {
    expect(isRecord({ a: 1 })).toBe(true);
  });

  it("rejects arrays, null, and primitives", () => {
    expect(isRecord([1, 2])).toBe(false);
    expect(isRecord(null)).toBe(false);
    expect(isRecord("x")).toBe(false);
  });
});

describe("firstArray", () => {
  it("returns the first array found among record values", () => {
    expect(firstArray({ count: 3, urls: ["a", "b"], more: ["c"] })).toEqual(["a", "b"]);
  });

  it("returns null when no value is an array", () => {
    expect(firstArray({ count: 3, name: "x" })).toBeNull();
  });

  it("returns null for non-record input", () => {
    expect(firstArray(["a"])).toBeNull();
    expect(firstArray(null)).toBeNull();
    expect(firstArray("x")).toBeNull();
  });
});

describe("shortId", () => {
  it("truncates values longer than 12 chars with an ellipsis", () => {
    expect(shortId("0123456789abcdef")).toBe("0123456789ab…");
  });

  it("leaves values of 12 chars or fewer unchanged", () => {
    expect(shortId("0123456789ab")).toBe("0123456789ab");
    expect(shortId("short")).toBe("short");
  });

  it("returns an empty string unchanged (callers guard empties)", () => {
    expect(shortId("")).toBe("");
  });
});

describe("titleCase", () => {
  it("capitalizes each whitespace-separated word", () => {
    expect(titleCase("hello world")).toBe("Hello World");
  });

  it("treats / and - as word boundaries", () => {
    expect(titleCase("crawl-list")).toBe("Crawl-List");
    expect(titleCase("a/b")).toBe("A/B");
  });

  it("capitalizes a single word", () => {
    expect(titleCase("status")).toBe("Status");
  });
});
