// @vitest-environment jsdom
//
// Behavioral render test for GitHubView: the structured view for the
// `github` palette action. Verifies the three payload shapes render (repo
// list, file tree, file preview), the error state, and that clicking a repo
// or a tree entry issues a fresh `github_browse` call via the mocked invoke
// seam (real navigation, not one-shot commands — see the component header
// comment for why).

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@/lib/invoke", () => ({
  isTauriRuntime: false,
  invoke: (...args: unknown[]) => invokeMock(...args),
  appWindow: { listen: () => Promise.resolve(() => {}) },
}));

import { __clearFeedCacheForTests } from "./GitHubFeedView";
import { GitHubView } from "./GitHubView";

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
  __clearFeedCacheForTests();
});

const reposResult = {
  ok: true,
  status: 200,
  kind: "repos",
  owner: "jmagar",
  repo: null,
  branch: null,
  path: null,
  payload: [
    {
      name: "axon",
      full_name: "jmagar/axon",
      description: "Self-hosted RAG stack",
      language: "Rust",
      stargazers_count: 12,
      forks_count: 3,
      private: false,
      default_branch: "main",
    },
  ],
  error: null,
  rateLimitRemaining: 59,
  rateLimitReset: null,
  authenticated: false,
};

const treeResult = {
  ok: true,
  status: 200,
  kind: "tree",
  owner: "jmagar",
  repo: "axon",
  branch: "main",
  path: null,
  payload: {
    tree: [
      { path: "README.md", type: "blob", size: 1024 },
      { path: "src", type: "tree" },
      { path: "src/main.rs", type: "blob", size: 256 },
    ],
  },
  error: null,
  rateLimitRemaining: 58,
  rateLimitReset: null,
  authenticated: false,
};

const fileResult = {
  ok: true,
  status: 200,
  kind: "file",
  owner: "jmagar",
  repo: "axon",
  branch: "main",
  path: "README.md",
  payload: {
    path: "README.md",
    content: btoa("# axon\n\nhello"),
    encoding: "base64",
    size: 15,
  },
  error: null,
  rateLimitRemaining: 57,
  rateLimitReset: null,
  authenticated: false,
};

const errorResult = {
  ok: false,
  status: 403,
  kind: "repos",
  owner: "jmagar",
  repo: null,
  branch: null,
  path: null,
  payload: null,
  error: "GitHub API rate limited — retry at 2024-01-01 00:00:00 UTC",
  rateLimitRemaining: 0,
  rateLimitReset: 1_700_000_000,
  authenticated: false,
};

