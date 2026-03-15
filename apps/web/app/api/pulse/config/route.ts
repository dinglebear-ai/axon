import { z } from 'zod'
import { runAxonCommandWsStream } from '@/lib/axon-ws-exec'
import { ensureRepoRootEnvLoaded } from '@/lib/pulse/server-env'
import { AcpConfigOption, PulseAgent } from '@/lib/pulse/types'
import { apiError, makeErrorId } from '@/lib/server/api-error'
import { logError } from '@/lib/server/logger'

const PulseConfigProbeRequestSchema = z.object({
  agent: PulseAgent.default('codex'),
  sessionId: z
    .string()
    .regex(/^[0-9a-f-]{8,64}$/i)
    .optional(),
  model: z.string().optional(),
})

// In-memory cache for config probe results. The probe spawns a full adapter lifecycle
// just to read config options, then tears everything down. Caching avoids repeating
// that expensive cycle on every settings panel render or page navigation.
const CONFIG_CACHE = new Map<
  string,
  { options: z.infer<typeof AcpConfigOption>[]; expires: number }
>()
const CONFIG_CACHE_TTL = 60_000 // 60 seconds

// In-flight probe coalescing: parallel requests for the same agent skip the
// duplicate ACP lifecycle and share the result of the first in-flight probe.
const IN_FLIGHT = new Map<string, Promise<z.infer<typeof AcpConfigOption>[]>>()

function normalizeConfigOptionsPayload(payload: unknown) {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return null
  const record = payload as Record<string, unknown>
  const type = typeof record.type === 'string' ? record.type : ''
  if (type !== 'config_options_update' && type !== 'config_option_update') {
    return null
  }
  const parsed = z.array(AcpConfigOption).safeParse(record.configOptions)
  return parsed.success ? parsed.data : null
}

export async function POST(request: Request) {
  ensureRepoRootEnvLoaded()

  let body: unknown
  try {
    body = await request.json()
  } catch {
    return apiError(400, 'Request body must be valid JSON')
  }

  const parsed = PulseConfigProbeRequestSchema.safeParse(body)
  if (!parsed.success) {
    return apiError(400, parsed.error.issues[0]?.message ?? 'Invalid request payload')
  }

  const req = parsed.data

  // Return cached config if still fresh — avoids spawning a full adapter lifecycle.
  const cacheKey = `${req.agent}:${req.model ?? 'default'}:${req.sessionId ?? 'default'}`
  const cached = CONFIG_CACHE.get(cacheKey)
  if (cached && cached.expires > Date.now()) {
    return Response.json({ configOptions: cached.options })
  }

  // Coalesce: if there's already an in-flight probe for this cache key, wait for it
  // instead of spawning a duplicate ACP adapter lifecycle.
  const existing = IN_FLIGHT.get(cacheKey)
  if (existing) {
    try {
      const configOptions = await existing
      return Response.json({ configOptions })
    } catch (error: unknown) {
      const errorId = makeErrorId('pulse-config')
      const message = error instanceof Error ? error.message : String(error)
      return apiError(502, 'ACP config probe failed', {
        code: 'pulse_config_probe_failed',
        errorId,
        detail: message,
      })
    }
  }

  const probePromise = (async (): Promise<z.infer<typeof AcpConfigOption>[]> => {
    let configOptions = [] as z.infer<typeof AcpConfigOption>[]
    let probeErrorMessage: string | null = null

    const flags: Record<string, string> = { agent: req.agent }
    if (req.sessionId) flags.session_id = req.sessionId
    if (req.model && req.model !== 'default') flags.model = req.model

    await runAxonCommandWsStream('pulse_chat_probe', {
      timeoutMs: 60_000,
      input: '',
      flags,
      onJson: (payload) => {
        const parsedOptions = normalizeConfigOptionsPayload(payload)
        if (parsedOptions) configOptions = parsedOptions
      },
      onError: (payload) => {
        probeErrorMessage = payload.message
      },
    })

    if (probeErrorMessage) {
      throw new Error(probeErrorMessage)
    }

    const now = Date.now()
    for (const [key, value] of CONFIG_CACHE) {
      if (value.expires <= now) CONFIG_CACHE.delete(key)
    }
    CONFIG_CACHE.set(cacheKey, { options: configOptions, expires: now + CONFIG_CACHE_TTL })
    return configOptions
  })()

  IN_FLIGHT.set(cacheKey, probePromise)

  try {
    const configOptions = await probePromise
    return Response.json({ configOptions })
  } catch (error: unknown) {
    const errorId = makeErrorId('pulse-config')
    const message = error instanceof Error ? error.message : String(error)
    logError('api.pulse.config.probe_failed', {
      errorId,
      message,
      agent: req.agent,
      sessionId: req.sessionId ?? null,
      model: req.model ?? null,
    })
    return apiError(502, 'ACP config probe failed', {
      code: 'pulse_config_probe_failed',
      errorId,
      detail: message,
    })
  } finally {
    IN_FLIGHT.delete(cacheKey)
  }
}
