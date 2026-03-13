'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { createClientId } from '@/lib/client-id'
import type { AcpConfigOption } from '@/lib/pulse/types'
import { getSessionItem, setSessionItem } from '@/lib/storage'
import type { WsUsageStats } from '@/lib/ws-protocol'
import { advanceAcpSessionLifecycle } from './use-axon-acp/session-id-lifecycle'
import { useFlushBufferedStream, useScheduleFlush } from './use-axon-acp/stream-flush'
import { handleAcpWsMessage, isAcpRelevantWsMessage } from './use-axon-acp/ws-handler'
import type { AxonMessage } from './use-axon-session'
import { useAxonWs } from './use-axon-ws'

// Re-export for consumers and tests that import directly from this module.
export { handleEditorMsg } from './use-axon-acp/editor-handler'

const STREAMING_TIMEOUT_MS = 300_000
const ACP_SESSION_STORAGE_KEY = 'axon-acp-session-id'

/** @deprecated Use {@link createClientId} from `@/lib/client-id` directly. */
export function createClientMessageId(prefix: string): string {
  return createClientId(prefix)
}

interface UseAxonAcpOptions {
  activeSessionId: string | null
  agent?: string
  model?: string
  sessionMode?: string
  enabledMcpServers?: string[]
  blockedMcpTools?: string[]
  /** When true, sends assistant_mode:true so backend uses assistant CWD. */
  assistantMode?: boolean
  handoffContext?: string | null
  onSessionIdChange: (newId: string) => void
  onSessionFallback?: (oldId: string, newId: string) => void
  onMessagesChange: (updater: (prev: AxonMessage[]) => AxonMessage[]) => void
  onAcpConfigOptionsUpdate?: (options: AcpConfigOption[]) => void
  onCommandsUpdate?: (commands: Array<{ name: string; description?: string }>) => void
  onHandoffConsumed?: () => void
  onTurnComplete?: () => void
  onEditorUpdate?: (content: string, operation: 'replace' | 'append') => void
  /** Called when an `editor_update` message is received. Use this to make the editor
   * pane visible on mobile viewports when the agent writes to the document. */
  onShowEditor?: () => void
  /** Called when the ACP bridge emits a `permission_request` event. The consumer
   * should use the provided ids to send a `permission_response` back via the
   * `send` function from `useAxonWs`. */
  onPermissionRequest?: (params: {
    session_id: string
    request_id: string
    tool_name: string
    tool_input: unknown
  }) => void
  enableFs?: boolean
  enableTerminal?: boolean
  permissionTimeoutSecs?: number | null
  adapterTimeoutSecs?: number | null
}

const EMPTY_SERVERS: string[] = []
const EMPTY_TOOLS: string[] = []