describe("GitHubView", () => {
  it("renders a repo list", () => {
    render(<GitHubView payload={reposResult} />);
    expect(screen.getByText("jmagar/axon")).toBeInTheDocument();
    expect(screen.getByText("Self-hosted RAG stack")).toBeInTheDocument();
  });

  it("drills into a repo's tree on click", async () => {
    invokeMock.mockResolvedValueOnce(treeResult);
    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByText("jmagar/axon"));
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith("github_browse", {
      request: { kind: "tree", owner: "jmagar", repo: "axon", branch: "main" },
    });
    // Directory entries (type "tree") are not listed as files.
    expect(screen.queryByText("src", { exact: true })).not.toBeInTheDocument();
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
  });

  it("renders a file tree directly when given a tree payload", () => {
    render(<GitHubView payload={treeResult} />);
    expect(screen.getByText("README.md")).toBeInTheDocument();
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
  });

  it("opens a file preview on click and decodes base64 markdown content", async () => {
    invokeMock.mockResolvedValueOnce(fileResult);
    render(<GitHubView payload={treeResult} />);
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("github_browse", {
      request: { kind: "file", owner: "jmagar", repo: "axon", path: "README.md", branch: "main" },
    });
  });

  it("renders a decoded file preview directly when given a file payload", () => {
    render(<GitHubView payload={fileResult} />);
    // Path shows in both the hero title and the detail card; content is
    // decoded from base64 and shown (markdown renderer lazy-loads behind
    // Suspense, so the immediate fallback is a raw <pre> with the same text).
    expect(screen.getAllByText("README.md").length).toBeGreaterThan(0);
    expect(screen.getByText(/hello/)).toBeInTheDocument();
  });

  it("renders the GitHub error state with the rate-limit message", () => {
    render(<GitHubView payload={errorResult} />);
    expect(screen.getByText("GitHub request failed")).toBeInTheDocument();
    expect(screen.getByText(/rate limited/)).toBeInTheDocument();
  });

  it("shows a Back-to-repos button once inside a repo, and returns to the repo list", async () => {
    invokeMock.mockResolvedValueOnce(treeResult);
    // backToRepos() re-fetches the repo list fresh (see GitHubView.tsx) — mock
    // a second response for that follow-up call.
    invokeMock.mockResolvedValueOnce(reposResult);
    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByText("jmagar/axon"));
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const backButton = screen.getByRole("button", { name: /back/i });
    fireEvent.click(backButton);
    await waitFor(() => expect(screen.getByText("jmagar/axon")).toBeInTheDocument());
    // Back from the repo-list view (the top level) does not show a Back button.
    expect(screen.queryByRole("button", { name: /back/i })).not.toBeInTheDocument();
  });

  it("renders the tree and a file's preview simultaneously in a split view", async () => {
    invokeMock.mockResolvedValueOnce(fileResult);
    render(<GitHubView payload={treeResult} />);
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByText(/hello/)).toBeInTheDocument());
    // The tree is still visible alongside the preview — this is the split-pane
    // behavior replacing the old sequential "tree screen -> separate preview
    // screen" navigation. "README.md" appears in both the tree row and the
    // preview pane's header, so assert presence via getAllByText rather than
    // getByText (same pattern as the existing "renders a decoded file preview
    // directly..." test above).
    expect(screen.getAllByText("README.md").length).toBeGreaterThan(0);
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
    expect(screen.getByText(/hello/)).toBeInTheDocument();
  });

  it("selecting a different file only swaps the preview pane, tree stays put", async () => {
    invokeMock.mockResolvedValueOnce(fileResult);
    render(<GitHubView payload={treeResult} />);
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByText(/hello/)).toBeInTheDocument());

    const secondFileResult = {
      ...fileResult,
      path: "src/main.rs",
      payload: { path: "src/main.rs", content: btoa("fn main() {}"), encoding: "base64", size: 12 },
    };
    invokeMock.mockResolvedValueOnce(secondFileResult);
    fireEvent.click(screen.getByText("src/main.rs"));
    await waitFor(() => expect(screen.getByText(/fn main/)).toBeInTheDocument());
    // Tree entries for BOTH files are still visible — no navigation occurred.
    expect(screen.getAllByText("README.md").length).toBeGreaterThan(0);
    expect(screen.getAllByText("src/main.rs").length).toBeGreaterThan(0);
  });

  it("shows a Feed tab and switches to it, fetching the owner's activity feed", async () => {
    const feedResult = {
      ok: true,
      status: 200,
      kind: "feed",
      owner: "jmagar",
      repo: null,
      branch: null,
      path: null,
      payload: { items: [], partial: false, errors: [] },
      error: null,
      rateLimitRemaining: 55,
      rateLimitReset: null,
      authenticated: false,
    };
    invokeMock.mockResolvedValueOnce(feedResult);
    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByRole("tab", { name: /feed/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("github_browse", { request: { kind: "feed", owner: "jmagar" } }),
    );
  });

  it("clicking a Feed item with a path jumps into the split view at that file", async () => {
    const feedItem = {
      kind: "push",
      repo: "jmagar/axon",
      actor: "jmagar",
      title: "fix: bug in `README.md`",
      url: "https://github.com/jmagar/axon/commits",
      path: "README.md",
      num: null,
      meta: "1 commits · main",
      badge: null,
      timestampUnix: Math.floor(Date.now() / 1000),
    };
    const feedResult = {
      ok: true,
      status: 200,
      kind: "feed",
      owner: "jmagar",
      repo: null,
      branch: null,
      path: null,
      payload: { items: [feedItem], partial: false, errors: [] },
      error: null,
      rateLimitRemaining: 55,
      rateLimitReset: null,
      authenticated: false,
    };
    invokeMock.mockResolvedValueOnce(feedResult); // feed fetch
    invokeMock.mockResolvedValueOnce(treeResult); // tree fetch for jmagar/axon
    invokeMock.mockResolvedValueOnce(fileResult); // file fetch for README.md

    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByRole("tab", { name: /feed/i }));
    await waitFor(() => expect(screen.getByText(/fix: bug/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/fix: bug/));

    await waitFor(() => expect(screen.getByText(/hello/)).toBeInTheDocument());
    // Landed in the split view with the tree visible alongside the preview.
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
  });
});
