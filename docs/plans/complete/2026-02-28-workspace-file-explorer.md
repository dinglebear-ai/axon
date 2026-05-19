# Workspace File Explorer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a first-class file explorer page (`/workspace`) to the Axon web UI that lets users browse and view files within `AXON_WORKSPACE` — the directory Claude operates in during Pulse chat sessions.

**Architecture:** A new Next.js page at `/workspace` with a collapsible directory tree sidebar (left) and a file content viewer (right), backed by a secure `/api/workspace` API route that serves directory listings and file contents scoped strictly to `AXON_WORKSPACE`. Navigation entry point is a new icon added to the omnibox right toolbar (consistent with `/mcp`, `/agents`, `/settings`). Design follows the existing glassmorphic dark aesthetic (pink/blue accent palette, radial gradient backgrounds).

**Tech Stack:** Next.js 15 App Router, TypeScript strict, Tailwind CSS, lucide-react icons, existing `ContentViewer` component (Plate.js for markdown), `node:fs/promises` in API route, `node:path` for security validation, `shiki` or inline CSS classes for code syntax display.

---

## Pre-Flight Checklist

Before starting, verify:
```bash
cd /home/jmagar/workspace/axon_rust/apps/web
cat package.json | grep -E '"shiki|"@shikijs|"highlight'  # check if syntax highlighter exists
ls components/
ls app/mcp/page.tsx app/settings/page.tsx  # confirm existing page patterns
grep -n "Settings2\|Network\|FolderOpen\|Files" components/omnibox.tsx | head -20  # find nav icon location
```

---

## Task 1: Create the `/api/workspace` Route

**Purpose:** Server-side API that safely lists directories and reads files scoped to `AXON_WORKSPACE`. This is the security boundary — all path validation lives here.

**Files:**
- Create: `apps/web/app/api/workspace/route.ts`

**Step 1: Write the route file**

```typescript
// apps/web/app/api/workspace/route.ts
import { NextRequest, NextResponse } from 'next/server'
import { promises as fs } from 'node:fs'
import path from 'node:path'

// AXON_WORKSPACE inside the axon-web container is /workspace (bind-mounted from host)
const WORKSPACE_ROOT = process.env.AXON_WORKSPACE ?? '/workspace'

const TEXT_EXTENSIONS = new Set([
  '.md', '.mdx', '.txt', '.log', '.csv',
  '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs',
  '.rs', '.go', '.py', '.sh', '.bash', '.zsh',
  '.toml', '.yaml', '.yml', '.json', '.jsonl', '.json5',
  '.env', '.example', '.gitignore', '.dockerignore',
  '.css', '.scss', '.html', '.xml', '.svg',
  '.sql', '.graphql', '.gql',
  '.lock', '.sum', 'Makefile', 'Dockerfile',
])

const IGNORE_DIRS = new Set([
  '.git', '.cache', 'node_modules', 'target', '__pycache__',
  '.next', '.turbo', 'dist', 'build', '.venv', '.mypy_cache',
  '.pytest_cache', '.ruff_cache', '.tox', 'coverage',
])

/** Returns null if path is safe, error string if not */
function validatePath(raw: string): { safe: string } | { error: string } {
  // Normalize: strip leading slash, resolve against workspace root
  const relative = raw.replace(/^\/+/, '')
  const resolved = path.resolve(WORKSPACE_ROOT, relative)
  const workspaceNorm = path.resolve(WORKSPACE_ROOT)

  // Must be at or below WORKSPACE_ROOT (prevent path traversal)
  if (resolved !== workspaceNorm && !resolved.startsWith(workspaceNorm + path.sep)) {
    return { error: 'Path is outside workspace' }
  }
  return { safe: resolved }
}

export async function GET(req: NextRequest) {
  const { searchParams } = req.nextUrl
  const action = searchParams.get('action') ?? 'list'
  const rawPath = searchParams.get('path') ?? ''

  const validation = validatePath(rawPath)
  if ('error' in validation) {
    return NextResponse.json({ error: validation.error }, { status: 400 })
  }
  const safePath = validation.safe

  if (action === 'list') {
    try {
      const stat = await fs.stat(safePath)
      if (!stat.isDirectory()) {
        return NextResponse.json({ error: 'Not a directory' }, { status: 400 })
      }

      const entries = await fs.readdir(safePath, { withFileTypes: true })
      const items = entries
        .filter(e => !e.name.startsWith('.') || e.name === '.env.example')
        .filter(e => !e.isDirectory() || !IGNORE_DIRS.has(e.name))
        .map(e => ({
          name: e.name,
          type: e.isDirectory() ? 'directory' : 'file',
          path: path.relative(WORKSPACE_ROOT, path.join(safePath, e.name)),
        }))
        .sort((a, b) => {
          // Dirs first, then files, then alphabetical within each group
          if (a.type !== b.type) return a.type === 'directory' ? -1 : 1
          return a.name.localeCompare(b.name)
        })

      return NextResponse.json({
        path: path.relative(WORKSPACE_ROOT, safePath) || '.',
        items,
      })
    } catch (err) {
      return NextResponse.json({ error: 'Directory not found' }, { status: 404 })
    }
  }

  if (action === 'read') {
    try {
      const stat = await fs.stat(safePath)
      if (stat.isDirectory()) {
        return NextResponse.json({ error: 'Is a directory' }, { status: 400 })
      }
      if (stat.size > 1_000_000) {
        return NextResponse.json({ error: 'File too large (>1MB)' }, { status: 413 })
      }

      const ext = path.extname(safePath).toLowerCase()
      const basename = path.basename(safePath)
      const isText = TEXT_EXTENSIONS.has(ext) || TEXT_EXTENSIONS.has(basename)

      if (!isText) {
        return NextResponse.json({
          type: 'binary',
          name: basename,
          size: stat.size,
          modified: stat.mtime.toISOString(),
        })
      }

      const content = await fs.readFile(safePath, 'utf8')
      return NextResponse.json({
        type: 'text',
        name: basename,
        ext: ext || '',
        size: stat.size,
        modified: stat.mtime.toISOString(),
        content,
      })
    } catch (err) {
      return NextResponse.json({ error: 'File not found' }, { status: 404 })
    }
  }

  return NextResponse.json({ error: 'Unknown action' }, { status: 400 })
}
```

