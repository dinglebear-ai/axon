import { describe, expect, it } from 'vitest'
import type { JobDetail } from '@/app/api/jobs/[id]/route'
import { flattenJsonEntries, getRefreshSummaryRows } from '@/app/jobs/[id]/job-detail-helpers'

describe('job detail metadata helpers', () => {
  it('renders full result_json and config_json key-value metadata sections', () => {
    const resultRows = flattenJsonEntries({
      phase: 'completed',
      nested: { a: 1, b: true },
      urls: ['https://example.com'],
    })
    const configRows = flattenJsonEntries({
      collection: 'cortex',
      limits: { max_pages: 10 },
    })

    expect(resultRows.find((r) => r.key === 'phase')?.value).toBe('completed')
    expect(resultRows.find((r) => r.key === 'nested.a')?.value).toBe('1')
    expect(resultRows.find((r) => r.key === 'nested.b')?.value).toBe('true')
    expect(resultRows.find((r) => r.key === 'urls')?.value).toContain('https://example.com')
    expect(configRows.find((r) => r.key === 'collection')?.value).toBe('cortex')
    expect(configRows.find((r) => r.key === 'limits.max_pages')?.value).toBe('10')
  })

  it('renders refresh-specific stats and URLs', () => {
    const refreshJob = {
      id: '11111111-1111-4111-8111-111111111111',
      type: 'refresh',
      status: 'completed',
      success: true,
      target: 'https://example.com/docs',
      collection: 'cortex',
      renderMode: null,
      maxDepth: null,
      maxPages: null,
      embed: null,
      createdAt: new Date().toISOString(),
      startedAt: new Date().toISOString(),
      finishedAt: new Date().toISOString(),
      elapsedMs: 100,
      errorText: null,
      pagesCrawled: null,
      pagesDiscovered: null,
      mdCreated: null,
      thinMd: null,
      filteredUrls: null,
      errorPages: null,
      wafBlockedPages: null,
      wafDiagnostics: null,
      cacheHit: null,
      outputDir: null,
      staleUrlsDeleted: null,
      thinUrls: null,
      wafBlockedUrls: null,
      observedUrls: null,
      markdownFiles: null,
      docsEmbedded: null,
      chunksEmbedded: null,
      urls: ['https://example.com/docs'],
      checked: 4,
      changed: 2,
      unchanged: 1,
      notModified: 1,
      failedCount: 0,
      total: 4,
      manifestPath: '/data/axon/output/domains/example/sync/manifest.jsonl',
      resultJson: null,
      configJson: null,
    } satisfies JobDetail

    const rows = getRefreshSummaryRows(refreshJob)
    expect(rows.find((r) => r.label === 'Checked')?.value).toBe(4)
    expect(rows.find((r) => r.label === 'Changed')?.value).toBe(2)
    expect(rows.find((r) => r.label === 'Manifest Path')?.value).toContain('manifest.jsonl')
    expect(refreshJob.urls?.[0]).toBe('https://example.com/docs')
  })
})
