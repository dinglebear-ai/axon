import { PassThrough, type Readable } from 'node:stream'
import Dockerode from 'dockerode'
import type { NextRequest } from 'next/server'
import { logError, logWarn } from '@/lib/server/logger'
import { enforceRateLimit } from '@/lib/server/rate-limit'

// biome-ignore lint/suspicious/noControlCharactersInRegex: intentional ANSI escape sequence stripping
const ANSI_RE = /\x1b\[[0-9;]*[mGKHFJABCDfnsuhl]/g
function stripAnsi(s: string): string {
  return s.replace(ANSI_RE, '')
}

export const dynamic = 'force-dynamic'

export const SERVICES = [
  'axon-workers',
  'axon-web',
  'axon-postgres',
  'axon-redis',
  'axon-rabbitmq',
  'axon-qdrant',
  'axon-chrome',
] as const

const ALLOWED_SERVICES = new Set<string>(SERVICES)

const ENABLE_DOCKER_SOCKET_LOGS = process.env.AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS === 'true'
const DOCKER_SOCKET_PATH = process.env.AXON_WEB_DOCKER_SOCKET_PATH ?? '/var/run/docker.sock'

type SendLine = (line: string, service?: string) => void

function attachContainerStream(
  svc: string,
  tail: number,
  sendLine: SendLine,
  docker: Dockerode,
  logStreams: Readable[],
  onEnd: () => void,
): void {
  docker
    .getContainer(svc)
    .logs({ follow: true, stdout: true, stderr: true, tail })
    .then((raw) => {
      const logStream = raw as Readable
      logStreams.push(logStream)

      const pt = new PassThrough()
      docker.modem.demuxStream(logStream, pt, pt)

      pt.on('data', (chunk: Buffer) => {
        for (const line of chunk.toString().split('\n')) {
          const clean = stripAnsi(line)
          if (clean.trim()) sendLine(clean, svc)
        }
      })
      pt.on('error', (err: Error) => {
        logError('api.logs.stream_error', { service: svc, message: err.message })
        sendLine(`[stream error] ${err.message}`, svc)
        onEnd()
      })
      pt.on('end', () => {
        onEnd()
      })
    })
    .catch((err: unknown) => {
      logError('api.logs.attach_failed', {
        service: svc,
        message: err instanceof Error ? err.message : String(err),
      })
      sendLine(`[stream error] ${err instanceof Error ? err.message : String(err)}`, svc)
      onEnd()
    })
}

export async function GET(req: NextRequest) {
  const limited = enforceRateLimit('api.logs', req, { max: 30, windowMs: 60_000 })
  if (limited) return limited

  if (!ENABLE_DOCKER_SOCKET_LOGS) {
    return new Response('Docker socket log streaming is disabled', { status: 503 })
  }

  const service = req.nextUrl.searchParams.get('service') ?? 'axon-workers'
  const tail = Math.min(Number(req.nextUrl.searchParams.get('tail') ?? '200'), 1000)
  const isAll = service === 'all'

  if (!isAll && !ALLOWED_SERVICES.has(service)) {
    return new Response('Invalid service', { status: 400 })
  }

  if (!Number.isFinite(tail) || tail < 1) {
    return new Response('Invalid tail value', { status: 400 })
  }

  const targets: string[] = isAll ? [...SERVICES] : [service]
  const encoder = new TextEncoder()
  const logStreams: Readable[] = []
  const docker = new Dockerode({ socketPath: DOCKER_SOCKET_PATH })

  const stream = new ReadableStream({
    start(controller) {
      function sendLine(line: string, svc?: string) {
        const payload = JSON.stringify({
          line,
          ts: Date.now(),
          ...(svc && isAll ? { service: svc } : {}),
        })
        controller.enqueue(encoder.encode(`data: ${payload}\n\n`))
      }

      function close() {
        try {
          controller.close()
        } catch {
          // already closed
        }
      }

      let activeStreams = targets.length
      const onStreamEnd = () => {
        activeStreams -= 1
        if (activeStreams <= 0) close()
      }

      for (const svc of targets) {
        attachContainerStream(svc, tail, sendLine, docker, logStreams, onStreamEnd)
      }

      req.signal.addEventListener('abort', () => {
        logWarn('api.logs.stream_aborted', { service, targetCount: targets.length })
        for (const s of logStreams) s.destroy()
        close()
      })
    },
  })

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
    },
  })
}
