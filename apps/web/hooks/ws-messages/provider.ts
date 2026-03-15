'use client'

import { usePathname } from 'next/navigation'
import type React from 'react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useAxonWs } from '@/hooks/use-axon-ws'
import { getAcpModelConfigOption } from '@/lib/pulse/acp-config'
import { probePulseConfigOptions } from '@/lib/pulse/config-api'
import type { AcpConfigOption } from '@/lib/pulse/types'
import { usePulseSlice, useWorkspaceSlice } from '@/lib/shell-store'
import type { CrawlFile, WsLifecycleEntry, WsServerMsg } from '@/lib/ws-protocol'
import { handleWsMessage, isRuntimeRelevantWsMessage } from './handlers'
import { makeInitialRuntimeState } from './runtime'
import {
  LS_PULSE_AGENT,
  LS_PULSE_MODEL,
  LS_PULSE_PERMISSION,
  LS_WORKSPACE_MODE,
  safeGetItem,
  safeRemoveItem,
  safeSetItem,
  VALID_AGENTS,
  VALID_PERMISSIONS,
  validateStoredEnum,
} from './storage'
import type {
  CancelResponseState,
  CrawlProgress,
  LogLine,
  PulseWorkspaceAgent,
  PulseWorkspaceModel,
  PulseWorkspacePermission,
  RecentRun,
  ScreenshotFile,
  WorkspaceContextState,
  WsMessagesActions,
  WsMessagesContextValue,
  WsMessagesExecutionState,
  WsMessagesRuntimeState,
  WsMessagesWorkspaceState,
} from './types'