export function useAxonAcp({
  activeSessionId,
  agent = 'claude',
  model,
  sessionMode,
  enabledMcpServers = EMPTY_SERVERS,
  blockedMcpTools = EMPTY_TOOLS,
  assistantMode = false,
  handoffContext = null,
  onSessionIdChange,
  onSessionFallback,
  onMessagesChange,
  onAcpConfigOptionsUpdate,
  onCommandsUpdate,
  onHandoffConsumed,
  onTurnComplete,
  onEditorUpdate,
  onShowEditor,
  onPermissionRequest,
  enableFs = true,
  enableTerminal = true,
  permissionTimeoutSecs = null,
  adapterTimeoutSecs = null,
}: UseAxonAcpOptions) {
  const [isStreaming, setIsStreaming] = useState(false)
  const streamingIdRef = useRef<string | null>(null)
  const streamingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingDeltaRef = useRef('')
  const pendingThinkingRef = useRef<string[]>([])
  // Pending usage/location patches accumulated during a flush window.
  // Applied alongside delta+thinking in a single prev.map() at flush time,
  // avoiding a second React state update per assistant_delta event.
  const pendingUsageRef = useRef<WsUsageStats | null>(null)
  const pendingLocationsRef = useRef<{
    toolCallId: string | undefined
    locations: string[]
  } | null>(null)
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const turnStartAtRef = useRef<number | null>(null)
  const firstDeltaAtRef = useRef<number | null>(null)
  const streamedCharsRef = useRef(0)
  const agentRef = useRef(agent)
  const modelRef = useRef(model)
  const sessionModeRef = useRef(sessionMode)
  const { send, subscribeByTypes, status } = useAxonWs()
  const connected = status === 'connected'
  const wasConnectedRef = useRef(false)
  const lifecycleStateRef = useRef<
    | 'idle'
    | 'resume_requested'
    | 'turn_in_flight'
    | 'session_bound'
    | 'fallback_applied'
    | 'resume_failed'
  >('idle')

  useEffect(() => {
    agentRef.current = agent
  }, [agent])
  useEffect(() => {
    modelRef.current = model
  }, [model])
  useEffect(() => {
    sessionModeRef.current = sessionMode
  }, [sessionMode])

  // Persist activeSessionId to sessionStorage for reconnect
  useEffect(() => {
    if (activeSessionId) {
      setSessionItem(ACP_SESSION_STORAGE_KEY, activeSessionId)
    }
  }, [activeSessionId])

  // On WS reconnect, attempt to resume the cached ACP session
  useEffect(() => {
    if (connected && wasConnectedRef.current === false) {
      // First connect or reconnect — try resume
      const storedId = getSessionItem(ACP_SESSION_STORAGE_KEY)
      if (storedId && !isStreaming) {
        send({ type: 'acp_resume', session_id: storedId })
        lifecycleStateRef.current = advanceAcpSessionLifecycle(
          lifecycleStateRef.current,
          'request_resume',
        )
      }
    }
    wasConnectedRef.current = connected
  }, [connected, send, isStreaming])

  const flushRefs = {
    streamingIdRef,
    pendingDeltaRef,
    pendingThinkingRef,
    pendingUsageRef,
    pendingLocationsRef,
    flushTimerRef,
  }
  const flushBufferedStream = useFlushBufferedStream(flushRefs, onMessagesChange)
  const scheduleFlushBufferedStream = useScheduleFlush(flushTimerRef, flushBufferedStream)

  useEffect(() => {
    const unsubscribe = subscribeByTypes(
      [
        'assistant_delta',
        'usage_update',
        'thinking_content',
        'session_fallback',
        'result',
        'error',
        'tool_use',
        'tool_use_update',
        'editor_update',
        'config_options_update',
        'config_option_update',
        'commands_update',
        'acp_resume_result',
        'permission_request',
        'command.output.json',
      ],
      (rawMsg) => {
        if (!isAcpRelevantWsMessage(rawMsg)) return
        handleAcpWsMessage(
          rawMsg,
          {
            streamingIdRef,
            streamingTimeoutRef,
            turnStartAtRef,
            firstDeltaAtRef,
            streamedCharsRef,
            pendingDeltaRef,
            pendingThinkingRef,
            pendingUsageRef,
            pendingLocationsRef,
          },
          {
            setIsStreaming,
            onMessagesChange,
            onSessionIdChange: (newId) => {
              lifecycleStateRef.current = advanceAcpSessionLifecycle(
                lifecycleStateRef.current,
                'bind_session',
              )
              onSessionIdChange(newId)
            },
            onSessionFallback: onSessionFallback
              ? (oldId, newId) => {
                  lifecycleStateRef.current = advanceAcpSessionLifecycle(
                    lifecycleStateRef.current,
                    'apply_fallback',
                  )
                  onSessionFallback(oldId, newId)
                }
              : undefined,
            onAcpConfigOptionsUpdate,
            onCommandsUpdate,
            onTurnComplete,
            onResumeSessionOk: () => {
              lifecycleStateRef.current = advanceAcpSessionLifecycle(
                lifecycleStateRef.current,
                'resume_ok',
              )
            },
            onResumeSessionMiss: () => {
              lifecycleStateRef.current = advanceAcpSessionLifecycle(
                lifecycleStateRef.current,
                'resume_miss',
              )
            },
            onPermissionRequest,
            onEditorUpdate,
            onShowEditor,
            flushBufferedStream,
            scheduleFlushBufferedStream,
          },
          { agent: agentRef.current, model: modelRef.current, sessionMode: sessionModeRef.current },
        )
      },
    )
    return () => {
      // Clear pending timeout to prevent setState/onMessagesChange calls
      // after the hook is unmounted.
      if (streamingTimeoutRef.current) {
        clearTimeout(streamingTimeoutRef.current)
        streamingTimeoutRef.current = null
      }
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current)
        flushTimerRef.current = null
      }
      unsubscribe()
    }
  }, [
    subscribeByTypes,
    onMessagesChange,
    onSessionIdChange,
    onSessionFallback,
    onAcpConfigOptionsUpdate,
    onCommandsUpdate,
    onTurnComplete,
    onPermissionRequest,
    onEditorUpdate,
    onShowEditor,
    flushBufferedStream,
    scheduleFlushBufferedStream,
  ])

  const submitPrompt = useCallback(
    (prompt: string) => {
      if (!connected || isStreaming) {
        return
      }

      const userId = createClientMessageId('user')
      const assistantId = createClientMessageId('assistant')
      lifecycleStateRef.current = advanceAcpSessionLifecycle(
        lifecycleStateRef.current,
        'start_turn',
      )
      streamingIdRef.current = assistantId
      turnStartAtRef.current = Date.now()
      firstDeltaAtRef.current = null
      streamedCharsRef.current = 0

      onMessagesChange((prev) => [
        ...prev,
        { id: userId, role: 'user' as const, content: prompt, timestamp: Date.now() },
        {
          id: assistantId,
          role: 'assistant' as const,
          content: '',
          timestamp: Date.now(),
          streaming: true,
        },
      ])
      setIsStreaming(true)

      // Fallback: clear stuck streaming state after timeout
      if (streamingTimeoutRef.current) clearTimeout(streamingTimeoutRef.current)
      streamingTimeoutRef.current = setTimeout(() => {
        const sid = streamingIdRef.current
        setIsStreaming(false)
        streamingIdRef.current = null
        if (sid) {
          onMessagesChange((prev) =>
            prev.map((m) =>
              m.id === sid
                ? {
                    ...m,
                    content: m.content || '⚠ No response received — check agent configuration',
                    streaming: false,
                  }
                : m,
            ),
          )
        }
      }, STREAMING_TIMEOUT_MS)

      const handoffPrefix = handoffContext
        ? `<system-handoff>\n${handoffContext}\n</system-handoff>\n\n`
        : ''
      const wirePrompt = `${handoffPrefix}${prompt}`

      send({
        type: 'execute',
        mode: 'pulse_chat',
        input: wirePrompt,
        flags: {
          ...(activeSessionId ? { session_id: activeSessionId } : {}),
          agent,
          ...(model && model !== 'default' ? { model } : {}),
          ...(sessionMode ? { session_mode: sessionMode } : {}),
          ...(enabledMcpServers.length > 0 ? { mcp_servers: enabledMcpServers } : {}),
          ...(blockedMcpTools.length > 0 ? { blocked_mcp_tools: blockedMcpTools } : {}),
          ...(assistantMode ? { assistant_mode: true } : {}),
          enable_fs: enableFs,
          enable_terminal: enableTerminal,
          ...(permissionTimeoutSecs ? { permission_timeout_secs: permissionTimeoutSecs } : {}),
          ...(adapterTimeoutSecs ? { adapter_timeout_secs: adapterTimeoutSecs } : {}),
        },
      })
      if (handoffContext) onHandoffConsumed?.()
    },
    [
      connected,
      isStreaming,
      activeSessionId,
      agent,
      model,
      sessionMode,
      enabledMcpServers,
      blockedMcpTools,
      assistantMode,
      handoffContext,
      onHandoffConsumed,
      send,
      onMessagesChange,
      adapterTimeoutSecs,
      enableFs,
      enableTerminal,
      permissionTimeoutSecs,
    ],
  )

  return { submitPrompt, isStreaming, connected }
}
