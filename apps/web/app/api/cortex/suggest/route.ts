import { NextResponse } from 'next/server'
import { runAxonCommandWs } from '@/lib/axon-ws-exec'
import { apiError } from '@/lib/server/api-error'
import { logError } from '@/lib/server/logger'

export const dynamic = 'force-dynamic'

export async function GET(req: Request) {
  try {
    const { searchParams } = new URL(req.url)
    const focus = (searchParams.get('q') ?? '').trim()
    const data = await runAxonCommandWs('suggest', 60_000, focus)
    return NextResponse.json({ ok: true, data })
  } catch (err) {
    logError('api.cortex.suggest.failed', {
      message: err instanceof Error ? err.message : String(err),
    })
    return apiError(500, 'Failed to generate crawl suggestions', { code: 'cortex_suggest' })
  }
}
