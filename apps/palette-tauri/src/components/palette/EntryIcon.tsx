import { FileArchive, FileCode, FileCog, File as FileIcon, FileText, Folder } from "lucide-react";

import { fileKind, type FileEntry } from "@/lib/filesModel";

export function EntryIcon({ entry }: { entry: FileEntry }) {
  if (entry.isDir) return <Folder size={15} className="files-icon-dir" aria-hidden="true" />;
  const kind = fileKind(entry.name);
  switch (kind) {
    case "doc":
      return <FileText size={15} className="files-icon-doc" aria-hidden="true" />;
    case "code":
      return <FileCode size={15} className="files-icon-code" aria-hidden="true" />;
    case "config":
      return <FileCog size={15} className="files-icon-config" aria-hidden="true" />;
    case "archive":
      return <FileArchive size={15} className="files-icon-muted" aria-hidden="true" />;
    default:
      return <FileIcon size={15} className="files-icon-muted" aria-hidden="true" />;
  }
}
