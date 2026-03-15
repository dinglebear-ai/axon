import { promises as fs } from 'node:fs'
import path from 'node:path'
import { type NextRequest, NextResponse } from 'next/server'

// AXON_WORKSPACE inside the axon-web container is /workspace (bind-mounted from host)
const WORKSPACE_ROOT = process.env.AXON_WORKSPACE ?? '/workspace'
// Claude CLI config dir — container default is /home/node/.claude (node user's home).
// For local dev, set CLAUDE_CONFIG to your host ~/.claude directory.
const CLAUDE_ROOT = path.resolve(process.env.CLAUDE_CONFIG ?? '/home/node/.claude')
const CLAUDE_PREFIX = '__claude'

const TEXT_EXTENSIONS = new Set([
  '.md',
  '.mdx',
  '.txt',
  '.log',
  '.csv',
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.rs',
  '.go',
  '.py',
  '.sh',
  '.bash',
  '.zsh',
  '.toml',
  '.yaml',
  '.yml',
  '.json',
  '.jsonl',
  '.json5',
  '.env',
  '.example',
  '.gitignore',
  '.dockerignore',
  '.css',
  '.scss',
  '.html',
  '.xml',
  '.svg',
  '.sql',
  '.graphql',
  '.gql',
  '.lock',
  '.sum',
  'Makefile',
  'Dockerfile',
])

const IGNORE_DIRS = new Set([
  '.git',
  '.cache',
  'node_modules',
  'target',
  '__pycache__',
  '.next',
  '.turbo',
  'dist',
  'build',
  '.venv',
  '.mypy_cache',
  '.pytest_cache',
  '.ruff_cache',
  '.tox',
  'coverage',
])

function isBlockedSecretFile(filePath: string): boolean {
  const name = path.basename(filePath).toLowerCase()
  if (name === '.env.example') return false
  return name === '.env' || name.startsWith('.env.')
}

/** Returns safe resolved path + which root it belongs to, or an error string */
function validatePath(raw: string): { safe: string; isClaudeRoot: boolean } | { error: string } {
  // Claude config paths: "__claude" or "__claude/..."
  if (raw === CLAUDE_PREFIX || raw.startsWith(`${CLAUDE_PREFIX}/`)) {
    const relative = raw.length > CLAUDE_PREFIX.length ? raw.slice(CLAUDE_PREFIX.length + 1) : ''
    const resolved = relative ? path.resolve(CLAUDE_ROOT, relative) : CLAUDE_ROOT
    if (resolved !== CLAUDE_ROOT && !resolved.startsWith(CLAUDE_ROOT + path.sep)) {
      return { error: 'Path is outside Claude config' }
    }
    return { safe: resolved, isClaudeRoot: true }
  }

  // Workspace paths: strip leading slash, resolve against workspace root
  const relative = raw.replace(/^\/+/, '')
  const resolved = path.resolve(WORKSPACE_ROOT, relative)
  const workspaceNorm = path.resolve(WORKSPACE_ROOT)
  if (resolved !== workspaceNorm && !resolved.startsWith(workspaceNorm + path.sep)) {
    return { error: 'Path is outside workspace' }
  }
  return { safe: resolved, isClaudeRoot: false }
}

/** Re-validates a symlink-resolved real path against the expected root (prevents symlink traversal) */
function validateRealPath(realPath: string, isClaudeRoot: boolean): boolean {
  if (isClaudeRoot) {
    return realPath === CLAUDE_ROOT || realPath.startsWith(CLAUDE_ROOT + path.sep)
  }
  const workspaceNorm = path.resolve(WORKSPACE_ROOT)
  return realPath === workspaceNorm || realPath.startsWith(workspaceNorm + path.sep)
}

/** Resolve symlinks in safePath and re-validate; throws with 400 status on traversal */
async function realpathGuard(safePath: string, isClaudeRoot: boolean): Promise<string> {
  try {
    const real = await fs.realpath(safePath)
    if (!validateRealPath(real, isClaudeRoot)) {
      throw Object.assign(new Error('Path is outside allowed root'), { status: 400 })
    }
    return real
  } catch (err: unknown) {
    if (err instanceof Error && 'status' in err) throw err
    // realpath throws ENOENT when path doesn't exist — pass through string-validated path;
    // the subsequent stat/readFile will throw ENOENT naturally and return 404.
    return safePath
  }
}

export async function GET(req: NextRequest) {
  const { searchParams } = req.nextUrl
  const action = searchParams.get('action') ?? 'list'
  const rawPath = searchParams.get('path') ?? ''

  const validation = validatePath(rawPath)
  if ('error' in validation) {
    return NextResponse.json({ error: validation.error }, { status: 400 })
  }
  const { isClaudeRoot } = validation
  let safePath: string
  try {
    safePath = await realpathGuard(validation.safe, isClaudeRoot)
  } catch {
    return NextResponse.json({ error: 'Path is outside allowed root' }, { status: 400 })
  }

  if (action === 'list') {
    try {
      const stat = await fs.stat(safePath)
      if (!stat.isDirectory()) {
        return NextResponse.json({ error: 'Not a directory' }, { status: 400 })
      }

      const entries = await fs.readdir(safePath, { withFileTypes: true })
      const items = entries
        .filter((e) => !e.name.startsWith('.') || e.name === '.env.example')
        .filter((e) => !e.isDirectory() || !IGNORE_DIRS.has(e.name))
        .map((e) => {
          const absoluteItem = path.join(safePath, e.name)
          const itemPath = isClaudeRoot
            ? `${CLAUDE_PREFIX}/${path.relative(CLAUDE_ROOT, absoluteItem)}`
            : path.relative(WORKSPACE_ROOT, absoluteItem)
          return {
            name: e.name,
            type: e.isDirectory() ? ('directory' as const) : ('file' as const),
            path: itemPath,
          }
        })
        .sort((a, b) => {
          // Dirs first, then files, then alphabetical within each group
          if (a.type !== b.type) return a.type === 'directory' ? -1 : 1
          return a.name.localeCompare(b.name)
        })

      const responsePath = isClaudeRoot
        ? safePath === CLAUDE_ROOT
          ? CLAUDE_PREFIX
          : `${CLAUDE_PREFIX}/${path.relative(CLAUDE_ROOT, safePath)}`
        : path.relative(WORKSPACE_ROOT, safePath) || '.'
      return NextResponse.json({ path: responsePath, items })
    } catch {
      return NextResponse.json({ error: 'Directory not found' }, { status: 404 })
    }
  }

  if (action === 'read') {
    try {
      if (isBlockedSecretFile(safePath)) {
        return NextResponse.json(
          { error: 'Access denied for sensitive environment files' },
          { status: 403 },
        )
      }
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
    } catch {
      return NextResponse.json({ error: 'File not found' }, { status: 404 })
    }
  }

  return NextResponse.json({ error: 'Unknown action' }, { status: 400 })
}
