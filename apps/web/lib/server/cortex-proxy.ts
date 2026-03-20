import { NextResponse } from 'next/server'
import { runAxonCommandWs } from '@/lib/axon-ws-exec'
import { apiError } from '@/lib/server/api-error'
import { logError } from '@/lib/server/logger'

/**
 * Factory for cortex proxy GET handlers.
 * All five cortex routes (doctor, domains, sources, stats, status) share the
 * same pattern: call the Rust backend via WS, wrap in `{ ok, data }`, handle errors.
 */
export function createCortexProxyHandler(
  command: string,
  timeoutMs: number,
  errorCode: string,
  errorMessage: string,
) {
  return async function GET() {
    try {
      const data = await runAxonCommandWs(command, timeoutMs)
      return NextResponse.json({ ok: true, data })
    } catch (err) {
      logError(`api.cortex.${command}.failed`, {
        error:
          err instanceof Error
            ? { message: err.message, name: err.name, stack: err.stack }
            : String(err),
      })
      return apiError(500, errorMessage, { code: errorCode })
    }
  }
}