**Step 2: Verify the route compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
npx tsc --noEmit 2>&1 | grep -E "workspace/route"
```

Expected: No errors for the new file.

**Step 3: Test the route manually (requires running dev server)**

```bash
# Start the dev server if not already running
# cd apps/web && pnpm dev

# Test directory listing:
curl "http://localhost:49010/api/workspace?action=list&path=" | jq .

# Test path traversal is blocked:
curl "http://localhost:49010/api/workspace?action=list&path=../../etc" | jq .
# Expected: { "error": "Directory not found" } or 400

# Test file read:
curl "http://localhost:49010/api/workspace?action=read&path=README.md" | jq .keys
```

**Step 4: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/app/api/workspace/route.ts
git commit -m "feat(web): add /api/workspace route for AXON_WORKSPACE file browsing"
```

---

## Task 2: Create the FileTree Component

**Purpose:** Recursive, collapsible directory tree rendered in the left sidebar panel. Single-level lazy loading — only loads children when a directory is expanded.

**Files:**
- Create: `apps/web/components/workspace/file-tree.tsx`

**Step 1: Create the component directory and file**

```bash
mkdir -p /home/jmagar/workspace/axon_rust/apps/web/components/workspace
```

**Step 2: Write the component**

```typescript
// apps/web/components/workspace/file-tree.tsx
'use client'

import { useState, useCallback } from 'react'
import {
  ChevronRight, ChevronDown, Folder, FolderOpen,
  FileText, FileCode, FileJson, File,
} from 'lucide-react'

export interface FileEntry {
  name: string
  type: 'file' | 'directory'
  path: string // relative to workspace root
}

interface FileTreeProps {
  entries: FileEntry[]
  selectedPath: string | null
  onSelect: (entry: FileEntry) => void
  depth?: number
}

function fileIcon(name: string) {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  if (['md', 'mdx', 'txt'].includes(ext)) return FileText
  if (['ts', 'tsx', 'js', 'jsx', 'rs', 'go', 'py', 'sh'].includes(ext)) return FileCode
  if (['json', 'jsonl', 'toml', 'yaml', 'yml'].includes(ext)) return FileJson
  return File
}

function TreeNode({
  entry,
  selectedPath,
  onSelect,
  depth,
}: {
  entry: FileEntry
  selectedPath: string | null
  onSelect: (e: FileEntry) => void
  depth: number
}) {
  const [expanded, setExpanded] = useState(false)
  const [children, setChildren] = useState<FileEntry[] | null>(null)
  const [loading, setLoading] = useState(false)
  const isSelected = selectedPath === entry.path

  const toggle = useCallback(async () => {
    if (entry.type !== 'directory') {
      onSelect(entry)
      return
    }
    if (!expanded && children === null) {
      setLoading(true)
      try {
        const res = await fetch(`/api/workspace?action=list&path=${encodeURIComponent(entry.path)}`)
        const data = await res.json()
        setChildren(data.items ?? [])
      } catch {
        setChildren([])
      } finally {
        setLoading(false)
      }
    }
    setExpanded(e => !e)
  }, [entry, expanded, children, onSelect])

  const indent = depth * 12
  const IconComp = entry.type === 'directory'
    ? (expanded ? FolderOpen : Folder)
    : fileIcon(entry.name)

  return (
    <div>
      <button
        onClick={toggle}
        className={[
          'flex w-full items-center gap-1.5 rounded px-2 py-[3px] text-left',
          'text-xs font-mono transition-colors duration-150',
          isSelected
            ? 'bg-[rgba(255,135,175,0.12)] text-[rgba(255,135,175,0.95)]'
            : 'text-[rgba(200,210,230,0.65)] hover:bg-[rgba(255,255,255,0.05)] hover:text-[rgba(200,210,230,0.9)]',
        ].join(' ')}
        style={{ paddingLeft: `${8 + indent}px` }}
      >
        {entry.type === 'directory' ? (
          <span className="shrink-0 text-[rgba(175,215,255,0.5)]">
            {loading
              ? <span className="inline-block size-3 animate-spin rounded-full border border-current border-t-transparent" />
              : expanded
                ? <ChevronDown className="size-3" />
                : <ChevronRight className="size-3" />}
          </span>
        ) : (
          <span className="size-3 shrink-0" />
        )}
        <IconComp className={[
          'size-3 shrink-0',
          entry.type === 'directory'
            ? 'text-[rgba(175,215,255,0.6)]'
            : 'text-[rgba(200,210,230,0.45)]',
        ].join(' ')} />
        <span className="truncate">{entry.name}</span>
      </button>

      {expanded && children && children.length > 0 && (
        <div>
          <FileTree
            entries={children}
            selectedPath={selectedPath}
            onSelect={onSelect}
            depth={depth + 1}
          />
        </div>
      )}
      {expanded && children && children.length === 0 && (
        <div
          className="py-0.5 text-[10px] text-[rgba(200,210,230,0.25)] italic"
          style={{ paddingLeft: `${8 + indent + 20}px` }}
        >
          empty
        </div>
      )}
    </div>
  )
}

export function FileTree({ entries, selectedPath, onSelect, depth = 0 }: FileTreeProps) {
  return (
    <div className="select-none">
      {entries.map(entry => (
        <TreeNode
          key={entry.path}
          entry={entry}
          selectedPath={selectedPath}
          onSelect={onSelect}
          depth={depth}
        />
      ))}
    </div>
  )
}
```

