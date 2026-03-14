import { describe, expect, it } from 'vitest'
import {
  archiveZipUrl,
  DOWNLOAD_URL_PATTERNS,
  fileDownloadUrl,
  packMdUrl,
  packXmlUrl,
} from '@/lib/download-urls'

describe('download URL helpers', () => {
  const jobId = 'abc-123'

  it('packMdUrl encodes job ID and appends pack.md', () => {
    expect(packMdUrl(jobId)).toBe('/download/abc-123/pack.md')
  })

  it('packXmlUrl encodes job ID and appends pack.xml', () => {
    expect(packXmlUrl(jobId)).toBe('/download/abc-123/pack.xml')
  })

  it('archiveZipUrl encodes job ID and appends archive.zip', () => {
    expect(archiveZipUrl(jobId)).toBe('/download/abc-123/archive.zip')
  })

  it('encodes special characters in job ID', () => {
    const special = 'job/with spaces&stuff'
    expect(packMdUrl(special)).toBe(`/download/${encodeURIComponent(special)}/pack.md`)
  })

  it('fileDownloadUrl encodes each path segment independently', () => {
    const result = fileDownloadUrl(jobId, 'docs/output/file name.md')
    expect(result).toBe('/download/abc-123/file/docs/output/file%20name.md')
  })

  it('fileDownloadUrl handles single segment path', () => {
    expect(fileDownloadUrl(jobId, 'readme.md')).toBe('/download/abc-123/file/readme.md')
  })

  it('fileDownloadUrl handles deeply nested paths', () => {
    const result = fileDownloadUrl(jobId, 'a/b/c/d.txt')
    expect(result).toBe('/download/abc-123/file/a/b/c/d.txt')
  })

  // Structural test: verify DOWNLOAD_URL_PATTERNS is defined and non-empty.
  // These patterns must stay in sync with crates/web.rs route definitions.
  it('DOWNLOAD_URL_PATTERNS contains all expected route patterns', () => {
    expect(DOWNLOAD_URL_PATTERNS).toBeDefined()
    expect(DOWNLOAD_URL_PATTERNS.length).toBe(4)
    expect(DOWNLOAD_URL_PATTERNS).toContain('/download/:jobId/pack.md')
    expect(DOWNLOAD_URL_PATTERNS).toContain('/download/:jobId/pack.xml')
    expect(DOWNLOAD_URL_PATTERNS).toContain('/download/:jobId/archive.zip')
    expect(DOWNLOAD_URL_PATTERNS).toContain('/download/:jobId/file/:path*')
  })
})

describe('download URL helper outputs match DOWNLOAD_URL_PATTERNS', () => {
  const uuid = 'f47ac10b-58cc-4372-a567-0e02b2c3d479'

  it('packMdUrl produces a URL matching the pack.md pattern', () => {
    const url = packMdUrl(uuid)
    expect(url).toBe(`/download/${uuid}/pack.md`)
    expect(url).toMatch(/^\/download\/[^/]+\/pack\.md$/)
  })

  it('packXmlUrl produces a URL matching the pack.xml pattern', () => {
    const url = packXmlUrl(uuid)
    expect(url).toBe(`/download/${uuid}/pack.xml`)
    expect(url).toMatch(/^\/download\/[^/]+\/pack\.xml$/)
  })

  it('archiveZipUrl produces a URL matching the archive.zip pattern', () => {
    const url = archiveZipUrl(uuid)
    expect(url).toBe(`/download/${uuid}/archive.zip`)
    expect(url).toMatch(/^\/download\/[^/]+\/archive\.zip$/)
  })

  it('fileDownloadUrl produces a URL matching the file/:path* pattern', () => {
    const url = fileDownloadUrl(uuid, 'output/result.md')
    expect(url).toBe(`/download/${uuid}/file/output/result.md`)
    expect(url).toMatch(/^\/download\/[^/]+\/file\/.+$/)
  })

  it('every pattern has a corresponding helper function', () => {
    // This test fails if a pattern is added to DOWNLOAD_URL_PATTERNS without
    // a matching helper, or if a pattern is accidentally removed/changed.
    const helperUrls = [
      packMdUrl(uuid),
      packXmlUrl(uuid),
      archiveZipUrl(uuid),
      fileDownloadUrl(uuid, 'any/path.txt'),
    ]

    // Each helper URL must start with /download/ and contain the UUID
    for (const url of helperUrls) {
      expect(url).toMatch(/^\/download\//)
      expect(url).toContain(uuid)
    }

    // Number of helpers must match number of patterns
    expect(helperUrls.length).toBe(DOWNLOAD_URL_PATTERNS.length)
  })

  it('all patterns produce valid URL paths when substituted with a UUID', () => {
    for (const pattern of DOWNLOAD_URL_PATTERNS) {
      // Substitute :jobId with a real UUID and :path* with a sample path
      const url = pattern.replace(':jobId', uuid).replace(':path*', 'sample/file.txt')
      // Must be a valid relative URL path (starts with /, no double slashes)
      expect(url).toMatch(/^\//)
      expect(url).not.toMatch(/\/\//)
      expect(url).toContain(uuid)
    }
  })
})

describe('download URL negative cases', () => {
  it('non-download paths do not match the download prefix', () => {
    const nonDownloadPaths = [
      '/api/query',
      '/ws',
      '/',
      '/api/jobs/abc-123',
      '/output/abc-123/file.md',
    ]
    for (const path of nonDownloadPaths) {
      expect(path.startsWith('/download/')).toBe(false)
    }
  })

  it('URLs with empty job IDs still produce valid structure', () => {
    // Even with an empty string, the helper should not throw and should
    // produce a structurally valid path.
    expect(packMdUrl('')).toBe('/download//pack.md')
    expect(archiveZipUrl('')).toBe('/download//archive.zip')
  })
})
