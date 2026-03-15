import { NextResponse } from 'next/server'
import { runAxonCommandWs } from '@/lib/axon-ws-exec'
import { apiError } from '@/lib/server/api-error'
import { logError } from '@/lib/server/logger'

export const dynamic = 'force-dynamic'

export async function GET() {
  try {
    const data = await runAxonCommandWs('status', 30_000)
    return NextResponse.json({ ok: true, data })
  } catch (err) {
    logError('api.cortex.status.failed', {
      message: err instanceof Error ? err.message : String(err),
    })
    return apiError(500, 'Failed to fetch job queue status', { code: 'cortex_status' })
  }
}
