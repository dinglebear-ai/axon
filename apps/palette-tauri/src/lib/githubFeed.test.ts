import { describe, expect, it } from "vitest";

import { feedKindLabel, groupFeedByDay, type FeedItem } from "./githubFeed";

function item(overrides: Partial<FeedItem>): FeedItem {
  return {
    kind: "push",
    repo: "jmagar/axon",
    actor: "jmagar",
    title: "fix: bug",
    url: "https://github.com/jmagar/axon/commits",
    path: null,
    num: null,
    meta: "3 commits · main",
    badge: null,
    timestampUnix: 0,
    ...overrides,
  };
}

describe("groupFeedByDay", () => {
  it("groups items into Today/Yesterday/Earlier relative to now", () => {
    const now = new Date("2024-06-15T12:00:00Z").getTime();
    const todayItem = item({ title: "today", timestampUnix: Math.floor(new Date("2024-06-15T08:00:00Z").getTime() / 1000) });
    const yesterdayItem = item({ title: "yesterday", timestampUnix: Math.floor(new Date("2024-06-14T08:00:00Z").getTime() / 1000) });
    const earlierItem = item({ title: "earlier", timestampUnix: Math.floor(new Date("2024-06-01T08:00:00Z").getTime() / 1000) });

    const groups = groupFeedByDay([earlierItem, todayItem, yesterdayItem], now);

    expect(groups.map((g) => g.label)).toEqual(["Today", "Yesterday", "Earlier"]);
    expect(groups[0].items).toEqual([todayItem]);
    expect(groups[1].items).toEqual([yesterdayItem]);
    expect(groups[2].items).toEqual([earlierItem]);
  });

  it("omits empty groups", () => {
    const now = new Date("2024-06-15T12:00:00Z").getTime();
    const onlyToday = item({ timestampUnix: Math.floor(new Date("2024-06-15T08:00:00Z").getTime() / 1000) });
    const groups = groupFeedByDay([onlyToday], now);
    expect(groups.map((g) => g.label)).toEqual(["Today"]);
  });

  it("returns an empty array for no items", () => {
    expect(groupFeedByDay([], Date.now())).toEqual([]);
  });
});

describe("feedKindLabel", () => {
  // Labels verified against palette-mock.html's real `var FEED_KIND = {...}`
  // object — NOT the first drafting pass's reconstruction (which used
  // "Pull request" lowercase-r and invented a "dependency-bump" kind that
  // does not exist in the mock).
  it.each([
    ["push", "Push"],
    ["pr", "Pull Request"],
    ["merge", "Merged"],
    ["review", "Review"],
    ["comment", "Comment"],
    ["conflict", "Conflict"],
    ["deps", "Dependencies"],
    ["issue", "Issue"],
    ["release", "Release"],
  ])("labels %s as %s", (kind, expected) => {
    expect(feedKindLabel(kind)).toBe(expected);
  });

  it("falls back to the raw kind for unknown values", () => {
    expect(feedKindLabel("mystery")).toBe("mystery");
  });
});
