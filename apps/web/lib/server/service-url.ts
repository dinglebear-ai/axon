import { existsSync } from 'node:fs'

const DOCKER_HOST_MAP: Record<string, string> = {
  'axon-postgres:5432': '127.0.0.1:53432',
  'axon-redis:6379': '127.0.0.1:53379',
  'axon-rabbitmq:5672': '127.0.0.1:45535',
  'axon-qdrant:6333': '127.0.0.1:53333',
  'axon-qdrant:6334': '127.0.0.1:53334',
  'axon-tei:80': '127.0.0.1:52000',
  'axon-chrome:6000': '127.0.0.1:6000',
  'axon-chrome:9222': '127.0.0.1:9222',
}

type NormalizeUrlOptions = {
  runningInDocker?: boolean
}

function defaultPortForProtocol(protocol: string): string {
  return protocol === 'https:' ? '443' : '80'
}

function runningInDocker(): boolean {
  return existsSync('/.dockerenv')
}

export function normalizeLocalServiceUrl(
  raw: string | undefined,
  options: NormalizeUrlOptions = {},
): string | undefined {
  if (!raw) return raw
  if (options.runningInDocker ?? runningInDocker()) return raw

  try {
    const parsed = new URL(raw)
    const hostPort = `${parsed.hostname}:${parsed.port || defaultPortForProtocol(parsed.protocol)}`
    const mapped = DOCKER_HOST_MAP[hostPort]
    if (!mapped) return raw

    const [host, port] = mapped.split(':') as [string, string]
    parsed.hostname = host
    parsed.port = port
    return parsed.toString().replace(/\/$/, '')
  } catch {
    return raw
  }
}
