import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronLeft,
  Copy,
  ExternalLink,
  File as FileIcon,
  FileArchive,
  FileCode,
  FileCog,
  FileText,
  FolderGit2,
  Loader2,
  MessageSquare,
} from "lucide-react";

import { GitHubFeedView } from "@/components/palette/GitHubFeedView";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { EmptyResult, ResultHero } from "@/components/palette/OperationResultViewShared";
import type { GitHubBrowseResult } from "@/lib/actionRequest";
import { fileKind, isMarkdownLike } from "@/lib/filesModel";
import type { FeedItem } from "@/lib/githubFeed";
import { invoke } from "@/lib/invoke";
import type { LoadState } from "@/lib/loadState";
import { isRecord } from "@/lib/payload";

// Note: this rewrite drops the plain Path/Size `DetailLine` detail card the
// first drafting pass used for the file preview header, replacing it with
// the mock-matched `pvHead`/`pvFoot` chrome in `FilePreview` below (path is
// now shown inline in the header, size moves to the footer strip) — so
// `DetailLine` is no longer imported here. It is still used elsewhere in the
// codebase (`OperationResultViewShared.tsx` consumers); this file's import
// list simply no longer needs it.

export type { GitHubBrowseResult } from "@/lib/actionRequest";

interface GitHubBrowseRequest {
  kind: "repos" | "repo" | "tree" | "file" | "feed";
  owner: string;
  repo?: string;
  branch?: string;
  path?: string;
}

interface RepoSummary {
  name: string;
  full_name: string;
  description: string | null;
  language: string | null;
  stargazers_count: number;
  forks_count: number;
  private: boolean;
  default_branch: string;
}

interface TreeEntry {
  path: string;
  type: "blob" | "tree" | string;
  size?: number;
}

interface FileContents {
  path: string;
  content?: string;
  encoding?: string;
  size?: number;
  truncated?: boolean;
}

async function browse(request: GitHubBrowseRequest): Promise<GitHubBrowseResult> {
  return invoke<GitHubBrowseResult>("github_browse", { request });
}

/**
 * Structured view for the `github` palette action — real GitHub browsing via
 * the `github_browse` Tauri command (never a direct renderer fetch; the
 * desktop CSP `connect-src` has no `api.github.com` origin — see the header
 * comment in `src-tauri/src/github_bridge.rs`).
 *
 * Two levels of navigation, not a single undo-able history stack:
 *   1. Repo list <-> a specific repo. "Back" always means "return to the repo
 *      list" — there is nothing else to go back to once you're inside a repo,
 *      because...
 *   2. ...inside a repo, the file tree and the selected file's preview are
 *      rendered SIMULTANEOUSLY in a two-pane split (`.github-body`), modeled
 *      on `FilesView.tsx`'s `.files-body`/`.files-tree`/`.files-preview`
 *      layout. Clicking a different tree entry only swaps `selectedPath` +
 *      re-fetches the preview pane; the tree itself never re-renders from
 *      scratch and there is no "back" from a file to the tree, because both
 *      are always on screen together.
 *
 * The initial payload comes from the dispatched action (`owner`,
 * `owner/repo`, or `owner/repo/path`) and seeds the tree/selection state
 * below rather than being replayed through a history array.
 */
