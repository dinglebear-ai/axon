import { useCallback, useEffect, useRef, useState } from 'react'
import { apiFetch } from '@/lib/api-fetch'
import type { PulseMessageBlock, PulseToolUse } from '@/lib/pulse/types'
import type { WsUsageStats } from '@/lib/ws-protocol'

export type ReasoningStep = {
  label: string
  description?: string
  status?: 'complete' | 'active' | 'pending'
}

export interface AxonMessage {
  id: string
  sourceMessageId?: string
  role: 'user' | 'assistant'
  content: string
  timestamp: number
  chainOfThought?: string[]
  files?: string[]
  streaming?: boolean
  blocks?: PulseMessageBlock[]
  toolUses?: PulseToolUse[]
  steps?: ReasoningStep[]
  usage?: WsUsageStats
}

/** @deprecated Use AxonMessage */
export type MessageItem = AxonMessage

interface ParsedMessage {
  role: 'user' | 'assistant'
  content: string
  sourceMessageId?: string
  chainOfThought?: string[]
  blocks?: PulseMessageBlock[]
  toolUses?: PulseToolUse[]
}

interface SessionResponse {
  project: string
  filename: string
  sessionId: string
  messages: ParsedMessage[]
}

interface UseAxonSessionResult {
  messages: AxonMessage[]
  loading: boolean
  /** `true` once the fetch has completed at least once (successfully or with an error).
   * Unlike `loading`, this flag never reverts to `false` on re-fetch — use it to
   * distinguish a legitimately empty session from one that has not yet loaded. */
  loaded: boolean
  error: string | null
  reload: () => void
}
interface UseAxonSessionOptions {
  assistantMode?: boolean
  forceRefresh?: boolean
}

// The Rust ACP adapter prepends a system context block to the first user prompt
// in new sessions. Never show that wrapper in the chat UI.
const USER_MESSAGE_MARKER = '[User message]'

function stripEditorPreamble(content: string): string {
  const markerIdx = content.indexOf(USER_MESSAGE_MARKER)
  if (markerIdx >= 0) {
    return content
      .slice(markerIdx + USER_MESSAGE_MARKER.length)
      .replace(/^\\n+/, '')
      .trimStart()
  }
  // Fallback for malformed wrappers missing [User message].
  if (/^\[System context[^\]]*\]/i.test(content)) {
    return content.replace(/^\[System context[^\]]*\]\s*/i, '').trimStart()
  }
  return content
}

// Retry delays in ms for 404 responses — the session file may not be on disk yet.
export const RETRY_DELAYS_MS = [200, 400, 800, 1600, 3200, 5000]

/**
 * Fetch a session by ID, retrying on 404 (session file may not be on disk yet).
 * Exported so tests can use the production implementation directly instead of
 * maintaining a mirrored copy that can drift from the real logic.
 */
export async function fetchSessionWithRetry(
  sessionId: string,
  isCancelled: () => boolean,
  options: UseAxonSessionOptions = {},
): Promise<SessionResponse> {
  const { assistantMode = false, forceRefresh = false } = options
  const params = new URLSearchParams()
  if (assistantMode) params.set('assistant_mode', '1')
  if (forceRefresh) params.set('fresh', '1')
  const query = params.toString() ? `?${params.toString()}` : ''
  for (let i = 0; i <= RETRY_DELAYS_MS.length; i++) {
    if (isCancelled()) throw new Error('cancelled')
    const res = await apiFetch(`/api/sessions/${encodeURIComponent(sessionId)}${query}`)
    if (res.ok) return res.json() as Promise<SessionResponse>
    if (res.status !== 404 || i === RETRY_DELAYS_MS.length) {
      throw new Error(`Failed to load session: ${res.status}`)
    }
    const delay = RETRY_DELAYS_MS[i] ?? 5000
    console.debug(`[session] 404 for ${sessionId}, retry ${i + 1} in ${delay}ms`)
    await new Promise<void>((resolve) => setTimeout(resolve, delay))
  }
  throw new Error('Session not found after retries')
}

export function useAxonSession(
  sessionId: string | null,
  options: UseAxonSessionOptions = {},
): UseAxonSessionResult {
  const { assistantMode = false } = options
  const [messages, setMessages] = useState<AxonMessage[]>([])
  const [loading, setLoading] = useState(false)
  // `loaded` is set to true once any fetch completes (success or error), and reset
  // to false only when the sessionId changes to a new value. This lets callers
  // distinguish a legitimately empty session from one that has not yet loaded,
  // avoiding derived loading flags that use `messages.length === 0` as a proxy.
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [version, setVersion] = useState(0)
  const prevSessionIdRef = useRef<string | null>(null)
  const prevAssistantModeRef = useRef<boolean | undefined>(undefined)

  const reload = useCallback(() => setVersion((v) => v + 1), [])

  // biome-ignore lint/correctness/useExhaustiveDependencies: version is an intentional reload trigger
  useEffect(() => {
    if (!sessionId) {
      setMessages([])
      setLoading(false)
      setLoaded(false)
      setError(null)
      prevSessionIdRef.current = null
      return
    }

    const sessionChanged = prevSessionIdRef.current !== sessionId
    const modeChanged = prevAssistantModeRef.current !== assistantMode
    prevSessionIdRef.current = sessionId
    prevAssistantModeRef.current = assistantMode
    let cancelled = false
    setLoading(true)
    if (sessionChanged || modeChanged) {
      setMessages([])
      setLoaded(false)
    }
    setError(null)

    fetchSessionWithRetry(sessionId, () => cancelled, { assistantMode })
      .then((data) => {
        if (cancelled) return
        setMessages(
          data.messages.map((msg, i) => ({
            id: msg.sourceMessageId ? `${sessionId}-${msg.sourceMessageId}` : `${sessionId}-${i}`,
            ...(msg.sourceMessageId ? { sourceMessageId: msg.sourceMessageId } : {}),
            role: msg.role,
            content: msg.role === 'user' ? stripEditorPreamble(msg.content) : msg.content,
            timestamp: Date.now(),
            ...(msg.chainOfThought?.length ? { chainOfThought: msg.chainOfThought } : {}),
            ...(msg.blocks?.length ? { blocks: msg.blocks } : {}),
            ...(msg.toolUses?.length ? { toolUses: msg.toolUses } : {}),
          })),
        )
      })
      .catch((err) => {
        if (cancelled) return
        const msg = err instanceof Error ? err.message : 'Failed to load session'
        if (msg === 'cancelled') return
        setError(msg)
        setMessages([])
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false)
          setLoaded(true)
        }
      })

    return () => {
      cancelled = true
    }
  }, [assistantMode, sessionId, version])

  return { messages, loading, loaded, error, reload }
}
