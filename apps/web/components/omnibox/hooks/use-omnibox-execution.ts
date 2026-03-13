'use client'

import { useCallback, useRef, useState } from 'react'
import { useAxonWs } from '@/hooks/use-axon-ws'
import {
  useWsExecutionState,
  useWsMessageActions,
  useWsWorkspaceState,
} from '@/hooks/use-ws-messages'
import type { CommandOptionValues } from '@/lib/command-options'
import type { CompletionStatus } from '@/lib/omnibox-types'
import type { ModeId } from '@/lib/ws-protocol'
import { NO_INPUT_MODES } from '@/lib/ws-protocol'
import {
  normalizeUrlInput,
  shouldPreservePulseWorkspaceForMode,
  shouldRunCommandForInput,
} from '../utils'

interface UseOmniboxExecutionInput {
  mode: ModeId
  input: string
  setInput: (value: string) => void
  buildInputWithFileContext: (
    input: string,
  ) => Promise<{ enrichedInput: string; contextFileLabels: string[] }>
}

export function useOmniboxExecution({
  mode,
  input,
  setInput,
  buildInputWithFileContext,
}: UseOmniboxExecutionInput) {
  const { send } = useAxonWs()
  const { currentJobId, currentMode } = useWsExecutionState()
  const {
    workspaceMode,
    workspaceContext,
    workspaceResumeSessionId,
    pulseAgent,
    pulseModel,
    pulsePermissionLevel,
    acpConfigOptions,
  } = useWsWorkspaceState()
  const {
    startExecution,
    activateWorkspace,
    submitWorkspacePrompt,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
  } = useWsMessageActions()

  const [isProcessing, setIsProcessing] = useState(false)
  const isProcessingRef = useRef(false)
  const [statusText, setStatusText] = useState('')
  const [statusType, setStatusType] = useState<'processing' | 'done' | 'error'>('processing')
  const [optionValues, setOptionValues] = useState<CommandOptionValues>({})
  const [completionStatus, setCompletionStatus] = useState<CompletionStatus | null>(null)

  const startTimeRef = useRef(0)
  const execIdRef = useRef(0)

  // isProcessingRef is set synchronously in executeCommand/cancel — no useEffect sync needed

  const executeCommand = useCallback(
    async (execMode: ModeId, execInput: string) => {
      if (isProcessingRef.current) return

      const trimmedInput = execInput.trim()
      if (!trimmedInput && !NO_INPUT_MODES.has(execMode)) return
      const shouldRunCommand = shouldRunCommandForInput(execMode, trimmedInput)
      if (!shouldRunCommand) {
        if (process.env.NODE_ENV === 'development') {
          console.log('[omnibox] pulse path — mode:', execMode, 'workspaceMode:', workspaceMode)
        }
        if (workspaceMode !== 'pulse') {
          activateWorkspace('pulse')
        }
        if (trimmedInput) submitWorkspacePrompt(trimmedInput)
        return
      }

      const normalizedInput = normalizeUrlInput(trimmedInput)
      isProcessingRef.current = true
      setIsProcessing(true)
      execIdRef.current += 1
      startTimeRef.current = Date.now()
      setStatusText('processing...')
      setStatusType('processing')

      try {
        const { enrichedInput, contextFileLabels } =
          await buildInputWithFileContext(normalizedInput)

        const flags: Record<string, string> = {}
        for (const [key, val] of Object.entries(optionValues)) {
          if (val === '' || val === false) continue
          flags[key] = String(val)
        }
        if (contextFileLabels.length > 0) {
          flags.context_files = contextFileLabels.join(',')
        }

        send({
          type: 'execute',
          mode: execMode,
          input: enrichedInput,
          flags,
        })

        const preservePulseWorkspace = shouldPreservePulseWorkspaceForMode(workspaceMode, execMode)
        startExecution(execMode, enrichedInput, { preserveWorkspace: preservePulseWorkspace })
      } catch {
        isProcessingRef.current = false
        setIsProcessing(false)
        setStatusText('failed to execute')
        setStatusType('error')
      }
    },
    [
      buildInputWithFileContext,
      activateWorkspace,
      workspaceMode,
      submitWorkspacePrompt,
      send,
      startExecution,
      optionValues,
    ],
  )

  const execute = useCallback(() => {
    const hasTypedInput = input.trim().length > 0
    void executeCommand(mode, input)
    if (hasTypedInput) setInput('')
  }, [executeCommand, mode, input, setInput])

  const cancel = useCallback(() => {
    if (!isProcessingRef.current) return
    const fallbackId = String(execIdRef.current)
    const cancelId = currentJobId ?? fallbackId
    send({
      type: 'cancel',
      id: cancelId,
      mode,
    })
    isProcessingRef.current = false
    setIsProcessing(false)
    const elapsed = Date.now() - startTimeRef.current
    const secs = (elapsed / 1000).toFixed(1)
    setStatusText(`${secs}s \u00b7 cancelled`)
    setStatusType('error')
  }, [currentJobId, mode, send])

  return {
    isProcessing,
    statusText,
    statusType,
    optionValues,
    completionStatus,
    workspaceMode,
    workspaceContext,
    workspaceResumeSessionId,
    pulseAgent,
    pulseModel,
    pulsePermissionLevel,
    acpConfigOptions,
    currentMode,
    currentJobId,
    setIsProcessing,
    setStatusText,
    setStatusType,
    setOptionValues,
    setCompletionStatus,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    execute,
    cancel,
    executeCommand,
  }
}