export const GitHubView = memo(function GitHubView({ payload }: { payload: Record<string, unknown> }) {
  const initial = payload as unknown as GitHubBrowseResult;

  const [repoRoot, setRepoRoot] = useState<GitHubBrowseResult>(initial);
  const [reposLoading, setReposLoading] = useState(false);
  const [reposError, setReposError] = useState<string | null>(null);

  const [selectedPath, setSelectedPath] = useState<string | null>(
    initial.ok && initial.kind === "file" ? (initial.path ?? null) : null,
  );
  const [file, setFile] = useState<LoadState<GitHubBrowseResult>>(
    initial.ok && initial.kind === "file" ? { kind: "loaded", value: initial } : { kind: "idle" },
  );
  const [treeState, setTreeState] = useState<LoadState<GitHubBrowseResult>>(
    initial.ok && initial.kind === "tree" ? { kind: "loaded", value: initial } : { kind: "idle" },
  );
  const [activeTab, setActiveTab] = useState<"browse" | "feed">("browse");

  const inRepo = repoRoot.ok && (repoRoot.kind === "tree" || repoRoot.kind === "file");

  const loadFile = useCallback(
    (path: string) => {
      if (!repoRoot.ok || !repoRoot.repo) return;
      // No-op when this path is already selected and loaded (or currently
      // loading) — avoids redundant `github_browse` calls on double-clicks or
      // repeated `onSelectFile` invocations for the same tree row.
      if (selectedPath === path && (file.kind === "loading" || file.kind === "loaded")) return;
      setSelectedPath(path);
      setFile({ kind: "loading" });
      browse({
        kind: "file",
        owner: repoRoot.owner,
        repo: repoRoot.repo,
        path,
        branch: repoRoot.branch ?? undefined,
      })
        .then((result) => setFile(result.ok ? { kind: "loaded", value: result } : { kind: "error", message: result.error ?? "Unable to load file." }))
        .catch((err) => setFile({ kind: "error", message: errorMessage(err) }));
    },
    [repoRoot, selectedPath, file.kind],
  );

  // When the initial payload is `kind: "file"` (deep-linked directly to a
  // file, e.g. from a Feed click — see openFeedItem), we have a file to
  // preview but no tree yet. Fetch it once.
  useEffect(() => {
    if (!repoRoot.ok || repoRoot.kind !== "file" || !repoRoot.repo) return;
    if (treeState.kind !== "idle") return;
    setTreeState({ kind: "loading" });
    browse({ kind: "tree", owner: repoRoot.owner, repo: repoRoot.repo, branch: repoRoot.branch ?? undefined })
      .then((result) => setTreeState(result.ok ? { kind: "loaded", value: result } : { kind: "error", message: result.error ?? "Unable to load file tree." }))
      .catch((err) => setTreeState({ kind: "error", message: errorMessage(err) }));
  }, [repoRoot, treeState.kind]);

  // Note: this does NOT reuse `loadFile` — `loadFile` closes over `repoRoot`,
  // which hasn't updated to the newly-opened repo yet at the point this
  // callback would invoke it (React state updates from `setRepoRoot` are
  // async). Fetching the file inline here avoids that stale-closure bug.
  const openFeedItem = useCallback((item: FeedItem) => {
    const [feedOwner, feedRepo] = item.repo.split("/");
    if (!feedOwner || !feedRepo) return;
    setActiveTab("browse");
    setReposLoading(true);
    browse({ kind: "tree", owner: feedOwner, repo: feedRepo })
      .then((result) => {
        if (!result.ok) {
          setReposError(result.error ?? "Unable to load repository.");
          return;
        }
        setRepoRoot(result);
        setTreeState({ kind: "loaded", value: result });
        if (item.path) {
          setSelectedPath(item.path);
          setFile({ kind: "loading" });
          browse({ kind: "file", owner: feedOwner, repo: feedRepo, path: item.path, branch: result.branch ?? undefined })
            .then((fileResult) =>
              setFile(
                fileResult.ok
                  ? { kind: "loaded", value: fileResult }
                  : { kind: "error", message: fileResult.error ?? "Unable to load file." },
              ),
            )
            .catch((err) => setFile({ kind: "error", message: errorMessage(err) }));
        } else {
          setSelectedPath(null);
          setFile({ kind: "idle" });
        }
      })
      .catch((err) => setReposError(errorMessage(err)))
      .finally(() => setReposLoading(false));
  }, []);

  async function openRepo(repo: RepoSummary) {
    setReposLoading(true);
    setReposError(null);
    try {
      const result = await browse({ kind: "tree", owner: repoRoot.owner, repo: repo.name, branch: repo.default_branch });
      if (result.ok) {
        setRepoRoot(result);
        setTreeState({ kind: "loaded", value: result });
        setSelectedPath(null);
        setFile({ kind: "idle" });
      } else {
        setReposError(result.error ?? "Unable to load repository.");
      }
    } finally {
      setReposLoading(false);
    }
  }

  function backToRepos() {
    setRepoRoot({ ...repoRoot, ok: true, kind: "repos", repo: null, branch: null, path: null, payload: [] });
    // Re-fetch the repo list fresh rather than trying to reconstruct it — the
    // initial `repos` payload is not retained once the user has drilled in.
    setReposLoading(true);
    browse({ kind: "repos", owner: repoRoot.owner })
      .then((result) => {
        if (result.ok) setRepoRoot(result);
        else setReposError(result.error ?? "Unable to load repositories.");
      })
      .finally(() => setReposLoading(false));
    setSelectedPath(null);
    setFile({ kind: "idle" });
    setTreeState({ kind: "idle" });
  }

  if (!repoRoot.ok) {
    return (
      <div className="output-body operation-view aurora-scrollbar">
        <ResultHero
          icon={<AlertTriangle size={16} />}
          title="GitHub request failed"
          tone="warn"
          metrics={[
            ["Status", repoRoot.status || "-"],
            ["Authenticated", repoRoot.authenticated ? "yes" : "no"],
          ]}
        />
        <section className="operation-section">
          <p className="operation-muted">{repoRoot.error ?? "Unknown GitHub error."}</p>
        </section>
      </div>
    );
  }

  return (
    <div className="output-body operation-view aurora-scrollbar github-view">
      <div className="github-header">
        <ResultHero
          icon={reposLoading ? <Loader2 size={16} className="github-spin" /> : <FolderGit2 size={16} />}
          title={githubTitle(repoRoot)}
          tone="neutral"
          metrics={[
            ["Rate limit", repoRoot.rateLimitRemaining ?? "-"],
            ["Auth", repoRoot.authenticated ? "token" : "anonymous"],
          ]}
        />
        {inRepo ? (
          <button type="button" className="github-back" onClick={backToRepos} disabled={reposLoading}>
            <ChevronLeft size={13} /> Back
          </button>
        ) : null}
      </div>
      <div className="github-seg" role="tablist" aria-label="GitHub view">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "browse"}
          className={`github-seg-btn${activeTab === "browse" ? " github-seg-btn-active" : ""}`}
          onClick={() => setActiveTab("browse")}
        >
          Browse
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "feed"}
          className={`github-seg-btn${activeTab === "feed" ? " github-seg-btn-active" : ""}`}
          onClick={() => setActiveTab("feed")}
        >
          Feed
        </button>
      </div>
      {activeTab === "feed" ? (
        <GitHubFeedView owner={repoRoot.owner} onOpenItem={openFeedItem} />
      ) : reposError ? (
        <section className="operation-section">
          <p className="operation-muted">{reposError}</p>
        </section>
      ) : repoRoot.kind === "repos" ? (
        <RepoListView payload={repoRoot.payload} onOpenRepo={openRepo} />
      ) : (
        <GitHubSplitView
          treePayload={
            repoRoot.kind === "tree"
              ? repoRoot.payload
              : treeState.kind === "loaded"
                ? treeState.value.payload
                : undefined
          }
          selectedPath={selectedPath}
          file={file}
          onSelectFile={loadFile}
          repo={repoRoot.repo ? `${repoRoot.owner}/${repoRoot.repo}` : repoRoot.owner}
          branch={repoRoot.branch}
        />
      )}
    </div>
  );
});

