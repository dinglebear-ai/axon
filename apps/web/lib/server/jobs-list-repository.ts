import type { JobType } from './job-types'
import { safeStatus } from './job-types'
import {
  isoDateOrNull,
  type StatusFilter,
  statusClause,
  summarizeUrls,
  truncateJobTarget,
} from './jobs'
import type { Job, StatusCounts } from './jobs-models'
import { getJobsPgPool } from './pg-pool'

interface JobsListResult {
  jobs: Job[]
  total: number
}

const STATUS_COUNTS_TTL_MS = 5_000
let statusCountsCache: { data: StatusCounts; expiresAt: number } | null = null

const JOB_STATUS_COUNT_TABLES = [
  'axon_crawl_jobs',
  'axon_extract_jobs',
  'axon_embed_jobs',
  'axon_ingest_jobs',
  'axon_refresh_jobs',
] as const

type JobStatusCountTable = (typeof JOB_STATUS_COUNT_TABLES)[number]

async function queryRefresh(
  statusFilter: StatusFilter,
  limit: number,
  offset: number,
): Promise<JobsListResult> {
  const { clause, params } = statusClause(statusFilter, 1)
  const n = params.length
  const rows = await getJobsPgPool().query(
    `SELECT id, urls_json, status, created_at, started_at, finished_at, error_text,
            config_json->>'collection' AS collection,
            COUNT(*) OVER() AS total
     FROM axon_refresh_jobs
     WHERE ${clause}
     ORDER BY created_at DESC
     LIMIT $${n + 1} OFFSET $${n + 2}`,
    [...params, limit, offset],
  )
  return {
    jobs: rows.rows.map((r) => ({
      id: r.id as string,
      type: 'refresh' as JobType,
      status: safeStatus(r.status as string),
      target: truncateJobTarget(summarizeUrls(r.urls_json)),
      collection: (r.collection as string) ?? null,
      createdAt: (r.created_at as Date).toISOString(),
      startedAt: isoDateOrNull(r.started_at),
      finishedAt: isoDateOrNull(r.finished_at),
      errorText: (r.error_text as string) ?? null,
    })),
    total: Number((rows.rows[0] as { total?: string } | undefined)?.total ?? 0),
  }
}

async function queryCrawl(
  statusFilter: StatusFilter,
  limit: number,
  offset: number,
): Promise<JobsListResult> {
  const { clause, params } = statusClause(statusFilter, 1)
  const n = params.length
  const rows = await getJobsPgPool().query(
    `SELECT id, url, status, created_at, started_at, finished_at, error_text,
            config_json->>'collection' AS collection,
            COUNT(*) OVER() AS total
     FROM axon_crawl_jobs
     WHERE ${clause}
     ORDER BY created_at DESC
     LIMIT $${n + 1} OFFSET $${n + 2}`,
    [...params, limit, offset],
  )
  return {
    jobs: rows.rows.map((r) => ({
      id: r.id as string,
      type: 'crawl' as JobType,
      status: safeStatus(r.status as string),
      target: truncateJobTarget(r.url as string),
      collection: (r.collection as string) ?? null,
      createdAt: (r.created_at as Date).toISOString(),
      startedAt: isoDateOrNull(r.started_at),
      finishedAt: isoDateOrNull(r.finished_at),
      errorText: (r.error_text as string) ?? null,
    })),
    total: Number((rows.rows[0] as { total?: string } | undefined)?.total ?? 0),
  }
}

async function queryExtract(
  statusFilter: StatusFilter,
  limit: number,
  offset: number,
): Promise<JobsListResult> {
  const { clause, params } = statusClause(statusFilter, 1)
  const n = params.length
  const rows = await getJobsPgPool().query(
    `SELECT id, urls_json, status, created_at, started_at, finished_at, error_text,
            COUNT(*) OVER() AS total
     FROM axon_extract_jobs
     WHERE ${clause}
     ORDER BY created_at DESC
     LIMIT $${n + 1} OFFSET $${n + 2}`,
    [...params, limit, offset],
  )
  return {
    jobs: rows.rows.map((r) => ({
      id: r.id as string,
      type: 'extract' as JobType,
      status: safeStatus(r.status as string),
      target: truncateJobTarget(summarizeUrls(r.urls_json)),
      collection: null,
      createdAt: (r.created_at as Date).toISOString(),
      startedAt: isoDateOrNull(r.started_at),
      finishedAt: isoDateOrNull(r.finished_at),
      errorText: (r.error_text as string) ?? null,
    })),
    total: Number((rows.rows[0] as { total?: string } | undefined)?.total ?? 0),
  }
}

async function queryEmbed(
  statusFilter: StatusFilter,
  limit: number,
  offset: number,
): Promise<JobsListResult> {
  const { clause, params } = statusClause(statusFilter, 1)
  const n = params.length
  const rows = await getJobsPgPool().query(
    `SELECT id, input_text, status, created_at, started_at, finished_at, error_text,
            config_json->>'collection' AS collection,
            COUNT(*) OVER() AS total
     FROM axon_embed_jobs
     WHERE ${clause}
     ORDER BY created_at DESC
     LIMIT $${n + 1} OFFSET $${n + 2}`,
    [...params, limit, offset],
  )
  return {
    jobs: rows.rows.map((r) => ({
      id: r.id as string,
      type: 'embed' as JobType,
      status: safeStatus(r.status as string),
      target: truncateJobTarget(r.input_text as string),
      collection: (r.collection as string) ?? null,
      createdAt: (r.created_at as Date).toISOString(),
      startedAt: isoDateOrNull(r.started_at),
      finishedAt: isoDateOrNull(r.finished_at),
      errorText: (r.error_text as string) ?? null,
    })),
    total: Number((rows.rows[0] as { total?: string } | undefined)?.total ?? 0),
  }
}

