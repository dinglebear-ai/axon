'use client'

import { useCallback, useEffect, useRef } from 'react'
import type { NeuralCanvasHandle } from '@/components/neural-canvas'
import { useAxonAcp } from '@/hooks/use-axon-acp'
import type { AxonMessage } from '@/hooks/use-axon-session'
import { fetchSessionWithRetry } from '@/hooks/use-axon-session'
import { useAxonWs } from '@/hooks/use-axon-ws'
import type { AcpConfigOption } from '@/lib/pulse/types'
import type { ContainerStats, WsServerMsg } from '@/lib/ws-protocol'

export interface AxonShellConnectionParams {
  /** Active ACP session ID (null = new session). */
  chatSessionId: string | null
  /** Agent identifier (e.g. 'claude'). */
  pulseAgent: string
  /** Model override or undefined. */
  pulseModel: string | undefined
  /** ACP session mode string. */
  sessionMode: string
  /** Enabled MCP server names. */
  enabledMcpServers: string[]
  /** Blocked MCP tool names. */
  blockedMcpTools: string[]
  /** Whether sidebar rail is in assistant mode. */
  assistantMode: boolean
  /** Pending handoff context (or null). */
  handoffContext: string | null
  /** Callbacks into session hook. */
  onSessionIdChange: (id: string) => void
  /** Callbacks into messages hook. */
  onMessagesChange: (updater: (prev: AxonMessage[]) => AxonMessage[]) => void
  /** ACP config options update handler. */
  onAcpConfigOptionsUpdate: (options: AcpConfigOption[]) => void
  /** MCP commands update handler. */
  onCommandsUpdate: (commands: Array<{ name: string; description?: string }>) => void
  /** Clear pending handoff context. */
  onHandoffConsumed: () => void
  /** Turn-complete callback (session reload, etc.). */
  onTurnComplete: () => void
  /** Editor update callback (content + operation). */
  onEditorUpdate: (content: string, operation: 'replace' | 'append') => void
  /** Canvas ref for intensity / stimulate calls. */
  canvasRef: React.RefObject<NeuralCanvasHandle | null>
  /** Settings: filesystem access. */
  enableFs: boolean
  /** Settings: terminal access. */
  enableTerminal: boolean
  /** Settings: permission timeout. */
  permissionTimeoutSecs: number | null
  /** Settings: adapter timeout. */
  adapterTimeoutSecs: number | null
  /** Rail mode for assistant-mode session fetch. */
  railMode: string
}

export interface AxonShellConnectionReturn {
  submitPrompt: ReturnType<typeof useAxonAcp>['submitPrompt']
  isStreaming: boolean
  connected: boolean
  handleStats: (data: {
    aggregate: { cpu_percent: number }
    containers: Record<string, ContainerStats>
    container_count: number
  }) => void
  bumpSessionInfoGen: () => void
  isStreamingRef: React.RefObject<boolean>
}