function GitHubSplitView({
  treePayload,
  selectedPath,
  file,
  onSelectFile,
  repo,
  branch,
}: {
  treePayload: unknown;
  selectedPath: string | null;
  file: LoadState<GitHubBrowseResult>;
  onSelectFile: (path: string) => void;
  /** `owner/repo` (or just `owner` if no repo is selected yet), passed through
   * to `FilePreview`'s pvHead/pvFoot actions (Copy/Open-on-GitHub/Ask). */
  repo: string;
  branch: string | null;
}) {
  const entries = useMemo(() => {
    const tree = isRecord(treePayload) ? treePayload.tree : undefined;
    return Array.isArray(tree) ? (tree as TreeEntry[]) : [];
  }, [treePayload]);
  const files = useMemo(
    () => entries.filter((entry) => entry.type === "blob").sort((a, b) => a.path.localeCompare(b.path)),
    [entries],
  );

  return (
    <div className="github-body">
      <div className="github-tree aurora-scrollbar" role="listbox" aria-label="Repository files">
        {files.length === 0 ? (
          <EmptyResult kind="generic" />
        ) : (
          files.slice(0, 300).map((entry) => (
            <button
              key={entry.path}
              type="button"
              role="option"
              aria-selected={selectedPath === entry.path}
              className={`github-tree-row${selectedPath === entry.path ? " github-tree-row-active" : ""}`}
              onClick={() => onSelectFile(entry.path)}
            >
              <TreeEntryIcon name={entry.path} />
              <span className="github-tree-path">{entry.path}</span>
              {typeof entry.size === "number" ? <span className="github-tree-size">{formatBytes(entry.size)}</span> : null}
            </button>
          ))
        )}
      </div>
      <div className="github-preview aurora-scrollbar">
        {!selectedPath ? (
          <div className="files-empty operation-muted">Select a file</div>
        ) : file.kind === "loading" ? (
          <div className="files-empty">
            <Loader2 size={16} className="github-spin" />
            <span>Loading...</span>
          </div>
        ) : file.kind === "error" ? (
          <div className="files-empty operation-muted">{file.message}</div>
        ) : file.kind === "loaded" ? (
          <FilePreview payload={file.value.payload} repo={repo} branch={branch} path={selectedPath} />
        ) : null}
      </div>
    </div>
  );
}

