import { describe, expect, it } from 'vitest'
import {
  buildWorkspaceHandoffPrompt,
  MAX_STDOUT_ITEMS,
  makeInitialRuntimeState,
  pushCapped,
  reduceRuntimeState,
  summarizeJsonValue,
  toCancelResponse,
  toCrawlProgress,
  toScreenshotFiles,
} from '@/hooks/ws-messages/runtime'
import type { RuntimeHandoffSnapshot } from '@/hooks/ws-messages/types'

describe('pushCapped', () => {
  it('appends item to array', () => {
    expect(pushCapped([1, 2], 3)).toEqual([1, 2, 3])
  })

  it('returns new array (immutable)', () => {
    const original = [1]
    const result = pushCapped(original, 2)
    expect(result).not.toBe(original)
  })

  it('trims 10% and appends when at cap', () => {
    const items = Array.from({ length: MAX_STDOUT_ITEMS }, (_, i) => i)
    const result = pushCapped(items, MAX_STDOUT_ITEMS)
    // Trims oldest 10% (500 items), keeps 4500, then appends 1 = 4501
    const trimmed = Math.floor(MAX_STDOUT_ITEMS * 0.9)
    expect(result).toHaveLength(trimmed + 1)
    expect(result[result.length - 1]).toBe(MAX_STDOUT_ITEMS)
    expect(result[0]).toBe(MAX_STDOUT_ITEMS - trimmed) // oldest surviving item
  })
})

describe('summarizeJsonValue', () => {
  it('returns "null" for null/undefined', () => {
    expect(summarizeJsonValue(null)).toBe('null')
    expect(summarizeJsonValue(undefined)).toBe('null')
  })

  it('stringifies numbers and booleans', () => {
    expect(summarizeJsonValue(42)).toBe('42')
    expect(summarizeJsonValue(true)).toBe('true')
  })

  it('returns string values directly (up to 1200 chars)', () => {
    expect(summarizeJsonValue('hello')).toBe('hello')
  })

  it('truncates long strings', () => {
    const long = 'x'.repeat(1500)
    const result = summarizeJsonValue(long)
    expect(result).toContain('[truncated 300 chars]')
    expect(result.startsWith('x'.repeat(1200))).toBe(true)
  })

  it('JSON-stringifies objects (up to 2400 chars)', () => {
    const result = summarizeJsonValue({ key: 'value' })
    expect(result).toContain('"key"')
    expect(result).toContain('"value"')
  })

  it('handles unserializable values', () => {
    const circular: Record<string, unknown> = {}
    circular.self = circular
    expect(summarizeJsonValue(circular)).toBe('[unserializable output]')
  })
})

describe('toCrawlProgress', () => {
  it('maps crawl_progress message fields', () => {
    const msg = {
      type: 'crawl_progress' as const,
      job_id: 'job-1',
      status: 'running',
      pages_crawled: 10,
      pages_discovered: 50,
      md_created: 8,
      thin_md: 2,
      phase: 'crawling',
    }
    expect(toCrawlProgress(msg)).toEqual({
      pages_crawled: 10,
      pages_discovered: 50,
      md_created: 8,
      thin_md: 2,
      phase: 'crawling',
    })
  })
})

describe('toScreenshotFiles', () => {
  it('maps artifact objects to screenshot files', () => {
    const artifacts = [
      { path: 'screenshots/page.png', download_url: '/dl/page.png', size_bytes: 1024 },
    ]
    const result = toScreenshotFiles(artifacts)
    expect(result).toEqual([
      {
        path: 'screenshots/page.png',
        name: 'page.png',
        serve_url: '/dl/page.png',
        size_bytes: 1024,
      },
    ])
  })

  it('filters out artifacts with empty path', () => {
    const artifacts = [
      { path: '', download_url: '/dl/empty', size_bytes: 0 },
      { path: 'valid.png', download_url: '/dl/valid', size_bytes: 100 },
    ]
    expect(toScreenshotFiles(artifacts)).toHaveLength(1)
  })

  it('extracts name from last path segment', () => {
    const artifacts = [{ path: 'a/b/c/file.png', download_url: '', size_bytes: 0 }]
    expect(toScreenshotFiles(artifacts)[0]!.name).toBe('file.png')
  })
})

describe('toCancelResponse', () => {
  it('maps cancel response with message', () => {
    const payload = { ok: true, message: 'Canceled', mode: 'crawl', job_id: 'j-1' }
    expect(toCancelResponse(payload)).toEqual({
      ok: true,
      message: 'Canceled',
      mode: 'crawl',
      job_id: 'j-1',
    })
  })

  it('uses default message when missing', () => {
    expect(toCancelResponse({ ok: true }).message).toBe('Cancel request accepted')
    expect(toCancelResponse({ ok: false }).message).toBe('Cancel request failed')
  })
})

describe('makeInitialRuntimeState', () => {
  it('returns zeroed state', () => {
    const state = makeInitialRuntimeState()
    expect(state.currentJobId).toBeNull()
    expect(state.commandMode).toBeNull()
    expect(state.markdownContent).toBe('')
    expect(state.crawlProgress).toBeNull()
    expect(state.screenshotFiles).toEqual([])
    expect(state.lifecycleEntries).toEqual([])
    expect(state.stdoutJson).toEqual([])
    expect(state.cancelResponse).toBeNull()
  })
})

