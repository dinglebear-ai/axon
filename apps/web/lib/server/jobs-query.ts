import type { JobType } from './job-types'
import type { StatusFilter } from './jobs'

export const VALID_JOB_FILTER_TYPES = new Set([
  'all',
  'crawl',
  'extract',
  'embed',
  'ingest',
  'refresh',
])
export const VALID_JOB_FILTER_STATUSES = new Set([
  'all',
  'active',
  'pending',
  'running',
  'completed',
  'failed',
  'canceled',
])

export interface JobsListParams {
  type: 'all' | JobType
  status: StatusFilter
  limit: number
  offset: number
}

export function parseJobsListParams(searchParams: URLSearchParams): JobsListParams {
  const typeRaw = searchParams.get('type') ?? 'all'
  if (!VALID_JOB_FILTER_TYPES.has(typeRaw)) {
    throw new Error(`Invalid type filter: ${typeRaw}`)
  }

  const statusRaw = searchParams.get('status') ?? 'all'
  if (!VALID_JOB_FILTER_STATUSES.has(statusRaw)) {
    throw new Error(`Invalid status filter: ${statusRaw}`)
  }

  return {
    type: typeRaw as 'all' | JobType,
    status: statusRaw as StatusFilter,
    limit: Math.min(Math.max(Number(searchParams.get('limit') ?? '50'), 1), 200),
    offset: Math.max(Number(searchParams.get('offset') ?? '0'), 0),
  }
}