function githubTitle(result: GitHubBrowseResult): string {
  switch (result.kind) {
    case "repos":
      return `${result.owner}'s repositories`;
    case "tree":
    case "file":
      return `${result.owner}/${result.repo}${result.branch ? ` @ ${result.branch}` : ""}`;
    default:
      return "GitHub";
  }
}

function RepoListView({
  payload,
  onOpenRepo,
}: {
  payload: unknown;
  onOpenRepo: (repo: RepoSummary) => void;
}) {
  const repos = useMemo(() => (Array.isArray(payload) ? (payload as RepoSummary[]) : []), [payload]);
  if (repos.length === 0) return <EmptyResult kind="generic" />;
  return (
    <section className="operation-section">
      <div className="github-repo-list">
        {repos.map((repo) => (
          <button key={repo.full_name} type="button" className="github-repo-row" onClick={() => onOpenRepo(repo)}>
            <div className="github-repo-main">
              <span className="github-repo-name">{repo.full_name}</span>
              {repo.private ? <span className="github-repo-badge">Private</span> : null}
            </div>
            {repo.description ? <p className="operation-muted">{repo.description}</p> : null}
            <div className="github-repo-meta">
              {repo.language ? <span>{repo.language}</span> : null}
              <span>★ {repo.stargazers_count}</span>
              <span>⑂ {repo.forks_count}</span>
            </div>
          </button>
        ))}
      </div>
    </section>
  );
}

function TreeEntryIcon({ name }: { name: string }) {
  const base = name.split("/").pop() ?? name;
  switch (fileKind(base)) {
    case "doc":
      return <FileText size={13} className="files-icon-doc" aria-hidden="true" />;
    case "code":
      return <FileCode size={13} className="files-icon-code" aria-hidden="true" />;
    case "config":
      return <FileCog size={13} className="files-icon-config" aria-hidden="true" />;
    case "archive":
      return <FileArchive size={13} className="files-icon-muted" aria-hidden="true" />;
    default:
      return <FileIcon size={13} className="files-icon-muted" aria-hidden="true" />;
  }
}

/**
 * Corrected against the real mock's `pvHead`/`pvFoot` chrome (verified in
 * `palette-mock.html`, search `pvHead`/`pvFoot`/`pvBody`): the mock's file
 * preview has a header action row (Copy contents, Open on GitHub, Ask about
 * this file) and a footer strip (byte size, file extension). The first
 * drafting pass's `FilePreview` (a bare Path/Size detail card + body) was
 * missing all of this — now added below as `pvHead`/`pvFoot`, ported as
 * closely as this codebase's existing primitives allow:
 *
 * - "Copy contents" and "Open on GitHub" are both straightforward: clipboard
 *   write + a toast, no new cross-component plumbing required.
 * - "Ask about this file" in the mock closes the GitHub view and pipes
 *   `"About {repo}/{path}"` into the ask action (mock: `runOp(find('ask'), ...)`).
 *   This codebase's `GitHubView` currently has no callback prop for
 *   dispatching a *different* palette action from inside a structured result
 *   view — `OperationResultView.tsx`'s `github` entry renders `<GitHubView
 *   payload={...} />` with no action-dispatch callback passed down (see
 *   `apps/palette-tauri/src/components/palette/OperationResultView.tsx`).
 *   Wiring a real "run the ask action and switch views" hookup is out of
 *   scope for this task — it would require adding an `onAskAbout` prop
 *   threaded through `OperationResultView` down to every structured view,
 *   which is a bigger, cross-cutting change than "add the mock's preview
 *   actions." **This plan ships the "Ask" button as a copy-to-clipboard
 *   affordance** (copies `"About {repo}/{path}"` to the clipboard with a
 *   toast, same mechanism as the other two buttons) rather than silently
 *   dropping the button or silently pretending it's wired end-to-end. Flagged
 *   again in "Open Questions" below — a follow-up task should add the real
 *   `onAskAbout` plumbing if full mock parity is wanted.
 */
