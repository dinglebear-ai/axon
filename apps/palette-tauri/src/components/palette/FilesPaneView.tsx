import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  ArrowDownAZ,
  ArrowDownWideNarrow,
  ArrowUpNarrowWide,
  Braces,
  ChevronRight,
  Columns2,
  Copy,
  FileArchive,
  FileCheck2,
  FileSearch,
  FileText,
  FolderSearch,
  GitCompare,
  Loader2,
  MessageCircleQuestion,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRight,
  Plug,
  PlugZap,
  RefreshCw,
  Save,
  Search,
  SlidersHorizontal,
  Sparkles,
  Square,
  Upload,
  WrapText,
} from "lucide-react";
import { type KeyboardEvent, type Ref, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { computeLineDiff } from "@/lib/aiEditModel";
import {
  countTextMatches,
  extractFileSymbols,
  languageForFile,
  selectionPrompt,
} from "@/lib/filesEnhancements";
import {
  breadcrumbSegments,
  type DirListing,
  type FileContents,
  type FileEntry,
  type FilesPane,
  formatBytes,
  formatModified,
  isChecked,
  isIndexable,
  isMarkdownLike,
  type LoadState,
} from "@/lib/filesModel";
import type { SftpConnectionProfile, SftpEntry } from "@/lib/sftpModel";
import { AiEditPanel } from "./AiEditPanel";
import { EntryIcon } from "./EntryIcon";
import { FilesContextMenu, type FilesContextMenuItem } from "./FilesContextMenu";
import { HighlightedCodeEditor } from "./HighlightedCodeEditor";
import { HighlightedFileCode } from "./HighlightedFileCode";
import { MarkdownBodyInner } from "./MarkdownBodyInner";
import { SftpTreeSection, type SftpTreeSectionHandle } from "./SftpTreeSection";

type IndexState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; ok: boolean; message: string };

/**
 * One pane of the FilesView split view: toolbar (breadcrumb, split/SFTP/
 * refresh controls), the directory tree (plus the SFTP tree section on the
 * left pane), and the file preview/edit/AI-edit area. Pure presentational —
 * all state is owned by the parent `FilesView` and reaches this component as
 * props; every interaction is reported back via callback props.
 */
