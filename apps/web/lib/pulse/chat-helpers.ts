import type React from 'react'
import type { ChatStreamEvent, runChatPrompt } from '@/lib/pulse/chat-api'
import { runSourcePrompt } from '@/lib/pulse/chat-api'
import { parseClaudeAssistantPayload } from '@/lib/pulse/claude-response'
import type { ValidationResult } from '@/lib/pulse/doc-ops'
import { validateDocOperations } from '@/lib/pulse/doc-ops'
import { checkPermission } from '@/lib/pulse/permissions'
import type {
  AcpConfigOption,
  AcpPermissionRequest,
  DocOperation,
  PulseAgent,
  PulseMessageBlock,
  PulseModel,
  PulsePermissionLevel,
  PulseToolUse,
} from '@/lib/pulse/types'
import type { ChatMessage } from '@/lib/pulse/workspace-persistence'

// ── Types ───────────────────────────────────────────────────────────────────

/** Snapshot of config values read from configRef at prompt submission time. */
export interface PromptConfig {
  chatSessionId: string | null
  documentMarkdown: string
  activeThreadSources: string[]
  scrapedContext: { url: string; markdown: string } | null
  permissionLevel: PulsePermissionLevel
  agent: PulseAgent
  model: PulseModel
}

/** Mutable accumulator shared across streaming event handlers for a single prompt. */
export interface StreamAccumulator {
  partialText: string
  draftAdded: boolean
  assistantDraftId: string | undefined
  partialTools: PulseToolUse[]
  partialBlocks: PulseMessageBlock[]
}

// ── Extracted helpers ───────────────────────────────────────────────────────

/**
 * Handle the "source" intent — index URLs and produce a summary message.
 * Returns true if handled (caller should return early).
 */
export async function handleSourceIntent(
  urls: string[],
  signal: AbortSignal,
  promptId: number,
  inFlightRef: React.RefObject<number>,
  setIndexedSources: React.Dispatch<React.SetStateAction<string[]>>,
  setActiveThreadSources: React.Dispatch<React.SetStateAction<string[]>>,
  setScrapedContext: React.Dispatch<React.SetStateAction<{ url: string; markdown: string } | null>>,
  setChatHistoryTracked: (action: React.SetStateAction<ChatMessage[]>) => void,
  createMessage: (partial: Omit<ChatMessage, 'id' | 'createdAt'>) => ChatMessage,
): Promise<void> {
  const result = await runSourcePrompt(urls, signal)
  if (inFlightRef.current !== promptId) return
  setIndexedSources((prev) => {
    const seen = new Set(prev)
    const next = [...prev]
    for (const url of result.indexed) {
      if (!seen.has(url)) {
        seen.add(url)
        next.push(url)
      }
    }
    return next.slice(-200)
  })
  setActiveThreadSources((prev) => {
    const seen = new Set(prev)
    const merged = [...prev]
    for (const url of result.indexed) {
      if (!seen.has(url)) {
        seen.add(url)
        merged.push(url)
      }
    }
    return merged.slice(-200)
  })
  if (result.markdownBySrc && Object.keys(result.markdownBySrc).length > 0) {
    const firstUrl = result.indexed[0]
    const md = firstUrl ? result.markdownBySrc[firstUrl] : undefined
    if (md && md.length > 0) {
      setScrapedContext({ url: firstUrl ?? '', markdown: md })
    }
  }
  const sourceList = result.indexed.map((url) => `- ${url}`).join('\n')
  const assistantMessage = [
    'Indexed new sources into Axon.',
    '',
    sourceList,
    '',
    'Ask a follow-up question to use this fresh context.',
  ].join('\n')
  setChatHistoryTracked((prev) => [
    ...prev,
    createMessage({ role: 'assistant', content: assistantMessage }),
  ])
}

/**
 * Build the onEvent callback for streaming chat responses.
 * Each event type is handled in a focused branch, updating the accumulator
 * and calling the appropriate state setters.
 */
