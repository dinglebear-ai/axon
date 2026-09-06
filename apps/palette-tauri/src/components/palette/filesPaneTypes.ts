import type { Ref } from "react";
import {
  type DirListing,
  type FileEntry,
  type FilesPane,
  isIndexable,
  type LoadState,
} from "@/lib/filesModel";
import type { SftpConnectionProfile, SftpEntry } from "@/lib/sftpModel";
import type { SftpTreeSectionHandle } from "./SftpTreeSection";

export type IndexState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; ok: boolean; message: string };

export interface FilesPaneViewProps {
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
}

export function visibleFileEntries(
  entries: FileEntry[],
  filter: string,
  showHidden: boolean,
  sortMode: "name" | "modified" | "size",
) {
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
}

export function fileDirectoryStats(entries: FileEntry[]) {
  const files = entries.filter((entry) => !entry.isDir);
  return {
    folders: entries.length - files.length,
    files: files.length,
    indexable: files.filter((entry) => isIndexable(entry.name)).length,
    size: files.reduce((total, entry) => total + entry.size, 0),
  };
}
