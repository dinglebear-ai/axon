import { NextResponse } from 'next/server'
import { runAxonCommandWs } from '@/lib/axon-ws-exec'
import { toCortexOverview } from '@/lib/cortex/overview-normalize'
import type {
  DoctorResult,
  DomainsResult,
  SourcesResult,
  StatsResult,
  StatusResult,
} from '@/lib/result-types'
import { apiError } from '@/lib/server/api-error'
import { type JobType, safeStatus } from '@/lib/server/job-types'
import { logError } from '@/lib/server/logger'
import { getJobsPgPool } from '@/lib/server/pg-pool'

export const dynamic = 'force-dynamic'
const OVERVIEW_SLICE_TIMEOUT_MS = 3_000

const FALLBACK_STATUS: StatusResult = {
  local_crawl_jobs: [],
  local_extract_jobs: [],
  local_embed_jobs: [],
  local_ingest_jobs: [],
}

const FALLBACK_DOCTOR: DoctorResult = {
  services: {},
  pipelines: {},
  queue_names: {},
  stale_jobs: 0,
  pending_jobs: 0,
  all_ok: false,
}

const FALLBACK_STATS: StatsResult = {
  collection: 'cortex',
  status: 'unknown',
  indexed_vectors_count: 0,
  points_count: 0,
  dimension: 0,
  distance: 'Cosine',
  segments_count: 0,
  docs_embedded_estimate: 0,
  avg_chunks_per_doc: 0,
  payload_fields: [],
  counts: {},
}

async function queryRecentJobs(limit = 25) {
  const rows = await getJobsPgPool().query<{
    id: string
    type: JobType
    target: string
    status: string
    created_at: Date
    started_at: Date | null
    finished_at: Date | null
  }>(
    `SELECT id, 'crawl'::text AS type, url AS target, status, created_at, started_at, finished_at
       FROM axon_crawl_jobs
     UNION ALL
     SELECT id, 'extract'::text, urls_json::text, status, created_at, started_at, finished_at
       FROM axon_extract_jobs
     UNION ALL
     SELECT id, 'embed'::text, input_text, status, created_at, started_at, finished_at
       FROM axon_embed_jobs
     UNION ALL
     SELECT id, 'ingest'::text, source_type || ': ' || target, status, created_at, started_at, finished_at
       FROM axon_ingest_jobs
     UNION ALL
     SELECT id, 'refresh'::text, urls_json::text, status, created_at, started_at, finished_at
       FROM axon_refresh_jobs
     ORDER BY created_at DESC
     LIMIT $1`,
    [limit],
  )

  return rows.rows.map((row) => ({
    ...row,
    status: safeStatus(row.status),
  }))
}

async function settleWithin<T>(
  promise: Promise<T>,
  fallback: T,
  timeoutMs = OVERVIEW_SLICE_TIMEOUT_MS,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      promise,
      new Promise<T>((resolve) => {
        timer = setTimeout(() => resolve(fallback), timeoutMs)
      }),
    ])
  } catch {
    return fallback
  } finally {
    if (timer) clearTimeout(timer)
  }
}

export async function GET() {
  try {
    const fallbackDomains: DomainsResult = { domains: [], limit: 0, offset: 0 }
    const fallbackSources: SourcesResult = { count: 0, limit: 0, offset: 0, urls: [] }
    const fallbackJobs: Awaited<ReturnType<typeof queryRecentJobs>> = []

    const [status, doctor, stats, domains, sources, jobsRows] = await Promise.all([
      settleWithin(
        runAxonCommandWs('status', OVERVIEW_SLICE_TIMEOUT_MS) as Promise<StatusResult>,
        FALLBACK_STATUS,
      ),
      settleWithin(
        runAxonCommandWs('doctor', OVERVIEW_SLICE_TIMEOUT_MS) as Promise<DoctorResult>,
        FALLBACK_DOCTOR,
      ),
      settleWithin(
        runAxonCommandWs('stats', OVERVIEW_SLICE_TIMEOUT_MS) as Promise<StatsResult>,
        FALLBACK_STATS,
      ),
      settleWithin(
        runAxonCommandWs('domains', OVERVIEW_SLICE_TIMEOUT_MS) as Promise<DomainsResult>,
        fallbackDomains,
      ),
      settleWithin(
        runAxonCommandWs('sources', OVERVIEW_SLICE_TIMEOUT_MS) as Promise<SourcesResult>,
        fallbackSources,
      ),
      settleWithin(queryRecentJobs(), fallbackJobs),
    ])

    const failures =
      Number(status === FALLBACK_STATUS) +
      Number(doctor === FALLBACK_DOCTOR) +
      Number(stats === FALLBACK_STATS) +
      Number(domains === fallbackDomains) +
      Number(sources === fallbackSources) +
      Number(jobsRows === fallbackJobs)

    return NextResponse.json({
      ok: true,
      data: toCortexOverview({
        status,
        doctor,
        stats,
        domains,
        sources,
        jobsRows,
      }),
      partial: failures > 0,
      failures,
    })
  } catch (err) {
    logError('api.cortex.overview.failed', {
      message: err instanceof Error ? err.message : String(err),
    })
    return apiError(500, 'Failed to build Cortex overview', { code: 'cortex_overview' })
  }
}