export function makeStreamEventHandler(
  acc: StreamAccumulator,
  promptId: number,
  inFlightRef: React.RefObject<number>,
  assistantDraft: ChatMessage,
  setStreamPhase: React.Dispatch<
    React.SetStateAction<'started' | 'thinking' | 'finalizing' | null>
  >,
  setLiveToolUses: React.Dispatch<React.SetStateAction<PulseToolUse[]>>,
  setChatHistoryTracked: (action: React.SetStateAction<ChatMessage[]>) => void,
  updateChatMessage: (messageId: string, transform: (m: ChatMessage) => ChatMessage) => void,
  setChatSessionId: React.Dispatch<React.SetStateAction<string | null>>,
  onAcpConfigUpdate?: (options: AcpConfigOption[]) => void,
  onPermissionRequest?: (req: AcpPermissionRequest) => void,
): { handler: (event: ChatStreamEvent) => void; flush: () => void } {
  let deltaFlushTimer: ReturnType<typeof setTimeout> | null = null
  let pendingDeltaFlush = false

  function ensureDraftAdded() {
    if (!acc.draftAdded) {
      acc.draftAdded = true
      setChatHistoryTracked((prev) => [...prev, assistantDraft])
    }
  }

  function flushDeltaToReact() {
    deltaFlushTimer = null
    pendingDeltaFlush = false
    updateChatMessage(assistantDraft.id!, (m) => ({
      ...m,
      content: acc.partialText,
      blocks: [...acc.partialBlocks],
    }))
  }

  function scheduleDeltaFlush() {
    if (pendingDeltaFlush) return
    pendingDeltaFlush = true
    deltaFlushTimer = setTimeout(flushDeltaToReact, 50)
  }

  function flush() {
    if (deltaFlushTimer !== null) {
      clearTimeout(deltaFlushTimer)
      flushDeltaToReact()
    }
  }

  const handler = (event: ChatStreamEvent) => {
    if (inFlightRef.current !== promptId) return

    if (event.type === 'status' && event.phase) {
      setStreamPhase(event.phase)
      return
    }

    if (event.type === 'thinking_content' && event.content) {
      ensureDraftAdded()
      const lastBlock = acc.partialBlocks[acc.partialBlocks.length - 1]
      if (lastBlock?.type === 'thinking') {
        acc.partialBlocks[acc.partialBlocks.length - 1] = {
          type: 'thinking',
          content: event.content,
        }
      } else {
        acc.partialBlocks.push({ type: 'thinking', content: event.content })
      }
      updateChatMessage(assistantDraft.id!, (m) => ({ ...m, blocks: [...acc.partialBlocks] }))
      return
    }

    if (event.type === 'assistant_delta' && event.delta) {
      ensureDraftAdded()
      acc.partialText += event.delta
      const lastBlock = acc.partialBlocks[acc.partialBlocks.length - 1]
      if (lastBlock?.type === 'text') {
        acc.partialBlocks[acc.partialBlocks.length - 1] = {
          ...lastBlock,
          content: lastBlock.content + event.delta,
        }
      } else {
        acc.partialBlocks.push({ type: 'text', content: event.delta })
      }
      // Throttle React updates: accumulate deltas, flush at most every 50ms
      scheduleDeltaFlush()
      return
    }

    if (event.type === 'tool_use' && event.tool) {
      // Flush any pending text delta before adding tool use
      flush()
      ensureDraftAdded()
      const now = Date.now()
      acc.partialTools.push({
        ...event.tool,
        sequence: (acc.partialTools.at(-1)?.sequence ?? 0) + 1,
        startedAtMs: event.tool.startedAtMs ?? now,
        updatedAtMs: now,
      })
      acc.partialBlocks.push({
        type: 'tool_use',
        name: event.tool.name,
        input: event.tool.input,
        toolCallId: event.tool.toolCallId,
        status: event.tool.status ?? 'running',
        sequence: acc.partialTools.at(-1)?.sequence,
        startedAtMs: event.tool.startedAtMs ?? now,
        updatedAtMs: now,
      })
      setLiveToolUses([...acc.partialTools])
      updateChatMessage(assistantDraft.id!, (m) => ({
        ...m,
        toolUses: [...acc.partialTools],
        blocks: [...acc.partialBlocks],
      }))
      return
    }

    if (event.type === 'tool_use_update' && event.toolCallId) {
      ensureDraftAdded()
      const now = Date.now()
      const blockIdx = acc.partialBlocks.findLastIndex(
        (b) => b.type === 'tool_use' && 'toolCallId' in b && b.toolCallId === event.toolCallId,
      )
      if (blockIdx >= 0) {
        // blockIdx is a valid index — findLastIndex only returns >= 0 for real matches
        const block = acc.partialBlocks[blockIdx]!
        if (block.type === 'tool_use') {
          acc.partialBlocks[blockIdx] = {
            ...block,
            status: event.status ?? block.status,
            content: event.content ? (block.content ?? '') + event.content : block.content,
            updatedAtMs: now,
            ...(event.status === 'completed' || event.status === 'success'
              ? {
                  completedAtMs: now,
                  durationMs: block.startedAtMs ? Math.max(0, now - block.startedAtMs) : undefined,
                }
              : {}),
          }
        }
      }
      const toolIdx = acc.partialTools.findLastIndex((t) => t.toolCallId === event.toolCallId)
      if (toolIdx >= 0) {
        // toolIdx is a valid index — findLastIndex only returns >= 0 for real matches
        const tool = acc.partialTools[toolIdx]!
        acc.partialTools[toolIdx] = {
          ...tool,
          status: event.status ?? tool.status,
          content: event.content ? (tool.content ?? '') + event.content : tool.content,
          updatedAtMs: now,
          ...(event.status === 'completed' || event.status === 'success'
            ? {
                completedAtMs: now,
                durationMs: tool.startedAtMs ? Math.max(0, now - tool.startedAtMs) : undefined,
              }
            : {}),
        }
      }
      setLiveToolUses([...acc.partialTools])
      updateChatMessage(assistantDraft.id!, (m) => ({
        ...m,
        toolUses: [...acc.partialTools],
        blocks: [...acc.partialBlocks],
      }))
      return
    }

    if (event.type === 'permission_request' && event.toolCallId) {
      // Match the permission request to its tool by toolCallId to avoid
      // referencing the wrong tool when events interleave.
      const matchedTool = acc.partialTools.findLast((t) => t.toolCallId === event.toolCallId)
      onPermissionRequest?.({
        sessionId: event.sessionId ?? '',
        toolCallId: event.toolCallId,
        options: event.permissionOptions ?? [],
        toolName: matchedTool?.name,
      })
      return
    }

    if (event.type === 'config_options_update' && event.configOptions) {
      onAcpConfigUpdate?.(event.configOptions)
      return
    }

    if (event.type === 'session_fallback' && event.newSessionId) {
      setChatSessionId(event.newSessionId)
      return
    }
  }

  return { handler, flush }
}

