import type { SessionFile } from '@/lib/sessions/session-scanner'
import { scanSessions } from '@/lib/sessions/session-scanner'

interface CacheEntry {
  /** When the cached data was last successfully fetched. */
  fetchedAt: number
  sessions: SessionFile[]
}

const cache = new Map<string, CacheEntry>()
const inflight = new Map<string, Promise<SessionFile[]>>()

interface SessionCacheOptions {
  assistantMode: boolean
  limit: number
  perAgentLimit: number
  ttlMs?: number
  forceRefresh?: boolean
}

function cacheKey(options: SessionCacheOptions): string {
  return `${options.assistantMode ? 'assistant' : 'default'}:${options.limit}:${options.perAgentLimit}`
}

/**
 * Stale-while-revalidate session cache.
 *
 * - If fresh data exists (within ttlMs), return it immediately.
 * - If stale data exists (past ttlMs), return it AND kick off a background
 *   refresh so the next caller gets fresh data. This avoids the 3–5 s cold-scan
 *   penalty that makes the session list appear to "flicker" or show "Loading…".
 * - If no data exists at all (first call), wait for the scan to complete.
 * - Concurrent callers for the same key share a single in-flight promise
 *   (thundering-herd protection).
 */
export async function getCachedSessions(options: SessionCacheOptions): Promise<SessionFile[]> {
  const key = cacheKey(options)
  const now = Date.now()
  const ttlMs = options.ttlMs ?? 30_000

  // forceRefresh: bypass stale cache entirely, always wait for fresh scan
  if (options.forceRefresh) {
    const pending = inflight.get(key)
    if (pending) return pending
    const promise = scanSessions(options.limit, options.perAgentLimit, {
      assistantMode: options.assistantMode,
    })
      .then((sessions) => {
        cache.set(key, { sessions, fetchedAt: Date.now() })
        inflight.delete(key)
        return sessions
      })
      .catch((err) => {
        inflight.delete(key)
        throw err
      })
    inflight.set(key, promise)
    return promise
  }

  const hit = cache.get(key)

  // Fresh cache — return immediately
  if (hit && now - hit.fetchedAt < ttlMs) {
    return hit.sessions
  }

  // Stale cache — return stale data, refresh in background
  if (hit) {
    if (!inflight.has(key)) {
      const promise = scanSessions(options.limit, options.perAgentLimit, {
        assistantMode: options.assistantMode,
      })
        .then((sessions) => {
          cache.set(key, { sessions, fetchedAt: Date.now() })
          return sessions
        })
        .finally(() => inflight.delete(key))
      inflight.set(key, promise)
    }
    return hit.sessions
  }

  // Cold start — no cached data at all. Must wait for the scan.
  const pending = inflight.get(key)
  if (pending) return pending

  const promise = scanSessions(options.limit, options.perAgentLimit, {
    assistantMode: options.assistantMode,
  })
    .then((sessions) => {
      cache.set(key, { sessions, fetchedAt: Date.now() })
      inflight.delete(key)
      return sessions
    })
    .catch((err) => {
      inflight.delete(key)
      throw err
    })

  inflight.set(key, promise)
  return promise
}
