/**
 * Download URL path templates.
 * IMPORTANT: These must stay in sync with the routes defined in crates/web.rs.
 * If you change a path here, update the corresponding route in crates/web.rs.
 * See: WEB-INTEGRATION-REVIEW.md L-4
 */

/**
 * All download URL path patterns. Used for structural validation tests
 * to ensure these templates remain defined and consistent.
 */
export const DOWNLOAD_URL_PATTERNS = [
  '/download/:jobId/pack.md',
  '/download/:jobId/pack.xml',
  '/download/:jobId/archive.zip',
  '/download/:jobId/file/:path*',
] as const

export function packMdUrl(jobId: string): string {
  return `/download/${encodeURIComponent(jobId)}/pack.md`
}

export function packXmlUrl(jobId: string): string {
  return `/download/${encodeURIComponent(jobId)}/pack.xml`
}

export function archiveZipUrl(jobId: string): string {
  return `/download/${encodeURIComponent(jobId)}/archive.zip`
}

export function fileDownloadUrl(jobId: string, relPath: string): string {
  const encoded = relPath
    .split('/')
    .map((segment) => encodeURIComponent(segment))
    .join('/')
  return `/download/${encodeURIComponent(jobId)}/file/${encoded}`
}
