import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  ArrowDownAZ,
  ArrowDownWideNarrow,
  ArrowUpNarrowWide,
  ChevronRight,
  Columns2,
  Copy,
  FileCheck2,
  FileText,
  FolderSearch,
  Loader2,
  PanelLeftClose,
  PanelLeftOpen,
  Plug,
  PlugZap,
  RefreshCw,
  Search,
  SlidersHorizontal,
  Square,
  Upload,
} from "lucide-react";
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import {
  breadcrumbSegments,
  type FileEntry,
  formatBytes,
  isChecked,
  isIndexable,
} from "@/lib/filesModel";
import { EntryIcon } from "./EntryIcon";
import type { FilesContextMenuItem } from "./FilesContextMenu";
import { FilesPaneContextMenu, type FilesPaneContextMenuState } from "./FilesPaneContextMenu";
import { FilesPanePreview } from "./FilesPanePreview";
import { type FilesPaneViewProps, fileDirectoryStats, visibleFileEntries } from "./filesPaneTypes";
import { SftpTreeSection } from "./SftpTreeSection";

export type { IndexState } from "./filesPaneTypes";

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
}: FilesPaneViewProps) {
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
  const [contextMenu, setContextMenu] = useState<FilesPaneContextMenuState>(null);
  const visibleEntries = useMemo(() => {
    return visibleFileEntries(entries, filter, showHidden, sortMode);
  }, [entries, filter, showHidden, sortMode]);
  const directoryStats = useMemo(() => {
    return fileDirectoryStats(entries);
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
          <FilesPanePreview
            {...{
              pane,
              listing,
              indexState,
              client,
              config,
              directoryStats,
              onSetEditing,
              onCancelEdit,
              onDraftChange,
              onSave,
              onIndex,
              onSparkleToggle,
              onSparkleQueryChange,
              onSparkleSubmit,
              onProposalDeny,
              onProposalApprove,
            }}
          />
        </div>
      </div>
      {contextMenu && (
        <FilesPaneContextMenu
          state={contextMenu}
          entryItems={entryMenuItems}
          folderItems={[
            { label: "Refresh folder", icon: RefreshCw, onSelect: onRefresh },
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
          ]}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
