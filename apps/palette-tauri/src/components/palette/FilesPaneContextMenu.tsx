import type { FileEntry } from "@/lib/filesModel";
import { FilesContextMenu, type FilesContextMenuItem } from "./FilesContextMenu";

export type FilesPaneContextMenuState =
  | { kind: "entry"; entry: FileEntry; x: number; y: number }
  | { kind: "folder"; x: number; y: number }
  | null;

interface FilesPaneContextMenuProps {
  state: Exclude<FilesPaneContextMenuState, null>;
  entryItems: (entry: FileEntry) => FilesContextMenuItem[];
  folderItems: FilesContextMenuItem[];
  onClose: () => void;
}

export function FilesPaneContextMenu({
  state,
  entryItems,
  folderItems,
  onClose,
}: FilesPaneContextMenuProps) {
  return (
    <FilesContextMenu
      x={state.x}
      y={state.y}
      label={state.kind === "entry" ? `${state.entry.name} actions` : "Folder actions"}
      items={state.kind === "entry" ? entryItems(state.entry) : folderItems}
      onClose={onClose}
    />
  );
}
