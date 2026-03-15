import { createReadStream } from 'node:fs'
import { access } from 'node:fs/promises'
import { createInterface } from 'node:readline'
import { safeStatus } from './job-types'
import {
  asJsonRecord,
  boolOrNull,
  isoDateOrNull,
  jobSuccessFromStatus,
  numberOrNull,
  stringArray,
  stringOrNull,
  summarizeUrls,
} from './jobs'
import type { CrawlMarkdownFile, JobDetail } from './jobs-models'
import { getJobsPgPool } from './pg-pool'

/** @deprecated Paths are now unified via AXON_DATA_DIR — no rewriting needed. */
export function normalizeOutputDirForWeb(outputDir: string | null): string | null {
  return outputDir || null
}

async function readCrawlManifest(outputDir: string): Promise<CrawlMarkdownFile[]> {
  const manifestPath = `${outputDir}/manifest.jsonl`
  try {
    await access(manifestPath)
  } catch {
    return []
  }

  const MAX_MANIFEST_ENTRIES = 10_000
  const files: CrawlMarkdownFile[] = []
  const stream = createReadStream(manifestPath, { encoding: 'utf8' })
  const reader = createInterface({ input: stream, crlfDelay: Infinity })
  try {
    for await (const rawLine of reader) {
      const line = rawLine.trim()
      if (!line) continue
      let parsed: unknown
      try {
        parsed = JSON.parse(line)
      } catch {
        continue
      }
      if (!parsed || typeof parsed !== 'object') continue
      const obj = parsed as Record<string, unknown>
      const url = typeof obj.url === 'string' ? obj.url : null
      const relativePath =
        typeof obj.relative_path === 'string'
          ? obj.relative_path
          : typeof obj.file_path === 'string'
            ? obj.file_path
            : null
      if (!url || !relativePath) continue
      files.push({
        url,
        relativePath,
        markdownChars: numberOrNull(obj.markdown_chars) ?? 0,
        changed: boolOrNull(obj.changed),
      })
      if (files.length >= MAX_MANIFEST_ENTRIES) break
    }
  } finally {
    reader.close()
    stream.destroy()
  }

  return files
}

async function findCrawlJob(id: string, includeArtifacts: boolean): Promise<JobDetail | null> {
  const r = await getJobsPgPool().query(
    `SELECT id, url, status, created_at, started_at, finished_at, error_text,
            result_json, config_json
     FROM axon_crawl_jobs WHERE id = $1`,
    [id],
  )
  if (!r.rows.length) return null
  const row = r.rows[0]
  const res = asJsonRecord(row.result_json)
  const cfg = asJsonRecord(row.config_json)
  const outputDir = normalizeOutputDirForWeb(stringOrNull(res.output_dir))
  const manifestFiles =
    includeArtifacts && outputDir ? await readCrawlManifest(outputDir).catch(() => []) : []
  const thinUrls = stringArray(res.thin_urls)
  const wafBlockedUrls = stringArray(res.waf_blocked_urls)
  const observedUrlSet = new Set<string>([
    ...manifestFiles.map((entry) => entry.url),
    ...thinUrls,
    ...wafBlockedUrls,
  ])
  const createdAt = row.created_at as Date
  const startedAt = row.started_at as Date | null
  const finishedAt = row.finished_at as Date | null
  return {
    id: row.id as string,
    type: 'crawl',
    status: safeStatus(row.status as string),
    success: jobSuccessFromStatus(row.status),
    target: row.url as string,
    collection: stringOrNull(cfg.collection),
    renderMode: stringOrNull(cfg.render_mode),
    maxDepth: cfg.max_depth != null ? Number(cfg.max_depth) : null,
    maxPages: cfg.max_pages != null ? Number(cfg.max_pages) : null,
    embed: cfg.embed != null ? Boolean(cfg.embed) : null,
    createdAt: createdAt.toISOString(),
    startedAt: isoDateOrNull(startedAt),
    finishedAt: isoDateOrNull(finishedAt),
    elapsedMs: res.elapsed_ms != null ? Number(res.elapsed_ms) : null,
    errorText: (row.error_text as string) ?? null,
    pagesCrawled: res.pages_crawled != null ? Number(res.pages_crawled) : null,
    pagesDiscovered: res.pages_discovered != null ? Number(res.pages_discovered) : null,
    mdCreated: res.md_created != null ? Number(res.md_created) : null,
    thinMd: res.thin_md != null ? Number(res.thin_md) : null,
    filteredUrls: res.filtered_urls != null ? Number(res.filtered_urls) : null,
    errorPages: res.error_pages != null ? Number(res.error_pages) : null,
    wafBlockedPages: res.waf_blocked_pages != null ? Number(res.waf_blocked_pages) : null,
    cacheHit: res.cache_hit != null ? Boolean(res.cache_hit) : null,
    outputDir,
    staleUrlsDeleted: res.stale_urls_deleted != null ? Number(res.stale_urls_deleted) : null,
    thinUrls,
    wafBlockedUrls,
    observedUrls: includeArtifacts ? [...observedUrlSet] : null,
    markdownFiles: includeArtifacts ? manifestFiles : null,
    docsEmbedded: null,
    chunksEmbedded: null,
    urls: null,
    checked: null,
    changed: null,
    unchanged: null,
    notModified: null,
    failedCount: null,
    total: null,
    manifestPath: null,
    resultJson: res,
    configJson: cfg,
  }
}