function FilePreview({ payload, repo, branch, path }: { payload: unknown; repo: string; branch: string | null; path: string | null }) {
  const file = isRecord(payload) ? (payload as unknown as FileContents) : null;
  const [toast, setToast] = useState<string | null>(null);
  if (!file) return <EmptyResult kind="generic" />;
  const decoded = decodeFileContent(file);
  const filePath = file.path ?? path ?? "";
  const fileName = filePath.split("/").pop() ?? filePath;
  const ext = fileName.includes(".") ? (fileName.split(".").pop()?.toUpperCase() ?? "FILE") : "FILE";

  function showToast(message: string) {
    setToast(message);
    setTimeout(() => setToast(null), 2000);
  }

  async function copyContents() {
    if (decoded === null) return;
    try {
      await navigator.clipboard.writeText(decoded);
      showToast(`Copied ${fileName}`);
    } catch {
      showToast("Copy failed");
    }
  }

  async function copyGitHubLink() {
    const url = `https://github.com/${repo}/blob/${branch ?? "main"}/${filePath}`;
    try {
      await navigator.clipboard.writeText(url);
      showToast(`GitHub link copied · ${fileName}`);
    } catch {
      showToast("Copy failed");
    }
  }

  async function copyAskPrompt() {
    // See this function's doc comment above: real ask-action dispatch is out
    // of scope for this task; this copies the mock's prompt text instead.
    try {
      await navigator.clipboard.writeText(`About ${repo}/${filePath}`);
      showToast("Ask prompt copied");
    } catch {
      showToast("Copy failed");
    }
  }

  return (
    <>
      <div className="github-preview-head">
        <TreeEntryIcon name={fileName} />
        <div className="github-preview-head-main">
          <div className="github-preview-head-name">{fileName}</div>
          <div className="github-preview-head-path">{repo} / {filePath}</div>
        </div>
        <button type="button" className="github-preview-action" title="Copy contents" aria-label="Copy contents" onClick={copyContents}>
          <Copy size={15} aria-hidden="true" />
        </button>
        <button type="button" className="github-preview-action" title="Open on GitHub" aria-label="Open on GitHub" onClick={copyGitHubLink}>
          <ExternalLink size={15} aria-hidden="true" />
        </button>
        <button type="button" className="github-preview-ask" title="Ask about this file" onClick={copyAskPrompt}>
          <MessageSquare size={12} aria-hidden="true" /> Ask
        </button>
      </div>
      <section className="operation-section operation-reader-section">
        {file.truncated ? (
          <p className="operation-muted">File too large to preview inline.</p>
        ) : decoded !== null ? (
          <div className="operation-reader">
            {isMarkdownLike(filePath) ? (
              <MarkdownBody>{decoded}</MarkdownBody>
            ) : (
              <pre className="output-body output-code github-file-code">{decoded}</pre>
            )}
          </div>
        ) : (
          <p className="operation-muted">Unable to decode file contents.</p>
        )}
      </section>
      <div className="github-preview-foot">
        {typeof file.size === "number" ? <span>{formatBytes(file.size)}</span> : null}
        <span className="github-preview-foot-spacer" />
        <span className="github-preview-foot-ext">{ext}</span>
      </div>
      {toast ? <div className="github-toast">{toast}</div> : null}
    </>
  );
}

function decodeFileContent(file: FileContents): string | null {
  if (typeof file.content !== "string" || !file.content) return null;
  try {
    if (file.encoding && file.encoding !== "base64") return file.content;
    const normalized = file.content.replace(/\n/g, "");
    const binary = atob(normalized);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  } catch {
    return null;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10240 ? 1 : 0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
