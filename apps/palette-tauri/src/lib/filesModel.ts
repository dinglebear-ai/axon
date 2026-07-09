// Pure helpers for the "Files" local action (FilesView.tsx). Kept out of the
// component per the palette convention: business logic lives in src/lib/*,
// components stay thin renderers over props/state.

import type { AiEditProposal } from "./aiEditModel";

export interface FileEntry {
  name: string;
  /** Path relative to the allowed files root (forward-slash separated). */
  path: string;
  isDir: boolean;
  size: number;
  /** Unix seconds since epoch, when the filesystem provided one. */
  modifiedUnix?: number | null;
  /** Which filesystem this entry came from. Undefined/"local" entries are the
   * default (allowed-root local filesystem, via files_bridge.rs); "sftp"
   * entries come from a connected SFTP profile's tree and gate out the
   * manual Edit and "Edit with the model" actions (SFTP is read-only in v1 —
   * see FilesView.tsx). */
  origin?: "local" | "sftp";
}

export interface DirListing {
  /** Path relative to the allowed files root ("" for the root itself). */
  path: string;
  root: string;
  entries: FileEntry[];
}

export interface FileContents {
  path: string;
  content: string;
  size: number;
}

/** Human-readable byte size, matching the mock's `fmtBytes` scale (B/KB/MB). */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) {
    const kb = bytes / 1024;
    return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Relative "time ago" label for a Unix-seconds timestamp, or "-" when absent. */
export function formatModified(unixSeconds: number | null | undefined, nowMs = Date.now()): string {
  if (unixSeconds == null) return "-";
  const deltaMs = nowMs - unixSeconds * 1000;
  if (deltaMs < 0) return "just now";
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (deltaMs < minute) return "just now";
  if (deltaMs < hour) return `${Math.floor(deltaMs / minute)}m ago`;
  if (deltaMs < day) return `${Math.floor(deltaMs / hour)}h ago`;
  if (deltaMs < 30 * day) return `${Math.floor(deltaMs / day)}d ago`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}

/** Split a root-relative path into breadcrumb segments (empty for the root). */
export function breadcrumbSegments(path: string): string[] {
  return path.split("/").filter(Boolean);
}

/** Join breadcrumb segments back into a root-relative path. */
export function joinSegments(segments: string[]): string {
  return segments.join("/");
}

/** Root-relative path of a directory entry's parent ("" at the root). */
export function parentPath(path: string): string {
  const segments = breadcrumbSegments(path);
  return joinSegments(segments.slice(0, -1));
}

/** Join a directory's root-relative path with a child entry name. */
export function childPath(dirPath: string, name: string): string {
  return dirPath ? `${dirPath}/${name}` : name;
}

const CODE_EXTENSIONS = new Set([
  "rs",
  "ts",
  "tsx",
  "js",
  "jsx",
  "py",
  "go",
  "css",
  "html",
  "sh",
  "bash",
  "toml",
]);
const DOC_EXTENSIONS = new Set(["md", "mdx", "txt", "rst"]);
const CONFIG_EXTENSIONS = new Set(["json", "jsonl", "yaml", "yml", "ini", "env", "lock"]);
const ARCHIVE_EXTENSIONS = new Set(["zip", "tar", "gz", "bz2", "xz", "7z"]);
const KNOWN_BINARY_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "avif",
  "ico",
  "bmp",
  "pdf",
  "woff",
  "woff2",
  "ttf",
  "otf",
  "mp3",
  "mp4",
  "mov",
  "avi",
  "wasm",
  "so",
  "dylib",
  "dll",
  "exe",
  "bin",
]);

export type FileKind = "doc" | "code" | "config" | "archive" | "binary" | "text";

export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function fileKind(name: string): FileKind {
  const ext = extensionOf(name);
  if (DOC_EXTENSIONS.has(ext)) return "doc";
  if (CODE_EXTENSIONS.has(ext)) return "code";
  if (CONFIG_EXTENSIONS.has(ext)) return "config";
  if (ARCHIVE_EXTENSIONS.has(ext)) return "archive";
  if (KNOWN_BINARY_EXTENSIONS.has(ext)) return "binary";
  return "text";
}

/** Whether a file kind is worth offering an "Ingest" action for. Archives and
 * known-binary extensions are excluded — embedding a zip/image as text markdown
 * is not useful and the read path would reject non-UTF-8 content anyway. */
export function isIngestable(name: string): boolean {
  const kind = fileKind(name);
  return kind !== "archive" && kind !== "binary";
}

export function isMarkdownLike(name: string): boolean {
  return fileKind(name) === "doc";
}

/** Sort directories first, then case-insensitive name. Mirrors the Rust-side
 * `files_list_dir` ordering so client-side re-sorts (if any) stay consistent. */
export function sortEntries(entries: FileEntry[]): FileEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  });
}

/** Shared async-load state shape for a single fetched value (dir listing or
 * file contents). Lifted here from FilesView.tsx so pane state can reuse it
 * without a component→component import. */
export type LoadState<T> =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; value: T }
  | { kind: "error"; message: string };

export type PaneId = "left" | "right";

/** One open file-browsing pane: its own cwd, selection, loaded file, and edit
 * state. Two panes (left/right) enable the split view; single-pane mode is
 * just "only the left pane is rendered."
 *
 * `loadGen` guards against out-of-order async resolution: every
 * loadDir/loadFile dispatch increments it, and a resolved fetch is only
 * applied if its captured generation still matches the pane's current one —
 * otherwise a slower, superseded request is silently dropped instead of
 * overwriting newer content. See filesViewState.ts's fileLoaded/fileError
 * reducer cases. */
export interface FilesPane {
  id: PaneId;
  cwd: string;
  selected: FileEntry | null;
  file: LoadState<FileContents>;
  loadGen: number;
  editing: boolean;
  draft: string;
  saving: boolean;
  /** Whether the "Edit with the model" inline instruction prompt is open. */
  sparkleOpen: boolean;
  /** Current text typed into the sparkle instruction prompt. */
  sparkleQuery: string;
  /** The most recently generated (not yet approved/denied) AI-edit proposal. */
  proposal: AiEditProposal | null;
  proposalState: "idle" | "pending" | "ready" | "approving" | "error";
  proposalErrorMessage: string | null;
}

export function createPane(id: PaneId, cwd = ""): FilesPane {
  return {
    id,
    cwd,
    selected: null,
    file: { kind: "idle" },
    loadGen: 0,
    editing: false,
    draft: "",
    saving: false,
    sparkleOpen: false,
    sparkleQuery: "",
    proposal: null,
    proposalState: "idle",
    proposalErrorMessage: null,
  };
}

/** Set of root-relative paths currently checked for bulk actions. Kept as a
 * plain `ReadonlySet<string>` (not a class) so it composes with the reducer's
 * state shape without extra wrapper methods; helpers below return new sets
 * (never mutate) to keep reducer updates predictable. */
export type CheckedPaths = ReadonlySet<string>;

export function toggleChecked(set: CheckedPaths, path: string): CheckedPaths {
  const next = new Set(set);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  return next;
}

export function checkAllIn(set: CheckedPaths, paths: string[]): CheckedPaths {
  const next = new Set(set);
  for (const path of paths) next.add(path);
  return next;
}

export function clearChecked(): CheckedPaths {
  return new Set();
}

export function isChecked(set: CheckedPaths, path: string): boolean {
  return set.has(path);
}
