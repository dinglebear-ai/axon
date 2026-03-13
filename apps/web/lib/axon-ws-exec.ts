/**
 * Execute an axon command via the axon-workers WebSocket bridge.
 *
 * Rather than spawning the axon binary from axon-web (where the binary may
 * not exist), this connects to the WS execution bridge that runs inside the
 * axon-workers container, which always has the binary available.
 *
 * Connection strategy: module-level singleton with multiplexed request routing.
 * A single persistent WebSocket connection is shared across all callers. Each
 * request gets a unique exec_id (correlation ID) so responses can be routed
 * back to the correct pending caller. The connection is kept alive for the
 * lifetime of the process and re-established automatically on close/error.
 */

const WORKERS_WS_URL =
  process.env.AXON_WORKERS_WS_URL ??
  process.env.NEXT_PUBLIC_AXON_WS_URL ??
  process.env.AXON_BACKEND_URL?.replace(/^http/i, 'ws').replace(/\/$/, '').concat('/ws') ??
  `ws://127.0.0.1:${process.env.NEXT_PUBLIC_AXON_PORT || '49000'}/ws`
const WORKERS_WS_TOKEN = process.env.AXON_WEB_API_TOKEN?.trim() ?? ''

const buildWorkersWsUrl = (): string => {
  if (!WORKERS_WS_TOKEN) return WORKERS_WS_URL
  try {
    const url = new URL(WORKERS_WS_URL)
    if (!url.searchParams.has('token')) {
      url.searchParams.set('token', WORKERS_WS_TOKEN)
    }
    return url.toString()
  } catch {
    // Fallback for malformed env URL values; preserve current behavior.
    return WORKERS_WS_URL
  }
}

interface WsMessageEvent {
  data: unknown
}

interface WsCloseEvent {
  code: number
}

interface WsLike {
  addEventListener(type: 'open', listener: () => void): void
  addEventListener(type: 'message', listener: (event: WsMessageEvent) => void): void
  addEventListener(type: 'error', listener: () => void): void
  addEventListener(type: 'close', listener: (event: WsCloseEvent) => void): void
  close(): void
  send(data: string): void
}

type WebSocketConstructor = new (url: string) => WsLike

export interface RunAxonCommandWsStreamOptions {
  timeoutMs?: number
  input?: string
  flags?: Record<string, string | boolean>
  signal?: AbortSignal
  onJson?: (data: unknown) => void
  onOutputLine?: (line: string) => void
  onDone?: (payload: { exit_code: number; elapsed_ms?: number }) => void
  onError?: (payload: { message: string; elapsed_ms?: number }) => void
}

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

const MAX_PENDING = 100

// ---------------------------------------------------------------------------
// Correlation-ID helpers
// ---------------------------------------------------------------------------

let _execIdCounter = 0

function nextExecId(): string {
  _execIdCounter += 1
  return `ws-exec-${_execIdCounter}`
}

// ---------------------------------------------------------------------------
// Pending request registry
// ---------------------------------------------------------------------------

interface PendingRequest {
  options: RunAxonCommandWsStreamOptions
  resolve: () => void
  reject: (err: Error) => void
  settled: boolean
  timer: ReturnType<typeof setTimeout> | undefined
}

const _pending = new Map<string, PendingRequest>()

function settlePending(execId: string, err?: Error): void {
  const pending = _pending.get(execId)
  if (!pending || pending.settled) return
  pending.settled = true
  clearTimeout(pending.timer)
  _pending.delete(execId)
  if (err) {
    pending.reject(err)
  } else {
    pending.resolve()
  }
}

// ---------------------------------------------------------------------------
// WebSocket constructor resolution (Node.js / browser)
// ---------------------------------------------------------------------------

let _wsConstructorCache: WebSocketConstructor | null = null

async function resolveWebSocketConstructor(): Promise<WebSocketConstructor> {
  if (_wsConstructorCache) return _wsConstructorCache

  const nativeConstructor = globalThis.WebSocket as unknown as WebSocketConstructor | undefined
  if (nativeConstructor) {
    _wsConstructorCache = nativeConstructor
    return nativeConstructor
  }

  // Use dynamic module name to avoid type-check coupling to ws type declarations.
  const wsModuleName = 'ws'
  const wsModule = (await import(wsModuleName)) as {
    WebSocket?: WebSocketConstructor
    default?: WebSocketConstructor
  }
  if (wsModule.WebSocket) {
    _wsConstructorCache = wsModule.WebSocket
    return wsModule.WebSocket
  }
  if (wsModule.default) {
    _wsConstructorCache = wsModule.default
    return wsModule.default
  }

  throw new Error('WebSocket runtime is unavailable. Install ws or use Node.js 22+.')
}

