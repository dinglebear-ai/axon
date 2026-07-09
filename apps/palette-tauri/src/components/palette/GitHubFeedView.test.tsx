// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, fireEvent } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@/lib/invoke", () => ({
  isTauriRuntime: false,
  invoke: (...args: unknown[]) => invokeMock(...args),
  appWindow: { listen: () => Promise.resolve(() => {}) },
}));

import { __clearFeedCacheForTests, GitHubFeedView } from "./GitHubFeedView";

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
  __clearFeedCacheForTests();
});

const feedResult = {
  ok: true,
  status: 200,
  kind: "feed",
  owner: "jmagar",
  repo: null,
  branch: null,
  path: null,
  payload: {
    items: [
      {
        kind: "push",
        repo: "jmagar/axon",
        actor: "jmagar",
        title: "fix: bug in `src/main.rs`",
        url: "https://github.com/jmagar/axon/commits",
        path: "src/main.rs",
        num: null,
        meta: "3 commits · main",
        badge: null,
        timestampUnix: Math.floor(Date.now() / 1000),
      },
    ],
    partial: false,
    errors: [],
  },
  error: null,
  rateLimitRemaining: 55,
  rateLimitReset: null,
  authenticated: true,
};

const emptyFeedResult = { ...feedResult, payload: { items: [], partial: false, errors: [] } };

const errorFeedResult = {
  ok: false,
  status: 403,
  kind: "feed",
  owner: "jmagar",
  repo: null,
  branch: null,
  path: null,
  payload: null,
  error: "GitHub API rate limited — retry later",
  rateLimitRemaining: 0,
  rateLimitReset: null,
  authenticated: false,
};

describe("GitHubFeedView", () => {
  it("fetches and renders feed items grouped by day", async () => {
    invokeMock.mockResolvedValueOnce(feedResult);
    render(<GitHubFeedView owner="jmagar" onOpenItem={() => {}} />);
    expect(invokeMock).toHaveBeenCalledWith("github_browse", { request: { kind: "feed", owner: "jmagar" } });
    await waitFor(() => expect(screen.getByText(/fix: bug/)).toBeInTheDocument());
    expect(screen.getByText("Today")).toBeInTheDocument();
  });

  it("renders an empty state when there are no items", async () => {
    invokeMock.mockResolvedValueOnce(emptyFeedResult);
    render(<GitHubFeedView owner="jmagar" onOpenItem={() => {}} />);
    await waitFor(() => expect(screen.getByText(/no activity/i)).toBeInTheDocument());
  });

  it("renders an error state on failure", async () => {
    invokeMock.mockResolvedValueOnce(errorFeedResult);
    render(<GitHubFeedView owner="jmagar" onOpenItem={() => {}} />);
    await waitFor(() => expect(screen.getByText(/rate limited/)).toBeInTheDocument());
  });

  it("calls onOpenItem when a feed row is clicked", async () => {
    invokeMock.mockResolvedValueOnce(feedResult);
    const onOpenItem = vi.fn();
    render(<GitHubFeedView owner="jmagar" onOpenItem={onOpenItem} />);
    await waitFor(() => expect(screen.getByText(/fix: bug/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/fix: bug/));
    expect(onOpenItem).toHaveBeenCalledWith(feedResult.payload.items[0]);
  });
});
