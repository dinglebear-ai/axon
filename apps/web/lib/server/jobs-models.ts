import type { JobStatus, JobType } from './job-types'

export interface Job {
  id: string
  type: JobType
  status: JobStatus
  target: string
  collection: string | null
  createdAt: string
  startedAt: string | null
  finishedAt: string | null
  errorText: string | null
}

export interface StatusCounts {
  running: number
  pending: number
  completed: number
  failed: number
}

export interface CrawlMarkdownFile {
  url: string
  relativePath: string
  markdownChars: number
  changed: boolean | null
}

export interface JobDetail {
  id: string
  type: JobType
  status: JobStatus
  success: boolean | null
  target: string
  collection: string | null
  renderMode: string | null
  maxDepth: number | null
  maxPages: number | null
  embed: boolean | null
  createdAt: string
  startedAt: string | null
  finishedAt: string | null
  elapsedMs: number | null
  errorText: string | null
  pagesCrawled: number | null
  pagesDiscovered: number | null
  mdCreated: number | null
  thinMd: number | null
  filteredUrls: number | null
  errorPages: number | null
  wafBlockedPages: number | null
  cacheHit: boolean | null
  outputDir: string | null
  staleUrlsDeleted: number | null
  thinUrls: string[] | null
  wafBlockedUrls: string[] | null
  observedUrls: string[] | null
  markdownFiles: CrawlMarkdownFile[] | null
  docsEmbedded: number | null
  chunksEmbedded: number | null
  urls: string[] | null
  checked: number | null
  changed: number | null
  unchanged: number | null
  notModified: number | null
  failedCount: number | null
  total: number | null
  manifestPath: string | null
  resultJson: Record<string, unknown> | null
  configJson: Record<string, unknown> | null
}