async function queryIngest(
  statusFilter: StatusFilter,
  limit: number,
  offset: number,
): Promise<JobsListResult> {
  const { clause, params } = statusClause(statusFilter, 1)
  const n = params.length
  const rows = await getJobsPgPool().query(
    `SELECT id, source_type, target, status, created_at, started_at, finished_at, error_text,
            COUNT(*) OVER() AS total
     FROM axon_ingest_jobs
     WHERE ${clause}
     ORDER BY created_at DESC
     LIMIT $${n + 1} OFFSET $${n + 2}`,
    [...params, limit, offset],
  )
  return {
    jobs: rows.rows.map((r) => ({
      id: r.id as string,
      type: 'ingest' as JobType,
      status: safeStatus(r.status as string),
      target: truncateJobTarget(`${r.source_type as string}: ${r.target as string}`),
      collection: null,
      createdAt: (r.created_at as Date).toISOString(),
      startedAt: isoDateOrNull(r.started_at),
      finishedAt: isoDateOrNull(r.finished_at),
      errorText: (r.error_text as string) ?? null,
    })),
    total: Number((rows.rows[0] as { total?: string } | undefined)?.total ?? 0),
  }
}

async function fetchStatusCounts(): Promise<StatusCounts> {
  const countSql = (table: JobStatusCountTable) =>
    getJobsPgPool().query<{ running: string; pending: string; completed: string; failed: string }>(
      `SELECT
        COUNT(*) FILTER (WHERE status = 'running')                    AS running,
        COUNT(*) FILTER (WHERE status = 'pending')                    AS pending,
        COUNT(*) FILTER (WHERE status = 'completed')                  AS completed,
        COUNT(*) FILTER (WHERE status IN ('failed','canceled'))       AS failed
       FROM ${table}`,
    )
  const countResults = await Promise.all(JOB_STATUS_COUNT_TABLES.map((table) => countSql(table)))
  const sum = (key: keyof StatusCounts) =>
    countResults.reduce(
      (acc, r) => acc + Number((r.rows[0] as Record<string, string> | undefined)?.[key] ?? 0),
      0,
    )
  return {
    running: sum('running'),
    pending: sum('pending'),
    completed: sum('completed'),
    failed: sum('failed'),
  }
}

export async function getStatusCounts(): Promise<StatusCounts> {
  const now = Date.now()
  if (statusCountsCache && statusCountsCache.expiresAt > now) {
    return statusCountsCache.data
  }
  const data = await fetchStatusCounts()
  statusCountsCache = { data, expiresAt: now + STATUS_COUNTS_TTL_MS }
  return data
}

export async function queryJobsList(params: {
  type: 'all' | JobType
  status: StatusFilter
  limit: number
  offset: number
}): Promise<JobsListResult> {
  const { type, status, limit, offset } = params

  if (type === 'all') {
    const { clause, params: queryParams } = statusClause(status, 1)
    const n = queryParams.length
    const [unionResult, countResult] = await Promise.all([
      getJobsPgPool().query(
        `SELECT id, 'crawl' AS type, url AS target, config_json->>'collection' AS collection_val, status, created_at, started_at, finished_at, error_text
           FROM axon_crawl_jobs WHERE ${clause}
         UNION ALL
         SELECT id, 'extract', urls_json::text, NULL, status, created_at, started_at, finished_at, error_text
           FROM axon_extract_jobs WHERE ${clause}
         UNION ALL
         SELECT id, 'embed', input_text, config_json->>'collection', status, created_at, started_at, finished_at, error_text
           FROM axon_embed_jobs WHERE ${clause}
         UNION ALL
         SELECT id, 'ingest', source_type || ': ' || target, NULL, status, created_at, started_at, finished_at, error_text
           FROM axon_ingest_jobs WHERE ${clause}
         UNION ALL
         SELECT id, 'refresh', urls_json::text, config_json->>'collection', status, created_at, started_at, finished_at, error_text
           FROM axon_refresh_jobs WHERE ${clause}
         ORDER BY created_at DESC
         LIMIT $${n + 1} OFFSET $${n + 2}`,
        [...queryParams, limit, offset],
      ),
      getJobsPgPool().query<{ count: string }>(
        `SELECT COUNT(*) AS count FROM (
           SELECT id FROM axon_crawl_jobs WHERE ${clause}
           UNION ALL
           SELECT id FROM axon_extract_jobs WHERE ${clause}
           UNION ALL
           SELECT id FROM axon_embed_jobs WHERE ${clause}
           UNION ALL
           SELECT id FROM axon_ingest_jobs WHERE ${clause}
           UNION ALL
           SELECT id FROM axon_refresh_jobs WHERE ${clause}
         ) t`,
        queryParams,
      ),
    ])

    return {
      jobs: unionResult.rows.map((r) => ({
        id: r.id as string,
        type: r.type as JobType,
        status: safeStatus(r.status as string),
        target: truncateJobTarget(r.target as string),
        collection: (r.collection_val as string) ?? null,
        createdAt: (r.created_at as Date).toISOString(),
        startedAt: isoDateOrNull(r.started_at),
        finishedAt: isoDateOrNull(r.finished_at),
        errorText: (r.error_text as string) ?? null,
      })),
      total: Number((countResult.rows[0] as { count?: string } | undefined)?.count ?? 0),
    }
  }

  const query =
    type === 'crawl'
      ? queryCrawl
      : type === 'extract'
        ? queryExtract
        : type === 'embed'
          ? queryEmbed
          : type === 'ingest'
            ? queryIngest
            : queryRefresh

  return query(status, limit, offset)
}