**Step 3: Verify it compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
npx tsc --noEmit 2>&1 | grep "workspace/file-tree"
```

Expected: No errors.

**Step 4: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/components/workspace/file-tree.tsx
git commit -m "feat(web): add FileTree component for workspace explorer sidebar"
```

---

## Task 3: Create the CodeViewer Component

**Purpose:** Display plain text / code files with a monospace font, horizontal scrolling, line numbers, and a copy button. Used for non-markdown text files. (Markdown files will use the existing `ContentViewer` component with Plate.js.)

**Files:**
- Create: `apps/web/components/workspace/code-viewer.tsx`

**Step 1: Check what syntax highlighting is available**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
cat package.json | grep -E "shiki|prism|highlight"
```

If nothing is found, we'll use plain styled `<pre>` — good enough and zero deps.

**Step 2: Write the component**

```typescript
// apps/web/components/workspace/code-viewer.tsx
'use client'

import { useState, useCallback } from 'react'
import { Copy, Check } from 'lucide-react'

interface CodeViewerProps {
  content: string
  language?: string
  fileName?: string
}

export function CodeViewer({ content, language, fileName }: CodeViewerProps) {
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(content).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }, [content])

  const lines = content.split('\n')

  return (
    <div className="relative flex h-full flex-col overflow-hidden rounded-lg border border-[rgba(175,215,255,0.08)]">
      {/* Toolbar */}
      {(fileName || language) && (
        <div className="flex items-center justify-between border-b border-[rgba(175,215,255,0.08)] bg-[rgba(4,10,20,0.8)] px-4 py-2">
          <span className="font-mono text-[11px] text-[rgba(175,215,255,0.5)]">
            {fileName ?? language}
          </span>
          <button
            onClick={handleCopy}
            className="flex items-center gap-1.5 rounded px-2 py-1 text-[11px] text-[rgba(175,215,255,0.5)] transition-colors hover:bg-[rgba(175,215,255,0.08)] hover:text-[rgba(175,215,255,0.9)]"
          >
            {copied
              ? <><Check className="size-3" /> Copied</>
              : <><Copy className="size-3" /> Copy</>}
          </button>
        </div>
      )}

      {/* Code body */}
      <div className="flex-1 overflow-auto bg-[rgba(2,4,11,0.6)]">
        <table className="w-full border-collapse font-mono text-xs">
          <tbody>
            {lines.map((line, i) => (
              <tr key={i} className="hover:bg-[rgba(255,255,255,0.02)]">
                <td
                  className="w-10 select-none border-r border-[rgba(255,255,255,0.04)] pr-3 text-right text-[rgba(200,210,230,0.2)]"
                  style={{ minWidth: '2.5rem', paddingLeft: '0.5rem' }}
                >
                  {i + 1}
                </td>
                <td className="whitespace-pre pl-4 text-[rgba(200,220,245,0.8)]">
                  {line || ' '}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
```

**Step 3: Verify it compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
npx tsc --noEmit 2>&1 | grep "workspace/code-viewer"
```

**Step 4: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/components/workspace/code-viewer.tsx
git commit -m "feat(web): add CodeViewer component with line numbers and copy button"
```

---

## Task 4: Create the Workspace Page

**Purpose:** The main `/workspace` page with glassmorphic design matching existing pages (`/mcp`, `/settings`). Left sidebar: collapsible directory tree. Right pane: file content viewer with breadcrumb, metadata, and "Open in Pulse" action.

**Files:**
- Create: `apps/web/app/workspace/page.tsx`

**Step 1: Study existing page patterns**

```bash
# Check the MCP page header pattern for the exact gradient values
grep -A5 "radial-gradient" /home/jmagar/workspace/axon_rust/apps/web/app/mcp/page.tsx | head -20

# Check ContentViewer import path
grep -rn "ContentViewer\|content-viewer" /home/jmagar/workspace/axon_rust/apps/web/components/ | head -10

# Check if ContentViewer accepts a `content` string directly
head -40 /home/jmagar/workspace/axon_rust/apps/web/components/content-viewer.tsx
```

**Step 2: Write the workspace page**

```typescript
// apps/web/app/workspace/page.tsx
'use client'

import { useState, useEffect, useCallback } from 'react'
import Link from 'next/link'
import {
  FolderOpen, ArrowLeft, ChevronRight, FileText,
  Clock, HardDrive, ExternalLink, AlertCircle,
} from 'lucide-react'
import { FileTree, type FileEntry } from '@/components/workspace/file-tree'
import { CodeViewer } from '@/components/workspace/code-viewer'

// Dynamically import ContentViewer to avoid SSR issues with Plate.js
import dynamic from 'next/dynamic'
const ContentViewer = dynamic(
  () => import('@/components/content-viewer').then(m => ({ default: m.ContentViewer })),
  { ssr: false, loading: () => <div className="animate-pulse h-4 bg-[rgba(255,255,255,0.05)] rounded" /> }
)

interface DirListing {
  path: string
  items: FileEntry[]
}

interface FileData {
  type: 'text' | 'binary'
  name: string
  ext?: string
  size: number
  modified: string
  content?: string
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString()
}

function Breadcrumb({ filePath, onNavigate }: { filePath: string | null; onNavigate: (p: string) => void }) {
  if (!filePath) return <span className="text-[rgba(175,215,255,0.3)] text-xs font-mono">/workspace</span>
  const parts = filePath.split('/').filter(Boolean)
  return (
    <div className="flex items-center gap-1 font-mono text-xs overflow-x-auto">
      <button
        onClick={() => onNavigate('')}
        className="text-[rgba(175,215,255,0.5)] hover:text-[rgba(175,215,255,0.9)] transition-colors shrink-0"
      >
        workspace
      </button>
      {parts.map((part, i) => {
        const partPath = parts.slice(0, i + 1).join('/')
        const isLast = i === parts.length - 1
        return (
          <span key={partPath} className="flex items-center gap-1 shrink-0">
            <ChevronRight className="size-3 text-[rgba(175,215,255,0.2)]" />
            {isLast ? (
              <span className="text-[rgba(255,135,175,0.8)]">{part}</span>
            ) : (
              <button
                onClick={() => onNavigate(partPath)}
                className="text-[rgba(175,215,255,0.5)] hover:text-[rgba(175,215,255,0.9)] transition-colors"
              >
                {part}
              </button>
            )}
          </span>
        )
      })}
    </div>
  )
}

export default function WorkspacePage() {
  const [rootEntries, setRootEntries] = useState<FileEntry[]>([])
  const [selectedFile, setSelectedFile] = useState<FileEntry | null>(null)
  const [fileData, setFileData] = useState<FileData | null>(null)
  const [loadingFile, setLoadingFile] = useState(false)
  const [fileError, setFileError] = useState<string | null>(null)
  const [sidebarOpen, setSidebarOpen] = useState(true)

  // Load root directory on mount
  useEffect(() => {
    fetch('/api/workspace?action=list&path=')
      .then(r => r.json())
      .then((data: DirListing) => setRootEntries(data.items ?? []))
      .catch(() => setRootEntries([]))
  }, [])

  const handleSelectFile = useCallback(async (entry: FileEntry) => {
    if (entry.type === 'directory') return
    setSelectedFile(entry)
    setFileData(null)
    setFileError(null)
    setLoadingFile(true)
    try {
      const res = await fetch(`/api/workspace?action=read&path=${encodeURIComponent(entry.path)}`)
      if (!res.ok) {
        const err = await res.json()
        setFileError(err.error ?? 'Failed to load file')
        return
      }
      const data: FileData = await res.json()
      setFileData(data)
    } catch {
      setFileError('Network error loading file')
    } finally {
      setLoadingFile(false)
    }
  }, [])

  const isMarkdown = fileData?.ext === '.md' || fileData?.ext === '.mdx'

  const openInPulse = useCallback(() => {
    if (!selectedFile) return
    const msg = `@${selectedFile.path}`
    window.location.href = `/?pulse=${encodeURIComponent(msg)}`
  }, [selectedFile])

  return (
    <div
      className="flex h-screen flex-col overflow-hidden text-white"
      style={{
        background: [
          'radial-gradient(ellipse at 14% 10%, rgba(175,215,255,0.08), transparent 34%)',
          'radial-gradient(ellipse at 82% 16%, rgba(255,135,175,0.07), transparent 38%)',
          'linear-gradient(180deg,#02040b 0%,#030712 60%,#040a14 100%)',
        ].join(', '),
      }}
    >
      {/* Header */}
      <header
        className="flex items-center gap-3 border-b px-4"
        style={{
          minHeight: '52px',
          borderColor: 'rgba(255,135,175,0.1)',
          background: 'rgba(3,7,18,0.9)',
          backdropFilter: 'blur(16px)',
        }}
      >
        <Link
          href="/"
          className="flex size-7 shrink-0 items-center justify-center rounded border border-[rgba(175,215,255,0.12)] bg-[rgba(175,215,255,0.05)] text-[rgba(175,215,255,0.5)] transition-colors hover:bg-[rgba(175,215,255,0.1)] hover:text-[rgba(175,215,255,0.9)]"
        >
          <ArrowLeft className="size-3.5" />
        </Link>

        <div className="flex size-7 shrink-0 items-center justify-center rounded border border-[rgba(175,215,255,0.12)] bg-[rgba(175,215,255,0.05)]">
          <FolderOpen className="size-3.5 text-[rgba(175,215,255,0.7)]" />
        </div>

        <div className="flex-1 min-w-0">
          <h1 className="text-sm font-semibold text-[rgba(200,220,245,0.9)] leading-none">Workspace</h1>
          <p className="mt-0.5 text-[10px] text-[rgba(175,215,255,0.35)] font-mono">
            {process.env.NEXT_PUBLIC_WORKSPACE_LABEL ?? '/workspace'}
          </p>
        </div>

        {selectedFile && (
          <button
            onClick={openInPulse}
            className="flex items-center gap-1.5 rounded border border-[rgba(255,135,175,0.15)] bg-[rgba(255,135,175,0.07)] px-3 py-1.5 text-xs text-[rgba(255,135,175,0.8)] transition-colors hover:bg-[rgba(255,135,175,0.12)] hover:text-[rgba(255,135,175,1)]"
          >
            <ExternalLink className="size-3" />
            Open in Pulse
          </button>
        )}
      </header>

      {/* Body: sidebar + viewer */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar: File Tree */}
        <aside
          className={[
            'flex-shrink-0 flex flex-col border-r overflow-hidden transition-all duration-300',
            sidebarOpen ? 'w-64' : 'w-0',
          ].join(' ')}
          style={{ borderColor: 'rgba(175,215,255,0.06)' }}
        >
          <div className="flex items-center justify-between border-b border-[rgba(175,215,255,0.06)] px-3 py-2">
            <span className="text-[10px] font-semibold uppercase tracking-widest text-[rgba(175,215,255,0.3)]">
              Explorer
            </span>
          </div>
          <div className="flex-1 overflow-y-auto overflow-x-hidden py-1 px-1">
            {rootEntries.length === 0 ? (
              <div className="px-3 py-4 text-[11px] text-[rgba(200,210,230,0.25)] italic">
                Loading workspace…
              </div>
            ) : (
              <FileTree
                entries={rootEntries}
                selectedPath={selectedFile?.path ?? null}
                onSelect={handleSelectFile}
              />
            )}
          </div>
        </aside>

        {/* Main: File Viewer */}
        <main className="flex flex-1 flex-col overflow-hidden">
          {/* Breadcrumb bar */}
          <div
            className="flex items-center gap-3 border-b px-4 py-2"
            style={{ borderColor: 'rgba(175,215,255,0.06)', minHeight: '36px' }}
          >
            <button
              onClick={() => setSidebarOpen(v => !v)}
              className="shrink-0 text-[rgba(175,215,255,0.3)] hover:text-[rgba(175,215,255,0.7)] transition-colors"
              title={sidebarOpen ? 'Collapse sidebar' : 'Expand sidebar'}
            >
              <FolderOpen className="size-3.5" />
            </button>
            <Breadcrumb
              filePath={selectedFile?.path ?? null}
              onNavigate={() => {}} // tree handles navigation; breadcrumb is display-only
            />
            {fileData && (
              <div className="ml-auto flex items-center gap-3 shrink-0">
                <span className="flex items-center gap-1 text-[10px] text-[rgba(175,215,255,0.3)]">
                  <HardDrive className="size-3" />
                  {formatBytes(fileData.size)}
                </span>
                <span className="flex items-center gap-1 text-[10px] text-[rgba(175,215,255,0.3)]">
                  <Clock className="size-3" />
                  {formatDate(fileData.modified)}
                </span>
              </div>
            )}
          </div>

          {/* Content area */}
          <div className="flex-1 overflow-auto p-4">
            {!selectedFile && (
              <div className="flex h-full items-center justify-center">
                <div className="text-center">
                  <FolderOpen className="mx-auto mb-3 size-10 text-[rgba(175,215,255,0.12)]" />
                  <p className="text-sm text-[rgba(200,210,230,0.3)]">Select a file to view its contents</p>
                  <p className="mt-1 text-[11px] text-[rgba(175,215,255,0.2)]">
                    Browse the workspace tree on the left
                  </p>
                </div>
              </div>
            )}

            {loadingFile && (
              <div className="flex h-full items-center justify-center">
                <div className="size-6 animate-spin rounded-full border-2 border-[rgba(175,215,255,0.2)] border-t-[rgba(175,215,255,0.7)]" />
              </div>
            )}

            {fileError && (
              <div className="flex items-center gap-2 rounded-lg border border-[rgba(255,135,175,0.15)] bg-[rgba(255,135,175,0.05)] px-4 py-3 text-sm text-[rgba(255,135,175,0.8)]">
                <AlertCircle className="size-4 shrink-0" />
                {fileError}
              </div>
            )}

            {fileData?.type === 'binary' && (
              <div className="rounded-lg border border-[rgba(175,215,255,0.08)] bg-[rgba(4,10,20,0.6)] p-6 text-center">
                <FileText className="mx-auto mb-3 size-8 text-[rgba(175,215,255,0.2)]" />
                <p className="text-sm text-[rgba(200,210,230,0.5)]">Binary file — cannot display</p>
                <p className="mt-1 text-xs text-[rgba(175,215,255,0.3)]">{formatBytes(fileData.size)}</p>
              </div>
            )}

            {fileData?.type === 'text' && fileData.content !== undefined && (
              isMarkdown ? (
                <div className="prose-invert max-w-none">
                  <ContentViewer content={fileData.content} />
                </div>
              ) : (
                <div className="h-full">
                  <CodeViewer
                    content={fileData.content}
                    language={fileData.ext?.slice(1)}
                    fileName={fileData.name}
                  />
                </div>
              )
            )}
          </div>
        </main>
      </div>
    </div>
  )
}
```

**Step 3: Check ContentViewer import path**

The `ContentViewer` dynamic import path (`@/components/content-viewer`) assumes the existing component exports a named export `ContentViewer`. Verify:

```bash
grep -n "export" /home/jmagar/workspace/axon_rust/apps/web/components/content-viewer.tsx | head -5
```

If the export is different (e.g., `export default` or different name), adjust the dynamic import accordingly.

**Step 4: Verify page compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
npx tsc --noEmit 2>&1 | grep "workspace/page"
```

Expected: No errors.

**Step 5: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/app/workspace/page.tsx
git commit -m "feat(web): add /workspace file explorer page with tree + viewer"
```

---

## Task 5: Wire Navigation Icon in Omnibox

**Purpose:** Add a `FolderOpen` icon button to the omnibox top-right toolbar so users can navigate to the workspace file explorer.

**Files:**
- Modify: `apps/web/components/omnibox.tsx`

**Step 1: Find the exact insertion point**

```bash
grep -n "Settings2\|href.*settings\|href.*mcp\|href.*agents" /home/jmagar/workspace/axon_rust/apps/web/components/omnibox.tsx
```

The output will show the lines where Settings2, Network (/mcp), and Bot (/agents) icons live. We'll add the FolderOpen icon for `/workspace` in the same block.

**Step 2: Find the import block and add FolderOpen**

```bash
grep -n "^import.*lucide" /home/jmagar/workspace/axon_rust/apps/web/components/omnibox.tsx | head -5
```

Open the file and find the lucide-react import. Add `FolderOpen` to it:

```typescript
// In the lucide-react import line, add FolderOpen
import { ..., FolderOpen, ... } from 'lucide-react'
```

**Step 3: Add the icon button in the right toolbar**

Look for the pattern around the Settings2 icon and insert a new icon + separator immediately before it:

```tsx
{/* Add immediately before the Settings2 separator */}
<div className="h-[20px] w-px shrink-0 bg-[rgba(255,135,175,0.12)]" />
<Link
  href="/workspace"
  className="flex size-6 items-center justify-center rounded text-[rgba(175,215,255,0.45)] transition-colors hover:bg-[rgba(175,215,255,0.08)] hover:text-[rgba(175,215,255,0.9)]"
  title="Workspace"
>
  <FolderOpen className="size-3.5" />
</Link>
```

> **IMPORTANT**: Match the exact className pattern used by the adjacent Network/Bot/Settings2 icons — don't invent new styles.

**Step 4: Verify it compiles**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
npx tsc --noEmit 2>&1 | grep omnibox
```

**Step 5: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/components/omnibox.tsx
git commit -m "feat(web): add workspace (FolderOpen) nav icon to omnibox toolbar"
```

---

## Task 6: End-to-End Smoke Test

**Purpose:** Verify the full feature works end-to-end in a running dev or prod container.

**Step 1: Verify the workspace is mounted**

```bash
# Inside the container or via docker exec:
docker exec axon-web printenv AXON_WORKSPACE
docker exec axon-web ls /workspace | head -10
```

Expected: The env var is set and `/workspace` contains your actual workspace files.

**Step 2: Test the API route**

```bash
# From the host:
curl "https://axon.tootie.tv/api/workspace?action=list&path=" | jq '.items[:5]'
```

Expected: Array of file/directory entries from the workspace root.

**Step 3: Test path traversal is blocked**

```bash
curl "https://axon.tootie.tv/api/workspace?action=list&path=../../etc" | jq .
```

Expected: `{ "error": "Directory not found" }` (because after normalization it's outside workspace).

```bash
curl "https://axon.tootie.tv/api/workspace?action=list&path=../secret" | jq .
```

Expected: `{ "error": "Path is outside workspace" }`.

**Step 4: Navigate to the page**

Open `https://axon.tootie.tv/workspace` in browser and verify:
- [ ] Directory tree loads with workspace root contents
- [ ] Clicking a directory expands it and loads children
- [ ] Clicking a `.md` file shows Plate.js rendered markdown
- [ ] Clicking a `.ts` or `.rs` file shows CodeViewer with line numbers
- [ ] "Open in Pulse" button appears when a file is selected
- [ ] Breadcrumb shows the current file path
- [ ] File size and modified date show in the toolbar
- [ ] Clicking the FolderOpen icon in the omnibox navigates to `/workspace`
- [ ] Sidebar toggle (FolderOpen button in breadcrumb bar) collapses/expands the tree

**Step 5: Final commit (if any cleanup needed)**

```bash
cd /home/jmagar/workspace/axon_rust
git add -p  # review any remaining changes
git commit -m "chore(web): workspace explorer smoke test cleanup"
```

---

## Implementation Notes

### ContentViewer Compatibility

The existing `ContentViewer` at `apps/web/components/content-viewer.tsx` renders markdown using Plate.js. Before writing Task 4, run:

```bash
head -30 /home/jmagar/workspace/axon_rust/apps/web/components/content-viewer.tsx
```

Check the component's props interface. If it accepts `content: string` as a direct prop, the plan above is correct. If it requires a different prop name (e.g., `markdown`, `value`, `children`), update the workspace page accordingly.

### "Open in Pulse" Deep-link

The `openInPulse` function in the workspace page navigates to `/?pulse=...`. This only works if the main page (`/`) reads a `pulse` query param and auto-populates the omnibox. If not, a simpler approach is to copy the file path to the clipboard with a toast notification, and the user can paste it into Pulse manually using the `@mention` syntax. Verify the behavior and adapt accordingly.

### File Type Extension List

The `TEXT_EXTENSIONS` set in the API route (`route.ts`) is conservative. Extend it as needed — e.g., adding `.fish`, `.nix`, `.tf`, `.kdl`, `.ron` etc.

### Monolith Policy Compliance

All new files are well under the 500-line limit:
- `route.ts` ~90 lines
- `file-tree.tsx` ~110 lines
- `code-viewer.tsx` ~70 lines
- `workspace/page.tsx` ~240 lines

Run the monolith check after completion:

```bash
cd /home/jmagar/workspace/axon_rust
python3 scripts/enforce_monoliths.py 2>&1 | grep workspace
```

Expected: No violations.

### NEXT_PUBLIC_WORKSPACE_LABEL

The header shows `/workspace` as a static label. If you want to show the actual host path, add to `apps/web/.env` (or docker-compose `axon-web` env):

```
NEXT_PUBLIC_WORKSPACE_LABEL=/home/yourname/workspace
```

This is safe to expose because it's a display-only label, not a path used for I/O.