// ---------------------------------------------------------------------------
// Singleton persistent connection
// ---------------------------------------------------------------------------

let _ws: WsLike | null = null
let _connectPromise: Promise<WsLike> | null = null
// Consecutive error-event count since the last successful open.
// Informational only — logged in the error handler. Reconnection is implicit:
// when _ws is null or closed, the next getConnection() call opens a new socket.
let _reconnectAttempts = 0

function handleIncomingMessage(event: WsMessageEvent): void {
  try {
    const parsed = JSON.parse(String(event.data)) as { type?: unknown; data?: unknown }
    const type = typeof parsed.type === 'string' ? parsed.type : ''
    const data =
      parsed.data && typeof parsed.data === 'object' && !Array.isArray(parsed.data)
        ? (parsed.data as Record<string, unknown>)
        : null

    // Extract the correlation exec_id from the ctx field (server echoes it back).
    const ctx =
      data?.ctx && typeof data.ctx === 'object' && !Array.isArray(data.ctx)
        ? (data.ctx as Record<string, unknown>)
        : null
    const execId = typeof ctx?.exec_id === 'string' ? ctx.exec_id : null

    if (!execId) return

    const pending = _pending.get(execId)
    if (!pending) return

    if (type === 'command.output.json') {
      const outputData = data && data.data !== undefined ? data.data : data
      pending.options.onJson?.(outputData)
      return
    }
    if (type === 'command.output.line') {
      pending.options.onOutputLine?.(typeof data?.line === 'string' ? data.line : '')
      return
    }
    if (type === 'command.done') {
      const payload =
        data?.payload && typeof data.payload === 'object' && !Array.isArray(data.payload)
          ? (data.payload as Record<string, unknown>)
          : null
      pending.options.onDone?.({
        exit_code: typeof payload?.exit_code === 'number' ? payload.exit_code : 0,
        elapsed_ms: typeof payload?.elapsed_ms === 'number' ? payload.elapsed_ms : undefined,
      })
      settlePending(execId)
      return
    }
    if (type === 'command.error') {
      const payload =
        data?.payload && typeof data.payload === 'object' && !Array.isArray(data.payload)
          ? (data.payload as Record<string, unknown>)
          : null
      pending.options.onError?.({
        message:
          typeof payload?.message === 'string' && payload.message.length > 0
            ? payload.message
            : 'axon command failed',
        elapsed_ms: typeof payload?.elapsed_ms === 'number' ? payload.elapsed_ms : undefined,
      })
      settlePending(execId)
    }
  } catch {
    /* ignore non-JSON messages */
  }
}

function failAllPending(err: Error): void {
  for (const execId of _pending.keys()) {
    settlePending(execId, err)
  }
}