export function useAxonShellConnection(
  params: AxonShellConnectionParams,
): AxonShellConnectionReturn {
  const { send: wsSend, subscribeByTypes: subscribeWsByTypes } = useAxonWs()

  // When true, permission requests are auto-approved by picking the first
  // option. When false (default), the request is ignored and the backend
  // permission prompt times out -- making the problem visible instead of
  // silently approving potentially destructive operations.
  const enableAutoApprove = false

  const onPermissionRequest = useCallback(
    ({
      session_id,
      tool_call_id,
      options,
    }: {
      session_id: string
      tool_call_id: string
      options: string[]
    }) => {
      if (!enableAutoApprove) {
        console.warn(
          `[acp] permission request ignored (auto-approve disabled) tool_call_id=${tool_call_id}`,
        )
        return
      }
      const chosen = options[0]
      if (!chosen) return
      console.info(
        `[acp] auto-responding to permission request tool_call_id=${tool_call_id} with option=${chosen}`,
      )
      wsSend({ type: 'permission_response', session_id, tool_call_id, option_id: chosen })
    },
    [wsSend],
  )

  // Guard against stale onSessionInfoUpdate responses overwriting a newer
  // session selection. Each invocation bumps the generation counter; after
  // the async fetch resolves we verify the generation hasn't advanced.
  const sessionInfoGenRef = useRef(0)

  const onSessionInfoUpdate = useCallback(
    (sessionId: string) => {
      const gen = ++sessionInfoGenRef.current
      fetchSessionWithRetry(sessionId, () => sessionInfoGenRef.current !== gen, {
        assistantMode: params.railMode === 'assistant',
        forceRefresh: true,
      })
        .then(() => {
          // Stale: user has moved to a different session since this fetch started.
          if (sessionInfoGenRef.current !== gen) return
          params.onSessionIdChange(sessionId)
        })
        .catch(() => {
          // Ignore fetch failures — the session may not yet be on disk.
        })
    },
    [params.railMode, params.onSessionIdChange],
  )

  const { submitPrompt, isStreaming, connected } = useAxonAcp({
    activeSessionId: params.chatSessionId,
    agent: params.pulseAgent,
    model: params.pulseModel,
    sessionMode: params.sessionMode,
    enabledMcpServers: params.enabledMcpServers,
    blockedMcpTools: params.blockedMcpTools,
    assistantMode: params.assistantMode,
    handoffContext: params.handoffContext,
    onSessionIdChange: params.onSessionIdChange,
    onSessionFallback: undefined,
    onSessionInfoUpdate,
    onMessagesChange: params.onMessagesChange,
    onAcpConfigOptionsUpdate: params.onAcpConfigOptionsUpdate,
    onCommandsUpdate: params.onCommandsUpdate,
    onHandoffConsumed: params.onHandoffConsumed,
    onTurnComplete: params.onTurnComplete,
    onEditorUpdate: params.onEditorUpdate,
    onPermissionRequest,
    enableFs: params.enableFs,
    enableTerminal: params.enableTerminal,
    permissionTimeoutSecs: params.permissionTimeoutSecs,
    adapterTimeoutSecs: params.adapterTimeoutSecs,
  })

  const isStreamingRef = useRef(false)
  useEffect(() => {
    isStreamingRef.current = isStreaming
  }, [isStreaming])

  useEffect(() => {
    return subscribeWsByTypes(['command.done', 'command.error'], (msg: WsServerMsg) => {
      if (msg.type === 'command.done' || msg.type === 'command.error') {
        params.canvasRef.current?.setIntensity(0.15)
        setTimeout(() => {
          if (!isStreamingRef.current) {
            params.canvasRef.current?.setIntensity(0)
          }
        }, 3000)
      }
    })
  }, [subscribeWsByTypes, params.canvasRef])

  useEffect(() => {
    if (isStreaming) {
      params.canvasRef.current?.setIntensity(1)
    }
  }, [isStreaming, params.canvasRef])

  const handleStats = useCallback(
    (data: {
      aggregate: { cpu_percent: number }
      containers: Record<string, ContainerStats>
      container_count: number
    }) => {
      params.canvasRef.current?.stimulate(data.containers)
      if (!isStreamingRef.current) {
        if (data.container_count === 0) {
          params.canvasRef.current?.setIntensity(0.02)
          return
        }
        const maxCpu = data.container_count * 100
        const norm = Math.min(data.aggregate.cpu_percent / maxCpu, 1)
        params.canvasRef.current?.setIntensity(0.02 + norm * 0.83)
      }
    },
    [params.canvasRef],
  )

  // Invalidate any in-flight onSessionInfoUpdate fetch so it cannot
  // overwrite a manual session selection with a stale session ID.
  const bumpSessionInfoGen = useCallback(() => {
    sessionInfoGenRef.current++
  }, [])

  return {
    submitPrompt,
    isStreaming,
    connected,
    handleStats,
    bumpSessionInfoGen,
    isStreamingRef,
  }
}
