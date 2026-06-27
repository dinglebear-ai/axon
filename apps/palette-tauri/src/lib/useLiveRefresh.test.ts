import { describe, expect, it } from "vitest";

import { isLiveRefreshablePath } from "./useLiveRefresh";

describe("isLiveRefreshablePath", () => {
  it("treats stats and status as live-refreshable", () => {
    expect(isLiveRefreshablePath("/v1/stats")).toBe(true);
    expect(isLiveRefreshablePath("/v1/status")).toBe(true);
  });

  it("does NOT auto-refresh slow-growing or one-shot endpoints", () => {
    expect(isLiveRefreshablePath("/v1/sources")).toBe(false);
    expect(isLiveRefreshablePath("/v1/domains")).toBe(false);
    expect(isLiveRefreshablePath("/v1/doctor")).toBe(false);
    expect(isLiveRefreshablePath("/v1/ask")).toBe(false);
  });

  it("is safe on undefined/empty", () => {
    expect(isLiveRefreshablePath(undefined)).toBe(false);
    expect(isLiveRefreshablePath("")).toBe(false);
  });
});
