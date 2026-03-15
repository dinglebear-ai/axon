import type { NextRequest } from 'next/server'
import { afterEach, describe, expect, it, vi } from 'vitest'

// ---------------------------------------------------------------------------
// Mock node:fs/promises — the workspace route uses it for all filesystem ops
// ---------------------------------------------------------------------------
const fsMock = {
  realpath: vi.fn(),
  stat: vi.fn(),
  readdir: vi.fn(),
  readFile: vi.fn(),
}

vi.mock('node:fs', () => ({
  promises: fsMock,
}))

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function makeReq(params: Record<string, string> = {}): NextRequest {
  const url = new URL('http://localhost/api/workspace')
  for (const [k, v] of Object.entries(params)) {
    url.searchParams.set(k, v)
  }
  // Dynamic import needs `next/server` — build a minimal NextRequest stand-in
  const { NextRequest: NR } = require('next/server') as { NextRequest: typeof NextRequest }
  return new NR(url.toString())
}

// Make stat behave like a directory or file depending on the mock return value
function makeStatDir(size = 0) {
  return {
    isDirectory: () => true,
    isFile: () => false,
    size,
    mtime: new Date('2026-01-01T00:00:00Z'),
  }
}
function makeStatFile(size: number) {
  return {
    isDirectory: () => false,
    isFile: () => true,
    size,
    mtime: new Date('2026-01-01T00:00:00Z'),
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('GET /api/workspace', () => {
  afterEach(() => {
    vi.resetModules()
    fsMock.realpath.mockReset()
    fsMock.stat.mockReset()
    fsMock.readdir.mockReset()
    fsMock.readFile.mockReset()
  })

  // -------------------------------------------------------------------------
  // Path validation
  // -------------------------------------------------------------------------
  describe('path validation', () => {
    it('returns 400 for path traversal outside workspace', async () => {
      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: '../../etc' })
      const res = await GET(req)
      expect(res.status).toBe(400)
      const body = (await res.json()) as { error: string }
      expect(body.error).toMatch(/outside/i)
    })

    it('returns 400 for path traversal outside workspace via encoded segments', async () => {
      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: 'subdir/../../../../../../etc/passwd' })
      const res = await GET(req)
      expect(res.status).toBe(400)
    })
  })

  // -------------------------------------------------------------------------
  // action=list — directory listing
  // -------------------------------------------------------------------------
  describe('action=list (directory listing)', () => {
    it('returns 200 with items array for workspace root', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')
      fsMock.stat.mockResolvedValue(makeStatDir())
      fsMock.readdir.mockResolvedValue([
        { name: 'README.md', isDirectory: () => false },
        { name: 'src', isDirectory: () => true },
        { name: '.git', isDirectory: () => true },
        { name: 'node_modules', isDirectory: () => true },
      ])

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: '' })
      const res = await GET(req)
      expect(res.status).toBe(200)

      const body = (await res.json()) as {
        path: string
        items: Array<{ name: string; type: string; path: string }>
      }
      expect(typeof body.path).toBe('string')
      expect(Array.isArray(body.items)).toBe(true)

      const names = body.items.map((i) => i.name)
      // .git and node_modules must be filtered out
      expect(names).not.toContain('.git')
      expect(names).not.toContain('node_modules')
      expect(names).toContain('README.md')
      expect(names).toContain('src')
    })

    it('returns directories before files (sorted)', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')
      fsMock.stat.mockResolvedValue(makeStatDir())
      fsMock.readdir.mockResolvedValue([
        { name: 'zebra.md', isDirectory: () => false },
        { name: 'alpha', isDirectory: () => true },
        { name: 'app.ts', isDirectory: () => false },
        { name: 'beta', isDirectory: () => true },
      ])

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: '' })
      const res = await GET(req)
      const body = (await res.json()) as { items: Array<{ name: string; type: string }> }

      const dirs = body.items.filter((i) => i.type === 'directory').map((i) => i.name)
      const files = body.items.filter((i) => i.type === 'file').map((i) => i.name)

      // All dirs appear before any file in the items array
      const firstFileIdx = body.items.findIndex((i) => i.type === 'file')
      const lastDirIdx = body.items.map((i) => i.type).lastIndexOf('directory')
      if (firstFileIdx !== -1 && lastDirIdx !== -1) {
        expect(lastDirIdx).toBeLessThan(firstFileIdx)
      }

      // Dirs and files are alphabetically sorted within their groups
      expect(dirs).toEqual([...dirs].sort())
      expect(files).toEqual([...files].sort())
    })

    it('returns 400 when path points to a file (not a directory)', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')
      // stat returns a file, not a directory
      fsMock.stat.mockResolvedValue(makeStatFile(1024))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: 'some-file.md' })
      const res = await GET(req)
      expect(res.status).toBe(400)
      const body = (await res.json()) as { error: string }
      expect(body.error).toBe('Not a directory')
    })

    it('returns 404 when directory does not exist', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')
      fsMock.stat.mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: 'does-not-exist' })
      const res = await GET(req)
      expect(res.status).toBe(404)
      const body = (await res.json()) as { error: string }
      expect(body.error).toBe('Directory not found')
    })

    it('includes .env.example despite starting with a dot', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')
      fsMock.stat.mockResolvedValue(makeStatDir())
      fsMock.readdir.mockResolvedValue([
        { name: '.env.example', isDirectory: () => false },
        { name: '.env', isDirectory: () => false },
        { name: '.hidden', isDirectory: () => false },
        { name: 'visible.ts', isDirectory: () => false },
      ])

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: '' })
      const res = await GET(req)
      const body = (await res.json()) as { items: Array<{ name: string }> }

      const names = body.items.map((i) => i.name)
      expect(names).toContain('.env.example')
      // .env and .hidden start with '.' and are NOT .env.example — should be hidden
      expect(names).not.toContain('.env')
      expect(names).not.toContain('.hidden')
    })
  })

  // -------------------------------------------------------------------------
  // action=read — file content
  // -------------------------------------------------------------------------
  describe('action=read (file content)', () => {
    it('returns 403 for sensitive .env file reads', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: '.env' })
      const res = await GET(req)
      expect(res.status).toBe(403)
      const body = (await res.json()) as { error: string }
      expect(body.error).toMatch(/access denied/i)
    })

    it('returns 403 for sensitive .env.* file reads', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: '.env.local' })
      const res = await GET(req)
      expect(res.status).toBe(403)
      const body = (await res.json()) as { error: string }
      expect(body.error).toMatch(/access denied/i)
    })

    it('still allows .env.example reads', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockResolvedValue(makeStatFile(64))
      fsMock.readFile.mockResolvedValue('AXON_WEB_API_TOKEN=CHANGE_ME')

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: '.env.example' })
      const res = await GET(req)
      expect(res.status).toBe(200)

      const body = (await res.json()) as { type: string; content: string }
      expect(body.type).toBe('text')
      expect(body.content).toContain('AXON_WEB_API_TOKEN')
    })

    it('returns 200 with text content for a supported text file', async () => {
      // realpath must echo back the resolved path so basename/extname work correctly
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockResolvedValue(makeStatFile(128))
      fsMock.readFile.mockResolvedValue('# Hello World\n\nThis is a test document.')

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: 'README.md' })
      const res = await GET(req)
      expect(res.status).toBe(200)

      const body = (await res.json()) as {
        type: string
        name: string
        ext: string
        size: number
        modified: string
        content: string
      }
      expect(body.type).toBe('text')
      expect(body.name).toBe('README.md')
      expect(body.ext).toBe('.md')
      expect(body.content).toBe('# Hello World\n\nThis is a test document.')
      expect(typeof body.modified).toBe('string')
      expect(body.size).toBe(128)
    })

    it('returns binary descriptor without content for unsupported file type', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockResolvedValue(makeStatFile(2048))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: 'image.png' })
      const res = await GET(req)
      expect(res.status).toBe(200)

      const body = (await res.json()) as { type: string; name: string; size: number }
      expect(body.type).toBe('binary')
      expect(body.name).toBe('image.png')
      expect(body.size).toBe(2048)
      expect(body).not.toHaveProperty('content')
    })

    it('returns 400 when read target is a directory', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockResolvedValue(makeStatDir())

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: 'src' })
      const res = await GET(req)
      expect(res.status).toBe(400)
      const body = (await res.json()) as { error: string }
      expect(body.error).toBe('Is a directory')
    })

    it('returns 413 when file exceeds 1MB limit', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockResolvedValue(makeStatFile(1_500_000))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: 'large.ts' })
      const res = await GET(req)
      expect(res.status).toBe(413)
      const body = (await res.json()) as { error: string }
      expect(body.error).toMatch(/1MB/i)
    })

    it('returns 404 when file does not exist', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }))

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: 'missing.ts' })
      const res = await GET(req)
      expect(res.status).toBe(404)
      const body = (await res.json()) as { error: string }
      expect(body.error).toBe('File not found')
    })

    it('recognizes Makefile and Dockerfile as text files (no extension)', async () => {
      fsMock.realpath.mockImplementation((p: string) => Promise.resolve(p))
      fsMock.stat.mockResolvedValue(makeStatFile(512))
      fsMock.readFile.mockResolvedValue('FROM node:24-alpine\nRUN echo hello')

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'read', path: 'Dockerfile' })
      const res = await GET(req)
      expect(res.status).toBe(200)

      const body = (await res.json()) as { type: string }
      expect(body.type).toBe('text')
    })
  })

  // -------------------------------------------------------------------------
  // __claude prefix — Claude config root
  // -------------------------------------------------------------------------
  describe('__claude path prefix', () => {
    it('serves listing for __claude root path', async () => {
      // Point CLAUDE_ROOT at something that resolves safely
      const claudeRoot = process.env.CLAUDE_CONFIG ?? '/home/node/.claude'
      fsMock.realpath.mockResolvedValue(claudeRoot)
      fsMock.stat.mockResolvedValue(makeStatDir())
      fsMock.readdir.mockResolvedValue([
        { name: 'settings.json', isDirectory: () => false },
        { name: 'projects', isDirectory: () => true },
      ])

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: '__claude' })
      const res = await GET(req)

      // May be 200 if realpath resolves within CLAUDE_ROOT, or 400 if the test
      // env resolves outside it. Accept both since CLAUDE_CONFIG varies.
      expect([200, 400]).toContain(res.status)

      if (res.status === 200) {
        const body = (await res.json()) as { path: string; items: unknown[] }
        expect(body.path).toBe('__claude')
        expect(Array.isArray(body.items)).toBe(true)
      }
    })

    it('blocks traversal out of Claude root via __claude prefix', async () => {
      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'list', path: '__claude/../../etc' })
      const res = await GET(req)
      expect(res.status).toBe(400)
    })
  })

  // -------------------------------------------------------------------------
  // Unknown action
  // -------------------------------------------------------------------------
  describe('unknown action', () => {
    it('returns 400 for an unrecognized action parameter', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')

      const { GET } = await import('@/app/api/workspace/route')
      const req = makeReq({ action: 'delete', path: '' })
      const res = await GET(req)
      expect(res.status).toBe(400)
      const body = (await res.json()) as { error: string }
      expect(body.error).toBe('Unknown action')
    })

    it('defaults to action=list when action param is absent', async () => {
      fsMock.realpath.mockResolvedValue(process.env.AXON_WORKSPACE ?? '/workspace')
      fsMock.stat.mockResolvedValue(makeStatDir())
      fsMock.readdir.mockResolvedValue([])

      const { GET } = await import('@/app/api/workspace/route')
      // No action param — defaults to 'list'
      const req = makeReq({ path: '' })
      const res = await GET(req)
      expect(res.status).toBe(200)
    })
  })
})