/**
 * Process the final chat response — update metadata, finalize the draft
 * message, and handle doc operations / permissions.
 */
export function finalizeStreamResponse(
  data: Awaited<ReturnType<typeof runChatPrompt>>,
  acc: StreamAccumulator,
  cfg: PromptConfig,
  setChatSessionId: React.Dispatch<React.SetStateAction<string | null>>,
  setLastResponseLatencyMs: React.Dispatch<React.SetStateAction<number | null>>,
  setLastResponseModel: React.Dispatch<React.SetStateAction<PulseModel | null>>,
  setLastContextStats: React.Dispatch<
    React.SetStateAction<{ contextCharsTotal: number; contextBudgetChars: number } | null>
  >,
  updateChatMessage: (messageId: string, transform: (m: ChatMessage) => ChatMessage) => void,
  onApplyOperations: (ops: DocOperation[]) => void,
  onPendingOps: (ops: DocOperation[] | null) => void,
  onPendingValidation: (v: ValidationResult | null) => void,
): void {
  if (data.sessionId) setChatSessionId(data.sessionId)
  if (data.metadata) {
    setLastResponseLatencyMs(data.metadata.elapsedMs)
    setLastResponseModel(data.metadata.model)
    setLastContextStats({
      contextCharsTotal: data.metadata.contextCharsTotal,
      contextBudgetChars: data.metadata.contextBudgetChars,
    })
  }

  updateChatMessage(acc.assistantDraftId!, (m) => ({
    ...m,
    content: data.text,
    citations: data.citations,
    operations: data.operations,
    toolUses: data.toolUses,
    blocks: data.blocks,
  }))

  if (data.operations.length > 0) {
    const permission = checkPermission(cfg.permissionLevel, data.operations, {
      isCurrentDoc: true,
      currentDocMarkdown: cfg.documentMarkdown,
    })
    if (permission.allowed && !permission.requiresConfirmation) {
      onApplyOperations(data.operations)
    } else if (permission.allowed && permission.requiresConfirmation) {
      const validation = validateDocOperations(data.operations, cfg.documentMarkdown)
      onPendingOps(data.operations)
      onPendingValidation(validation)
    }
  }
}

/**
 * Handle errors during prompt execution — recover partial content when
 * possible, otherwise display the error in-place.
 */
export function handlePromptError(
  err: unknown,
  acc: StreamAccumulator,
  trimmed: string,
  showRequestNotice: (msg: string) => void,
  updateChatMessage: (messageId: string, transform: (m: ChatMessage) => ChatMessage) => void,
  setChatHistoryTracked: (action: React.SetStateAction<ChatMessage[]>) => void,
  createMessage: (partial: Omit<ChatMessage, 'id' | 'createdAt'>) => ChatMessage,
): void {
  if (err instanceof Error && err.name === 'AbortError') {
    showRequestNotice('Request stopped. Partial response preserved.')
    return
  }
  const parsedPartial =
    acc.draftAdded && acc.assistantDraftId ? parseClaudeAssistantPayload(acc.partialText) : null
  const message = err instanceof Error ? err.message : 'Unknown error'
  if (parsedPartial?.text && acc.assistantDraftId) {
    updateChatMessage(acc.assistantDraftId, (m) => ({ ...m, content: parsedPartial.text }))
  } else if (acc.draftAdded && acc.assistantDraftId) {
    updateChatMessage(acc.assistantDraftId, (m) => ({
      ...m,
      content: message,
      isError: true,
      retryPrompt: trimmed,
    }))
  } else {
    setChatHistoryTracked((prev) => [
      ...prev,
      createMessage({
        role: 'assistant',
        content: message,
        isError: true,
        retryPrompt: trimmed,
      }),
    ])
  }
}
