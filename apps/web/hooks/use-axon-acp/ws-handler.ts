'use client'

import type { MutableRefObject } from 'react'
import type { z } from 'zod'
import type { AcpConfigOption } from '@/lib/pulse/types'
import type { WsUsageStats } from '@/lib/ws-protocol'
import type { AxonMessage } from '../use-axon-session'
import { handleEditorMsg } from './editor-handler'
import { applyToolUse, applyToolUseUpdate } from './tool-use-handlers'
import {
  AcpResumeResultSchema,
  AssistantDeltaSchema,
  CommandOutputJsonEnvelopeSchema,
  CommandsUpdateSchema,
  ConfigOptionsUpdateSchema,
  EditorUpdateSchema,
  ErrorSchema,
  PermissionRequestSchema,
  ResultSchema,
  SessionFallbackSchema,
  SessionInfoUpdateSchema,
  SynthesisDeltaSchema,
  ThinkingContentSchema,
  ToolUseSchema,
  ToolUseUpdateSchema,
  UsageUpdateSchema,
} from './ws-schemas'

/** All refs the WS message handler reads or writes. */
export interface WsHandlerRefs {
  streamingIdRef: MutableRefObject<string | null>
  streamingTimeoutRef: MutableRefObject<ReturnType<typeof setTimeout> | null>
  turnStartAtRef: MutableRefObject<number | null>
  firstDeltaAtRef: MutableRefObject<number | null>
  streamedCharsRef: MutableRefObject<number>
  pendingDeltaRef: MutableRefObject<string>
  pendingThinkingRef: MutableRefObject<string[]>
  pendingUsageRef: MutableRefObject<WsUsageStats | null>
  pendingLocationsRef: MutableRefObject<{
    toolCallId: string | undefined
    locations: string[]
  } | null>
}

/** Callbacks the WS message handler may invoke. */
export interface WsHandlerCallbacks {
  setIsStreaming: (value: boolean) => void
  onMessagesChange: (updater: (prev: AxonMessage[]) => AxonMessage[]) => void
  onSessionIdChange: (newId: string) => void
  onSessionFallback?: (oldId: string, newId: string) => void
  onAcpConfigOptionsUpdate?: (options: AcpConfigOption[]) => void
  onCommandsUpdate?: (commands: Array<{ name: string; description?: string }>) => void
  onTurnComplete?: () => void
  onResumeSessionOk?: () => void
  onResumeSessionMiss?: () => void
  onPermissionRequest?: (params: {
    session_id: string
    tool_call_id: string
    options: string[]
  }) => void
  onSessionInfoUpdate?: (sessionId: string) => void
  onEditorUpdate?: (content: string, operation: 'replace' | 'append') => void
  onShowEditor?: () => void
  flushBufferedStream: () => void
  scheduleFlushBufferedStream: () => void
}

/** Telemetry context fields forwarded from the hook for dev-mode logging. */
export interface WsHandlerTelemetry {
  agent: string
  model: string | undefined
  sessionMode: string | undefined
}

const ACP_SESSION_STORAGE_KEY = 'axon-acp-session-id'

type ParsedAcpWsMessage =
  | z.infer<typeof AssistantDeltaSchema>
  | z.infer<typeof UsageUpdateSchema>
  | z.infer<typeof ThinkingContentSchema>
  | z.infer<typeof SessionFallbackSchema>
  | z.infer<typeof ResultSchema>
  | z.infer<typeof ErrorSchema>
  | z.infer<typeof ToolUseSchema>
  | z.infer<typeof ToolUseUpdateSchema>
  | z.infer<typeof ConfigOptionsUpdateSchema>
  | z.infer<typeof CommandsUpdateSchema>
  | z.infer<typeof AcpResumeResultSchema>
  | z.infer<typeof EditorUpdateSchema>
  | z.infer<typeof PermissionRequestSchema>
  | z.infer<typeof SessionInfoUpdateSchema>
  | z.infer<typeof SynthesisDeltaSchema>

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null
  return value as Record<string, unknown>
}

function unwrapAcpPayload(rawMsg: unknown): unknown {
  const envelope = CommandOutputJsonEnvelopeSchema.safeParse(rawMsg)
  if (!envelope.success) return rawMsg
  if (envelope.data.data.ctx.mode !== 'pulse_chat') return rawMsg
  return envelope.data.data.data
}

export function isAcpRelevantWsMessage(rawMsg: unknown): boolean {
  const unwrapped = unwrapAcpPayload(rawMsg)
  const msg = asRecord(unwrapped)
  if (!msg) return false
  const type = typeof msg.type === 'string' ? msg.type : ''
  return (
    type === 'assistant_delta' ||
    type === 'usage_update' ||
    type === 'thinking_content' ||
    type === 'session_fallback' ||
    type === 'result' ||
    type === 'error' ||
    type === 'tool_use' ||
    type === 'tool_use_update' ||
    type === 'editor_update' ||
    type === 'config_options_update' ||
    type === 'config_option_update' ||
    type === 'commands_update' ||
    type === 'acp_resume_result' ||
    type === 'permission_request' ||
    type === 'session_info_update' ||
    type === 'synthesis_delta'
  )
}