export function useWsMessagesProvider() {
  const pathname = usePathname()
  const { subscribeByTypes, send } = useAxonWs()
  const [markdownContent, setMarkdownContent] = useState('')
  const [logLines, setLogLines] = useState<LogLine[]>([])
  const [errorMessage, setErrorMessage] = useState('')
  const [recentRuns, setRecentRuns] = useState<RecentRun[]>([])
  const runIdCounter = useRef(0)
  const [isProcessing, setIsProcessing] = useState(false)
  const [hasResults, setHasResults] = useState(false)
  const [crawlFiles, setCrawlFiles] = useState<CrawlFile[]>([])
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [_virtualFileContentByPath, setVirtualFileContentByPath] = useState<Record<string, string>>(
    {},
  )
  const [_currentOutputDir, setCurrentOutputDir] = useState<string | null>(null)
  const currentModeRef = useRef('')
  const currentInputRef = useRef('')
  const [currentMode, setCurrentMode] = useState('')
  const [crawlProgress, setCrawlProgress] = useState<CrawlProgress | null>(null)
  const [stdoutLines, setStdoutLines] = useState<string[]>([])
  const [stdoutJson, setStdoutJson] = useState<unknown[]>([])
  const [commandMode, setCommandMode] = useState<string | null>(null)
  const [screenshotFiles, setScreenshotFiles] = useState<ScreenshotFile[]>([])
  const [currentJobId, setCurrentJobId] = useState<string | null>(null)
  const currentJobIdRef = useRef<string | null>(null)
  const [lifecycleEntries, setLifecycleEntries] = useState<WsLifecycleEntry[]>([])
  const [cancelResponse, setCancelResponse] = useState<CancelResponseState | null>(null)

  // ACP-related state — grouped together since they are logically coupled
  // and frequently updated as a set (agent change triggers config probe,
  // config probe updates options, options influence model selection).
  const {
    pulseAgent,
    pulseModel,
    pulsePermissionLevel,
    acpConfigOptions,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    setAcpConfigOptions,
  } = usePulseSlice() as {
    pulseAgent: PulseWorkspaceAgent
    pulseModel: PulseWorkspaceModel
    pulsePermissionLevel: PulseWorkspacePermission
    acpConfigOptions: AcpConfigOption[]
    setPulseAgent: (agent: PulseWorkspaceAgent) => void
    setPulseModel: (model: PulseWorkspaceModel) => void
    setPulsePermissionLevel: (level: PulseWorkspacePermission) => void
    setAcpConfigOptions: (options: AcpConfigOption[]) => void
  }
  const {
    workspaceMode,
    workspacePrompt,
    workspacePromptVersion,
    workspaceResumeSessionId,
    workspaceResumeVersion,
    workspaceContext,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
    bumpWorkspacePromptVersion,
    setWorkspaceResumeSessionId,
    setWorkspaceResumeVersion,
    bumpWorkspaceResumeVersion,
    setWorkspaceContext,
  } = useWorkspaceSlice()

  const selectedFileRef = useRef<string | null>(null)
  const crawlFilesRef = useRef<CrawlFile[]>([])
  const stdoutJsonRef = useRef<unknown[]>([])
  const currentOutputDirRef = useRef<string | null>(null)
  const virtualFileContentByPathRef = useRef<Record<string, string>>({})
  const runtimeStateRef = useRef<WsMessagesRuntimeState>(makeInitialRuntimeState())

  const setCrawlFilesTracked = useCallback((action: React.SetStateAction<CrawlFile[]>) => {
    if (typeof action === 'function') {
      setCrawlFiles((prev) => {
        const next = action(prev)
        crawlFilesRef.current = next
        return next
      })
    } else {
      crawlFilesRef.current = action
      setCrawlFiles(action)
    }
  }, [])

  const setSelectedFileTracked = useCallback((action: React.SetStateAction<string | null>) => {
    if (typeof action === 'function') {
      setSelectedFile((prev) => {
        const next = action(prev)
        selectedFileRef.current = next
        return next
      })
    } else {
      selectedFileRef.current = action
      setSelectedFile(action)
    }
  }, [])

  const setStdoutJsonTracked = useCallback((action: React.SetStateAction<unknown[]>) => {
    if (typeof action === 'function') {
      setStdoutJson((prev) => {
        const next = action(prev)
        stdoutJsonRef.current = next
        return next
      })
    } else {
      stdoutJsonRef.current = action
      setStdoutJson(action)
    }
  }, [])

  const setCurrentOutputDirTracked = useCallback((action: React.SetStateAction<string | null>) => {
    if (typeof action === 'function') {
      setCurrentOutputDir((prev) => {
        const next = action(prev)
        currentOutputDirRef.current = next
        return next
      })
    } else {
      currentOutputDirRef.current = action
      setCurrentOutputDir(action)
    }
  }, [])

  const setVirtualFileContentByPathTracked = useCallback(
    (action: React.SetStateAction<Record<string, string>>) => {
      if (typeof action === 'function') {
        setVirtualFileContentByPath((prev) => {
          const next = action(prev)
          virtualFileContentByPathRef.current = next
          return next
        })
      } else {
        virtualFileContentByPathRef.current = action
        setVirtualFileContentByPath(action)
      }
    },
    [],
  )

  // ── localStorage: read on mount (once) ──────────────────────────────────

  useEffect(() => {
    const storedMode = safeGetItem(LS_WORKSPACE_MODE)
    if (storedMode) setWorkspaceMode(storedMode)

    const storedAgent = validateStoredEnum(
      safeGetItem(LS_PULSE_AGENT),
      VALID_AGENTS,
      'claude' as PulseWorkspaceAgent,
    )
    setPulseAgent(storedAgent)

    const storedModel = safeGetItem(LS_PULSE_MODEL)
    if (storedModel && storedModel.length > 0) setPulseModel(storedModel)

    const storedPermission = validateStoredEnum(
      safeGetItem(LS_PULSE_PERMISSION),
      VALID_PERMISSIONS,
      'accept-edits' as PulseWorkspacePermission,
    )
    setPulsePermissionLevel(storedPermission)
  }, [setPulseAgent, setPulseModel, setPulsePermissionLevel])

  // ── localStorage: consolidated write effect ─────────────────────────────

  useEffect(() => {
    if (workspaceMode === null) {
      safeRemoveItem(LS_WORKSPACE_MODE)
    } else {
      safeSetItem(LS_WORKSPACE_MODE, workspaceMode)
    }
    safeSetItem(LS_PULSE_AGENT, pulseAgent)
    safeSetItem(LS_PULSE_MODEL, pulseModel ?? '')
    safeSetItem(LS_PULSE_PERMISSION, pulsePermissionLevel)
  }, [workspaceMode, pulseAgent, pulseModel, pulsePermissionLevel])

  // biome-ignore lint/correctness/useExhaustiveDependencies: pulseModel is read inside but intentionally excluded — re-probing on model change would create an infinite loop since the probe itself can set the model
  useEffect(() => {
    let cancelled = false

    void probePulseConfigOptions({ agent: pulseAgent })
      .then((options) => {
        if (cancelled) return
        setAcpConfigOptions(options)

        if (options.length === 0) return
        const modelConfig = getAcpModelConfigOption(options)
        if (!modelConfig || modelConfig.options.length === 0) return
        const hasCurrent = modelConfig.options.some((option) => option.value === pulseModel)
        if (hasCurrent) return
        setPulseModel(modelConfig.currentValue || modelConfig.options[0]!.value)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        console.warn('[pulse] config probe failed', error)
        setAcpConfigOptions([])
      })

    return () => {
      cancelled = true
    }
  }, [pathname, pulseAgent])

  const setCurrentJobIdTracked = useCallback((jobId: string | null) => {
    currentJobIdRef.current = jobId
    setCurrentJobId(jobId)
  }, [])

  useEffect(() => {
    const refs = {
      currentModeRef,
      currentInputRef,
      currentJobIdRef,
      selectedFileRef,
      crawlFilesRef,
      stdoutJsonRef,
      currentOutputDirRef,
      virtualFileContentByPathRef,
      runIdCounter,
      runtimeStateRef,
    }
    const setters = {
      setLogLines,
      setMarkdownContent,
      setHasResults,
      setCrawlFiles: setCrawlFilesTracked,
      setCurrentOutputDir: setCurrentOutputDirTracked,
      setSelectedFile: setSelectedFileTracked,
      setCrawlProgress,
      setCommandMode,
      setStdoutLines,
      setStdoutJson: setStdoutJsonTracked,
      setVirtualFileContentByPath: setVirtualFileContentByPathTracked,
      setScreenshotFiles,
      setLifecycleEntries,
      setCancelResponse,
      setIsProcessing,
      setErrorMessage,
      setRecentRuns,
      setWorkspaceMode,
      setWorkspacePrompt,
      setWorkspacePromptVersion: (action: React.SetStateAction<number>) => {
        if (typeof action === 'function') {
          setWorkspacePromptVersion(action(workspacePromptVersion))
          return
        }
        setWorkspacePromptVersion(action)
      },
      setCurrentJobIdTracked,
    }
    return subscribeByTypes(
      [
        'log',
        'file_content',
        'crawl_files',
        'crawl_progress',
        'command.start',
        'command.output.line',
        'command.output.json',
        'command.done',
        'command.error',
        'job.status',
        'job.progress',
        'artifact.list',
        'artifact.content',
        'job.cancel.response',
      ],
      (msg: WsServerMsg) => {
        if (!isRuntimeRelevantWsMessage(msg)) return
        handleWsMessage(msg, refs, setters)
      },
    )
  }, [
    setCrawlFilesTracked,
    setCurrentJobIdTracked,
    setCurrentOutputDirTracked,
    setSelectedFileTracked,
    setStdoutJsonTracked,
    setVirtualFileContentByPathTracked,
    subscribeByTypes,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
    workspacePromptVersion,
  ])

  const selectFile = useCallback(
    (relativePath: string) => {
      setSelectedFileTracked(relativePath)
      setMarkdownContent('')
      const virtualContent = virtualFileContentByPathRef.current[relativePath]
      if (typeof virtualContent === 'string') {
        setMarkdownContent(virtualContent)
        return
      }
      send({ type: 'read_file', path: relativePath })
    },
    [send, setSelectedFileTracked],
  )

  const resetExecutionRuntime = useCallback(
    ({ hasResults, isProcessing }: { hasResults: boolean; isProcessing: boolean }) => {
      setMarkdownContent('')
      setLogLines([])
      setErrorMessage('')
      setHasResults(hasResults)
      setIsProcessing(isProcessing)
      setCrawlFilesTracked([])
      setSelectedFileTracked(null)
      setVirtualFileContentByPathTracked({})
      setCurrentOutputDirTracked(null)
      setCrawlProgress(null)
      setStdoutLines([])
      setStdoutJsonTracked([])
      setCommandMode(null)
      setScreenshotFiles([])
      setCurrentJobIdTracked(null)
      setLifecycleEntries([])
      setCancelResponse(null)
      // Reset runtimeStateRef so reduceRuntimeState sees fresh state on the
      // next execution rather than stale values from the previous run.
      runtimeStateRef.current = makeInitialRuntimeState()
    },
    [
      setCrawlFilesTracked,
      setCurrentJobIdTracked,
      setCurrentOutputDirTracked,
      setSelectedFileTracked,
      setStdoutJsonTracked,
      setVirtualFileContentByPathTracked,
    ],
  )

  const resetWorkspaceRuntime = useCallback(
    (mode: string | null) => {
      setWorkspaceMode(mode)
      setWorkspacePrompt(null)
      setWorkspacePromptVersion(0)
      setWorkspaceContext(null)
    },
    [setWorkspaceContext, setWorkspaceMode, setWorkspacePrompt, setWorkspacePromptVersion],
  )

  const startExecution = useCallback(
    (mode: string, input?: string, options?: { preserveWorkspace?: boolean }) => {
      const preserveWorkspace = options?.preserveWorkspace === true
      currentModeRef.current = mode
      currentInputRef.current = input ?? ''
      setCurrentMode(mode)
      resetExecutionRuntime({ hasResults: true, isProcessing: true })
      if (!preserveWorkspace) {
        resetWorkspaceRuntime(null)
      }
    },
    [resetExecutionRuntime, resetWorkspaceRuntime],
  )

  const activateWorkspace = useCallback(
    (mode: string) => {
      currentModeRef.current = mode
      currentInputRef.current = ''
      setCurrentMode(mode)
      resetExecutionRuntime({ hasResults: false, isProcessing: false })
      resetWorkspaceRuntime(mode)
    },
    [resetExecutionRuntime, resetWorkspaceRuntime],
  )

  const submitWorkspacePrompt = useCallback(
    (prompt: string) => {
      setWorkspaceMode('pulse')
      setHasResults(true)
      setWorkspaceResumeSessionId(null)
      setWorkspaceResumeVersion(0)
      setWorkspacePrompt(prompt)
      bumpWorkspacePromptVersion()
    },
    [
      bumpWorkspacePromptVersion,
      setHasResults,
      setWorkspaceMode,
      setWorkspacePrompt,
      setWorkspaceResumeSessionId,
      setWorkspaceResumeVersion,
    ],
  )

  const resumeWorkspaceSession = useCallback(
    (sessionId: string) => {
      setWorkspaceMode('pulse')
      setHasResults(true)
      setWorkspacePrompt(null)
      setWorkspacePromptVersion(0)
      setWorkspaceResumeSessionId(sessionId)
      bumpWorkspaceResumeVersion()
    },
    [
      bumpWorkspaceResumeVersion,
      setHasResults,
      setWorkspaceMode,
      setWorkspacePrompt,
      setWorkspacePromptVersion,
      setWorkspaceResumeSessionId,
    ],
  )

  const clearWorkspaceResumeSession = useCallback(() => {
    setWorkspaceResumeSessionId(null)
    setWorkspaceResumeVersion(0)
  }, [setWorkspaceResumeSessionId, setWorkspaceResumeVersion])

  const deactivateWorkspace = useCallback(() => {
    currentModeRef.current = ''
    currentInputRef.current = ''
    setCurrentMode('')
    setWorkspaceMode(null)
    safeRemoveItem(LS_WORKSPACE_MODE)
    setWorkspacePrompt(null)
    setWorkspacePromptVersion(0)
    setWorkspaceResumeSessionId(null)
    setWorkspaceResumeVersion(0)
    setWorkspaceContext(null)
  }, [
    setWorkspaceContext,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
    setWorkspaceResumeSessionId,
    setWorkspaceResumeVersion,
  ])

  const updateWorkspaceContext = useCallback(
    (context: WorkspaceContextState | null) => {
      setWorkspaceContext(context)
    },
    [setWorkspaceContext],
  )

  const executionState = useMemo<WsMessagesExecutionState>(
    () => ({
      markdownContent,
      logLines,
      errorMessage,
      recentRuns,
      isProcessing,
      hasResults,
      currentMode,
      crawlFiles,
      selectedFile,
      crawlProgress,
      stdoutLines,
      stdoutJson,
      commandMode,
      screenshotFiles,
      currentJobId,
      lifecycleEntries,
      cancelResponse,
    }),
    [
      markdownContent,
      logLines,
      errorMessage,
      recentRuns,
      isProcessing,
      hasResults,
      currentMode,
      crawlFiles,
      selectedFile,
      crawlProgress,
      stdoutLines,
      stdoutJson,
      commandMode,
      screenshotFiles,
      currentJobId,
      lifecycleEntries,
      cancelResponse,
    ],
  )

  const workspaceState = useMemo<WsMessagesWorkspaceState>(
    () => ({
      workspaceMode,
      workspacePrompt,
      workspacePromptVersion,
      workspaceResumeSessionId,
      workspaceResumeVersion,
      workspaceContext,
      pulseAgent,
      pulseModel,
      pulsePermissionLevel,
      acpConfigOptions,
    }),
    [
      workspaceMode,
      workspacePrompt,
      workspacePromptVersion,
      workspaceResumeSessionId,
      workspaceResumeVersion,
      workspaceContext,
      pulseAgent,
      pulseModel,
      pulsePermissionLevel,
      acpConfigOptions,
    ],
  )

  const actions = useMemo<WsMessagesActions>(
    () => ({
      selectFile,
      setPulseAgent,
      setPulseModel,
      setPulsePermissionLevel,
      setAcpConfigOptions,
      activateWorkspace,
      submitWorkspacePrompt,
      resumeWorkspaceSession,
      clearWorkspaceResumeSession,
      deactivateWorkspace,
      updateWorkspaceContext,
      startExecution,
    }),
    [
      selectFile,
      activateWorkspace,
      submitWorkspacePrompt,
      resumeWorkspaceSession,
      clearWorkspaceResumeSession,
      deactivateWorkspace,
      updateWorkspaceContext,
      startExecution,
      setAcpConfigOptions,
      setPulseAgent,
      setPulseModel,
      setPulsePermissionLevel,
    ],
  )

  const value = useMemo<WsMessagesContextValue>(
    () => ({
      ...executionState,
      ...workspaceState,
      ...actions,
    }),
    [executionState, workspaceState, actions],
  )

  return { executionState, workspaceState, actions, value }
}
