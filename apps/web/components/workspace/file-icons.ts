import {
  BookOpen,
  Bot,
  Clock,
  File,
  FileCode,
  FileJson,
  FileText,
  Folder,
  FolderOpen,
  HardDrive,
  Star,
  Tag,
} from 'lucide-react'
import type { FileEntry } from './file-tree'

/** Map a filename to an appropriate lucide icon component based on extension. */
export function fileIcon(name: string) {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  if (['md', 'mdx', 'txt'].includes(ext)) return FileText
  if (['ts', 'tsx', 'js', 'jsx', 'rs', 'go', 'py', 'sh'].includes(ext)) return FileCode
  if (['json', 'jsonl', 'toml', 'yaml', 'yml'].includes(ext)) return FileJson
  return File
}

/** Map a directory's iconType to an appropriate lucide icon component. */
export function dirIcon(iconType: FileEntry['iconType'] | undefined, expanded?: boolean) {
  switch (iconType) {
    case 'workspace':
      return HardDrive
    case 'docs':
      return BookOpen
    case 'favorites':
      return Star
    case 'recents':
      return Clock
    case 'tags':
      return Tag
    case 'claude':
      return Bot
    default:
      return expanded ? FolderOpen : Folder
  }
}