function parseAcpWsMessage(rawMsg: unknown): ParsedAcpWsMessage | null {
  const unwrapped = unwrapAcpPayload(rawMsg)
  const msg = asRecord(unwrapped)
  if (!msg) return null
  const type = typeof msg.type === 'string' ? msg.type : ''

  switch (type) {
    case 'assistant_delta': {
      const parsed = AssistantDeltaSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'usage_update': {
      const parsed = UsageUpdateSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'thinking_content': {
      const parsed = ThinkingContentSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'session_fallback': {
      const parsed = SessionFallbackSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'result': {
      const parsed = ResultSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'error': {
      const parsed = ErrorSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'tool_use': {
      const parsed = ToolUseSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'tool_use_update': {
      const parsed = ToolUseUpdateSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'editor_update': {
      const parsed = EditorUpdateSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'config_options_update':
    case 'config_option_update': {
      const parsed = ConfigOptionsUpdateSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'commands_update': {
      const parsed = CommandsUpdateSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'acp_resume_result': {
      const parsed = AcpResumeResultSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'permission_request': {
      const parsed = PermissionRequestSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'session_info_update': {
      const parsed = SessionInfoUpdateSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    case 'synthesis_delta': {
      const parsed = SynthesisDeltaSchema.safeParse(msg)
      return parsed.success ? parsed.data : null
    }
    default:
      return null
  }
}

/**
 * Process a single raw WebSocket message dispatched by the ACP subscription.
 * Unwraps the `command.output.json` envelope and dispatches on `msg.type`.
 *
 * Pure side-effectful function — all state mutations go through the provided
 * refs and callbacks, so the hook body stays focused on wiring.
 */
export function handleAcpWsMessage(
  rawMsg: unknown,
  refs: WsHandlerRefs,
  callbacks: WsHandlerCallbacks,
  telemetry: WsHandlerTelemetry,
): void {
  const msg = parseAcpWsMessage(rawMsg)
  if (!msg) return

  switch (msg.type) {
    case 'assistant_delta': {
      const delta = msg.delta
      const sid = refs.streamingIdRef.current
      if (!sid) return

      const usage = msg.usage as WsUsageStats | undefined
      const locations = msg.tool_locations

      if (delta) {
        if (refs.firstDeltaAtRef.current === null) refs.firstDeltaAtRef.current = Date.now()
        refs.streamedCharsRef.current += delta.length
        refs.pendingDeltaRef.current += delta
        callbacks.scheduleFlushBufferedStream()
      }

      // Accumulate usage/locations into refs so flushBufferedStream can
      // apply them together with the text delta in one prev.map() pass.
      if (usage) {
        refs.pendingUsageRef.current = refs.pendingUsageRef.current
          ? { ...refs.pendingUsageRef.current, ...usage }
          : usage
        callbacks.scheduleFlushBufferedStream()
      }
      if (locations) {
        // Last writer wins: if two deltas in the same window carry locations,
        // the latest one is the authoritative set for that tool call.
        refs.pendingLocationsRef.current = {
          toolCallId: msg.tool_call_id,
          locations,
        }
        callbacks.scheduleFlushBufferedStream()
      }
      break
    }

    case 'usage_update': {
      const usage = msg.usage as WsUsageStats | undefined
      if (!usage) break
      const sid = refs.streamingIdRef.current
      if (!sid) return
      callbacks.onMessagesChange((prev) =>
        prev.map((m) =>
          m.id === sid
            ? ({
                ...m,
                usage: m.usage ? { ...m.usage, ...usage } : usage,
              } as AxonMessage)
            : m,
        ),
      )
      break
    }

    case 'thinking_content': {
      const content = msg.content
      const sid = refs.streamingIdRef.current
      if (!sid || !content) return
      refs.pendingThinkingRef.current.push(content)
      callbacks.scheduleFlushBufferedStream()
      break
    }

    case 'synthesis_delta': {
      // Research synthesis streaming — treated as assistant text so the UI
      // renders it inline with the message being built.
      const text = msg.text
      if (!text || !refs.streamingIdRef.current) break
      refs.pendingDeltaRef.current += text
      callbacks.scheduleFlushBufferedStream()
      break
    }

    case 'session_fallback': {
      const oldId = msg.old_session_id
      const newId = msg.new_session_id
      if (newId) {
        callbacks.onSessionIdChange(newId)
        callbacks.onSessionFallback?.(oldId, newId)
      }
      break
    }

    case 'result': {
      callbacks.flushBufferedStream()
      // Check BEFORE clearing — if already null the turn timed out; skip
      // onTurnComplete/onSessionIdChange to prevent a late result from a
      // slow agent (e.g. Gemini) polluting the next turn's session state.
      const wasActiveTurn = refs.streamingIdRef.current !== null
      const resultSid = refs.streamingIdRef.current
      const newSessionId = msg.session_id
      if (refs.streamingTimeoutRef.current) clearTimeout(refs.streamingTimeoutRef.current)
      if (process.env.NODE_ENV !== 'production' && refs.turnStartAtRef.current !== null) {
        const end = Date.now()
        const durationMs = end - refs.turnStartAtRef.current
        const firstDeltaMs =
          refs.firstDeltaAtRef.current === null
            ? null
            : refs.firstDeltaAtRef.current - refs.turnStartAtRef.current
        const charsPerSec =
          durationMs > 0
            ? Number(((refs.streamedCharsRef.current * 1000) / durationMs).toFixed(1))
            : 0
        console.debug('[acp-stream-telemetry]', {
          agent: telemetry.agent,
          model: telemetry.model ?? 'default',
          sessionMode: telemetry.sessionMode ?? 'default',
          durationMs,
          firstDeltaMs,
          streamedChars: refs.streamedCharsRef.current,
          charsPerSec,
        })
      }
      callbacks.setIsStreaming(false)
      refs.streamingIdRef.current = null
      refs.turnStartAtRef.current = null
      refs.firstDeltaAtRef.current = null
      refs.streamedCharsRef.current = 0
      // Mark the message as no longer streaming so the typing-dots indicator
      // is removed even if no deltas were received (e.g. lost events).
      if (resultSid) {
        callbacks.onMessagesChange((prev) =>
          prev.map((m) => (m.id === resultSid ? { ...m, streaming: false } : m)),
        )
      }
      if (wasActiveTurn) {
        callbacks.onTurnComplete?.()
        // With the persistent adapter, session data is written incrementally —
        // trigger session fetch immediately without waiting for a polling event.
        if (newSessionId) {
          callbacks.onSessionIdChange(newSessionId)
        }
      }
      break
    }

    case 'error': {
      callbacks.flushBufferedStream()
      const errSid = refs.streamingIdRef.current
      const errMsg = msg.message || msg.error || 'Agent error'
      if (refs.streamingTimeoutRef.current) clearTimeout(refs.streamingTimeoutRef.current)
      callbacks.setIsStreaming(false)
      refs.streamingIdRef.current = null
      refs.turnStartAtRef.current = null
      refs.firstDeltaAtRef.current = null
      refs.streamedCharsRef.current = 0
      if (errSid) {
        callbacks.onMessagesChange((prev) =>
          prev.map((m) =>
            m.id === errSid ? { ...m, content: `⚠ ${errMsg}`, streaming: false } : m,
          ),
        )
      }
      break
    }

    case 'tool_use': {
      const sid = refs.streamingIdRef.current
      if (!sid) return
      applyToolUse(sid, msg, callbacks.onMessagesChange)
      break
    }

    case 'tool_use_update': {
      const sid = refs.streamingIdRef.current
      if (!sid) return
      applyToolUseUpdate(sid, msg, callbacks.onMessagesChange)
      break
    }

    case 'editor_update': {
      handleEditorMsg(msg, callbacks.onEditorUpdate, callbacks.onShowEditor)
      break
    }

    case 'config_options_update':
    case 'config_option_update': {
      callbacks.onAcpConfigOptionsUpdate?.(msg.configOptions as AcpConfigOption[])
      break
    }

    case 'commands_update': {
      callbacks.onCommandsUpdate?.(msg.commands)
      break
    }

    case 'acp_resume_result': {
      const ok = msg.ok
      const replayed = msg.replayed
      const sessionId = msg.session_id
      if (ok) {
        console.info(`[acp] resumed session, replayed ${replayed ?? 0} buffered events`)
        callbacks.onResumeSessionOk?.()
        if (sessionId) callbacks.onSessionIdChange(sessionId)
      } else {
        console.info('[acp] session resume failed — session expired or unknown')
        callbacks.onResumeSessionMiss?.()
        try {
          sessionStorage.removeItem(ACP_SESSION_STORAGE_KEY)
        } catch {
          /* noop */
        }
      }
      break
    }

    case 'permission_request': {
      callbacks.onPermissionRequest?.({
        session_id: msg.session_id,
        tool_call_id: msg.tool_call_id,
        options: msg.options,
      })
      break
    }

    case 'session_info_update': {
      callbacks.onSessionInfoUpdate?.(msg.session_id)
      break
    }
  }
}