function getConnection(): Promise<WsLike> {
  if (_ws) {
    // Check readyState if available (native WebSocket / ws package both expose it).
    const state = (_ws as unknown as { readyState?: number }).readyState
    if (state === undefined || state === 1 /* OPEN */) {
      return Promise.resolve(_ws)
    }
    // Socket exists but is no longer open; drop the reference.
    _ws = null
  }

  if (_connectPromise) return _connectPromise

  // Assign _connectPromise synchronously before any await so that concurrent
  // callers coalesce onto a single connection attempt rather than each spawning
  // a new WebSocket. Using .then() chains instead of async/await guarantees the
  // assignment happens in the current microtask, not after an await suspension.
  const workersWsUrl = buildWorkersWsUrl()

  _connectPromise = resolveWebSocketConstructor()
    .then((WebSocketImpl) => {
      return new Promise<WsLike>((resolve, reject) => {
        const ws = new WebSocketImpl(workersWsUrl)

        ws.addEventListener('open', () => {
          _reconnectAttempts = 0
          _ws = ws
          _connectPromise = null
          try {
            const { hostname, pathname } = new URL(workersWsUrl)
            console.log(`[axon-ws] persistent connection established to ${hostname}${pathname}`)
          } catch {
            console.log('[axon-ws] persistent connection established')
          }
          resolve(ws)
        })

        ws.addEventListener('message', handleIncomingMessage)

        ws.addEventListener('error', () => {
          _reconnectAttempts += 1
          console.error(
            `[axon-ws] connection error (consecutive error count: ${_reconnectAttempts})`,
          )
        })

        ws.addEventListener('close', (event) => {
          console.log(`[axon-ws] connection closed (code ${event.code})`)
          if (_ws === ws) _ws = null
          _connectPromise = null
          // Fail all in-flight requests — they will reconnect on next call.
          failAllPending(new Error(`WebSocket closed unexpectedly (code ${event.code})`))
          reject(new Error(`WebSocket closed before open (code ${event.code})`))
        })
      })
    })
    .catch((error: unknown) => {
      _connectPromise = null
      const reason = error instanceof Error ? error.message : 'unknown error'
      throw new Error(`Failed to initialize WebSocket runtime: ${reason}`)
    })

  return _connectPromise
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Run a synchronous axon command via the axon-workers WS bridge and return
 * the parsed JSON result. Rejects on timeout, connection error, or if the
 * command itself fails.
 */
export async function runAxonCommandWs(
  mode: string,
  timeoutMs = 30_000,
  input = '',
  flags: Record<string, string | boolean> = {},
): Promise<unknown> {
  let result: unknown
  let commandErrorMessage: string | null = null

  await runAxonCommandWsStream(mode, {
    timeoutMs,
    input,
    flags,
    onJson: (data) => {
      result = data
    },
    onError: (payload) => {
      commandErrorMessage = payload.message
    },
  })

  if (commandErrorMessage) {
    throw new Error(commandErrorMessage)
  }
  return result
}

export async function runAxonCommandWsStream(
  mode: string,
  options: RunAxonCommandWsStreamOptions = {},
): Promise<void> {
  const timeoutMs = options.timeoutMs ?? 30_000
  const input = options.input ?? ''
  const flags = options.flags ?? {}
  const abortSignal = options.signal

  const execId = nextExecId()

  return new Promise<void>((resolve, reject) => {
    if (_pending.size >= MAX_PENDING) {
      reject(new Error('too many in-flight requests — axon-ws-exec pending map full'))
      return
    }

    const finish = (err?: Error) => {
      settlePending(execId, err)
      abortSignal?.removeEventListener('abort', onAbort)
    }

    const onAbort = () => {
      // Send a cancel message to the Rust server so the server-side command is
      // stopped and does not continue consuming resources until timeout.
      const currentWs = _ws
      if (currentWs) {
        const state = (currentWs as unknown as { readyState?: number }).readyState
        if (state === undefined || state === 1 /* OPEN */) {
          try {
            currentWs.send(JSON.stringify({ type: 'cancel', id: execId, mode }))
          } catch {
            /* ignore send errors during abort — we still settle the promise */
          }
        }
      }
      finish(new Error(`axon ${mode} request aborted`))
    }

    if (abortSignal?.aborted) {
      reject(new Error(`axon ${mode} request aborted`))
      return
    }
    abortSignal?.addEventListener('abort', onAbort, { once: true })

    const timer = setTimeout(
      () => finish(new Error(`Timeout waiting for axon ${mode} (${timeoutMs}ms)`)),
      timeoutMs,
    )

    const pending: PendingRequest = {
      options,
      resolve: () => {
        abortSignal?.removeEventListener('abort', onAbort)
        resolve()
      },
      reject: (err: Error) => {
        abortSignal?.removeEventListener('abort', onAbort)
        reject(err)
      },
      settled: false,
      timer,
    }

    // NOTE: We add to _pending BEFORE the connection resolves intentionally.
    // This ensures that a fast failAllPending() (e.g., on connection error during getConnection())
    // correctly rejects this entry rather than leaving it dangling.
    _pending.set(execId, pending)

    getConnection()
      .then((conn) => {
        if (pending.settled) return
        try {
          console.log(`[axon-ws] executing mode=${mode} exec_id=${execId}`)
          conn.send(JSON.stringify({ type: 'execute', mode, input, flags, exec_id: execId }))
        } catch (err) {
          finish(
            new Error(
              `Failed to send execute message: ${err instanceof Error ? err.message : String(err)}`,
            ),
          )
        }
      })
      .catch((err: unknown) => {
        if (!pending.settled) {
          finish(
            new Error(
              `WebSocket connection error (${WORKERS_WS_URL}): ${err instanceof Error ? err.message : String(err)}`,
            ),
          )
        }
      })
  })
}

/**
 * Close the singleton connection and reset module state.
 * Intended for test teardown — do not call in production code.
 */
export function closeConnection(): void {
  try {
    _ws?.close()
  } catch {
    /* ignore */
  }
  _ws = null
  _connectPromise = null
  _wsConstructorCache = null
  _reconnectAttempts = 0
  failAllPending(new Error('Connection closed by closeConnection()'))
  _pending.clear()
}
