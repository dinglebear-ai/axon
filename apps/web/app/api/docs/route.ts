import { createReadStream, promises as fs } from 'node:fs'
import path from 'node:path'
import { createInterface } from 'node:readline'
import { type NextRequest, NextResponse } from 'next/server'
import { logWarn } from '@/lib/server/logger'

// Normalize once — strips trailing slashes, resolves canonical path
const OUTPUT_ROOT = path.resolve(
  process.env.AXON_OUTPUT_DIR ??
    (process.env.AXON_DATA_DIR
      ? path.join(process.env.AXON_DATA_DIR, 'axon/output')
      : '.cache/axon-rust/output'),
)
const MAX_WALK_DEPTH = 8

export interface DocEntry {
  url: string
  domain: string
  /** Path relative to OUTPUT_ROOT */
  relPath: string
  chars: number
}

// ── manifest parsing ──────────────────────────────────────────────────────────

interface ManifestLine {
  url: string
  relative_path: string
  markdown_chars: number
}

async function readManifest(manifestPath: string): Promise<ManifestLine[]> {
  const entries: ManifestLine[] = []
  try {
    const rl = createInterface({
      input: createReadStream(manifestPath, { encoding: 'utf8' }),
      crlfDelay: Infinity,
    })
    for await (const line of rl) {
      const trimmed = line.trim()
      if (!trimmed) continue
      try {
        const parsed = JSON.parse(trimmed) as Partial<ManifestLine>
        if (parsed.url && parsed.relative_path) {
          entries.push({
            url: parsed.url,
            relative_path: parsed.relative_path,
            markdown_chars: parsed.markdown_chars ?? 0,
          })
        }
      } catch {
        // skip malformed lines
      }
    }
  } catch (err) {
    logWarn('api.docs.manifest_read_failed', {
      manifestPath,
      message: err instanceof Error ? err.message : String(err),
    })
  }
  return entries
}

// ── directory walker ───────────────────────────────────────────────────────────

/** Recursively find all manifest.jsonl files under a root, up to maxDepth levels. */
async function findManifests(dir: string, depth = 0): Promise<string[]> {
  if (depth > MAX_WALK_DEPTH) return []
  const results: string[] = []
  let entries: import('node:fs').Dirent[]
  try {
    entries = await fs.readdir(dir, { withFileTypes: true })
  } catch {
    return results
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      const nested = await findManifests(full, depth + 1)
      results.push(...nested)
    } else if (entry.isFile() && entry.name === 'manifest.jsonl') {
      results.push(full)
    }
  }
  return results
}

function safeDomain(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return 'unknown'
  }
}

// ── action=list ───────────────────────────────────────────────────────────────

async function listDocs(): Promise<NextResponse> {
  const manifests = await findManifests(OUTPUT_ROOT)

  // Collect entries; dedupe by URL (last write wins)
  const byUrl = new Map<string, DocEntry>()

  for (const manifestPath of manifests) {
    const manifestDir = path.dirname(manifestPath)
    const lines = await readManifest(manifestPath)
    for (const line of lines) {
      // relative_path in the manifest is relative to the dir containing the manifest
      const absFile = path.resolve(manifestDir, line.relative_path)
      const relPath = path.relative(OUTPUT_ROOT, absFile)
      if (relPath.startsWith('..')) continue // outside root — skip
      byUrl.set(line.url, {
        url: line.url,
        domain: safeDomain(line.url),
        relPath,
        chars: line.markdown_chars,
      })
    }
  }

  const docs = Array.from(byUrl.values()).sort((a, b) => a.url.localeCompare(b.url))
  return NextResponse.json({ docs, total: docs.length })
}

// ── action=read ───────────────────────────────────────────────────────────────

async function readDoc(relPath: string): Promise<NextResponse> {
  // Cheap check first — only .md files are served
  if (!relPath.endsWith('.md')) {
    return NextResponse.json({ error: 'Only .md files are readable' }, { status: 400 })
  }

  // Path traversal guard — resolve against the normalized root
  const abs = path.resolve(OUTPUT_ROOT, relPath)
  if (!abs.startsWith(OUTPUT_ROOT + path.sep) && abs !== OUTPUT_ROOT) {
    return NextResponse.json({ error: 'Invalid path' }, { status: 400 })
  }

  try {
    const content = await fs.readFile(abs, 'utf8')
    return NextResponse.json({ content, chars: content.length })
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'Read failed'
    return NextResponse.json({ error: msg }, { status: 404 })
  }
}

// ── route handler ─────────────────────────────────────────────────────────────

export async function GET(req: NextRequest): Promise<NextResponse> {
  const { searchParams } = req.nextUrl
  const action = searchParams.get('action') ?? 'list'

  if (action === 'list') return listDocs()

  if (action === 'read') {
    const relPath = searchParams.get('path')
    if (!relPath) return NextResponse.json({ error: 'path param required' }, { status: 400 })
    return readDoc(relPath)
  }

  return NextResponse.json({ error: `Unknown action: ${action}` }, { status: 400 })
}