async function findRefreshJob(id: string): Promise<JobDetail | null> {
  const r = await getJobsPgPool().query(
    `SELECT id, status, created_at, started_at, finished_at, error_text, urls_json, result_json, config_json
     FROM axon_refresh_jobs WHERE id = $1`,
    [id],
  )
  if (!r.rows.length) return null
  const row = r.rows[0]
  const res = asJsonRecord(row.result_json)
  const cfg = asJsonRecord(row.config_json)
  const urls = Array.isArray(row.urls_json) ? (row.urls_json as string[]) : []
  const createdAt = row.created_at as Date
  const startedAt = row.started_at as Date | null
  const finishedAt = row.finished_at as Date | null
  return {
    id: row.id as string,
    type: 'refresh',
    status: safeStatus(row.status as string),
    success: jobSuccessFromStatus(row.status),
    target: summarizeUrls(row.urls_json),
    collection: stringOrNull(cfg.collection) ?? stringOrNull(res.collection),
    renderMode: null,
    maxDepth: null,
    maxPages: null,
    embed: null,
    createdAt: createdAt.toISOString(),
    startedAt: isoDateOrNull(startedAt),
    finishedAt: isoDateOrNull(finishedAt),
    elapsedMs: res.elapsed_ms != null ? Number(res.elapsed_ms) : null,
    errorText: (row.error_text as string) ?? null,
    pagesCrawled: null,
    pagesDiscovered: null,
    mdCreated: null,
    thinMd: null,
    filteredUrls: null,
    errorPages: null,
    wafBlockedPages: null,
    cacheHit: null,
    outputDir: null,
    staleUrlsDeleted: null,
    thinUrls: null,
    wafBlockedUrls: null,
    observedUrls: null,
    markdownFiles: null,
    docsEmbedded: null,
    chunksEmbedded: null,
    urls,
    checked: res.checked != null ? Number(res.checked) : null,
    changed: res.changed != null ? Number(res.changed) : null,
    unchanged: res.unchanged != null ? Number(res.unchanged) : null,
    notModified: res.not_modified != null ? Number(res.not_modified) : null,
    failedCount: res.failed != null ? Number(res.failed) : null,
    total: res.total != null ? Number(res.total) : null,
    manifestPath: normalizeOutputDirForWeb(stringOrNull(res.manifest_path)),
    resultJson: res,
    configJson: cfg,
  }
}

async function findEmbedJob(id: string): Promise<JobDetail | null> {
  const r = await getJobsPgPool().query(
    `SELECT id, input_text, status, created_at, started_at, finished_at, error_text,
            result_json, config_json
     FROM axon_embed_jobs WHERE id = $1`,
    [id],
  )
  if (!r.rows.length) return null
  const row = r.rows[0]
  const res = asJsonRecord(row.result_json)
  const cfg = asJsonRecord(row.config_json)
  const createdAt = row.created_at as Date
  const startedAt = row.started_at as Date | null
  const finishedAt = row.finished_at as Date | null
  return {
    id: row.id as string,
    type: 'embed',
    status: safeStatus(row.status as string),
    success: jobSuccessFromStatus(row.status),
    target: row.input_text as string,
    collection: stringOrNull(res.collection) ?? stringOrNull(cfg.collection),
    renderMode: null,
    maxDepth: null,
    maxPages: null,
    embed: null,
    createdAt: createdAt.toISOString(),
    startedAt: isoDateOrNull(startedAt),
    finishedAt: isoDateOrNull(finishedAt),
    elapsedMs: null,
    errorText: (row.error_text as string) ?? null,
    pagesCrawled: null,
    pagesDiscovered: null,
    mdCreated: null,
    thinMd: null,
    filteredUrls: null,
    errorPages: null,
    wafBlockedPages: null,
    cacheHit: null,
    outputDir: null,
    staleUrlsDeleted: null,
    thinUrls: null,
    wafBlockedUrls: null,
    observedUrls: null,
    markdownFiles: null,
    docsEmbedded: res.docs_embedded != null ? Number(res.docs_embedded) : null,
    chunksEmbedded: res.chunks_embedded != null ? Number(res.chunks_embedded) : null,
    urls: null,
    checked: null,
    changed: null,
    unchanged: null,
    notModified: null,
    failedCount: null,
    total: null,
    manifestPath: null,
    resultJson: res,
    configJson: cfg,
  }
}

