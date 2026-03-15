import { type NextRequest, NextResponse } from 'next/server'
import { apiError } from '@/lib/server/api-error'
import type { JobStatus, JobType } from '@/lib/server/job-types'
import { getStatusCounts, queryJobsList } from '@/lib/server/jobs-list-repository'
import type { Job, StatusCounts } from '@/lib/server/jobs-models'
import {
  parseJobsListParams,
  VALID_JOB_FILTER_STATUSES,
  VALID_JOB_FILTER_TYPES,
} from '@/lib/server/jobs-query'
import { logError } from '@/lib/server/logger'

// ── Types ──────────────────────────────────────────────────────────────────────

export type { JobType, JobStatus }

interface JobsResponse {
  jobs: Job[]
  total: number
  hasMore: boolean
  counts: StatusCounts
}

// ── GET /api/jobs ──────────────────────────────────────────────────────────────

export async function GET(req: NextRequest): Promise<NextResponse> {
  const { searchParams } = req.nextUrl

  let type: 'all' | JobType
  let limit: number
  let offset: number
  let safeStatusFilter: ReturnType<typeof parseJobsListParams>['status']
  try {
    const parsed = parseJobsListParams(searchParams)
    type = parsed.type
    safeStatusFilter = parsed.status
    limit = parsed.limit
    offset = parsed.offset
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    const isTypeError = message.startsWith('Invalid type filter:')
    const invalidValue = message.split(':').slice(1).join(':').trim()
    return apiError(400, message, {
      code: isTypeError ? 'invalid_type_filter' : 'invalid_status_filter',
      detail: `Allowed values: ${[
        ...(isTypeError ? VALID_JOB_FILTER_TYPES : VALID_JOB_FILTER_STATUSES),
      ].join(', ')}`,
      ...(invalidValue ? { invalidValue } : {}),
    })
  }

  try {
    const counts = await getStatusCounts()
    const { jobs, total } = await queryJobsList({
      type,
      status: safeStatusFilter,
      limit,
      offset,
    })

    const response: JobsResponse = {
      jobs,
      total,
      hasMore: offset + jobs.length < total,
      counts,
    }
    return NextResponse.json(response)
  } catch (err) {
    logError('api.jobs.db_error', { message: err instanceof Error ? err.message : String(err) })
    return apiError(500, 'Failed to query jobs', { code: 'jobs_db_error' })
  }
}

// ── POST /api/jobs/cancel ──────────────────────────────────────────────────────

export async function POST(): Promise<NextResponse> {
  return NextResponse.json(
    { ok: false, message: 'Cancel not yet supported from UI' },
    { status: 200 },
  )
}
