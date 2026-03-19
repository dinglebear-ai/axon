import { Pool } from 'pg'
import { normalizeLocalServiceUrl } from '@/lib/server/service-url'

const DEFAULT_AXON_PG_URL = 'postgresql://axon:postgres@127.0.0.1:53432/axon'

type GlobalWithPgPool = typeof globalThis & {
  __axonJobsPgPool?: Pool
}

const globalWithPgPool = globalThis as GlobalWithPgPool

function createPool(): Pool {
  const rawConnectionString =
    process.env.AXON_PG_URL ?? process.env.AXON_PG_MCP_URL ?? DEFAULT_AXON_PG_URL
  const connectionString = normalizeLocalServiceUrl(rawConnectionString) ?? rawConnectionString
  const pool = new Pool({
    connectionString,
    max: 5,
    connectionTimeoutMillis: 5_000,
    idleTimeoutMillis: 30_000,
  })

  pool.on('connect', (client) => {
    client.query("SET statement_timeout = '15000'").catch(() => undefined)
  })

  return pool
}

export function getJobsPgPool(): Pool {
  if (!globalWithPgPool.__axonJobsPgPool) {
    globalWithPgPool.__axonJobsPgPool = createPool()
  }
  return globalWithPgPool.__axonJobsPgPool
}