async function findExtractJob(id: string): Promise<JobDetail | null> {
  const r = await getJobsPgPool().query(
    `SELECT id, urls_json, status, created_at, started_at, finished_at, error_text,
            result_json, config_json
     FROM axon_extract_jobs WHERE id = $1`,
    [id],
  )
  if (!r.rows.length) return null
  const row = r.rows[0]
  const res = asJsonRecord(row.result_json)
  const cfg = asJsonRecord(row.config_json)
  const urls = Array.isArray(row.urls_json) ? (row.urls_json as string[]) : []
  const createdAt = row.created_at as Date
  const startedAt = row.started_at as Date | null
  const finishedAt = row.finished_at as Date | null
  return {
    id: row.id as string,
    type: 'extract',
    status: safeStatus(row.status as string),
    success: jobSuccessFromStatus(row.status),
    target: summarizeUrls(row.urls_json),
    collection: stringOrNull(cfg.collection),
    renderMode: null,
    maxDepth: null,
    maxPages: null,
    embed: null,
    createdAt: createdAt.toISOString(),
    startedAt: isoDateOrNull(startedAt),
    finishedAt: isoDateOrNull(finishedAt),
    elapsedMs: null,
    errorText: (row.error_text as string) ?? null,
    pagesCrawled: null,
    pagesDiscovered: null,
    mdCreated: null,
    thinMd: null,
    filteredUrls: null,
    errorPages: null,
    wafBlockedPages: null,
    cacheHit: null,
    outputDir: null,
    staleUrlsDeleted: null,
    thinUrls: null,
    wafBlockedUrls: null,
    observedUrls: null,
    markdownFiles: null,
    docsEmbedded: null,
    chunksEmbedded: null,
    urls,
    checked: null,
    changed: null,
    unchanged: null,
    notModified: null,
    failedCount: null,
    total: null,
    manifestPath: null,
    resultJson: res,
    configJson: cfg,
  }
}

async function findIngestJob(id: string): Promise<JobDetail | null> {
  const r = await getJobsPgPool().query(
    `SELECT id, source_type, target, status, created_at, started_at, finished_at, error_text,
            result_json, config_json
     FROM axon_ingest_jobs WHERE id = $1`,
    [id],
  )
  if (!r.rows.length) return null
  const row = r.rows[0]
  const res = asJsonRecord(row.result_json)
  const cfg = asJsonRecord(row.config_json)
  const createdAt = row.created_at as Date
  const startedAt = row.started_at as Date | null
  const finishedAt = row.finished_at as Date | null
  return {
    id: row.id as string,
    type: 'ingest',
    status: safeStatus(row.status as string),
    success: jobSuccessFromStatus(row.status),
    target: `${row.source_type as string}: ${row.target as string}`,
    collection: null,
    renderMode: null,
    maxDepth: null,
    maxPages: null,
    embed: null,
    createdAt: createdAt.toISOString(),
    startedAt: isoDateOrNull(startedAt),
    finishedAt: isoDateOrNull(finishedAt),
    elapsedMs: null,
    errorText: (row.error_text as string) ?? null,
    pagesCrawled: null,
    pagesDiscovered: null,
    mdCreated: null,
    thinMd: null,
    filteredUrls: null,
    errorPages: null,
    wafBlockedPages: null,
    cacheHit: null,
    outputDir: null,
    staleUrlsDeleted: null,
    thinUrls: null,
    wafBlockedUrls: null,
    observedUrls: null,
    markdownFiles: null,
    docsEmbedded: null,
    chunksEmbedded: null,
    urls: null,
    checked: null,
    changed: null,
    unchanged: null,
    notModified: null,
    failedCount: null,
    total: null,
    manifestPath: null,
    resultJson: res,
    configJson: cfg,
  }
}

export async function getJobDetail(
  id: string,
  includeArtifacts: boolean,
): Promise<JobDetail | null> {
  const [crawl, embed, extract, ingest, refresh] = await Promise.all([
    findCrawlJob(id, includeArtifacts),
    findEmbedJob(id),
    findExtractJob(id),
    findIngestJob(id),
    findRefreshJob(id),
  ])

  return crawl ?? embed ?? extract ?? ingest ?? refresh
}