export function FilesPaneView({
  pane,
  listing,
  entries,
  indexState,
  isLeftPane,
  splitOpen,
  treeWidth,
  checked,
  indexedPaths,
  client,
  config,
  activeSftpConnectionId,
  activeSftpProfile,
  sftpTreeRef,
  onOpenEntry,
  onOpenSftpFile,
  onToggleChecked,
  onSetCwd,
  onGoToBreadcrumb,
  onActivatePane,
  onToggleSplit,
  onToggleSftp,
  onRefresh,
  onSetEditing,
  onCancelEdit,
  onDraftChange,
  onSave,
  onIndex,
  onIndexEntry,
  onSparkleToggle,
  onSparkleQueryChange,
  onSparkleSubmit,
  onProposalDeny,
  onProposalApprove,
}: {
  pane: FilesPane;
  listing: LoadState<DirListing>;
  entries: FileEntry[];
  indexState: IndexState;
  isLeftPane: boolean;
  splitOpen: boolean;
  treeWidth: number;
  checked: ReadonlySet<string>;
  indexedPaths: ReadonlySet<string>;
  client: unknown;
  config: unknown;
  activeSftpConnectionId: string | null;
  activeSftpProfile: SftpConnectionProfile | undefined;
  sftpTreeRef: Ref<SftpTreeSectionHandle>;
  onOpenEntry: (entry: FileEntry) => void;
  onOpenSftpFile: (connectionId: string, entry: SftpEntry) => void;
  onToggleChecked: (path: string) => void;
  onSetCwd: (cwd: string) => void;
  onGoToBreadcrumb: (index: number) => void;
  onActivatePane: () => void;
  onToggleSplit: () => void;
  onToggleSftp: () => void;
  onRefresh: () => void;
  onSetEditing: (editing: boolean) => void;
  onCancelEdit: () => void;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onIndex: () => void;
  onIndexEntry: (entry: FileEntry) => void;
  onSparkleToggle: () => void;
  onSparkleQueryChange: (value: string) => void;
  onSparkleSubmit: () => void;
  onProposalDeny: () => void;
  onProposalApprove: () => void;
}) {
  const segments = breadcrumbSegments(pane.cwd);
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(
    () => localStorage.getItem("axon.files.showHidden") === "true",
  );
  const [sortMode, setSortMode] = useState<"name" | "modified" | "size">(
    () => (localStorage.getItem("axon.files.sort") as "name" | "modified" | "size") || "name",
  );
  const [focusedIndex, setFocusedIndex] = useState(0);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [recentPaths, setRecentPaths] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem("axon.files.recent") ?? "[]") as string[];
    } catch {
      return [];
    }
  });
  const filterRef = useRef<HTMLInputElement>(null);
  const [contextMenu, setContextMenu] = useState<
    | { kind: "entry"; entry: FileEntry; x: number; y: number }
    | { kind: "folder"; x: number; y: number }
    | null
  >(null);
  const visibleEntries = useMemo(() => {
    const needle = filter.trim().toLocaleLowerCase();
    const filtered = entries.filter(
      (entry) =>
        (showHidden || !entry.name.startsWith(".")) &&
        (!needle || entry.name.toLocaleLowerCase().includes(needle)),
    );
    return [...filtered].sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      if (sortMode === "modified") return (b.modifiedUnix ?? 0) - (a.modifiedUnix ?? 0);
      if (sortMode === "size") return b.size - a.size;
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    });
  }, [entries, filter, showHidden, sortMode]);
  const directoryStats = useMemo(() => {
    const files = entries.filter((entry) => !entry.isDir);
    return {
      folders: entries.length - files.length,
      files: files.length,
      indexable: files.filter((entry) => isIndexable(entry.name)).length,
      size: files.reduce((total, entry) => total + entry.size, 0),
    };
  }, [entries]);
  const rootPath = listing.kind === "loaded" ? listing.value.root.replace(/\/$/, "") : null;
  const currentFolderPath = rootPath ? `${rootPath}${pane.cwd ? `/${pane.cwd}` : ""}` : null;

  function copyText(value: string) {
    void navigator.clipboard.writeText(value);
  }

  function openEntry(entry: FileEntry) {
    if (!entry.isDir) {
      const recent = JSON.parse(localStorage.getItem("axon.files.recent") ?? "[]") as string[];
      localStorage.setItem(
        "axon.files.recent",
        JSON.stringify([entry.path, ...recent.filter((path) => path !== entry.path)].slice(0, 5)),
      );
      setRecentPaths([entry.path, ...recent.filter((path) => path !== entry.path)].slice(0, 5));
    }
    onOpenEntry(entry);
  }

  function handleTreeKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (visibleEntries.length === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      setFocusedIndex((value) => Math.max(0, Math.min(visibleEntries.length - 1, value + delta)));
    } else if (event.key === "Enter" || event.key === "ArrowRight") {
      event.preventDefault();
      openEntry(visibleEntries[focusedIndex]);
    } else if (event.key === "ArrowLeft" || event.key === "Backspace") {
      if (!filter && pane.cwd) {
        event.preventDefault();
        onSetCwd(segments.slice(0, -1).join("/"));
      }
    } else if (event.key === " ") {
      const entry = visibleEntries[focusedIndex];
      if (!entry.isDir) {
        event.preventDefault();
        onToggleChecked(entry.path);
      }
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
      event.preventDefault();
      const entry = visibleEntries[focusedIndex];
      copyText(rootPath ? `${rootPath}/${entry.path}` : entry.path);
    }
  }

  useEffect(() => {
    localStorage.setItem("axon.files.showHidden", String(showHidden));
  }, [showHidden]);
  useEffect(() => localStorage.setItem("axon.files.sort", sortMode), [sortMode]);
  useEffect(() => setFocusedIndex(0), [filter, pane.cwd, sortMode, showHidden]);
  useEffect(() => {
    const focusFilter = (event: globalThis.KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && ["f", "p"].includes(event.key.toLowerCase())) {
        event.preventDefault();
        filterRef.current?.focus();
      }
    };
    window.addEventListener("keydown", focusFilter);
    return () => window.removeEventListener("keydown", focusFilter);
  }, []);

  function entryMenuItems(entry: FileEntry): FilesContextMenuItem[] {
    const absolutePath = entry.origin === "sftp" || !rootPath ? null : `${rootPath}/${entry.path}`;
    const checkedForIndex = isChecked(checked, entry.path);
    return [
      {
        label: entry.isDir ? "Open folder" : "Open preview",
        icon: entry.isDir ? FolderSearch : FileText,
        onSelect: () => openEntry(entry),
      },
      ...(!entry.isDir
        ? [
            {
              label: checkedForIndex ? "Remove from index selection" : "Select for indexing",
              icon: checkedForIndex ? Square : FileCheck2,
              onSelect: () => onToggleChecked(entry.path),
            },
          ]
        : []),
      {
        label: entry.isDir ? "Index folder" : "Index file",
        icon: Upload,
        disabled: !client || !config || (!entry.isDir && !isIndexable(entry.name)),
        onSelect: () => onIndexEntry(entry),
      },
      {
        label: "Copy path",
        icon: Copy,
        separatorBefore: true,
        onSelect: () => copyText(absolutePath ?? entry.path),
      },
      ...(absolutePath
        ? [
            {
              label: "Show in Finder",
              icon: FolderSearch,
              onSelect: () => void revealItemInDir(absolutePath),
            },
          ]
        : []),
    ];
  }

  // Pane activation on mousedown is a pointer-only convenience for the split
  // view (mirrors clicking anywhere in a window to focus it) — the pane's
  // own focusable controls (rows, buttons, textarea) remain independently
  // keyboard-reachable via normal tab order, so no keyboard equivalent is
  // lost here.
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: see comment above.
    <div
      className="files-pane"
      style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}
      onMouseDown={onActivatePane}
    >
      {/* biome-ignore lint/a11y/noStaticElementInteractions: Dragging this non-control toolbar moves the native palette window. */}
      <div
        className="files-toolbar"
        onContextMenu={(event) => {
          event.preventDefault();
          setContextMenu({ kind: "folder", x: event.clientX, y: event.clientY });
        }}
      >
        <nav className="files-breadcrumb" aria-label="Current directory">
          <Button variant="plain" size="unstyled" type="button" onClick={() => onSetCwd("")}>
            ~
          </Button>
          {segments.map((segment, index) => (
            <span key={segments.slice(0, index + 1).join("/")} className="files-breadcrumb-segment">
              <ChevronRight size={12} />
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                onClick={() => onGoToBreadcrumb(index)}
              >
                {segment}
              </Button>
            </span>
          ))}
        </nav>
        <span className="files-toolbar-count">
          {entries.length} {entries.length === 1 ? "item" : "items"}
        </span>
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          title={sidebarCollapsed ? "Show navigator" : "Hide navigator"}
          aria-label={sidebarCollapsed ? "Show file navigator" : "Hide file navigator"}
          onClick={() => setSidebarCollapsed((value) => !value)}
        >
          {sidebarCollapsed ? <PanelLeftOpen size={14} /> : <PanelLeftClose size={14} />}
        </Button>
        {isLeftPane && (
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            title={splitOpen ? "Close split" : "Split view"}
            aria-label={splitOpen ? "Close split" : "Split view"}
            onClick={onToggleSplit}
          >
            <Columns2 size={14} />
          </Button>
        )}
        {isLeftPane && (
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            title={activeSftpConnectionId ? "Disconnect SFTP" : "Connect SFTP"}
            aria-label={activeSftpConnectionId ? "Disconnect SFTP" : "Connect SFTP"}
            onClick={onToggleSftp}
          >
            {activeSftpConnectionId ? <PlugZap size={14} /> : <Plug size={14} />}
          </Button>
        )}
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          onClick={onRefresh}
          title="Refresh"
          aria-label="Refresh directory listing"
        >
          <RefreshCw size={14} />
        </Button>
      </div>
      <div className="files-body">
        <div
          className={`files-sidebar${sidebarCollapsed ? " is-collapsed" : ""}`}
          style={{
            width: sidebarCollapsed ? 0 : treeWidth,
            flex: `0 0 ${sidebarCollapsed ? 0 : treeWidth}px`,
          }}
        >
          <label className="files-filter">
            <Search size={14} aria-hidden="true" />
            <span className="sr-only">Filter files</span>
            <input
              ref={filterRef}
              type="search"
              value={filter}
              placeholder="Filter this folder"
              onChange={(event) => setFilter(event.currentTarget.value)}
            />
            {filter && (
              <button type="button" aria-label="Clear file filter" onClick={() => setFilter("")}>
                ×
              </button>
            )}
          </label>
          <div className="files-navigator-controls">
            <button
              type="button"
              className={showHidden ? "is-active" : ""}
              aria-pressed={showHidden}
              onClick={() => setShowHidden((value) => !value)}
            >
              <SlidersHorizontal size={12} />
              {showHidden ? "Hidden shown" : "Hide dotfiles"}
            </button>
            <button
              type="button"
              title="Change sort order"
              onClick={() =>
                setSortMode((value) =>
                  value === "name" ? "modified" : value === "modified" ? "size" : "name",
                )
              }
            >
              {sortMode === "name" ? (
                <ArrowDownAZ size={12} />
              ) : sortMode === "modified" ? (
                <ArrowDownWideNarrow size={12} />
              ) : (
                <ArrowUpNarrowWide size={12} />
              )}
              {sortMode === "name" ? "Name" : sortMode === "modified" ? "Modified" : "Size"}
            </button>
          </div>
          {recentPaths.length > 0 && (
            <div className="files-recent-strip" role="group" aria-label="Recent files">
              <span>Recent</span>
              {recentPaths.slice(0, 3).map((path) => (
                <button
                  type="button"
                  key={path}
                  title={path}
                  aria-label={`Open recent file ${path}`}
                  data-name={path.split("/").pop() ?? path}
                  onClick={() =>
                    openEntry({
                      name: path.split("/").pop() ?? path,
                      path,
                      isDir: false,
                      size: 0,
                      origin: "local",
                    })
                  }
                />
              ))}
            </div>
          )}
          <div
            className="files-tree aurora-scrollbar"
            role="listbox"
            aria-label="Directory entries"
            style={{ width: treeWidth }}
            tabIndex={0}
            aria-activedescendant={
              visibleEntries[focusedIndex]?.path ? `file-row-${pane.id}-${focusedIndex}` : undefined
            }
            onKeyDown={handleTreeKeyDown}
          >
            {listing.kind === "loading" ? (
              <div className="files-empty">
                <Loader2 size={16} className="files-spin" />
                <span>Loading...</span>
              </div>
            ) : listing.kind === "error" ? (
              <div className="files-empty operation-muted">{listing.message}</div>
            ) : visibleEntries.length === 0 ? (
              <div className="files-list-empty operation-muted">
                {entries.length === 0 ? "This folder is empty" : `No matches for “${filter}”`}
              </div>
            ) : (
              visibleEntries.map((entry) => (
                <button
                  key={entry.path}
                  id={`file-row-${pane.id}-${visibleEntries.indexOf(entry)}`}
                  type="button"
                  role="option"
                  aria-selected={pane.selected?.path === entry.path}
                  className={`files-row${pane.selected?.path === entry.path ? " files-row-active" : ""}${visibleEntries[focusedIndex]?.path === entry.path ? " files-row-focused" : ""}`}
                  onFocus={() => setFocusedIndex(visibleEntries.indexOf(entry))}
                  onClick={() => openEntry(entry)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    setContextMenu({ kind: "entry", entry, x: event.clientX, y: event.clientY });
                  }}
                >
                  {!entry.isDir && (
                    <input
                      type="checkbox"
                      className="files-row-checkbox"
                      aria-label="Select for bulk indexing"
                      checked={isChecked(checked, entry.path)}
                      onClick={(event) => event.stopPropagation()}
                      onChange={() => onToggleChecked(entry.path)}
                    />
                  )}
                  <EntryIcon entry={entry} />
                  <span className="files-row-name">{entry.name}</span>
                  {!entry.isDir && indexedPaths.has(entry.path) && (
                    <span
                      className="files-row-indexed"
                      role="img"
                      title="Indexed"
                      aria-label="Indexed"
                    />
                  )}
                  {!entry.isDir && (
                    <span className="files-row-size">{formatBytes(entry.size)}</span>
                  )}
                </button>
              ))
            )}
            {isLeftPane && (
              <SftpTreeSection
                ref={sftpTreeRef}
                activeConnectionId={activeSftpConnectionId}
                activeProfile={activeSftpProfile}
                onOpenFile={onOpenSftpFile}
              />
            )}
          </div>
        </div>
        <div className="files-preview aurora-scrollbar">
          {!pane.selected ? (
            <div className="files-welcome">
              <span className="sr-only">Select a file</span>
              <div className="files-welcome-icon">
                <FileSearch size={20} />
              </div>
              <h3>Select a file to inspect</h3>
              <p>Preview, edit, or index content from this folder.</p>
              <div
                className="files-directory-summary"
                role="group"
                aria-label="Current folder summary"
              >
                <span>
                  <strong>{directoryStats.files}</strong> files
                </span>
                <span>
                  <strong>{directoryStats.folders}</strong> folders
                </span>
                <span>
                  <strong>{directoryStats.indexable}</strong> indexable
                </span>
                <span>
                  <strong>{formatBytes(directoryStats.size)}</strong> total
                </span>
              </div>
            </div>
          ) : pane.file.kind === "loading" ? (
            <div className="files-empty">
              <Loader2 size={16} className="files-spin" />
              <span>Loading...</span>
            </div>
          ) : pane.file.kind === "error" ? (
            <div className="files-empty operation-muted">{pane.file.message}</div>
          ) : pane.file.kind === "loaded" ? (
            <FilePreview
              selectedPath={pane.selected.path}
              absolutePath={
                pane.selected.origin === "sftp" || listing.kind !== "loaded"
                  ? null
                  : `${listing.value.root.replace(/\/$/, "")}/${pane.selected.path}`
              }
              modifiedUnix={pane.selected.modifiedUnix}
              file={pane.file.value}
              editing={pane.editing}
              draft={pane.draft}
              saving={pane.saving}
              indexState={indexState}
              canIndex={Boolean(client && config) && isIndexable(pane.selected.name)}
              canEdit={pane.selected.origin !== "sftp"}
              onEdit={() => onSetEditing(true)}
              onCancelEdit={onCancelEdit}
              onDraftChange={onDraftChange}
              onSave={onSave}
              onIndex={onIndex}
              sparkleOpen={pane.sparkleOpen}
              sparkleQuery={pane.sparkleQuery}
              proposal={pane.proposal}
              proposalState={pane.proposalState}
              proposalErrorMessage={pane.proposalErrorMessage}
              onSparkleToggle={onSparkleToggle}
              onSparkleQueryChange={onSparkleQueryChange}
              onSparkleSubmit={onSparkleSubmit}
              onProposalDeny={onProposalDeny}
              onProposalApprove={onProposalApprove}
            />
          ) : null}
        </div>
      </div>
      {contextMenu && (
        <FilesContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          label={
            contextMenu.kind === "entry" ? `${contextMenu.entry.name} actions` : "Folder actions"
          }
          items={
            contextMenu.kind === "entry"
              ? entryMenuItems(contextMenu.entry)
              : [
                  {
                    label: "Refresh folder",
                    icon: RefreshCw,
                    onSelect: onRefresh,
                  },
                  {
                    label: "Copy folder path",
                    icon: Copy,
                    separatorBefore: true,
                    disabled: !currentFolderPath,
                    onSelect: () => currentFolderPath && copyText(currentFolderPath),
                  },
                  {
                    label: "Show folder in Finder",
                    icon: FolderSearch,
                    disabled: !currentFolderPath,
                    onSelect: () => currentFolderPath && void revealItemInDir(currentFolderPath),
                  },
                ]
          }
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}

function FilePreview({
  selectedPath,
  absolutePath,
  modifiedUnix,
  file,
  editing,
  draft,
  saving,
  indexState,
  canIndex,
  canEdit,
  onEdit,
  onCancelEdit,
  onDraftChange,
  onSave,
  onIndex,
  sparkleOpen,
  sparkleQuery,
  proposal,
  proposalState,
  proposalErrorMessage,
  onSparkleToggle,
  onSparkleQueryChange,
  onSparkleSubmit,
  onProposalDeny,
  onProposalApprove,
}: {
  selectedPath: string;
  absolutePath: string | null;
  modifiedUnix?: number | null;
  file: FileContents;
  editing: boolean;
  draft: string;
  saving: boolean;
  indexState: IndexState;
  canIndex: boolean;
  /** SFTP is v1 read-only browsing: both the manual Edit button and the
   * "Edit with the model" sparkle button are hard-disabled (not rendered)
   * for any file whose pane resolves to an SFTP-origin entry. */
  canEdit: boolean;
  onEdit: () => void;
  onCancelEdit: () => void;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onIndex: () => void;
  sparkleOpen: boolean;
  sparkleQuery: string;
  proposal: FilesPane["proposal"];
  proposalState: FilesPane["proposalState"];
  proposalErrorMessage: string | null;
  onSparkleToggle: () => void;
  onSparkleQueryChange: (value: string) => void;
  onSparkleSubmit: () => void;
  onProposalDeny: () => void;
  onProposalApprove: () => void;
}) {
  const name = selectedPath.split("/").pop() ?? selectedPath;
  const [wrapLines, setWrapLines] = useState(true);
  const [markdownPreview, setMarkdownPreview] = useState(true);
  const [copied, setCopied] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [symbolsOpen, setSymbolsOpen] = useState(false);
  const [minimapEnabled, setMinimapEnabled] = useState(
    () => localStorage.getItem("axon.files.minimap") !== "false",
  );
  const [language, setLanguage] = useState(() => languageForFile(name));
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const findInputRef = useRef<HTMLInputElement>(null);
  const lineCount = file.content.length === 0 ? 0 : file.content.split("\n").length;
  const previewTruncated = file.content.length > 750_000;
  const previewContent = previewTruncated ? file.content.slice(0, 750_000) : file.content;
  const previewLineCount = previewContent.length === 0 ? 0 : previewContent.split("\n").length;
  const symbols = useMemo(() => extractFileSymbols(file.content), [file.content]);
  const findMatches = useMemo(
    () => countTextMatches(editing ? draft : file.content, findQuery),
    [draft, editing, file.content, findQuery],
  );
  const manualDiff = useMemo(
    () => (editing ? computeLineDiff(file.content, draft) : []),
    [draft, editing, file.content],
  );
  const changedLines = manualDiff.filter((line) => line.kind !== "same").length;

  useEffect(() => {
    setLanguage(languageForFile(name));
    setFindQuery("");
    setFindOpen(false);
    setSymbolsOpen(false);
  }, [name]);
  useEffect(
    () => localStorage.setItem("axon.files.minimap", String(minimapEnabled)),
    [minimapEnabled],
  );
  useEffect(() => {
    if (findOpen) findInputRef.current?.focus();
  }, [findOpen]);

  function copyPath() {
    void navigator.clipboard.writeText(selectedPath).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: The preview surface exposes a contextual right-click menu.
    <div
      className="files-preview-inner"
      onContextMenu={(event) => {
        event.preventDefault();
        setContextMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <div className="files-preview-header">
        <div className="files-preview-identity" title={selectedPath}>
          <span className="files-preview-file-icon">
            <FileText size={15} />
          </span>
          <span>
            <strong className="files-preview-name">{name}</strong>
            <small className="files-preview-meta">
              {formatBytes(file.size)} · modified {formatModified(modifiedUnix)}
            </small>
          </span>
        </div>
        <div className="files-preview-actions">
          {editing ? (
            <>
              <Button variant="ghost" size="sm" type="button" onClick={onCancelEdit}>
                Cancel
              </Button>
              <Button variant="aurora" size="sm" type="button" onClick={onSave} disabled={saving}>
                <Save size={13} />
                {saving ? "Saving..." : "Save"}
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                title={copied ? "Copied" : "Copy path"}
                aria-label={copied ? "Path copied" : "Copy path"}
                onClick={copyPath}
              >
                <Copy size={14} />
              </Button>
              {absolutePath && (
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  title="Show in Finder"
                  aria-label="Show in Finder"
                  onClick={() => void revealItemInDir(absolutePath)}
                >
                  <FolderSearch size={14} />
                </Button>
              )}
              {canEdit && (
                <Button variant="ghost" size="sm" type="button" onClick={onEdit}>
                  Edit
                </Button>
              )}
              {canEdit && (
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  title="Edit with the model"
                  aria-label="Edit with the model"
                  onClick={onSparkleToggle}
                >
                  <Sparkles size={14} />
                </Button>
              )}
              {canIndex && (
                <Button
                  variant="aurora"
                  size="sm"
                  type="button"
                  onClick={onIndex}
                  disabled={indexState.kind === "running"}
                >
                  <Upload size={13} />
                  {indexState.kind === "running" ? "Indexing..." : "Index"}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
      {indexState.kind === "done" && (
        <div className={`files-index-status${indexState.ok ? "" : " files-index-status-error"}`}>
          {indexState.message}
        </div>
      )}
      {editing ? (
        <div className="files-edit-workspace">
          <div className="files-edit-status">
            <span className={changedLines ? "is-dirty" : ""}>
              <GitCompare size={12} />
              {changedLines ? `${changedLines} changed lines` : "No changes"}
            </span>
            <span>Tab inserts two spaces</span>
          </div>
          <HighlightedCodeEditor value={draft} language={language} onChange={onDraftChange} />
        </div>
      ) : file.dataUrl && file.mediaType ? (
        <div className="files-media-stage">
          {file.mediaType.startsWith("image/") ? (
            <img src={file.dataUrl} alt={`Preview of ${name}`} />
          ) : file.mediaType === "application/pdf" ? (
            <iframe src={file.dataUrl} title={`Preview of ${name}`} />
          ) : file.mediaType.startsWith("audio/") ? (
            // biome-ignore lint/a11y/useMediaCaption: Local audio previews do not have a separate caption track.
            <audio src={file.dataUrl} controls aria-label={`Preview of ${name}`} />
          ) : file.mediaType.startsWith("video/") ? (
            // biome-ignore lint/a11y/useMediaCaption: Local video previews do not have a separate caption track.
            <video src={file.dataUrl} controls aria-label={`Preview of ${name}`} />
          ) : null}
        </div>
      ) : file.content.length === 0 && file.size > 0 ? (
        <div className="files-binary-stage">
          <FileArchive size={28} />
          <strong>Preview unavailable</strong>
          <p>
            This {formatBytes(file.size)} file does not expose readable text or supported media.
          </p>
          <div>
            <span>{languageForFile(name)}</span>
            <span>binary or packaged content</span>
          </div>
          {absolutePath ? (
            <Button
              variant="ghost"
              size="sm"
              type="button"
              onClick={() => void revealItemInDir(absolutePath)}
            >
              Show in Finder
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="files-document">
          <div className="files-document-toolbar">
            <span>
              {lineCount} {lineCount === 1 ? "line" : "lines"}
            </span>
            <span>{formatBytes(file.size)}</span>
            <label className="files-language-picker">
              <span className="sr-only">Syntax language</span>
              <Braces size={12} />
              <select value={language} onChange={(event) => setLanguage(event.target.value)}>
                {[
                  language,
                  "text",
                  "html",
                  "css",
                  "javascript",
                  "typescript",
                  "tsx",
                  "json",
                  "markdown",
                  "bash",
                  "rust",
                  "python",
                  "sql",
                  "yaml",
                  "toml",
                  "xml",
                  "diff",
                ]
                  .filter((value, index, values) => values.indexOf(value) === index)
                  .map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
              </select>
            </label>
            <button
              type="button"
              className={findOpen ? "is-active" : ""}
              aria-pressed={findOpen}
              onClick={() => setFindOpen((value) => !value)}
            >
              <Search size={13} /> Find
            </button>
            {symbols.length > 0 && (
              <button
                type="button"
                className={symbolsOpen ? "is-active" : ""}
                aria-pressed={symbolsOpen}
                onClick={() => setSymbolsOpen((value) => !value)}
              >
                <Braces size={13} /> {symbols.length} symbols
              </button>
            )}
            {lineCount > 200 && (
              <button
                type="button"
                className={minimapEnabled ? "is-active" : ""}
                aria-pressed={minimapEnabled}
                onClick={() => setMinimapEnabled((value) => !value)}
              >
                <PanelRight size={13} /> Minimap
              </button>
            )}
            {isMarkdownLike(name) && (
              <button
                type="button"
                className={markdownPreview ? "is-active" : ""}
                aria-pressed={markdownPreview}
                onClick={() => setMarkdownPreview((value) => !value)}
              >
                {markdownPreview ? "Preview" : "Raw"}
              </button>
            )}
            <button
              type="button"
              className={wrapLines ? "is-active" : ""}
              aria-pressed={wrapLines}
              onClick={() => setWrapLines((value) => !value)}
            >
              <WrapText size={13} />
              Wrap
            </button>
          </div>
          {findOpen && (
            <div className="files-findbar">
              <Search size={13} />
              <input
                ref={findInputRef}
                type="search"
                value={findQuery}
                onChange={(event) => setFindQuery(event.currentTarget.value)}
                placeholder="Find in file"
                aria-label="Find in file"
              />
              <span>{findQuery ? `${findMatches} matches` : "Type to search"}</span>
              <button
                type="button"
                onClick={() => {
                  setFindOpen(false);
                  setFindQuery("");
                }}
              >
                ×
              </button>
            </div>
          )}
          {previewTruncated ? (
            <div className="files-large-notice" role="status">
              Large file · showing the first {formatBytes(previewContent.length)} of{" "}
              {formatBytes(file.content.length)}. Editing and saving still use the complete file.
            </div>
          ) : null}
          {symbolsOpen && symbols.length > 0 && (
            <nav className="files-symbol-strip" aria-label="File symbols">
              {symbols.map((symbol) => (
                <button
                  type="button"
                  key={`${symbol.line}:${symbol.label}`}
                  title={`Line ${symbol.line}`}
                  onClick={() => {
                    const line = document.querySelector(
                      `.files-shiki .line:nth-child(${symbol.line})`,
                    );
                    line?.scrollIntoView({ block: "center", behavior: "smooth" });
                  }}
                >
                  <span>{symbol.kind}</span>
                  {symbol.label}
                  <em>{symbol.line}</em>
                </button>
              ))}
            </nav>
          )}
          {isMarkdownLike(name) && markdownPreview ? (
            <div className="files-markdown-preview">
              <span className="sr-only" aria-hidden="true">
                {file.content}
              </span>
              <MarkdownBodyInner>{file.content}</MarkdownBodyInner>
            </div>
          ) : (
            <div className={`files-code-surface${wrapLines ? " is-wrapped" : ""}`}>
              <pre className="files-line-numbers" aria-hidden="true">
                {Array.from(
                  { length: Math.max(previewLineCount, 1) },
                  (_, index) => index + 1,
                ).join("\n")}
              </pre>
              <div className="files-preview-text files-preview-code">
                <HighlightedFileCode
                  code={previewContent}
                  language={language}
                  searchQuery={findQuery}
                />
              </div>
              {lineCount > 200 && minimapEnabled ? (
                <pre className="files-minimap" aria-hidden="true">
                  {previewContent}
                </pre>
              ) : null}
            </div>
          )}
        </div>
      )}
      <AiEditPanel
        sparkleOpen={sparkleOpen}
        sparkleQuery={sparkleQuery}
        proposal={proposal}
        proposalState={proposalState}
        proposalErrorMessage={proposalErrorMessage}
        onSparkleToggle={onSparkleToggle}
        onSparkleQueryChange={onSparkleQueryChange}
        onSparkleSubmit={onSparkleSubmit}
        onProposalDeny={onProposalDeny}
        onProposalApprove={onProposalApprove}
      />
      {contextMenu && (
        <FilesContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          label={`${name} actions`}
          items={[
            {
              label: "Copy contents",
              icon: Copy,
              onSelect: () => void navigator.clipboard.writeText(file.content),
            },
            {
              label: "Ask about selection",
              icon: MessageCircleQuestion,
              onSelect: () => {
                const selection = window.getSelection()?.toString().trim() || file.content;
                if (!sparkleOpen) onSparkleToggle();
                onSparkleQueryChange(selectionPrompt("ask", selection));
              },
            },
            {
              label: "Explain selection",
              icon: Sparkles,
              onSelect: () => {
                const selection = window.getSelection()?.toString().trim() || file.content;
                if (!sparkleOpen) onSparkleToggle();
                onSparkleQueryChange(selectionPrompt("explain", selection));
              },
            },
            {
              label: "Copy path",
              icon: FileText,
              onSelect: copyPath,
            },
            ...(absolutePath
              ? [
                  {
                    label: "Show in Finder",
                    icon: FolderSearch,
                    onSelect: () => void revealItemInDir(absolutePath),
                  },
                ]
              : []),
            ...(canEdit
              ? [
                  {
                    label: "Edit file",
                    icon: FileText,
                    separatorBefore: true,
                    onSelect: onEdit,
                  },
                  {
                    label: "Edit with Axon",
                    icon: Sparkles,
                    onSelect: onSparkleToggle,
                  },
                ]
              : []),
            ...(canIndex
              ? [
                  {
                    label: indexState.kind === "running" ? "Indexing…" : "Index file",
                    icon: Upload,
                    separatorBefore: !canEdit,
                    disabled: indexState.kind === "running",
                    onSelect: onIndex,
                  },
                ]
              : []),
          ]}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
