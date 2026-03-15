import { type NextRequest, NextResponse } from 'next/server'
import { apiError } from '@/lib/server/api-error'
import { getJobDetail, normalizeOutputDirForWeb } from '@/lib/server/jobs-detail-repository'
import type { JobDetail } from '@/lib/server/jobs-models'
import { logError } from '@/lib/server/logger'
export type { JobDetail }
export { normalizeOutputDirForWeb }

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<NextResponse> {
  const { id } = await params
  const includeArtifacts = req.nextUrl.searchParams.get('includeArtifacts') === '1'

  if (!id || !/^[0-9a-f-]{36}$/i.test(id)) {
    return apiError(400, 'Invalid job ID')
  }

  try {
    const job = await getJobDetail(id, includeArtifacts)

    if (!job) {
      return apiError(404, 'Job not found')
    }

    return NextResponse.json(job)
  } catch (err) {
    logError('api.jobs.detail.db_error', {
      message: err instanceof Error ? err.message : String(err),
    })
    return apiError(500, 'Failed to fetch job details', { code: 'jobs_db_error' })
  }
}
