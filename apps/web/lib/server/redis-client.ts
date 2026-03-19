import { createClient, type RedisClientType } from 'redis'
import { logError, logInfo } from '@/lib/server/logger'
import { normalizeLocalServiceUrl } from '@/lib/server/service-url'

let client: RedisClientType | null = null
let connectPromise: Promise<void> | null = null

function buildRedisUrl(): string | null {
  const raw = process.env.AXON_REDIS_URL?.trim()
  if (!raw) return null
  return normalizeLocalServiceUrl(raw) ?? raw
}

export function getRedisClient(): RedisClientType | null {
  const redisUrl = buildRedisUrl()
  if (!redisUrl) return null

  if (!client) {
    client = createClient({ url: redisUrl })
    client.on('error', (error) => {
      logError('redis.client_error', {
        message: error instanceof Error ? error.message : String(error),
      })
    })
    client.on('ready', () => {
      logInfo('redis.client_ready')
    })
  }

  if (!client.isOpen && !connectPromise) {
    connectPromise = client
      .connect()
      .then(() => {
        connectPromise = null
      })
      .catch((error) => {
        connectPromise = null
        logError('redis.client_connect_failed', {
          message: error instanceof Error ? error.message : String(error),
        })
      })
  }

  return client
}