describe('reduceRuntimeState', () => {
  it('handles command.output.json — extracts job_id and appends data', () => {
    const state = makeInitialRuntimeState()
    const next = reduceRuntimeState(state, {
      type: 'command.output.json',
      data: { data: { job_id: 'j-1', pages: 5 } },
      // biome-ignore lint/suspicious/noExplicitAny: test fixture — partial message shape
    } as any)
    expect(next.currentJobId).toBe('j-1')
    expect(next.stdoutJson).toHaveLength(1)
  })

  it('handles command.start — sets mode and clears stdout', () => {
    const state = { ...makeInitialRuntimeState(), stdoutJson: [{ old: true }] }
    const next = reduceRuntimeState(state, {
      type: 'command.start',
      data: { ctx: { mode: 'crawl' } },
      // biome-ignore lint/suspicious/noExplicitAny: test fixture — partial message shape
    } as any)
    expect(next.commandMode).toBe('crawl')
    expect(next.stdoutJson).toEqual([])
  })

  it('handles crawl_progress', () => {
    const state = makeInitialRuntimeState()
    const next = reduceRuntimeState(state, {
      type: 'crawl_progress',
      job_id: 'job-1',
      status: 'running',
      pages_crawled: 5,
      pages_discovered: 20,
      md_created: 4,
      thin_md: 1,
      phase: 'crawling',
      // biome-ignore lint/suspicious/noExplicitAny: intentional test fixture
    } as any)
    expect(next.crawlProgress).toEqual({
      pages_crawled: 5,
      pages_discovered: 20,
      md_created: 4,
      thin_md: 1,
      phase: 'crawling',
    })
  })

  it('handles artifact.content', () => {
    const state = makeInitialRuntimeState()
    const next = reduceRuntimeState(state, {
      type: 'artifact.content',
      data: { content: '# Hello' },
      // biome-ignore lint/suspicious/noExplicitAny: intentional test fixture
    } as any)
    expect(next.markdownContent).toBe('# Hello')
  })

  it('handles unknown types gracefully', () => {
    const state = makeInitialRuntimeState()
    // biome-ignore lint/suspicious/noExplicitAny: intentional test fixture
    const next = reduceRuntimeState(state, { type: 'unknown_type' } as any)
    expect(next).toEqual(state)
  })

  it('does not mutate original state', () => {
    const state = makeInitialRuntimeState()
    const next = reduceRuntimeState(state, {
      type: 'artifact.content',
      data: { content: 'new' },
      // biome-ignore lint/suspicious/noExplicitAny: intentional test fixture
    } as any)
    expect(state.markdownContent).toBe('')
    expect(next.markdownContent).toBe('new')
  })
})

describe('buildWorkspaceHandoffPrompt', () => {
  const baseSnapshot: RuntimeHandoffSnapshot = {
    modeLabel: 'crawl',
    targetInput: 'https://example.com',
    filesSnapshot: [
      { relative_path: 'output/page.md', markdown_chars: 500, url: 'https://example.com' },
    ],
    outputDir: '/tmp/output',
    stdoutSnapshot: [{ pages: 10 }],
    virtualFileContentByPath: {},
  }

  it('includes mode and target', () => {
    const prompt = buildWorkspaceHandoffPrompt(baseSnapshot)
    expect(prompt).toContain('crawl')
    expect(prompt).toContain('https://example.com')
  })

  it('lists files in output', () => {
    const prompt = buildWorkspaceHandoffPrompt(baseSnapshot)
    expect(prompt).toContain('output/page.md')
    expect(prompt).toContain('500 chars')
  })

  it('includes output directory', () => {
    const prompt = buildWorkspaceHandoffPrompt(baseSnapshot)
    expect(prompt).toContain('/tmp/output')
  })

  it('handles scrape mode with virtual file content', () => {
    const scrapeSnapshot: RuntimeHandoffSnapshot = {
      modeLabel: 'scrape',
      targetInput: 'https://example.com/page',
      filesSnapshot: [
        {
          relative_path: 'virtual/scrape-page',
          markdown_chars: 200,
          url: 'https://example.com/page',
        },
      ],
      outputDir: null,
      stdoutSnapshot: [],
      virtualFileContentByPath: { 'virtual/scrape-page': '# Scraped Content' },
    }
    const prompt = buildWorkspaceHandoffPrompt(scrapeSnapshot)
    expect(prompt).toContain('scraped')
    expect(prompt).toContain('# Scraped Content')
  })

  it('handles empty stdout gracefully', () => {
    const snapshot = { ...baseSnapshot, stdoutSnapshot: [] }
    const prompt = buildWorkspaceHandoffPrompt(snapshot)
    expect(prompt).toContain('No JSON summary available')
  })

  it('handles empty file list', () => {
    const snapshot = { ...baseSnapshot, filesSnapshot: [] }
    const prompt = buildWorkspaceHandoffPrompt(snapshot)
    expect(prompt).toContain('No files listed yet')
  })
})
